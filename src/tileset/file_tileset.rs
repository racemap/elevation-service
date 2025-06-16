use crate::tileset::hgt::HGT;
use crate::tileset::tileset::{TileSetOptions, TileSetWithCache};
use flate2::read::GzDecoder;
use std::io::{Error, Read};
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
        lat: i32,
        lng: i32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let tile_path: String = TileSetWithCache::get_file_path(lat, lng);
        let file_path = self.folder.join(tile_path);
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

    pub async fn get_elevation(&self, lat_lng: (f64, f64)) -> Result<i16, Error> {
        let lat = lat_lng.0.floor() as i32;
        let lng = lat_lng.1.floor() as i32;

        // Fetch the tile data
        let buffer = self
            .get_tile(lat, lng)
            .await
            .expect("Failed to get tile data");

        // Create HGT instance and get elevation
        let hgt = HGT::new(buffer, (lat as f64, lng as f64))?;
        hgt.get_elevation(lat_lng)
    }
}
