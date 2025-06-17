use crate::tileset::tileset::{TileSetOptions, TileSetWithCache};
use env_logger;
use log::{debug, info};

mod config;
mod tileset;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Access the configuration values
    let config = config::CONFIG.clone();

    // Initialize the logger
    env_logger::init();

    debug!("Cache Size: {}", config.cache_size);
    debug!("Tile Folder: {:?}", config.tile_folder);
    debug!("Max Post Size: {}", config.max_post_size);
    debug!(
        "Max Parallel Processing: {}",
        config.max_parallel_processing
    );

    let options = TileSetOptions {
        path: config.tile_folder.clone(),
        cache_size: config.cache_size,
        gzip: true,
    };
    let tileset = TileSetWithCache::new(options)?;
    let elevation = tileset.get_elevation(45.123, 9.456).await?;
    info!("Elevation: {} meters", elevation);

    Ok(())
}
