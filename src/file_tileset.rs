use crate::tileset::{TileSet, TileSetOptions};
use flate2::read::GzDecoder;
use std::io::Read;
use std::path::{Path, PathBuf};
use tokio::fs;

pub struct FileTileSet {
    folder: PathBuf,
    tileset: TileSet,
}

impl FileTileSet {
    pub fn new(folder: PathBuf, options: TileSetOptions) -> Self {
        Self {
            folder,
            tileset: TileSet::new(options),
        }
    }

    pub async fn get_tile(
        &self,
        lat: i32,
        lng: i32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let tile_path: String = TileSet::get_file_path(lat, lng);
        let file_path = self.folder.join(tile_path);
        let buffer = fs::read(file_path.as_path()).await?;

        // Handle gzip decompression if needed
        if self.tileset.options.gzip {
            let mut decoder = GzDecoder::new(&buffer[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        } else {
            Ok(buffer)
        }
    }
}
