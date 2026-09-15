use crate::tileset::error::TileError;
use crate::tileset::{TileSetOptions, TileSetWithCache};
use flate2::read::GzDecoder;
use std::io::{ErrorKind, Read};
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

    #[instrument(level="debug", name = "get_tile_file", skip_all, fields(coord = format!("{},{}", lat, lng)))]
    pub async fn get_tile(&self, lat: f64, lng: f64) -> Result<Vec<u8>, TileError> {
        let tile_path = TileSetWithCache::get_file_path(lat, lng)
            .map_err(|e| TileError::decode(format!("{},{}", lat, lng), e))?;
        let file_path = self.folder.join(tile_path);
        let key = file_path.display().to_string();
        debug!("Fetching tile from: {:?}", file_path);

        let buffer = fs::read(file_path.as_path()).await.map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                TileError::not_found(&key, "no such file")
            } else {
                TileError::transport(&key, e)
            }
        })?;

        // Handle gzip decompression if needed
        if self.options.gzip {
            let mut decoder = GzDecoder::new(&buffer[..]);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| TileError::decode(&key, e))?;
            Ok(decompressed)
        } else {
            Ok(buffer)
        }
    }
}
