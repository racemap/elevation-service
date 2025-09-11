use crate::tileset::{TileSetOptions, TileSetWithCache};
use flate2::read::GzDecoder;
use std::io::Read;
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, instrument};

pub struct FileTileSet {
    folder: PathBuf,
    options: TileSetOptions,
}

impl FileTileSet {
    pub fn new(folder: String, options: TileSetOptions) -> Self {
        Self {
            folder: PathBuf::from(folder),
            options,
        }
    }

    #[instrument(name = "get_tile_file", skip_all, fields(coord = format!("{},{}", lat, lng)))]
    pub async fn get_tile(
        &self,
        lat: f64,
        lng: f64,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let tile_path: String =
            TileSetWithCache::get_file_path(lat, lng).expect("Failed to get tile path");
        let file_path = self.folder.join(tile_path);
        debug!("Fetching tile from: {:?}", file_path);

        let buffer = fs::read(file_path.as_path()).await?;

        // Handle gzip decompression if needed
        if self.options.gzip {
            let mut decoder = GzDecoder::new(&buffer[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        } else {
            Ok(buffer)
        }
    }
}
