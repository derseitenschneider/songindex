use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub music_dir: PathBuf,
}

pub fn data_dir() -> PathBuf {
    let dir = dirs::data_dir()
        .expect("Cannot determine data directory")
        .join("songindex");
    std::fs::create_dir_all(&dir).expect("Cannot create data directory");
    dir
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.toml")
}

pub fn load_config() -> Option<Config> {
    let path = config_path();
    let content = std::fs::read_to_string(&path).ok()?;
    toml::from_str(&content).ok()
}

pub fn save_config(config: &Config) {
    let path = config_path();
    let content = toml::to_string_pretty(config).expect("Failed to serialize config");
    std::fs::write(&path, content).expect("Failed to write config");
}
