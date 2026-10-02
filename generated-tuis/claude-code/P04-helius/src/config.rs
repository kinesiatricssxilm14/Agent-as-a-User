//! Where the ledger lives, and how the user can move it.
//!
//! Resolution order (first match wins):
//!   1. `--db <PATH>` on the command line
//!   2. `$TOOLD_DB`
//!   3. `db_path = <PATH>` in `$TOOLD_CONFIG`, else `<config dir>/toold/config.toml`
//!   4. `$TOOLD_DATA_DIR/ledger.db`
//!   5. `$XDG_DATA_HOME/toold/ledger.db`, else `$HOME/.local/share/toold/ledger.db`

use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "toold";
const DB_FILE: &str = "ledger.db";

/// How the active database path was chosen, shown in the help view so the user
/// can tell which override is in effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbSource {
    CliFlag,
    EnvVar,
    ConfigFile,
    DataDirEnv,
    Default,
}

impl DbSource {
    pub fn describe(self) -> &'static str {
        match self {
            DbSource::CliFlag => "--db command-line flag",
            DbSource::EnvVar => "TOOLD_DB environment variable",
            DbSource::ConfigFile => "db_path in config file",
            DbSource::DataDirEnv => "TOOLD_DATA_DIR environment variable",
            DbSource::Default => "default data directory",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub db_path: PathBuf,
    pub db_source: DbSource,
    pub config_path: Option<PathBuf>,
}

/// Outcome of parsing argv: either run the TUI, or print something and exit.
pub enum Startup {
    Run(Config),
    Help,
    Version,
    WhereIs(Config),
    Error(String),
}

/// Parse command-line arguments. Kept deliberately small — the tool is meant to
/// be launched as bare `toold`.
pub fn startup_from_args<I: IntoIterator<Item = String>>(args: I) -> Startup {
    let mut cli_db: Option<PathBuf> = None;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Startup::Help,
            "-V" | "--version" => return Startup::Version,
            "--where" | "--db-path" => {
                return match resolve(cli_db) {
                    Ok(cfg) => Startup::WhereIs(cfg),
                    Err(e) => Startup::Error(e),
                };
            }
            "--db" | "--database" | "-d" => match iter.next() {
                Some(v) if !v.is_empty() => cli_db = Some(PathBuf::from(v)),
                _ => return Startup::Error(format!("{arg} requires a path argument")),
            },
            other => {
                if let Some(rest) = other.strip_prefix("--db=") {
                    if rest.is_empty() {
                        return Startup::Error("--db= requires a path".to_string());
                    }
                    cli_db = Some(PathBuf::from(rest));
                } else {
                    return Startup::Error(format!(
                        "unrecognised argument '{other}' (try `{APP_NAME} --help`)"
                    ));
                }
            }
        }
    }

    match resolve(cli_db) {
        Ok(cfg) => Startup::Run(cfg),
        Err(e) => Startup::Error(e),
    }
}

fn resolve(cli_db: Option<PathBuf>) -> Result<Config, String> {
    let config_path = config_file_path();

    if let Some(path) = cli_db {
        return Ok(Config { db_path: expand(&path), db_source: DbSource::CliFlag, config_path });
    }

    if let Some(v) = non_empty_env("TOOLD_DB") {
        return Ok(Config {
            db_path: expand(Path::new(&v)),
            db_source: DbSource::EnvVar,
            config_path,
        });
    }

    if let Some(path) = config_path.as_deref() {
        if let Some(v) = read_db_path_from_config(path)? {
            return Ok(Config {
                db_path: expand(Path::new(&v)),
                db_source: DbSource::ConfigFile,
                config_path,
            });
        }
    }

    if let Some(dir) = non_empty_env("TOOLD_DATA_DIR") {
        return Ok(Config {
            db_path: expand(Path::new(&dir)).join(DB_FILE),
            db_source: DbSource::DataDirEnv,
            config_path,
        });
    }

    Ok(Config { db_path: default_db_path(), db_source: DbSource::Default, config_path })
}

fn default_db_path() -> PathBuf {
    if let Some(xdg) = non_empty_env("XDG_DATA_HOME") {
        return expand(Path::new(&xdg)).join(APP_NAME).join(DB_FILE);
    }
    match non_empty_env("HOME") {
        Some(home) => PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(APP_NAME)
            .join(DB_FILE),
        // No HOME (an unusual container): fall back to the working directory so
        // the tool still starts rather than failing at the first write.
        None => PathBuf::from(DB_FILE),
    }
}

