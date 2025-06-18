use crate::{
    config::{Config, get_uri_from_config},
    handlers::{get_elevation, get_status, post_elevations},
    tileset::{TileSetOptions, TileSetWithCache},
};
use env_logger;
use log::debug;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use warp::Filter;

mod config;
mod handlers;
mod tileset;

#[derive(Deserialize)]
pub struct LatLng {
    lat: f64,
    lng: f64,
}

#[derive(Serialize)]
struct ElevationResponse {
    elevations: Vec<f64>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Access the configuration values
    let config = config::CONFIG.clone();

    // Initialize the logger
    env_logger::init();

    debug!("Cache Size: {}", config.cache_size);
    debug!("Tile Set Path: {:?}", config.tile_set_path);
    debug!("Max Post Size: {}", config.max_post_size);
    debug!("Port: {}", config.port);
    debug!("S3 Endpoint: {:?}", config.s3_endpoint);
    debug!("S3 Bucket: {:?}", config.s3_bucket);

    let path = get_uri_from_config(config.clone());

    let options = TileSetOptions {
        path: path,
        cache_size: config.cache_size,
        gzip: true,
    };
    let tileset = Arc::new(TileSetWithCache::new(options)?);

    // Create a shared filter for tileset
    let tileset_filter = warp::any().map(move || tileset.clone());

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
        .and_then(get_elevation);

    // Define the POST route for elevations
    let post_elevation_route = warp::path::end()
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::body::content_length_limit(
            config.max_post_size.as_u64(),
        ))
        .and(tileset_filter.clone())
        .and_then(post_elevations);

    // Combine routes
    let routes = warp::any().and(
        status_route
            .or(get_elevation_route)
            .or(post_elevation_route),
    );

    // Start the server
    warp::serve(routes).run((config.bind, config.port)).await;

    Ok(())
}
