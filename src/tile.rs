use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}
impl Tile {
    pub fn new(z: &String, x: &String, y: &String) -> Result<Self, String> {
        // Parse path & validate type
        let z = z.parse().map_err(|_| "Invalid z value".to_string())?;
        let x = x.parse().map_err(|_| "Invalid x value".to_string())?;
        let y = y.parse().map_err(|_| "Invalid y value".to_string())?;

        // Check if X and Y are within the valid range
        let max_index = (1 << z) as u32 - 1;
        if !(x <= max_index && y <= max_index) {
            return Err("X and Y must be within the valid range".to_string());
        }

        Ok(Self { x, y, z })
    }
}
