//! Named server list, persisted to `~/.config/toolj/servers.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_URI: &str = "redis://localhost:6379/0";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Server {
    pub name: String,
    pub uri: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub servers: Vec<Server>,
}

impl Config {
    pub fn path() -> PathBuf {
        if let Ok(p) = std::env::var("TOOLJ_CONFIG") {
            return PathBuf::from(p);
        }
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("/root/.config"));
        base.join("toolj").join("servers.toml")
    }

    /// Load the saved servers, always guaranteeing a `local` default entry.
    pub fn load() -> Config {
        let mut cfg = Self::read(&Self::path()).unwrap_or_default();
        if !cfg.servers.iter().any(|s| s.uri == DEFAULT_URI) {
            cfg.servers.insert(
                0,
                Server {
                    name: "local".into(),
                    uri: DEFAULT_URI.into(),
                },
            );
        }
        cfg
    }

    fn read(path: &Path) -> Result<Config> {
        let text = fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
        Ok(())
    }

    /// Insert or replace a server by name; returns true when it was new.
    pub fn upsert(&mut self, name: &str, uri: &str) -> bool {
        if let Some(existing) = self.servers.iter_mut().find(|s| s.name == name) {
            existing.uri = uri.to_string();
            false
        } else {
            self.servers.push(Server {
                name: name.to_string(),
                uri: uri.to_string(),
            });
            true
        }
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.servers.len();
        self.servers.retain(|s| s.name != name);
        self.servers.len() != before
    }
}

/// Accept `redis://host:port/db`, `host:port`, `host`, or `:port` and normalise
/// it into a URI the client understands.
pub fn normalize_uri(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return DEFAULT_URI.to_string();
    }
    if s.contains("://") {
        return s.to_string();
    }
    if let Some(port) = s.strip_prefix(':') {
        return format!("redis://localhost:{port}");
    }
    format!("redis://{s}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(normalize_uri(""), DEFAULT_URI);
        assert_eq!(normalize_uri("localhost:6380"), "redis://localhost:6380");
        assert_eq!(normalize_uri(":6380"), "redis://localhost:6380");
        assert_eq!(normalize_uri("redis://h/2"), "redis://h/2");
    }

    #[test]
    fn upsert_replaces() {
        let mut c = Config::default();
        assert!(c.upsert("a", "redis://x"));
        assert!(!c.upsert("a", "redis://y"));
        assert_eq!(c.servers.len(), 1);
        assert_eq!(c.servers[0].uri, "redis://y");
        assert!(c.remove("a"));
    }
}
