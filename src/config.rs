use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub music_dir: PathBuf,
}

pub fn config_path() -> PathBuf {
    std::env::current_dir()
        .expect("Cannot get current directory")
        .join("config.toml")
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
