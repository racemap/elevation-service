use crate::tileset::file_tileset::FileTileSet;
use crate::tileset::hgt::HGT;
use crate::tileset::s3_tileset::S3TileSet;
use std::collections::HashMap;
use std::sync::Mutex;

mod file_tileset;
mod hgt;
mod s3_tileset;

#[derive(Debug, Clone)]
pub struct TileSetOptions {
    pub path: String,
    pub cache_size: u64,
    pub gzip: bool,
}

impl Default for TileSetOptions {
    fn default() -> Self {
        Self {
            path: String::new(),
            cache_size: 128,
            gzip: true,
        }
    }
}

pub enum TileSet {
    File(FileTileSet),
    S3(S3TileSet),
}

impl TileSet {
    pub fn new(options: TileSetOptions) -> Result<Self, Box<dyn std::error::Error>> {
        if options.path.starts_with("s3://") {
            let base_url = options.path.trim_start_matches("s3://").to_string();
            Ok(TileSet::S3(S3TileSet::new(base_url, options)))
        } else {
            Ok(TileSet::File(FileTileSet::new(
                options.path.clone(),
                options,
            )))
        }
    }
}

pub struct TileSetCache {
    cache: Mutex<HashMap<(i32, i32), Vec<u8>>>,
}

impl TileSetCache {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, key: &(i32, i32)) -> Option<Vec<u8>> {
        let cache = self.cache.lock().unwrap();
        cache.get(key).cloned()
    }

    pub fn insert(&self, key: (i32, i32), value: Vec<u8>) {
        let mut cache = self.cache.lock().unwrap();
        cache.insert(key, value);
    }
}

pub struct TileSetWithCache {
    tileset: TileSet,
    cache: TileSetCache,
}

impl TileSetWithCache {
    pub fn new(options: TileSetOptions) -> Result<Self, Box<dyn std::error::Error>> {
        let tileset = TileSet::new(options)?;
        let cache = TileSetCache::new();
        Ok(Self { tileset, cache })
    }

    pub fn get_file_path(lat: f64, lng: f64) -> Result<String, std::io::Error> {
        if lat < -90.0 || lat > 90.0 || lng < -180.0 || lng > 180.0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Latitude must be between -90 and 90, and longitude must be between -180 and 180.",
            )
            .into());
        }

        let lat_prefix = if lat < 0.0 { "S" } else { "N" };
        let lng_prefix = if lng < 0.0 { "W" } else { "E" };
        let lat_file_name = format!("{}{:02}", lat_prefix, lat.abs() as i32);
        let lng_file_name = format!("{}{:03}", lng_prefix, lng.abs() as i32);
        Ok(format!(
            "{}/{}{}.hgt.gz",
            lat_file_name, lat_file_name, lng_file_name
        ))
    }

    pub async fn get_elevation(
        &self,
        lat: f64,
        lng: f64,
    ) -> Result<i16, Box<dyn std::error::Error>> {
        // Simulate fetching the tile (this would be implemented in FileTileSet or S3TileSet)
        let lat_floor = lat.floor();
        let lng_floor = lng.floor();
        let cache_key = (lat_floor as i32, lng_floor as i32);

        let tile_data = if let Some(data) = self.cache.get(&cache_key) {
            data.clone()
        } else {
            // Fetch the tile data (this would be async in a real implementation)
            let tile_data = match &self.tileset {
                TileSet::File(file_tileset) => file_tileset.get_tile(lat_floor, lng_floor).await?,
                TileSet::S3(s3_tileset) => s3_tileset.get_tile(lat_floor, lng_floor).await?,
            };
            self.cache.insert(cache_key, tile_data.clone());
            tile_data
        };

        let hgt = HGT::new(tile_data, (lat_floor, lng_floor))?;
        hgt.get_elevation(lat, lng).map_err(|e| e.into())
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_get_elevation() {
        let options = TileSetOptions {
            path: String::from("test_files"),
            cache_size: 128,
            gzip: true,
        };
        let tileset = TileSetWithCache::new(options).unwrap();
        let elevation = tileset.get_elevation(45.123, 9.456).await;
        assert!(elevation.is_ok());
        let elevation_value = elevation.unwrap();
        assert_eq!(elevation_value, 48);
    }

    #[test]
    fn test_get_file_path() {
        let lat = 45.123;
        let lng = 9.456;
        let expected_path = "N45/N45E009.hgt.gz";
        let file_path = TileSetWithCache::get_file_path(lat, lng);
        assert!(file_path.is_ok());
        assert_eq!(file_path.unwrap(), expected_path);
    }

    #[test]
    fn test_get_file_path_negative() {
        let lat = -45.123;
        let lng = -9.456;
        let expected_path = "S45/S45W009.hgt.gz";
        let file_path = TileSetWithCache::get_file_path(lat, lng);
        assert!(file_path.is_ok());
        assert_eq!(file_path.unwrap(), expected_path);
    }

    #[test]
    fn test_get_file_path_zero() {
        let lat = 0.0;
        let lng = 0.0;
        let expected_path = "N00/N00E000.hgt.gz";
        let file_path = TileSetWithCache::get_file_path(lat, lng);
        assert!(file_path.is_ok());
        assert_eq!(file_path.unwrap(), expected_path);
    }

    #[test]
    fn test_get_file_path_negative_zero() {
        let lat = -0.0;
        let lng = -0.0;
        let expected_path = "N00/N00E000.hgt.gz";
        let file_path = TileSetWithCache::get_file_path(lat, lng);
        assert!(file_path.is_ok());
        assert_eq!(file_path.unwrap(), expected_path);
    }

    #[test]
    fn test_get_file_path_large_coordinates() {
        let lat = 90.0;
        let lng = 180.0;
        let expected_path = "N90/N90E180.hgt.gz";
        let file_path = TileSetWithCache::get_file_path(lat, lng);
        assert!(file_path.is_ok());
        assert_eq!(file_path.unwrap(), expected_path);
    }

    #[test]
    fn test_get_file_out_of_bounds() {
        let lat = 91.0; // Invalid latitude
        let lng = 181.0; // Invalid longitude
        let file_path = TileSetWithCache::get_file_path(lat, lng);
        assert!(file_path.is_err());
        assert_eq!(file_path.unwrap_err().kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn test_tile_set_options_default() {
        let options = TileSetOptions::default();
        assert_eq!(options.path, "");
        assert_eq!(options.cache_size, 128);
        assert!(options.gzip);
    }

    #[test]
    fn test_tile_set_options_custom() {
        let options = TileSetOptions {
            path: String::from("custom_path"),
            cache_size: 256,
            gzip: false,
        };

        assert_eq!(options.path, "custom_path");
        assert_eq!(options.cache_size, 256);
        assert!(!options.gzip);
    }
}
