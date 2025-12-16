use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime::Tokio, trace::Sampler};
use opentelemetry_stdout as stdout;
use std::time::Duration;
use tracing::{info, warn};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan, prelude::*};

pub fn init_telemetry() -> Result<(), Box<dyn std::error::Error>> {
    // Get service name from environment or use default
    let service_name =
        std::env::var("SERVICE_NAME").unwrap_or_else(|_| "elevation-service".to_string());

    // Check for debug mode - prints traces to console
    let debug_traces = std::env::var("OTEL_DEBUG_TRACES").is_ok();

    // Check if any OTLP endpoint is configured (general or signal-specific)
    let general_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    let traces_endpoint = std::env::var("OTEL_TRACES_COLLECTOR_URL").ok();

    if debug_traces {
        // Debug mode: print traces to stdout
        let provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_simple_exporter(stdout::SpanExporter::default())
            .with_config(
                opentelemetry_sdk::trace::config()
                    .with_sampler(Sampler::AlwaysOn)
                    .with_resource(opentelemetry_sdk::Resource::new(vec![
                        opentelemetry_semantic_conventions::resource::SERVICE_NAME
                            .string(service_name.clone()),
                    ])),
            )
            .build();

        let tracer = provider.tracer(service_name.clone());

        // Set as global provider so it doesn't get dropped
        opentelemetry::global::set_tracer_provider(provider);

        let telemetry_layer = OpenTelemetryLayer::new(tracer);

        tracing_subscriber::registry()
            .with(EnvFilter::from_default_env())
            .with(tracing_subscriber::fmt::layer())
            .with(telemetry_layer)
            .try_init()
            .map_err(|e| format!("Failed to initialize tracing: {}", e))?;

        warn!("OpenTelemetry DEBUG mode - traces printed to stdout");
    } else if general_endpoint.is_some() || traces_endpoint.is_some() {
        // Determine which endpoint to use for traces (signal-specific takes priority)
        let trace_endpoint = traces_endpoint
            .or_else(|| general_endpoint.clone())
            .unwrap_or_else(|| "http://localhost:4317".to_string());

        // Build the OTLP exporter
        let exporter = opentelemetry_otlp::new_exporter()
            .http()
            .with_endpoint(&trace_endpoint)
            .with_timeout(Duration::from_secs(10))
            .build_span_exporter()?;

        // Configure batch processor with larger queue to handle high throughput
        let batch_processor =
            opentelemetry_sdk::trace::BatchSpanProcessor::builder(exporter, Tokio)
                .with_max_queue_size(8192) // Increase queue size (default: 2048)
                .with_max_export_batch_size(512) // Export in larger batches (default: 512)
                .with_scheduled_delay(Duration::from_secs(2)) // Export every 2 seconds (default: 5s)
                .with_max_timeout(Duration::from_secs(10)) // Fail fast if collector is slow
                .build();

        // Build the tracer provider
        let provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_span_processor(batch_processor)
            .with_config(
                opentelemetry_sdk::trace::config()
                    .with_sampler(Sampler::AlwaysOn)
                    .with_resource(opentelemetry_sdk::Resource::new(vec![
                        opentelemetry_semantic_conventions::resource::SERVICE_NAME
                            .string(service_name.clone()),
                        opentelemetry_semantic_conventions::resource::SERVICE_VERSION
                            .string(env!("CARGO_PKG_VERSION")),
                    ])),
            )
            .build();

        let tracer = provider.tracer(service_name.clone());

        // Set as global provider so it doesn't get dropped
        opentelemetry::global::set_tracer_provider(provider);

        // Set up tracing subscriber with OpenTelemetry layer
        let telemetry_layer = OpenTelemetryLayer::new(tracer);

        tracing_subscriber::registry()
            .with(EnvFilter::from_default_env())
            .with(tracing_subscriber::fmt::layer())
            .with(telemetry_layer)
            .try_init()
            .map_err(|e| format!("Failed to initialize tracing with OpenTelemetry: {}", e))?;

        info!("OpenTelemetry tracing initialized successfully");
        info!("Trace endpoint: {}", trace_endpoint);
    } else {
        warn!("OTEL_EXPORTER_OTLP_ENDPOINT not set, falling back to basic tracing");
        let stdout_layer = tracing_subscriber::fmt::layer()
            .with_ansi(true)
            .with_target(true)
            .with_span_events(FmtSpan::CLOSE);

        // Fall back to basic tracing without OTLP export
        tracing_subscriber::registry()
            .with(EnvFilter::from_default_env())
            .with(stdout_layer)
            .try_init()
            .map_err(|e| format!("Failed to initialize basic tracing: {}", e))?;

        info!("Basic tracing initialized (no OTLP export)");
    }

    Ok(())
}
