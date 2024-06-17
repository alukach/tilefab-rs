use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
use crate::tile::Tile;

#[derive(Debug, Deserialize, Serialize)]
pub struct Bounds {
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
}
impl Bounds {
    pub fn from(tile: &Tile) -> Self {
        let n = 2.0_f64.powi(tile.z as i32);

        let x_min = tile.x as f64 / n * 360.0 - 180.0;
        let x_max = (tile.x as f64 + 1.0) / n * 360.0 - 180.0;

        let y_min_rad = (PI * (1.0 - 2.0 * (tile.y as f64 + 1.0) / n)).sinh().atan();
        let y_max_rad = (PI * (1.0 - 2.0 * tile.y as f64 / n)).sinh().atan();

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
