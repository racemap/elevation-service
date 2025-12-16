use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::runtime::Tokio;
use tracing::{info, warn};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan, prelude::*};

pub fn init_telemetry() -> Result<(), Box<dyn std::error::Error>> {
    // Get service name from environment or use default
    let service_name =
        std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "elevation-service".to_string());

    // Check if any OTLP endpoint is configured (general or signal-specific)
    let general_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    let traces_endpoint = std::env::var("OTEL_TRACES_COLLECTOR_URL").ok();

    if general_endpoint.is_some() || traces_endpoint.is_some() {
        // Determine which endpoint to use for traces (signal-specific takes priority)
        let trace_endpoint = traces_endpoint
            .or_else(|| general_endpoint.clone())
            .unwrap_or_else(|| "http://localhost:4317".to_string());

        // Configure OTLP exporter for traces using HTTP (more compatible with proxies)
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .http()
                    .with_endpoint(&trace_endpoint),
            )
            .with_trace_config(opentelemetry_sdk::trace::config().with_resource(
                opentelemetry_sdk::Resource::new(vec![
                        opentelemetry_semantic_conventions::resource::SERVICE_NAME
                            .string(service_name.clone()),
                        opentelemetry_semantic_conventions::resource::SERVICE_VERSION
                            .string(env!("CARGO_PKG_VERSION")),
                    ]),
            ))
            .install_batch(Tokio)?;

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
