use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Debug, Deserialize, Serialize)]
pub struct Bounds {
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
}

#[derive(Debug)]
pub struct Tile {
    x: u32,
    y: u32,
    z: u32,
}
impl Tile {
    pub fn from(z: &String, x: &String, y: &String) -> Result<Self, String> {
        // Parse path & validate type
        let z = z
            .parse()
            .map_err(|_| "Invalid z value".to_string())?;
        let x = x
            .parse()
            .map_err(|_| "Invalid x value".to_string())?;
        let y = y
            .parse()
            .map_err(|_| "Invalid y value".to_string())?;

        // Check if X and Y are within the valid range
        let max_index = (1 << z) as u32 - 1;
        if !(x <= max_index && y <= max_index) {
            return Err("X and Y must be within the valid range".to_string());
        }

        Ok(Self { x, y, z })
    }

    /// Convert a tile number to a latitude/longitude bounding box
    /// Returns (min_lat, min_lng, max_lat, max_lng)
    pub fn as_bounds(&self) -> Bounds {
        let n = 2.0_f64.powi(self.z as i32);

        let x_min = self.x as f64 / n * 360.0 - 180.0;
        let x_max = (self.x as f64 + 1.0) / n * 360.0 - 180.0;

        let y_min_rad = (PI * (1.0 - 2.0 * (self.y as f64 + 1.0) / n)).sinh().atan();
        let y_max_rad = (PI * (1.0 - 2.0 * self.y as f64 / n)).sinh().atan();

        let y_min = y_min_rad * 180.0 / PI;
        let y_max = y_max_rad * 180.0 / PI;

        Bounds {
            y_min,
            x_min,
            y_max,
            x_max,
        }
    }
}
