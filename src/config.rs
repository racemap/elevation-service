// src/config.rs

use dotenvy::dotenv;
use once_cell::sync::Lazy;
use std::{env, path::PathBuf};

// Define the Config struct
#[derive(Clone, Debug)]
pub struct Config {
    pub cache_size: u64,
    pub tile_folder: PathBuf,
    pub max_post_size: String,
    pub max_parallel_processing: u64,
}

// Initialize dotenv and config only once
pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    dotenv().ok(); // Loads .env (only the first time it's called)

    Config {
        cache_size: env::var("TILE_SET_CACHE")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(128),
        tile_folder: env::var("TILE_SET_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| env::current_dir().expect("Failed to get current_dir")),
        max_post_size: env::var("MAX_POST_SIZE").unwrap_or_else(|_| "500kb".to_string()),
        max_parallel_processing: env::var("MAX_PARALLEL_PROCESSING")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(500),
    }
});
