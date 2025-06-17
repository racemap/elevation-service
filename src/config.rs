// src/config.rs

use byte_unit::Byte;
use dotenvy::dotenv;
use once_cell::sync::Lazy;
use std::env;

// Define the Config struct
#[derive(Clone, Debug)]
pub struct Config {
    pub cache_size: u64,
    pub tile_folder: String,
    pub max_post_size: Byte,
    pub port: u16,
}

// Initialize dotenv and config only once
pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    dotenv().ok(); // Loads .env (only the first time it's called)

    Config {
        cache_size: env::var("TILE_SET_CACHE")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(128),
        tile_folder: env::var("TILE_SET_PATH").unwrap_or_else(|_| {
            String::from(
                env::current_dir()
                    .expect("Failed to get current_dir")
                    .to_str()
                    .expect("Failed to convert path to string"),
            )
        }),
        max_post_size: env::var("MAX_POST_SIZE")
            .ok()
            .and_then(|s| Byte::parse_str(s, true).ok())
            .unwrap_or_else(|| {
                Byte::parse_str("500kb", true).unwrap() // Default to 10 MB
            }),
        port: env::var("PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(3000),
    }
});
