use crate::tileset::tileset::{TileSetOptions, TileSetWithCache};

mod config;
mod tileset;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Access the configuration values
    let config = config::CONFIG.clone();

    println!("Cache Size: {}", config.cache_size);
    println!("Tile Folder: {:?}", config.tile_folder);
    println!("Max Post Size: {}", config.max_post_size);
    println!(
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
    println!("Elevation: {} meters", elevation);

    Ok(())
}
