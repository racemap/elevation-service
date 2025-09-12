// src/config.rs

use byte_unit::Byte;
use dotenvy::dotenv;
use once_cell::sync::Lazy;
use std::{env, net::Ipv4Addr};

// Define the Config struct
#[derive(Clone, Debug)]
pub struct Config {
    pub cache_size: u64,
    pub tile_set_path: String,
    pub max_post_size: Byte,
    pub max_parallel_processing: usize,
    pub max_tokio_threads: Option<usize>,
    pub max_concurrent_handlers: usize,
    pub port: u16,
    pub bind: Ipv4Addr,
    pub s3_endpoint: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub s3_region: Option<String>,
}

// Initialize dotenv and config only once
pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    dotenv().ok(); // Loads .env (only the first time it's called)

    Config {
        cache_size: parse_env_var::<u64>("TILE_SET_CACHE").unwrap_or(128),
        tile_set_path: get_non_empty_env_var("TILE_SET_PATH").unwrap_or_else(|| {
            String::from(
                env::current_dir()
                    .expect("Failed to get current_dir")
                    .to_str()
                    .expect("Failed to convert path to string"),
            )
        }),
        max_post_size: get_non_empty_env_var("MAX_POST_SIZE")
            .and_then(|s| Byte::parse_str(s, true).ok())
            .unwrap_or_else(|| Byte::parse_str("500kb", true).unwrap()),
        max_parallel_processing: parse_env_var::<usize>("MAX_PARALLEL_PROCESSING").unwrap_or(500),
        max_tokio_threads: parse_env_var::<usize>("MAX_THREADS"),
        max_concurrent_handlers: parse_env_var::<usize>("MAX_CONCURRENT_HANDLERS").unwrap_or(1000),
        port: parse_env_var::<u16>("PORT").unwrap_or(3000),
        bind: parse_env_var::<Ipv4Addr>("BIND").unwrap_or(Ipv4Addr::new(0, 0, 0, 0)),
        s3_endpoint: get_non_empty_env_var("S3_ENDPOINT"),
        s3_bucket: get_non_empty_env_var("S3_BUCKET"),
        s3_access_key_id: get_non_empty_env_var("S3_ACCESS_KEY_ID")
            .or_else(|| get_non_empty_env_var("AWS_ACCESS_KEY_ID")),
        s3_secret_access_key: get_non_empty_env_var("S3_SECRET_ACCESS_KEY")
            .or_else(|| get_non_empty_env_var("AWS_SECRET_ACCESS_KEY")),
        s3_region: get_non_empty_env_var("S3_REGION")
            .or_else(|| get_non_empty_env_var("AWS_REGION")),
    }
});

pub fn get_uri_from_config(config: Config) -> String {
    let tile_folder = config.tile_set_path;
    let s3_endpoint = config.s3_endpoint.as_ref().filter(|s| !s.trim().is_empty());
    let s3_bucket = config.s3_bucket.as_ref().filter(|s| !s.trim().is_empty());

    // If S3 credentials are provided, prefer S3 direct access over HTTP
    if config.s3_access_key_id.is_some() && config.s3_secret_access_key.is_some() {
        if let (Some(_endpoint), Some(bucket)) = (s3_endpoint, s3_bucket) {
            // For explicit S3 endpoint and bucket, construct s3:// URL for direct S3 access
            let path = tile_folder.trim_start_matches('/');
            return format!("s3://{}/{}", bucket, path);
        } else if tile_folder.starts_with("s3://") {
            // Already an S3 URL, return as-is for direct S3 access
            return tile_folder;
        } else if tile_folder.starts_with("https://") && tile_folder.contains(".s3.") {
            // Convert HTTPS S3 URL back to s3:// for authenticated access
            // https://bucket.s3.amazonaws.com/path -> s3://bucket/path
            if let Some(url) = tile_folder.strip_prefix("https://") {
                if let Some(dot_pos) = url.find(".s3.") {
                    let bucket = &url[..dot_pos];
                    if let Some(slash_pos) = url.find('/') {
                        let path = &url[slash_pos + 1..];
                        return format!("s3://{}/{}", bucket, path);
                    } else {
                        return format!("s3://{}", bucket);
                    }
                }
            }
        }
    }

    // Fallback to HTTP access for public buckets
    if let (Some(endpoint), Some(bucket)) = (s3_endpoint, s3_bucket) {
        format!("https://{}.{}", bucket, endpoint)
    } else if tile_folder.starts_with("s3://") {
        let without_s3 = tile_folder.strip_prefix("s3://").unwrap_or(&tile_folder);
        let parts: Vec<&str> = without_s3.split('/').collect();
        if parts.len() < 2 {
            panic!("Invalid S3 path: {}", tile_folder);
        }

        let bucket = parts[0];
        let key = parts[1..].join("/");
        format!("https://{}.s3.amazonaws.com/{}", bucket, key)
    } else {
        tile_folder
    }
}

/// Helper function to get environment variable, treating empty strings as None
/// This handles Docker Compose behavior where unset variables become empty strings
fn get_non_empty_env_var(key: &str) -> Option<String> {
    env::var(key).ok().and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Helper function to parse environment variable as a specific type, treating empty strings as None
fn parse_env_var<T: std::str::FromStr>(key: &str) -> Option<T> {
    get_non_empty_env_var(key).and_then(|s| s.parse().ok())
}
