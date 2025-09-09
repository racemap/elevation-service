use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::runtime::Tokio;
use tracing::{info, warn};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan, prelude::*};

pub fn init_telemetry() -> Result<(), Box<dyn std::error::Error>> {
    // Get service name from environment or use default
    let service_name =
        std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "elevation-service".to_string());

    // Check if OTLP endpoint is configured
    if let Ok(otlp_endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        info!(
            "Initializing OpenTelemetry with OTLP endpoint: {}",
            otlp_endpoint
        );

        // Configure OTLP exporter
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(&otlp_endpoint),
            )
            .with_trace_config(opentelemetry_sdk::trace::config().with_resource(
                opentelemetry_sdk::Resource::new(vec![
                        opentelemetry_semantic_conventions::resource::SERVICE_NAME
                            .string(service_name),
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

    // Note: Skipping LogTracer::init() to avoid conflicts with existing loggers.
    // Modern approach is to use tracing directly instead of log macros.

    Ok(())
}
