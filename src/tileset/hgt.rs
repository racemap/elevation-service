use std::io::{Error, ErrorKind};
use tracing::{debug, instrument};

/// Sentinel written into HGT tiles for "no data".
const VOID: i16 = -32768;

#[derive(Debug, Clone)]
pub struct HGT {
    buffer: Vec<u8>,
    sw_lat_lng: (f64, f64),
    size: usize,
}

impl HGT {
    #[instrument(level="debug", skip_all, fields(coord = format!("{},{}", sw_lat_lng.0, sw_lat_lng.1)))]
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

    #[instrument(level = "debug", skip_all, fields(coord = format!("{},{}", lat, lng)))]
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

        debug!("row = {}", row);
        debug!("col = {}", col);
        debug!("row_low = {}", row_low);
        debug!("row_high = {}", row_high);
        debug!("row_frac = {}", row_frac);
        debug!("col_low = {}", col_low);
        debug!("col_high = {}", col_high);
        debug!("col_frac = {}", col_frac);

        debug!("value_low_low = {:?}", value_low_low);
        debug!("value_high_low = {:?}", value_high_low);
        debug!("value_high_high = {:?}", value_high_high);
        debug!("value_low_high = {:?}", value_low_high);

        let samples = [
            (value_low_low, (1.0 - row_frac) * (1.0 - col_frac)),
            (value_low_high, (1.0 - row_frac) * col_frac),
            (value_high_low, row_frac * (1.0 - col_frac)),
            (value_high_high, row_frac * col_frac),
        ];

        let mut weighted_sum = 0.0;
        let mut weight_sum = 0.0;
        for (sample, weight) in samples {
            if let Some(sample) = sample {
                weighted_sum += sample as f64 * weight;
                weight_sum += weight;
            }
        }

        if weight_sum == 0.0 {
            return Err(Error::new(
                ErrorKind::NotFound,
                "No elevation data (void samples) at the requested coordinates.",
            ));
        }

        let value = (weighted_sum / weight_sum) as i16;
        debug!("Final interpolated value: {}", value);
        Ok(value)
    }

    #[instrument(skip_all, fields(row, col), level = "trace")]
    fn get_row_col_value(&self, row: f64, col: f64) -> Result<Option<i16>, Error> {
        let offset = ((self.size - row as usize - 1) * self.size + col as usize) * 2;
        if offset + 1 >= self.buffer.len() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Offset exceeds buffer length.",
            ));
        }

        let elevation = i16::from_be_bytes([self.buffer[offset], self.buffer[offset + 1]]);
        if elevation == VOID {
            return Ok(None);
        }
        Ok(Some(elevation))
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

    // 3 arcsecond tile: 1201 x 1201 samples.
    const TEST_TILE_SIZE: usize = 1201;
    const TEST_TILE_BYTES: usize = 2884802;

    fn set_sample(buffer: &mut [u8], row: usize, col: usize, value: i16) {
        let offset = ((TEST_TILE_SIZE - row - 1) * TEST_TILE_SIZE + col) * 2;
        buffer[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }

    #[test]
    fn test_void_corner_is_excluded_from_interpolation() {
        let mut buffer = vec![0; TEST_TILE_BYTES];
        set_sample(&mut buffer, 600, 600, 100);
        set_sample(&mut buffer, 600, 601, 200);
        set_sample(&mut buffer, 601, 600, 300);
        set_sample(&mut buffer, 601, 601, VOID);

        let hgt = HGT::new(buffer, (0.0, 0.0)).unwrap();
        // Centre of the cell, so the three valid corners weigh the same.
        let lat = 600.5 / (TEST_TILE_SIZE - 1) as f64;
        let elevation = hgt.get_elevation(lat, lat).unwrap();
        assert!(
            (elevation as f64 - 200.0).abs() <= 1.0,
            "expected the mean of the three valid corners (200), got {}",
            elevation
        );
    }

    #[test]
    fn test_all_void_cell_reports_no_data() {
        let mut buffer = vec![0; TEST_TILE_BYTES];
        for row in 600..=601 {
            for col in 600..=601 {
                set_sample(&mut buffer, row, col, VOID);
            }
        }

        let hgt = HGT::new(buffer, (0.0, 0.0)).unwrap();
        let lat = 600.5 / (TEST_TILE_SIZE - 1) as f64;
        let error = hgt.get_elevation(lat, lat).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
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
