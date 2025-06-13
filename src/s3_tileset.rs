use crate::tileset::{TileSet, TileSetOptions};
use flate2::read::GzDecoder;
use reqwest::Client;
use std::io::Read;

pub struct S3TileSet {
    base_url: String,
    tileset: TileSet,
}

impl S3TileSet {
    pub fn new(base_url: String, options: TileSetOptions) -> Self {
        Self {
            base_url,
            tileset: TileSet::new(options),
        }
    }

    pub async fn get_tile(
        &self,
        lat: i32,
        lng: i32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let file_path = format!("{}/{}", self.base_url, TileSet::get_file_path(lat, lng));
        let response = Client::new().get(&file_path).send().await?.bytes().await?;
        // Handle gzip decompression if needed
        if self.tileset.options.gzip {
            let mut decoder = GzDecoder::new(&response[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        } else {
            Ok(response.to_vec())
        }
    }
}
