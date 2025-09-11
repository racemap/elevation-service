use std::io::{Error, ErrorKind};
use tracing::{debug, instrument};

#[derive(Debug, Clone)]
pub struct HGT {
    buffer: Vec<u8>,
    sw_lat_lng: (f64, f64),
    size: usize,
}

impl HGT {
    #[instrument(skip_all, fields(coord = format!("{},{}", sw_lat_lng.0, sw_lat_lng.1)))]
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

        debug!(
            "HGT tile created with size: {}, resolution: {}",
            size, resolution
        );

        Ok(Self {
            buffer,
            sw_lat_lng,
            size,
        })
    }

    #[instrument(skip_all, fields(coord = format!("{},{}", lat, lng)))]
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

    #[instrument(skip_all, fields(row, col), level = "trace")]
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

        debug!("row = {}", row);
        debug!("col = {}", col);
        debug!("row_low = {}", row_low);
        debug!("row_high = {}", row_high);
        debug!("row_frac = {}", row_frac);
        debug!("col_low = {}", col_low);
        debug!("col_high = {}", col_high);
        debug!("col_frac = {}", col_frac);

        debug!("value_low_low = {}", value_low_low);
        debug!("value_high_low = {}", value_high_low);
        debug!("value_high_high = {}", value_high_high);
        debug!("value_low_high = {}", value_low_high);

        debug!("value_low = {}", value_low);
        debug!("value_high = {}", value_high);

        let value = (value_low as f64 * (1.0 - row_frac) + value_high as f64 * row_frac) as i16;
        debug!("Final interpolated value: {}", value);
        Ok(value)
    }

    #[instrument(skip_all, fields(row, col), level = "trace")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;

    #[test]
    fn test_hgt_creation_valid_buffer() {
        let buffer = vec![0; 25934402]; // Valid buffer size for 1 arcsecond resolution
        let sw_lat_lng = (0.0, 0.0);
        let hgt = HGT::new(buffer, sw_lat_lng);
        assert!(hgt.is_ok());
    }

    #[test]
    fn test_hgt_creation_invalid_buffer() {
        let buffer = vec![0; 100]; // Invalid buffer size
        let sw_lat_lng = (0.0, 0.0);
        let hgt = HGT::new(buffer, sw_lat_lng);
        assert!(hgt.is_err());
        assert_eq!(hgt.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_get_elevation_valid_coordinates() {
        let buffer = vec![0; 25934402]; // Valid buffer size for 1 arcsecond resolution
        let sw_lat_lng = (0.0, 0.0);
        let hgt = HGT::new(buffer, sw_lat_lng).unwrap();
        let elevation = hgt.get_elevation(0.5, 0.5);
        assert!(elevation.is_ok());
        assert_eq!(elevation.unwrap(), 0); // Default buffer values lead to elevation 0
    }

    #[test]
    fn test_get_elevation_out_of_bounds() {
        let buffer = vec![0; 25934402]; // Valid buffer size for 1 arcsecond resolution
        let sw_lat_lng = (0.0, 0.0);
        let hgt = HGT::new(buffer, sw_lat_lng).unwrap();
        let elevation = hgt.get_elevation(-1.0, -1.0);
        assert!(elevation.is_err());
        assert_eq!(elevation.unwrap_err().kind(), ErrorKind::InvalidInput);
    }
}
