use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub lens_diameter: i32,
    pub zoom: f64,
    pub ring_width: i32,
    pub ring_color: u32,
    pub follow_smoothing: f64,
    pub zoom_step: f64,
    pub min_zoom: f64,
    pub max_zoom: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lens_diameter: 400,
            zoom: 2.5,
            ring_width: 5,
            ring_color: 0x00FFD400,
            follow_smoothing: 0.03,
            zoom_step: 0.25,
            min_zoom: 1.25,
            max_zoom: 6.0,
        }
    }
}

pub fn dir() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("CircleYe")
}

pub fn path() -> PathBuf {
    dir().join("settings.json")
}

impl Config {
    pub fn load() -> Self {
        fs::read_to_string(path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = fs::create_dir_all(dir());
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path(), s);
        }
    }
}
