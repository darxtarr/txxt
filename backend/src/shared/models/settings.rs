use std::{error::Error, fs};
use serde::Deserialize;

const SETTINGS_FILENAME: &str = "settings.json";

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub tcp_socket_binding: String,
    pub tcp_socket_port: u16
}

impl Settings {
    pub fn load() -> Result<Settings, Box<dyn Error>> {
        let content = fs::read_to_string(SETTINGS_FILENAME).expect(format!("Cannot read settings file {}", SETTINGS_FILENAME).as_str());
        let settings = serde_json::from_str(&content).expect(format!("Cannot parse JSON content from file {}", SETTINGS_FILENAME).as_str());
        Ok(settings)
    }
}