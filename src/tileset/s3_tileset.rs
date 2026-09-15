use crate::tileset::error::TileError;
use crate::tileset::retry::{RetryPolicy, with_retry};
use crate::tileset::{TileSetOptions, TileSetWithCache};
use flate2::read::GzDecoder;
use s3::{Bucket, Region, creds::Credentials};
use std::io::Read;
use tracing::{debug, instrument};

pub struct S3TileSet {
    bucket: Box<Bucket>,
    key_prefix: String,
    gzip: bool,
    retry_policy: RetryPolicy,
}

impl S3TileSet {
    pub fn new(
        bucket_name: String,
        key_prefix: String,
        options: TileSetOptions,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let TileSetOptions {
            gzip,
            s3_access_key_id: access_key_id,
            s3_secret_access_key: secret_access_key,
            s3_region: region,
            s3_endpoint: endpoint,
            max_fetch_attempts,
            retry_base_delay,
            ..
        } = options;

        // Set up credentials
        let credentials =
            if let (Some(access_key), Some(secret_key)) = (access_key_id, secret_access_key) {
                Credentials::new(
                    Some(&access_key),
                    Some(&secret_key),
                    None, // security_token
                    None, // session_token
                    None, // expiration
                )?
            } else {
                // Try to use default credentials (environment variables, IAM roles, etc.)
                Credentials::default()?
            };

        // Set up region
        let region = if let Some(region_str) = region {
            if let Some(endpoint_url) = endpoint {
                // Custom endpoint (e.g., MinIO, DigitalOcean Spaces)
                Region::Custom {
                    region: region_str,
                    endpoint: endpoint_url,
                }
            } else {
                // Standard AWS region
                region_str.parse().unwrap_or(Region::UsEast1)
            }
        } else if let Some(endpoint_url) = endpoint {
            // Custom endpoint with default region
            Region::Custom {
                region: "us-east-1".to_string(),
                endpoint: endpoint_url,
            }
        } else {
            // Default to us-east-1 for AWS
            Region::UsEast1
        };

        // Create bucket instance
        let bucket = Bucket::new(&bucket_name, region, credentials)?;

        Ok(Self {
            bucket,
            key_prefix,
            gzip,
            retry_policy: RetryPolicy {
                max_attempts: max_fetch_attempts,
                base_delay: retry_base_delay,
            },
        })
    }

    fn key_for(&self, lat: f64, lng: f64) -> Result<String, TileError> {
        let file_path = TileSetWithCache::get_file_path(lat, lng)
            .map_err(|e| TileError::decode(format!("{},{}", lat, lng), e))?;
        Ok(if self.key_prefix.is_empty() {
            file_path
        } else {
            format!("{}/{}", self.key_prefix, file_path)
        })
    }

    #[instrument(level="debug", name="get_tile_s3", skip_all, fields(coord = format!("{},{}", lat, lng)))]
    pub async fn get_tile(&self, lat: f64, lng: f64) -> Result<Vec<u8>, TileError> {
        let key = self.key_for(lat, lng)?;

        debug!("Fetching tile from S3: s3://{}/{}", self.bucket.name, key);

        with_retry(self.retry_policy, &key, || self.fetch(&key)).await
    }

    /// Reachability probe for `/status`: a HEAD against a known key.
    ///
    /// This must never be served from a cache. It is the only thing standing
    /// between us and a health check that reports `ok` while object storage is
    /// down. A HEAD costs no meaningful bandwidth (no body, versus ~25 MB for
    /// the tile itself), so it is cheap enough to run on every poll while still
    /// exercising DNS, TLS, credentials, the bucket and the key.
    #[instrument(level="debug", name="probe_s3", skip_all, fields(coord = format!("{},{}", lat, lng)))]
    pub async fn probe(&self, lat: f64, lng: f64) -> Result<(), TileError> {
        let key = self.key_for(lat, lng)?;

        debug!("Probing S3: HEAD s3://{}/{}", self.bucket.name, key);

        with_retry(self.retry_policy, &key, || self.head(&key)).await
    }

    async fn head(&self, key: &str) -> Result<(), TileError> {
        let (_, status) = self
            .bucket
            .head_object(key)
            .await
            .map_err(|e| TileError::transport(key, e))?;

        if !(200..300).contains(&status) {
            return Err(TileError::upstream(key, status, "(HEAD, no body)"));
        }
        Ok(())
    }

    async fn fetch(&self, key: &str) -> Result<Vec<u8>, TileError> {
        let response = self
            .bucket
            .get_object(key)
            .await
            .map_err(|e| TileError::transport(key, e))?;

        // `rust-s3` is built without the `fail-on-err` feature, so `get_object`
        // resolves to `Ok` for any HTTP status and hands back the error body.
        // Without this check a 404 or a transient 5xx reaches the gzip decoder
        // and is misreported as `invalid gzip header`.
        let status = response.status_code();
        if !(200..300).contains(&status) {
            return Err(TileError::upstream(
                key,
                status,
                response.as_str().unwrap_or(""),
            ));
        }

        let bytes = response.bytes().to_vec();

        // Handle gzip decompression if needed
        if self.gzip {
            let mut decoder = GzDecoder::new(&bytes[..]);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| TileError::decode(key, e))?;
            Ok(decompressed)
        } else {
            Ok(bytes)
        }
    }
}
