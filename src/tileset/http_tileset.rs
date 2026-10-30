use crate::tileset::error::TileError;
use crate::tileset::retry::{RetryPolicy, with_retry};
use crate::tileset::{TileSetOptions, TileSetWithCache};
use flate2::read::GzDecoder;
use reqwest::Client;
use std::io::Read;
use tracing::{debug, instrument};

pub struct HTTPTileSet {
    base_url: String,
    options: TileSetOptions,
    client: Client,
}

impl HTTPTileSet {
    pub fn new(base_url: String, options: TileSetOptions) -> Self {
        // One client, so the connection pool is shared across requests instead
        // of a fresh TLS handshake per tile. The timeouts stop a stalled
        // upstream from holding a request (and a semaphore permit) forever;
        // `timeout` covers the whole exchange, response body included.
        let client = Client::builder()
            .connect_timeout(options.fetch_connect_timeout)
            .timeout(options.fetch_timeout)
            .build()
            .expect("failed to build HTTP tile client");

        Self {
            base_url,
            options,
            client,
        }
    }

    fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy {
            max_attempts: self.options.max_fetch_attempts,
            base_delay: self.options.retry_base_delay,
        }
    }

    /// reqwest's `Display` for a timeout is just "error sending request for
    /// url (...)", which hides why it failed. Spell the timeout out so it is
    /// visible in the logs and in the `/status` error text.
    fn transport_error(&self, url: &str, err: reqwest::Error) -> TileError {
        if err.is_timeout() {
            let (phase, budget) = if err.is_connect() {
                ("connect", self.options.fetch_connect_timeout)
            } else {
                ("request", self.options.fetch_timeout)
            };
            TileError::transport(
                url,
                format!("{phase} timed out after {}ms", budget.as_millis()),
            )
        } else {
            TileError::transport(url, err)
        }
    }

    fn url_for(&self, lat: f64, lng: f64) -> Result<String, TileError> {
        Ok(format!(
            "{}/{}",
            self.base_url,
            TileSetWithCache::get_file_path(lat, lng)
                .map_err(|e| TileError::decode(format!("{},{}", lat, lng), e))?
        ))
    }

    #[instrument(level="debug", name="get_tile_http", skip_all, fields(coord = format!("{},{}", lat, lng)))]
    pub async fn get_tile(&self, lat: f64, lng: f64) -> Result<Vec<u8>, TileError> {
        let url = self.url_for(lat, lng)?;
        debug!("Fetching tile from: {}", url);

        with_retry(self.retry_policy(), &url, || self.fetch(&url)).await
    }

    /// Reachability probe for `/status` — see `S3TileSet::probe`. Never cached.
    #[instrument(level="debug", name="probe_http", skip_all, fields(coord = format!("{},{}", lat, lng)))]
    pub async fn probe(&self, lat: f64, lng: f64) -> Result<(), TileError> {
        let url = self.url_for(lat, lng)?;
        debug!("Probing: HEAD {}", url);

        with_retry(self.retry_policy(), &url, || self.head(&url)).await
    }

    async fn head(&self, url: &str) -> Result<(), TileError> {
        let response = self
            .client
            .head(url)
            .send()
            .await
            .map_err(|e| self.transport_error(url, e))?;

        let status = response.status();
        if !status.is_success() {
            return Err(TileError::upstream(url, status.as_u16(), "(HEAD, no body)"));
        }
        Ok(())
    }

    async fn fetch(&self, url: &str) -> Result<Vec<u8>, TileError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| self.transport_error(url, e))?;

        // Same defect as the S3 backend: without this check an error body is
        // handed to the gzip decoder and surfaces as `invalid gzip header`.
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(TileError::upstream(url, status.as_u16(), body));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| self.transport_error(url, e))?;

        // Handle gzip decompression if needed
        if self.options.gzip {
            let mut decoder = GzDecoder::new(&bytes[..]);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| TileError::decode(url, e))?;
            Ok(decompressed)
        } else {
            Ok(bytes.to_vec())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HTTPTileSet;
    use crate::tileset::TileSetOptions;
    use crate::tileset::error::TileError;
    use std::net::SocketAddr;
    use std::time::{Duration, Instant};
    use tokio::net::TcpListener;

    /// Starts a server that accepts every connection and never writes a byte.
    async fn stalled_server() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _server = tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                // Hold the socket open without answering.
                tokio::spawn(async move {
                    let _socket = socket;
                    tokio::time::sleep(Duration::from_secs(60)).await;
                });
            }
        });
        addr
    }

    fn timed_options(attempts: u32, timeout: Duration) -> TileSetOptions {
        TileSetOptions {
            max_fetch_attempts: attempts,
            retry_base_delay: Duration::from_millis(1),
            fetch_connect_timeout: timeout,
            fetch_timeout: timeout,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn a_stalled_upstream_times_out_instead_of_hanging() {
        let addr = stalled_server().await;
        let tileset = HTTPTileSet::new(
            format!("http://{addr}/tiles"),
            timed_options(1, Duration::from_millis(100)),
        );

        let started = Instant::now();
        let err = tileset.get_tile(1.0, 1.0).await.unwrap_err();

        assert!(matches!(err, TileError::Transport { .. }), "{err}");
        assert!(
            err.to_string().to_lowercase().contains("timed out"),
            "a timeout must say so: {err}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the request should be cut short, took {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn a_timed_out_attempt_is_retried_within_the_documented_bound() {
        let addr = stalled_server().await;
        let tileset = HTTPTileSet::new(
            format!("http://{addr}/tiles"),
            timed_options(3, Duration::from_millis(100)),
        );

        let started = Instant::now();
        let err = tileset.get_tile(1.0, 1.0).await.unwrap_err();
        let elapsed = started.elapsed();

        assert!(matches!(err, TileError::Transport { .. }), "{err}");
        // All three attempts must run, so the wait is bounded by
        // attempts * timeout plus the (capped) backoffs.
        assert!(
            elapsed >= Duration::from_millis(250),
            "timeouts should be retried, gave up after {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(3),
            "worst case must stay bounded, took {elapsed:?}"
        );
    }
}
