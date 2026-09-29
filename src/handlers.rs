use futures::stream::StreamExt;
use std::{
    io::{Error, ErrorKind},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::sync::Semaphore;
use tracing::{error, info, instrument, warn};
use warp::{Rejection, Reply, filters::path::FullPath, reply};

use crate::{
    config::Config,
    tileset::TileSetWithCache,
    types::{ElevationResponse, LatLng, LatLngs, StatusResponse},
};

#[derive(Debug)]
struct InternalError;

impl warp::reject::Reject for InternalError {}

/// Tracks how the tile backend has been behaving across `/status` calls.
///
/// The old check sampled a uniformly random coordinate and downloaded the whole
/// ~25 MB tile, so every poll was an expensive live round trip that inherited
/// the bucket's ~1-2% transient error rate and turned it straight into a 500.
///
/// Two changes make the signal trustworthy without making it deaf:
///
/// - The probe is a **HEAD against a fixed, known key**, issued live on every
///   poll and deliberately bypassing the tile cache. It still detects object
///   storage being down, unreachable, or misconfigured, but costs no bandwidth.
///   A fixed key also means a 404 is meaningful (the bucket or prefix is
///   wrong), where a random coordinate could legitimately miss.
/// - A single failure is not fatal. Reporting 500 only after
///   `failure_threshold` consecutive failures separates a real outage from the
///   transient errors object storage produces as a matter of course.
#[derive(Debug)]
pub struct StatusState {
    consecutive_failures: AtomicU32,
    failure_threshold: u32,
    probe_lat: f64,
    probe_lng: f64,
    probe_timeout: Duration,
}

impl StatusState {
    pub fn new(
        probe_lat: f64,
        probe_lng: f64,
        failure_threshold: u32,
        probe_timeout: Duration,
    ) -> Self {
        Self {
            consecutive_failures: AtomicU32::new(0),
            failure_threshold: failure_threshold.max(1),
            probe_lat,
            probe_lng,
            probe_timeout,
        }
    }

    fn record_success(&self) -> u32 {
        self.consecutive_failures.swap(0, Ordering::Relaxed)
    }

    fn record_failure(&self) -> u32 {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1
    }
}

/// Liveness check: is this process up and serving?
///
/// Deliberately free of any dependency on object storage, so the container
/// health check cannot restart a healthy service because a third party had a
/// bad minute.
#[instrument]
pub async fn get_health() -> Result<impl Reply, Rejection> {
    Ok(reply::with_status("Ok", warp::http::StatusCode::OK))
}

/// Readiness check: is this process up *and* is the tile backend answering?
#[instrument(skip_all)]
pub async fn get_status(
    tileset: Arc<TileSetWithCache>,
    state: Arc<StatusState>,
    semaphore: Arc<Semaphore>,
) -> Result<impl Reply, Rejection> {
    let _permit = semaphore.acquire().await.map_err(|_| {
        error!("Failed to acquire semaphore permit for status check");
        warp::reject::custom(InternalError)
    })?;

    info!("Status check requested");

    // Bounded, so a hanging backend cannot hang the health check itself.
    let probe = tileset.probe_backend(state.probe_lat, state.probe_lng);
    let outcome = match tokio::time::timeout(state.probe_timeout, probe).await {
        Ok(result) => result,
        Err(_) => Err(Error::new(
            ErrorKind::TimedOut,
            format!(
                "tile backend probe timed out after {}ms",
                state.probe_timeout.as_millis()
            ),
        )),
    };

    match outcome {
        Ok(_) => {
            let recovered_from = state.record_success();
            if recovered_from > 0 {
                info!(
                    previous_consecutive_failures = recovered_from,
                    "Status check recovered"
                );
            } else {
                info!("Status check passed");
            }
            Ok(status_reply(
                StatusResponse {
                    status: "ok",
                    tileset: "ok",
                    consecutive_failures: 0,
                    error: None,
                },
                warp::http::StatusCode::OK,
            ))
        }
        Err(e) => {
            let failures = state.record_failure();
            let sustained = failures >= state.failure_threshold;

            // Log the reason. The previous `Err(_)` discarded it, which is why a
            // failing check said only "Status check failed".
            if sustained {
                error!(
                    error = %e,
                    consecutive_failures = failures,
                    failure_threshold = state.failure_threshold,
                    probe_coord = %format!("{},{}", state.probe_lat, state.probe_lng),
                    "Status check failed; reporting service as unhealthy"
                );
            } else {
                warn!(
                    error = %e,
                    consecutive_failures = failures,
                    failure_threshold = state.failure_threshold,
                    probe_coord = %format!("{},{}", state.probe_lat, state.probe_lng),
                    "Status check failed; still within tolerance, reporting degraded"
                );
            }

            let (status, code) = if sustained {
                ("unhealthy", warp::http::StatusCode::INTERNAL_SERVER_ERROR)
            } else {
                ("degraded", warp::http::StatusCode::OK)
            };

            Ok(status_reply(
                StatusResponse {
                    status,
                    tileset: "error",
                    consecutive_failures: failures,
                    error: Some(e.to_string()),
                },
                code,
            ))
        }
    }
}

fn status_reply(body: StatusResponse, code: warp::http::StatusCode) -> impl Reply {
    reply::with_status(reply::json(&body), code)
}

#[instrument(skip_all, fields(coord = format!("{},{}", query.lat, query.lng)))]
pub async fn get_elevation(
    query: LatLng,
    tileset: Arc<TileSetWithCache>,
    semaphore: Arc<Semaphore>,
) -> Result<impl Reply, Rejection> {
    let _permit = semaphore.acquire().await.map_err(|_| {
        error!("Failed to acquire semaphore permit for elevation request");
        warp::reject::custom(InternalError)
    })?;

    info!("Single elevation request");
    let elevation = match tileset.get_elevation(query.lat, query.lng).await {
        Ok(elevation) => {
            info!(elevation = elevation, "Elevation retrieved successfully");
            elevation
        }
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
    semaphore: Arc<Semaphore>,
) -> Result<impl Reply, Rejection> {
    let _permit = semaphore.acquire().await.map_err(|_| {
        error!("Failed to acquire semaphore permit for batch elevation request");
        warp::reject::custom(InternalError)
    })?;

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

    info!(
        elevations_count = elevations.len(),
        "Batch elevation request completed"
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tileset::TileSetOptions;
    use serde_json::Value;
    use warp::Filter;
    use warp::http::StatusCode;

    /// The fixture set only contains N45/N45E009, so (45.5, 9.5) resolves and
    /// any other coordinate stands in for an unreachable backend.
    const PRESENT_LAT: f64 = 45.5;
    const PRESENT_LNG: f64 = 9.5;
    const MISSING_LAT: f64 = 10.5;
    const MISSING_LNG: f64 = 20.5;

    fn state(probe_lat: f64, probe_lng: f64, threshold: u32) -> Arc<StatusState> {
        Arc::new(StatusState::new(
            probe_lat,
            probe_lng,
            threshold,
            Duration::from_secs(5),
        ))
    }

    fn tileset_at(path: &str) -> Arc<TileSetWithCache> {
        Arc::new(
            TileSetWithCache::new(TileSetOptions {
                path: String::from(path),
                ..Default::default()
            })
            .unwrap(),
        )
    }

    fn tileset() -> Arc<TileSetWithCache> {
        tileset_at("test_files")
    }

    fn status_filter_with(
        tileset: Arc<TileSetWithCache>,
        state: Arc<StatusState>,
    ) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
        let semaphore = Arc::new(Semaphore::new(8));
        warp::path("status")
            .and(warp::get())
            .and(warp::any().map(move || tileset.clone()))
            .and(warp::any().map(move || state.clone()))
            .and(warp::any().map(move || semaphore.clone()))
            .and_then(get_status)
    }

    async fn call_status_on(
        tileset: Arc<TileSetWithCache>,
        state: Arc<StatusState>,
    ) -> (StatusCode, Value) {
        let response = warp::test::request()
            .path("/status")
            .reply(&status_filter_with(tileset, state))
            .await;
        let code = response.status();
        let body: Value = serde_json::from_slice(response.body()).expect("status body is JSON");
        (code, body)
    }

    async fn call_status(state: Arc<StatusState>) -> (StatusCode, Value) {
        call_status_on(tileset(), state).await
    }

    #[tokio::test]
    async fn status_is_ok_when_the_probe_tile_resolves() {
        let state = state(PRESENT_LAT, PRESENT_LNG, 3);
        let (code, body) = call_status(state).await;

        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["tileset"], "ok");
        assert_eq!(body["consecutive_failures"], 0);
        assert!(body.get("error").is_none());
    }

    #[tokio::test]
    async fn a_single_probe_failure_is_degraded_not_down() {
        // The whole point of the fix: one transient object-storage blip must
        // not turn into an uptime incident.
        let state = state(MISSING_LAT, MISSING_LNG, 3);
        let (code, body) = call_status(state).await;

        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["status"], "degraded");
        assert_eq!(body["tileset"], "error");
        assert_eq!(body["consecutive_failures"], 1);
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "the failure reason must be reported, not swallowed: {}",
            body
        );
    }

    #[tokio::test]
    async fn sustained_probe_failures_report_unhealthy() {
        let state = state(MISSING_LAT, MISSING_LNG, 3);

        for expected in 1..3 {
            let (code, body) = call_status(state.clone()).await;
            assert_eq!(
                code,
                StatusCode::OK,
                "failure {} should be tolerated",
                expected
            );
            assert_eq!(body["consecutive_failures"], expected);
        }

        let (code, body) = call_status(state).await;
        assert_eq!(code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["status"], "unhealthy");
        assert_eq!(body["consecutive_failures"], 3);
    }

    #[tokio::test]
    async fn a_threshold_of_one_fails_immediately() {
        let state = state(MISSING_LAT, MISSING_LNG, 1);
        let (code, body) = call_status(state).await;

        assert_eq!(code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["status"], "unhealthy");
    }

    #[tokio::test]
    async fn a_threshold_of_zero_is_clamped_to_one() {
        let state = state(MISSING_LAT, MISSING_LNG, 0);
        let (code, _) = call_status(state).await;
        assert_eq!(code, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn a_success_clears_accumulated_failures() {
        let state = state(PRESENT_LAT, PRESENT_LNG, 3);
        assert_eq!(state.record_failure(), 1);
        assert_eq!(state.record_failure(), 2);

        let (code, body) = call_status(state.clone()).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["consecutive_failures"], 0);

        // And the next failure starts counting from scratch.
        assert_eq!(state.record_failure(), 1);
    }

    #[tokio::test]
    async fn status_probes_the_same_key_on_every_poll() {
        let state = state(PRESENT_LAT, PRESENT_LNG, 3);
        for _ in 0..5 {
            let (code, body) = call_status(state.clone()).await;
            assert_eq!(code, StatusCode::OK);
            assert_eq!(body["status"], "ok");
        }
    }

    /// Regression test for the flaw @normanrz caught in review: if the probe
    /// went through the tile cache, `/status` would answer `ok` from memory
    /// forever once warmed, and a completely dead backend would go unnoticed.
    ///
    /// Warm the cache, then take the backend away, and the status check must
    /// still notice.
    #[tokio::test]
    async fn status_detects_a_dead_backend_even_after_the_tile_is_cached() {
        let dir = std::env::temp_dir().join(format!(
            "elevation-probe-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let tile_dir = dir.join("N45");
        std::fs::create_dir_all(&tile_dir).unwrap();
        let tile = tile_dir.join("N45E009.hgt.gz");
        std::fs::copy("test_files/N45/N45E009.hgt.gz", &tile).unwrap();

        let tileset = tileset_at(dir.to_str().unwrap());

        // Warm the cache the way real traffic would.
        assert!(
            tileset
                .get_elevation(PRESENT_LAT, PRESENT_LNG)
                .await
                .is_ok(),
            "fixture should resolve before the backend is removed"
        );
        let state = state(PRESENT_LAT, PRESENT_LNG, 1);
        let (code, body) = call_status_on(tileset.clone(), state.clone()).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["status"], "ok");

        // Now the backend goes away. The tile is still cached, so
        // `get_elevation` keeps succeeding -- which is precisely why the probe
        // must not use it.
        std::fs::remove_file(&tile).unwrap();
        assert!(
            tileset
                .get_elevation(PRESENT_LAT, PRESENT_LNG)
                .await
                .is_ok(),
            "the cached tile should still serve, proving the cache is warm"
        );

        let (code, body) = call_status_on(tileset, state).await;
        assert_eq!(
            code,
            StatusCode::INTERNAL_SERVER_ERROR,
            "a dead backend must be reported even with a warm cache: {}",
            body
        );
        assert_eq!(body["status"], "unhealthy");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn health_is_ok_without_touching_the_tile_backend() {
        // Pointed at a folder that does not exist: liveness must not care.
        let tileset = TileSetWithCache::new(TileSetOptions {
            path: String::from("/nonexistent-tile-folder"),
            ..Default::default()
        })
        .unwrap();
        assert!(
            tileset
                .get_elevation(PRESENT_LAT, PRESENT_LNG)
                .await
                .is_err()
        );

        let route = warp::path("health")
            .and(warp::get())
            .and(warp::path::end())
            .and_then(get_health);
        let response = warp::test::request().path("/health").reply(&route).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body(), "Ok");
    }
}
