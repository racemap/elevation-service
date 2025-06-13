use crate::{file_tileset::FileTileSet, tileset::TileSetOptions};

mod config;
mod file_tileset;
mod hgt;
mod s3_tileset;
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

    let options = TileSetOptions::default();
    let file_tileset = FileTileSet::new(config.tile_folder, options);

    let tile_data = file_tileset.get_tile(45, 9).await?;
    println!("Tile data size: {} bytes", tile_data.len());

    Ok(())
}
