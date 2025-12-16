use crate::{
    config::get_uri_from_config,
    handlers::{get_elevation, get_status, handle_options, post_elevations},
    telemetry::init_telemetry,
    tileset::{TileSetOptions, TileSetWithCache},
    types::{LatLng, LatLngs},
};
use opentelemetry::global;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info};
use warp::Filter;

mod config;
mod handlers;
mod telemetry;
mod tileset;
mod types;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Access the configuration values early to use them in runtime creation
    let config = config::CONFIG.clone();

    // Create tokio runtime with optional thread limit
    let runtime = if let Some(max_threads) = config.max_tokio_threads {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(max_threads)
            .enable_all()
            .build()?
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
    };

    runtime.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize telemetry FIRST, before anything else
    init_telemetry()?;

    // Access the configuration values
    let config = config::CONFIG.clone();
    let port = config.port;
    let bind = config.bind;
    let max_post_size = config.max_post_size;
    let path = get_uri_from_config(config.clone());

    info!("Starting elevation service");
    debug!("Cache Size: {}", config.cache_size);
    debug!("Tile Set Path: {:?}", path);
    debug!("Max Post Size: {}", max_post_size);
    debug!("Port: {}", port);
    debug!("Bind Address: {:?}", bind);
    debug!(
        "Max Parallel Processing: {}",
        config.max_parallel_processing
    );
    debug!("Max Number of Threads: {:?}", config.max_tokio_threads);
    debug!(
        "Max Concurrent Handlers: {}",
        config.max_concurrent_handlers
    );
    debug!("S3 Endpoint: {:?}", config.s3_endpoint);
    debug!("S3 Bucket: {:?}", config.s3_bucket);

    // Create semaphore for limiting concurrent handlers
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_handlers));

    let options = TileSetOptions {
        path: path,
        cache_size: config.cache_size,
        gzip: true,
        s3_access_key_id: config.s3_access_key_id.clone(),
        s3_secret_access_key: config.s3_secret_access_key.clone(),
        s3_region: config.s3_region.clone(),
        s3_endpoint: config.s3_endpoint.clone(),
    };
    let tileset = Arc::new(TileSetWithCache::new(options)?);

    // Create a shared filter for tileset
    let tileset_filter = warp::any().map(move || tileset.clone());
    let config_filter = warp::any().map(move || config.clone());
    let semaphore_filter = warp::any().map(move || semaphore.clone());

    // Define the /status route
    let status_route = warp::path("status")
        .and(warp::get())
        .and(tileset_filter.clone())
        .and(semaphore_filter.clone())
        .and_then(get_status);

    // Define the GET route for elevation
    let get_elevation_route = warp::path::end()
        .and(warp::get())
        .and(warp::query::<LatLng>())
        .and(tileset_filter.clone())
        .and(semaphore_filter.clone())
        .and_then(get_elevation)
        .or(warp::path("api")
            .and(warp::get())
            .and(warp::query::<LatLng>())
            .and(tileset_filter.clone())
            .and(semaphore_filter.clone())
            .and_then(get_elevation));

    // Define the POST route for elevations
    let post_elevation_route = warp::path::end()
        .and(warp::post())
        .and(warp::body::content_length_limit(max_post_size.as_u64()))
        .and(warp::body::json::<LatLngs>())
        .and(tileset_filter.clone())
        .and(config_filter.clone())
        .and(semaphore_filter.clone())
        .and_then(post_elevations)
        .or(warp::path("api")
            .and(warp::post())
            .and(warp::body::content_length_limit(max_post_size.as_u64()))
            .and(warp::body::json::<LatLngs>())
            .and(tileset_filter.clone())
            .and(config_filter.clone())
            .and(semaphore_filter.clone())
            .and_then(post_elevations));

    // Define OPTIONS route to handle CORS preflight requests
    let options_route = warp::options()
        .and(warp::path::full())
        .and_then(handle_options);

    // Create CORS configuration
    let cors = warp::cors()
        .allow_any_origin()
        .allow_headers(vec!["Content-Type", "Authorization"])
        .allow_methods(vec!["GET", "POST", "OPTIONS"]);

    // Combine routes and apply CORS
    let routes = warp::any()
        .and(
            status_route
                .or(get_elevation_route)
                .or(post_elevation_route)
                .or(options_route),
        )
        .with(warp::log("elevation-service"))
        .with(cors);

    // Start the server
    info!("Starting server on {}:{}", bind, port);
    warp::serve(routes).run((bind, port)).await;

    info!("Server shutting down");

    // Shutdown telemetry
    global::shutdown_tracer_provider();

    Ok(())
}
