use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider,
};
use opentelemetry_stdout as stdout;
use std::time::Duration;
use tracing::{info, warn};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan, prelude::*};

/// Initialize tracing and, when configured, OpenTelemetry trace export.
///
/// The returned provider must be kept alive for the lifetime of the process and
/// shut down on exit so buffered spans are flushed.
pub fn init_telemetry() -> Result<Option<SdkTracerProvider>, Box<dyn std::error::Error>> {
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
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(stdout::SpanExporter::default())
            .with_sampler(Sampler::AlwaysOn)
            .with_resource(
                Resource::builder()
                    .with_service_name(service_name.clone())
                    .build(),
            )
            .build();

        let tracer = provider.tracer(service_name.clone());

        // Set as global provider so it doesn't get dropped
        global::set_tracer_provider(provider.clone());

        let telemetry_layer = OpenTelemetryLayer::new(tracer);

        tracing_subscriber::registry()
            .with(EnvFilter::from_default_env())
            .with(tracing_subscriber::fmt::layer())
            .with(telemetry_layer)
            .try_init()
            .map_err(|e| format!("Failed to initialize tracing: {}", e))?;

        warn!("OpenTelemetry DEBUG mode - traces printed to stdout");

        Ok(Some(provider))
    } else if general_endpoint.is_some() || traces_endpoint.is_some() {
        // The exporter appends the signal path only to endpoints it reads from the
        // environment, so a programmatic endpoint has to be the full trace URL.
        let collector = traces_endpoint
            .or_else(|| general_endpoint.clone())
            .unwrap_or_else(|| "http://localhost:4318".to_string())
            .trim_end_matches('/')
            .trim_end_matches("/v1/traces")
            .trim_end_matches('/')
            .to_string();
        let trace_endpoint = format!("{}/v1/traces", collector);

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(&trace_endpoint)
            .with_timeout(Duration::from_secs(10))
            .build()?;

        // Larger queue and shorter interval to keep up with request throughput
        let batch_config = BatchConfigBuilder::default()
            .with_max_queue_size(8192)
            .with_max_export_batch_size(512)
            .with_scheduled_delay(Duration::from_secs(2))
            .build();

        let provider = SdkTracerProvider::builder()
            .with_span_processor(
                BatchSpanProcessor::builder(exporter)
                    .with_batch_config(batch_config)
                    .build(),
            )
            .with_sampler(Sampler::AlwaysOn)
            .with_resource(
                Resource::builder()
                    .with_service_name(service_name.clone())
                    .with_attribute(KeyValue::new(
                        opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
                        env!("CARGO_PKG_VERSION"),
                    ))
                    .build(),
            )
            .build();

        let tracer = provider.tracer(service_name.clone());

        // Set as global provider so it doesn't get dropped
        global::set_tracer_provider(provider.clone());

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

        Ok(Some(provider))
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

        Ok(None)
    }
}
