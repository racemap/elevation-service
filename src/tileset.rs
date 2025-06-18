use crate::tileset::file_tileset::FileTileSet;
use crate::tileset::hgt::HGT;
use crate::tileset::http_tileset::HTTPTileSet;
use std::collections::{HashMap, VecDeque};
use tokio::sync::{Mutex, oneshot};

mod file_tileset;
mod hgt;
mod http_tileset;

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
    HTTP(HTTPTileSet),
}

impl TileSet {
    pub fn new(options: TileSetOptions) -> Result<Self, Box<dyn std::error::Error>> {
        if options.path.starts_with("http://") || options.path.starts_with("https://") {
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

pub struct TileSetCache {
    cache: Mutex<HashMap<(i32, i32), Vec<u8>>>,
    order: Mutex<VecDeque<(i32, i32)>>, // Tracks the order of keys for LRU eviction
    max_size: usize,                    // Maximum number of tiles in the cache
    pending_fetches: Mutex<HashMap<(i32, i32), Vec<oneshot::Sender<Vec<u8>>>>>, // Track in-progress fetches
}

impl TileSetCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            order: Mutex::new(VecDeque::new()),
            max_size,
            pending_fetches: Mutex::new(HashMap::new()),
        }
    }

    // Try to lock a key for fetching, returns true if lock acquired, false if already locked
    pub async fn lock_key(&self, key: &(i32, i32)) -> Option<oneshot::Receiver<Vec<u8>>> {
        let mut pending = self.pending_fetches.lock().await;

        // If this key is already being fetched, register for notification when it completes
        if pending.contains_key(key) {
            let (sender, receiver) = oneshot::channel();
            pending.get_mut(key).unwrap().push(sender);
            return Some(receiver);
        }

        // No one is fetching this key yet, so we'll create a new entry with empty waiters
        pending.insert(key.clone(), Vec::new());
        None
    }

    // Unlock a key and notify all waiters with the result
    pub async fn unlock_key(&self, key: &(i32, i32), value: Vec<u8>) {
        let mut pending = self.pending_fetches.lock().await;

        if let Some(waiters) = pending.remove(key) {
            // Notify all waiters with the result
            for sender in waiters {
                let _ = sender.send(value.clone());
            }
        }
    }

    pub async fn get(&self, key: &(i32, i32)) -> Option<Vec<u8>> {
        // First check if the value is in the cache
        let cache = self.cache.lock().await;
        if let Some(value) = cache.get(key).cloned() {
            let mut order = self.order.lock().await;
            if let Some(pos) = order.iter().position(|&k| k == *key) {
                // Move the accessed key to the back (most recently used)
                let key = order.remove(pos).unwrap();
                order.push_back(key);
            }
            return Some(value);
        }
        drop(cache); // Release the cache lock

        // If the key is being fetched by another task, wait for it to complete
        if let Some(receiver) = self.lock_key(key).await {
            // Another task is already fetching this key, wait for the result
            match receiver.await {
                Ok(value) => return Some(value),
                Err(_) => return None, // The fetching task failed
            }
        }

        // We've acquired the lock but the data isn't in the cache yet
        None
    }

    pub async fn insert(&self, key: (i32, i32), value: Vec<u8>) {
        let mut cache = self.cache.lock().await;
        let mut order = self.order.lock().await;

        if cache.len() >= self.max_size {
            // Evict the least recently used item
            if let Some(lru_key) = order.pop_front() {
                cache.remove(&lru_key);
            }
        }

        // Insert the new item and mark it as most recently used
        cache.insert(key, value.clone());
        order.push_back(key);

        // Notify any waiters and release the lock
        self.unlock_key(&key, value).await;
    }
}

pub struct TileSetWithCache {
    tileset: TileSet,
    cache: TileSetCache,
}

impl TileSetWithCache {
    pub fn new(options: TileSetOptions) -> Result<Self, Box<dyn std::error::Error>> {
        let tileset = TileSet::new(options.clone())?;
        let cache = TileSetCache::new(options.cache_size as usize); // Use cache_size as limit
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

        let tile_data = if let Some(data) = self.cache.get(&cache_key).await {
            data.clone()
        } else {
            // Fetch the tile data (this would be async in a real implementation)
            let tile_data = match &self.tileset {
                TileSet::File(file_tileset) => file_tileset.get_tile(lat_floor, lng_floor).await?,
                TileSet::HTTP(s3_tileset) => s3_tileset.get_tile(lat_floor, lng_floor).await?,
            };
            self.cache.insert(cache_key, tile_data.clone()).await;
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
