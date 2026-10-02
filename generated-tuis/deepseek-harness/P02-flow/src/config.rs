//! Tool-level configuration (default board root, etc.).
//!
//! Configuration is stored as TOML. The default location is
//! `$XDG_CONFIG_HOME/toolb/config.toml` (falling back to
//! `~/.config/toolb/config.toml`). `TOOLB_CONFIG` or `--config` override the
//! path. `--board` overrides the board root for a single run.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub board_root: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            board_root: PathBuf::from("/bench/data/board"),
        }
    }
}

pub fn default_config_path() -> PathBuf {
    if let Ok(p) = std::env::var("TOOLB_CONFIG") {
        return PathBuf::from(p);
    }
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(x).join("toolb").join("config.toml");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".config").join("toolb").join("config.toml");
    }
    PathBuf::from("toolb.toml")
}

pub fn load_config(path: Option<&Path>) -> Config {
    let p = path.map(|x| x.to_path_buf()).unwrap_or_else(default_config_path);
    if let Ok(s) = fs::read_to_string(&p) {
        if let Ok(c) = toml::from_str::<Config>(&s) {
            return c;
        }
    }
    Config::default()
}

pub fn save_config(path: Option<&Path>, cfg: &Config) -> Result<(), String> {
    let p = path.map(|x| x.to_path_buf()).unwrap_or_else(default_config_path);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&p, s).map_err(|e| e.to_string())
}
