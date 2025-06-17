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

        println!(
            "HGT tile created with size: {}, resolution: {}",
            size, resolution
        );

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

        HGT::interpolation(self, row, col)
    }

    fn interpolation(&self, row: f64, col: f64) -> Result<i16, Error> {
        let row_low = row.floor();
        let row_high = row_low + 1.0;
        let row_frac = row - row_low;

        let col_low = col.floor();
        let col_high = col_low + 1.0;
        let col_frac = col - col_low;

        let value_low_low = HGT::get_row_col_value(self, row_low, col_low)?;
        let value_low_high = HGT::get_row_col_value(self, row_low, col_high)?;
        let value_high_low = HGT::get_row_col_value(self, row_high, col_low)?;
        let value_high_high = HGT::get_row_col_value(self, row_high, col_high)?;
        let value_low =
            (value_low_low as f64 * (1.0 - col_frac) + value_low_high as f64 * col_frac) as i16;
        let value_high =
            (value_high_low as f64 * (1.0 - col_frac) + value_high_high as f64 * col_frac) as i16;

        println!("row = {}", row);
        println!("col = {}", col);
        println!("row_low = {}", row_low);
        println!("row_high = {}", row_high);
        println!("row_frac = {}", row_frac);
        println!("col_low = {}", col_low);
        println!("col_high = {}", col_high);
        println!("col_frac = {}", col_frac);

        println!("value_low_low = {}", value_low_low);
        println!("value_high_low = {}", value_high_low);
        println!("value_high_high = {}", value_high_high);
        println!("value_low_high = {}", value_low_high);

        println!("value_low = {}", value_low);
        println!("value_high = {}", value_high);

        let value = (value_low as f64 * (1.0 - row_frac) + value_high as f64 * row_frac) as i16;
        println!("Final interpolated value: {}", value);
        Ok(value)
    }

    fn get_row_col_value(&self, row: f64, col: f64) -> Result<i16, Error> {
        let offset = ((self.size - row as usize - 1) * self.size + col as usize) * 2;
        if offset + 1 >= self.buffer.len() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Offset exceeds buffer length.",
            ));
        }

        let elevation = i16::from_be_bytes([self.buffer[offset], self.buffer[offset + 1]]);
        Ok(elevation)
    }
}
