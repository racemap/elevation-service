use crate::tileset::{
    hgt::HGT,
    tileset::{TileSetOptions, TileSetWithCache},
};
use flate2::read::GzDecoder;
use reqwest::Client;
use std::io::Read;

pub struct S3TileSet {
    base_url: String,
    options: TileSetOptions,
}

impl S3TileSet {
    pub fn new(base_url: String, options: TileSetOptions) -> Self {
        Self { base_url, options }
    }

    pub async fn get_tile(
        &self,
        lat: i32,
        lng: i32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let file_path = format!(
            "{}/{}",
            self.base_url,
            TileSetWithCache::get_file_path(lat, lng)
        );
        let response = Client::new().get(&file_path).send().await?.bytes().await?;
        // Handle gzip decompression if needed
        if self.options.gzip {
            let mut decoder = GzDecoder::new(&response[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        } else {
            Ok(response.to_vec())
        }
    }

    pub async fn get_elevation(
        &self,
        lat_lng: (f64, f64),
    ) -> Result<i16, Box<dyn std::error::Error>> {
        let lat = lat_lng.0.floor() as i32;
        let lng = lat_lng.1.floor() as i32;

        let file_path = format!(
            "{}/{}",
            self.base_url,
            TileSetWithCache::get_file_path(lat, lng)
        );
        let response = Client::new().get(&file_path).send().await?.bytes().await?;
        let buffer = if self.options.gzip {
            let mut decoder = GzDecoder::new(&response[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            decompressed
        } else {
            response.to_vec()
        };

        let hgt = HGT::new(buffer, (lat as f64, lng as f64))?;
        hgt.get_elevation(lat_lng).map_err(|e| e.into())
    }
}
