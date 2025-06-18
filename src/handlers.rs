use rand;
use std::sync::Arc;
use warp::{Rejection, Reply, reply};

use crate::{
    tileset::TileSetWithCache,
    types::{ElevationResponse, LatLng, LatLngs},
};

pub async fn get_status(tileset: Arc<TileSetWithCache>) -> Result<impl Reply, Rejection> {
    let random_lat = rand::random::<f64>() * 180.0 - 90.0;
    let random_lng = rand::random::<f64>() * 360.0 - 180.0;
    match tileset.get_elevation(random_lat, random_lng).await {
        Ok(_) => Ok(reply::with_status("Ok", warp::http::StatusCode::OK)),
        Err(_) => Ok(reply::with_status(
            "Error",
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        )),
    }
}

pub async fn get_elevation(
    query: LatLng,
    tileset: Arc<TileSetWithCache>,
) -> Result<impl Reply, Rejection> {
    if query.lat < -90.0 || query.lat > 90.0 || query.lng < -180.0 || query.lng > 180.0 {
        return Ok(reply::with_status(
            "Invalid Latitude or Longitude",
            warp::http::StatusCode::BAD_REQUEST,
        )
        .into_response());
    }

    let elevation = match tileset.get_elevation(query.lat, query.lng).await {
        Ok(elevation) => elevation,
        Err(_) => {
            return Ok(
                reply::with_status("Error", warp::http::StatusCode::INTERNAL_SERVER_ERROR)
                    .into_response(),
            );
        }
    };

    Ok(reply::json(&elevation).into_response())
}

pub async fn post_elevations(
    locations: LatLngs,
    tileset: Arc<TileSetWithCache>,
) -> Result<impl Reply, Rejection> {
    let mut elevations = Vec::new();
    for loc in locations {
        if loc.0 < -90.0 || loc.0 > 90.0 || loc.1 < -180.0 || loc.1 > 180.0 {
            return Ok(reply::with_status(
                "Invalid Latitude or Longitude",
                warp::http::StatusCode::BAD_REQUEST,
            )
            .into_response());
        }
        match tileset.get_elevation(loc.0, loc.1).await {
            Ok(elevation) => elevations.push(elevation as f64),
            Err(_) => {
                return Ok(reply::with_status(
                    "Error",
                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                )
                .into_response());
            }
        }
    }
    Ok(reply::json(&ElevationResponse { elevations }).into_response())
}
