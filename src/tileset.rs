use crate::tileset::file_tileset::FileTileSet;
use crate::tileset::hgt::HGT;
use crate::tileset::http_tileset::HTTPTileSet;
use crate::tileset::s3_tileset::S3TileSet;
use moka::future::Cache;

mod file_tileset;
mod hgt;
mod http_tileset;
mod s3_tileset;

#[derive(Debug, Clone)]
pub struct TileSetOptions {
    pub path: String,
    pub cache_size: u64,
    pub gzip: bool,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
}

impl Default for TileSetOptions {
    fn default() -> Self {
        Self {
            path: String::new(),
            cache_size: 128,
            gzip: true,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_region: None,
            s3_endpoint: None,
        }
    }
}

pub enum TileSet {
    File(FileTileSet),
    HTTP(HTTPTileSet),
    S3(S3TileSet),
}

impl TileSet {
    pub fn new(options: TileSetOptions) -> Result<Self, Box<dyn std::error::Error>> {
        if options.path.starts_with("s3://") {
            // Parse S3 path: s3://bucket/key_prefix
            let without_s3 = options.path.strip_prefix("s3://").unwrap_or(&options.path);
            let parts: Vec<&str> = without_s3.split('/').collect();
            if parts.is_empty() {
                return Err("Invalid S3 path: bucket name is required".into());
            }

            let bucket = parts[0].to_string();
            let key_prefix = if parts.len() > 1 {
                parts[1..].join("/")
            } else {
                String::new()
            };

            Ok(TileSet::S3(S3TileSet::new(
                bucket,
                key_prefix,
                options.clone(),
                options.s3_access_key_id,
                options.s3_secret_access_key,
                options.s3_region,
                options.s3_endpoint,
            )?))
        } else if options.path.starts_with("http://") || options.path.starts_with("https://") {
            Ok(TileSet::HTTP(HTTPTileSet::new(
                options.path.clone(),
                options,
            )))
        } else {
            Ok(TileSet::File(FileTileSet::new(
                options.path.clone(),
                options,
            )))
        }
    }
}

pub struct TileSetWithCache {
    tileset: TileSet,
    cache: Cache<(i32, i32), Vec<u8>>,
}

impl TileSetWithCache {
    pub fn new(options: TileSetOptions) -> Result<Self, Box<dyn std::error::Error>> {
        let tileset = TileSet::new(options.clone())?;
        let cache = Cache::new(options.cache_size);
        Ok(Self { tileset, cache })
    }

    pub fn get_file_path(lat: f64, lng: f64) -> Result<String, std::io::Error> {
        let lat_prefix = if lat < 0.0 { "S" } else { "N" };
        let lng_prefix = if lng < 0.0 { "W" } else { "E" };
        let lat_file_name = format!("{}{:02}", lat_prefix, lat.abs() as i32);
        let lng_file_name = format!("{}{:03}", lng_prefix, lng.abs() as i32);
        Ok(format!(
            "{}/{}{}.hgt.gz",
            lat_file_name, lat_file_name, lng_file_name
        ))
    }

    pub async fn get_elevation(&self, lat: f64, lng: f64) -> Result<i16, tokio::io::Error> {
        TileSetWithCache::validate_coordinates(lat, lng)?;

        // Simulate fetching the tile (this would be implemented in FileTileSet or S3TileSet)
        let lat_floor = lat.floor();
        let lng_floor = lng.floor();
        let cache_key = (lat_floor as i32, lng_floor as i32);

        let tile_data = self
            .cache
            .try_get_with(cache_key, self.get_tile_data(lat_floor, lng_floor))
            .await
            .map_err(|e| tokio::io::Error::new(tokio::io::ErrorKind::Other, e))?;

        let hgt = HGT::new(tile_data, (lat_floor, lng_floor))?;
        hgt.get_elevation(lat, lng).map_err(|e| e.into())
    }

    fn validate_coordinates(lat: f64, lng: f64) -> Result<(), std::io::Error> {
        if lat < -90.0 || lat > 90.0 || lng < -180.0 || lng > 180.0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Latitude must be between -90 and 90, and longitude must be between -180 and 180.",
            ));
        }
        Ok(())
    }

    async fn get_tile_data(
        &self,
        lat_floor: f64,
        lng_floor: f64,
    ) -> Result<Vec<u8>, tokio::io::Error> {
        let tileset = match &self.tileset {
            TileSet::File(file_tileset) => file_tileset.get_tile(lat_floor, lng_floor).await,
            TileSet::HTTP(s3_tileset) => s3_tileset.get_tile(lat_floor, lng_floor).await,
            TileSet::S3(s3_tileset) => s3_tileset.get_tile(lat_floor, lng_floor).await,
        };

        match tileset {
            Ok(data) => Ok(data),
            Err(e) => Err(tokio::io::Error::new(
                tokio::io::ErrorKind::NotFound,
                format!(
                    "Tile not found for coordinates ({}, {}): {}",
                    lat_floor, lng_floor, e
                ),
            )),
        }
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
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_region: None,
            s3_endpoint: None,
        };
        let tileset = TileSetWithCache::new(options).unwrap();
        let elevation = tileset.get_elevation(45.123, 9.456).await;
        assert!(elevation.is_ok());
        let elevation_value = elevation.unwrap();
        assert_eq!(elevation_value, 48);
    }

    #[tokio::test]
    async fn test_get_elevation_invalid_coordinates() {
        let options = TileSetOptions {
            path: String::from("test_files"),
            cache_size: 128,
            gzip: true,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_region: None,
            s3_endpoint: None,
        };
        let tileset = TileSetWithCache::new(options).unwrap();
        let elevation = tileset.get_elevation(100.0, 200.0).await;
        assert!(elevation.is_err());
        let error = elevation.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
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
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_region: None,
            s3_endpoint: None,
        };

        assert_eq!(options.path, "custom_path");
        assert_eq!(options.cache_size, 256);
        assert!(!options.gzip);
    }
}
