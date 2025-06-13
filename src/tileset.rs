use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct TileSetOptions {
    pub cache_size: usize,
    pub gzip: bool,
}

impl Default for TileSetOptions {
    fn default() -> Self {
        Self {
            cache_size: 128,
            gzip: true,
        }
    }
}

pub struct TileSet {
    pub options: TileSetOptions,
    cache: Mutex<HashMap<(i32, i32), Vec<u8>>>, // Simulating a cache
}

impl TileSet {
    pub fn new(options: TileSetOptions) -> Self {
        Self {
            options,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_file_path(lat: i32, lng: i32) -> String {
        let lat_prefix = if lat < 0 { "S" } else { "N" };
        let lng_prefix = if lng < 0 { "W" } else { "E" };
        let lat_file_name = format!("{}{:02}", lat_prefix, lat.abs());
        let lng_file_name = format!("{}{:03}", lng_prefix, lng.abs());
        format!(
            "{}/{}{}.hgt.gz",
            lat_file_name, lat_file_name, lng_file_name
        )
    }
}
