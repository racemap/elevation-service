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
        Self {
            base_url,
            options,
            // One client, so the connection pool is shared across requests
            // instead of a fresh TLS handshake per tile.
            client: Client::new(),
        }
    }

    fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy {
            max_attempts: self.options.max_fetch_attempts,
            base_delay: self.options.retry_base_delay,
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
            .map_err(|e| TileError::transport(url, e))?;

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
            .map_err(|e| TileError::transport(url, e))?;

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
            .map_err(|e| TileError::transport(url, e))?;

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
