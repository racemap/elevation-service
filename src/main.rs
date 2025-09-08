use crate::{
    config::get_uri_from_config,
    handlers::{get_elevation, get_status, handle_options, post_elevations},
    tileset::{TileSetOptions, TileSetWithCache},
    types::{LatLng, LatLngs},
};
use env_logger;
use log::debug;
use std::sync::Arc;
use warp::Filter;

mod config;
mod handlers;
mod tileset;
mod types;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Access the configuration values
    let config = config::CONFIG.clone();
    let port = config.port;
    let bind = config.bind;
    let max_post_size = config.max_post_size;

    // Initialize the logger
    env_logger::init();

    debug!("Cache Size: {}", config.cache_size);
    debug!("Tile Set Path: {:?}", config.tile_set_path);
    debug!("Max Post Size: {}", max_post_size);
    debug!("Port: {}", port);
    debug!("Bind Address: {:?}", bind);
    debug!(
        "Max Parallel Processing: {}",
        config.max_parallel_processing
    );
    debug!("S3 Endpoint: {:?}", config.s3_endpoint);
    debug!("S3 Bucket: {:?}", config.s3_bucket);

    let path = get_uri_from_config(config.clone());

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

    // Define the /status route
    let status_route = warp::path("status")
        .and(warp::get())
        .and(tileset_filter.clone())
        .and_then(get_status);

    // Define the GET route for elevation
    let get_elevation_route = warp::path::end()
        .and(warp::get())
        .and(warp::query::<LatLng>())
        .and(tileset_filter.clone())
        .and_then(get_elevation)
        .or(warp::path("api")
            .and(warp::get())
            .and(warp::query::<LatLng>())
            .and(tileset_filter.clone())
            .and_then(get_elevation));

    // Define the POST route for elevations
    let post_elevation_route = warp::path::end()
        .and(warp::post())
        .and(warp::body::content_length_limit(max_post_size.as_u64()))
        .and(warp::body::json::<LatLngs>())
        .and(tileset_filter.clone())
        .and(config_filter.clone())
        .and_then(post_elevations)
        .or(warp::path("api")
            .and(warp::post())
            .and(warp::body::content_length_limit(max_post_size.as_u64()))
            .and(warp::body::json::<LatLngs>())
            .and(tileset_filter.clone())
            .and(config_filter.clone())
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
        .with(warp::log("elevation_service"))
        .with(cors);

    // Start the server
    warp::serve(routes).run((bind, port)).await;

    Ok(())
}
