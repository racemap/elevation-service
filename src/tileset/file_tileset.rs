use crate::tileset::{TileSetOptions, TileSetWithCache};
use flate2::read::GzDecoder;
use log::debug;
use std::io::Read;
use std::path::PathBuf;
use tokio::fs;

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

    pub async fn get_tile(
        &self,
        lat: f64,
        lng: f64,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let tile_path: String = TileSetWithCache::get_file_path(lat, lng);
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
