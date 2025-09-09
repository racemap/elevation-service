use futures::stream::StreamExt;
use rand;
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};
use tracing::{error, info, instrument};
use warp::{Rejection, Reply, filters::path::FullPath, reply};

use crate::{
    config::Config,
    tileset::TileSetWithCache,
    types::{ElevationResponse, LatLng, LatLngs},
};

#[instrument(skip(tileset))]
pub async fn get_status(tileset: Arc<TileSetWithCache>) -> Result<impl Reply, Rejection> {
    info!("Status check requested");
    let random_lat = rand::random::<f64>() * 180.0 - 90.0;
    let random_lng = rand::random::<f64>() * 360.0 - 180.0;
    match tileset.get_elevation(random_lat, random_lng).await {
        Ok(_) => {
            info!("Status check passed");
            Ok(reply::with_status("Ok", warp::http::StatusCode::OK))
        },
        Err(_) => {
            error!("Status check failed");
            Ok(reply::with_status(
                "Error",
                warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            ))
        },
    }
}

#[instrument(skip_all, fields(lat = query.lat, lng = query.lng))]
pub async fn get_elevation(
    query: LatLng,
    tileset: Arc<TileSetWithCache>,
) -> Result<impl Reply, Rejection> {
    info!("Single elevation request");
    let elevation = match tileset.get_elevation(query.lat, query.lng).await {
        Ok(elevation) => {
            info!(elevation = elevation, "Elevation retrieved successfully");
            elevation
        },
        Err(e) => {
            error!(error = %e, "Failed to get elevation");
            return Ok(convert_io_error_to_warp_replay(e).into_response());
        }
    };

    Ok(reply::json(&elevation).into_response())
}

#[instrument(skip_all, fields(points_count = locations.latlngs.len()))]
pub async fn post_elevations(
    locations: LatLngs,
    tileset: Arc<TileSetWithCache>,
    config: Config,
) -> Result<impl Reply, Rejection> {
    info!("Batch elevation request");
    let elevation_futures = locations.into_iter().map(|loc| {
        let lat = loc.0;
        let lng = loc.1;
        let tileset = tileset.clone();

        async move {
            tileset
                .get_elevation(lat, lng)
                .await
                .map(|elevation| elevation)
        }
    });

    let results = futures::stream::iter(elevation_futures)
        .buffered(config.max_parallel_processing)
        .collect::<Vec<_>>()
        .await;

    let mut elevations = Vec::new();
    for result in results {
        match result {
            Ok(elevation) => elevations.push(elevation),
            Err(e) => {
                error!(error = %e, "Failed to get elevation in batch request");
                return Ok(convert_io_error_to_warp_replay(e).into_response());
            }
        }
    }

    info!(elevations_count = elevations.len(), "Batch elevation request completed");
    Ok(reply::json(&ElevationResponse { elevations }).into_response())
}

#[instrument]
pub async fn handle_options(_: FullPath) -> Result<impl warp::Reply, warp::Rejection> {
    info!("CORS preflight request handled");
    Ok(warp::reply::with_status("", warp::http::StatusCode::OK))
}

fn convert_io_error_to_warp_replay(err: Error) -> impl Reply {
    let status = match err.kind() {
        ErrorKind::NotFound => warp::http::StatusCode::NOT_FOUND,
        ErrorKind::InvalidInput => warp::http::StatusCode::BAD_REQUEST,
        _ => {
            error!(error = %err, "Error fetching elevation");
            warp::http::StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    return reply::with_status(err.to_string(), status).into_response();
}
