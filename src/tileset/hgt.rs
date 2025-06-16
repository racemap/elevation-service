use std::io::{Error, ErrorKind};

pub struct HGT {
    buffer: Vec<u8>,
    sw_lat_lng: (f64, f64),
    size: usize,
    resolution: usize,
}

impl HGT {
    pub fn new(buffer: Vec<u8>, sw_lat_lng: (f64, f64)) -> Result<Self, Error> {
        let size;
        let resolution;

        match buffer.len() {
            25934402 => {
                resolution = 1;
                size = 3601;
            }
            2884802 => {
                resolution = 3;
                size = 1201;
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Unknown tile format (1 arcsecond and 3 arcsecond supported).",
                ));
            }
        }

        Ok(Self {
            buffer,
            sw_lat_lng,
            size,
            resolution,
        })
    }

    pub fn get_elevation(&self, lat: f64, lng: f64) -> Result<i16, Error> {
        let size = self.size - 1;
        let row = (lat - self.sw_lat_lng.0) * size as f64;
        let col = (lng - self.sw_lat_lng.1) * size as f64;

        if row < 0.0 || col < 0.0 || row > size as f64 || col > size as f64 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "Latitude/longitude is outside tile bounds (row={}, col={}; size={})",
                    row, col, size
                ),
            ));
        }

        let row_low = row.floor() as usize;
        let col_low = col.floor() as usize;
        let offset = ((self.size - row_low - 1) * self.size + col_low) * 2;

        let elevation = i16::from_be_bytes([self.buffer[offset], self.buffer[offset + 1]]);

        Ok(elevation)
    }
}