fn config_file_path() -> Option<PathBuf> {
    if let Some(v) = non_empty_env("TOOLD_CONFIG") {
        return Some(expand(Path::new(&v)));
    }
    if let Some(v) = non_empty_env("XDG_CONFIG_HOME") {
        return Some(expand(Path::new(&v)).join(APP_NAME).join("config.toml"));
    }
    non_empty_env("HOME")
        .map(|h| PathBuf::from(h).join(".config").join(APP_NAME).join("config.toml"))
}

/// Read `db_path` from the config file. Deliberately a minimal key/value scan
/// rather than a TOML dependency: one key is all this file holds.
fn read_db_path_from_config(path: &Path) -> Result<Option<String>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read config {}: {e}", path.display())),
    };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        if !matches!(key.trim(), "db_path" | "db" | "database") {
            continue;
        }
        let value = value.trim().trim_matches(['"', '\'']).trim();
        if !value.is_empty() {
            return Ok(Some(value.to_string()));
        }
    }
    Ok(None)
}

/// Expand a leading `~` so config files and env vars can use it.
fn expand(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s == "~" {
        if let Some(home) = non_empty_env("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = non_empty_env("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

pub fn help_text() -> String {
    format!(
        "\
{APP_NAME} {version} — local-first personal finance ledger TUI

USAGE:
    {APP_NAME} [OPTIONS]

Run with no arguments to open the interactive ledger. Press ? inside the
application for the full list of key bindings.

OPTIONS:
    -d, --db <PATH>    Use a specific SQLite ledger file
        --where        Print the resolved ledger path and exit
    -h, --help         Print this help and exit
    -V, --version      Print the version and exit

LEDGER LOCATION (first match wins):
    1. --db <PATH>
    2. $TOOLD_DB
    3. db_path = \"<PATH>\" in $TOOLD_CONFIG or <config dir>/{APP_NAME}/config.toml
    4. $TOOLD_DATA_DIR/{DB_FILE}
    5. $XDG_DATA_HOME/{APP_NAME}/{DB_FILE}, else ~/.local/share/{APP_NAME}/{DB_FILE}

The database and its parent directories are created on first launch.
Amounts are stored as integer cents; the default currency is {currency}.
",
        version = env!("CARGO_PKG_VERSION"),
        currency = crate::db::DEFAULT_CURRENCY,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_help_and_version() {
        assert!(matches!(startup_from_args(args(&["--help"])), Startup::Help));
        assert!(matches!(startup_from_args(args(&["-h"])), Startup::Help));
        assert!(matches!(startup_from_args(args(&["--version"])), Startup::Version));
        assert!(matches!(startup_from_args(args(&["-V"])), Startup::Version));
    }

    #[test]
    fn cli_flag_wins_and_is_reported() {
        match startup_from_args(args(&["--db", "/tmp/custom.db"])) {
            Startup::Run(cfg) => {
                assert_eq!(cfg.db_path, PathBuf::from("/tmp/custom.db"));
                assert_eq!(cfg.db_source, DbSource::CliFlag);
            }
            _ => panic!("expected Run"),
        }
        match startup_from_args(args(&["--db=/tmp/eq.db"])) {
            Startup::Run(cfg) => assert_eq!(cfg.db_path, PathBuf::from("/tmp/eq.db")),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(matches!(startup_from_args(args(&["--db"])), Startup::Error(_)));
        assert!(matches!(startup_from_args(args(&["--nope"])), Startup::Error(_)));
        assert!(matches!(startup_from_args(args(&["stray"])), Startup::Error(_)));
    }

    #[test]
    fn reads_db_path_from_config_file() {
        let dir = std::env::temp_dir().join(format!("toold-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");

        std::fs::write(&path, "# a comment\n[ledger]\ndb_path = \"/tmp/from-config.db\"\n").unwrap();
        assert_eq!(
            read_db_path_from_config(&path).unwrap(),
            Some("/tmp/from-config.db".to_string())
        );

        std::fs::write(&path, "db = '/tmp/alt.db'\n").unwrap();
        assert_eq!(read_db_path_from_config(&path).unwrap(), Some("/tmp/alt.db".to_string()));

        std::fs::write(&path, "unrelated = 1\n").unwrap();
        assert_eq!(read_db_path_from_config(&path).unwrap(), None);

        assert_eq!(read_db_path_from_config(&dir.join("absent.toml")).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn help_text_mentions_overrides() {
        let text = help_text();
        assert!(text.contains("TOOLD_DB"));
        assert!(text.contains("--db"));
        assert!(text.contains("CNY"));
    }
}
