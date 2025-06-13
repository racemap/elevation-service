mod config;
fn main() {
    // Access the configuration values
    let config = config::CONFIG.clone();

    println!("Cache Size: {}", config.cache_size);
    println!("Tile Folder: {:?}", config.tile_folder);
    println!("Max Post Size: {}", config.max_post_size);
    println!(
        "Max Parallel Processing: {}",
        config.max_parallel_processing
    );
}
