use crate::tileset::TileSetWithCache;
use flate2::read::GzDecoder;
use log::debug;
use s3::{Bucket, Region, creds::Credentials};
use std::io::Read;

pub struct S3TileSet {
    bucket: Box<Bucket>,
    key_prefix: String,
    gzip: bool,
}

impl S3TileSet {
    pub fn new(
        bucket_name: String,
        key_prefix: String,
        gzip: bool,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        region: Option<String>,
        endpoint: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
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
        })
    }

    pub async fn get_tile(
        &self,
        lat: f64,
        lng: f64,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let file_path = TileSetWithCache::get_file_path(lat, lng)?;
        let key = if self.key_prefix.is_empty() {
            file_path
        } else {
            format!("{}/{}", self.key_prefix, file_path)
        };

        debug!("Fetching tile from S3: s3://{}/{}", self.bucket.name, key);

        let response = self.bucket.get_object(&key).await?;
        let bytes = response.bytes().to_vec();

        // Handle gzip decompression if needed
        if self.gzip {
            let mut decoder = GzDecoder::new(&bytes[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        } else {
            Ok(bytes)
        }
    }
}
