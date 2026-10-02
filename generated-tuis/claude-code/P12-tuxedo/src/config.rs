//! Command-line and configuration handling.
//!
//! Resolution order for the task file, highest priority first:
//!   1. `--file PATH` / `-f PATH` (or a bare positional path)
//!   2. `$TOOLL_TODO_FILE`
//!   3. `file = PATH` in the config file
//!   4. `/bench/data/todo.txt`
//!
//! The config file itself is `--config PATH`, else `$TOOLL_CONFIG`, else
//! `$XDG_CONFIG_HOME/tooll/config.toml`, else `~/.config/tooll/config.toml`.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Built-in default task file, as mandated by the runtime environment.
pub const DEFAULT_TODO_FILE: &str = "/bench/data/todo.txt";
pub const ENV_TODO_FILE: &str = "TOOLL_TODO_FILE";
pub const ENV_CONFIG: &str = "TOOLL_CONFIG";

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the process should do after parsing arguments.
#[derive(Debug, PartialEq, Eq)]
pub enum Invocation {
    /// Launch the interactive TUI.
    Run(Config),
    /// Print the rendered task list and exit (handy for scripts and smoke tests).
    List(Config),
    /// Print help text and exit successfully.
    Help,
    /// Print the version and exit successfully.
    Version,
}

/// Effective configuration for a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The todo.txt file to read and write.
    pub file: PathBuf,
    /// Where the path came from, shown in the UI header.
    pub file_source: FileSource,
    /// Config file that was read, if any.
    pub config_path: Option<PathBuf>,
    /// Hide completed tasks at startup.
    pub hide_done: bool,
    /// Sort the list on load (does not rewrite the file until a change is saved).
    pub sort_on_load: bool,
}

/// Provenance of the resolved task file path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSource {
    Argument,
    Environment,
    ConfigFile,
    Default,
}

impl FileSource {
    pub fn label(self) -> &'static str {
        match self {
            FileSource::Argument => "argument",
            FileSource::Environment => "env",
            FileSource::ConfigFile => "config",
            FileSource::Default => "default",
        }
    }
}

/// A user-facing argument or configuration error.
#[derive(Debug, PartialEq, Eq)]
pub struct ArgError(pub String);

impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ArgError {}

/// Environment lookups, injectable so the resolution logic stays testable.
pub trait Env {
    fn var(&self, key: &str) -> Option<String>;
    fn home(&self) -> Option<PathBuf>;
    /// Read a config file; `None` when it does not exist.
    fn read_file(&self, path: &Path) -> Option<String>;
}

/// The real process environment.
pub struct SystemEnv;

impl Env for SystemEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.is_empty())
    }

    fn home(&self) -> Option<PathBuf> {
        self.var("HOME").map(PathBuf::from)
    }

    fn read_file(&self, path: &Path) -> Option<String> {
        fs::read_to_string(path).ok()
    }
}

pub const HELP: &str = concat!(
    "tooll ",
    env!("CARGO_PKG_VERSION"),
    " - interactive todo.txt task manager\n",
    "\n",
    "USAGE:\n",
    "    tooll [OPTIONS] [TODO_FILE]\n",
    "\n",
    "OPTIONS:\n",
    "    -f, --file <PATH>     Task file to open (default: /bench/data/todo.txt)\n",
    "    -c, --config <PATH>   Config file to read instead of the default location\n",
    "        --no-config       Ignore any config file\n",
    "        --hide-done       Start with completed tasks hidden\n",
    "        --sort            Sort tasks on load (open first, then by priority)\n",
    "    -l, --list            Print the task list to stdout and exit\n",
    "    -h, --help            Show this help and exit\n",
    "    -V, --version         Show version and exit\n",
    "\n",
    "ENVIRONMENT:\n",
    "    TOOLL_TODO_FILE       Task file, used when no path is given on the command line\n",
    "    TOOLL_CONFIG          Config file location\n",
    "\n",
    "CONFIG FILE:\n",
    "    Looked up at $TOOLL_CONFIG, else $XDG_CONFIG_HOME/tooll/config.toml,\n",
    "    else ~/.config/tooll/config.toml. Simple `key = value` lines:\n",
    "\n",
    "        file = /bench/data/todo.txt\n",
    "        hide_done = false\n",
    "        sort_on_load = false\n",
    "\n",
    "KEYS:\n",
    "    Press ? inside tooll for the full, always-current key list.\n",
);

/// Parse process arguments (excluding argv[0]).
pub fn parse_args<I, S>(args: I, env: &dyn Env) -> Result<Invocation, ArgError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();

    let mut file_arg: Option<String> = None;
    let mut config_arg: Option<String> = None;
    let mut no_config = false;
    let mut hide_done = false;
    let mut sort_on_load = false;
    let mut list_only = false;
    let mut positional_seen = false;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-h" | "--help" => return Ok(Invocation::Help),
            "-V" | "--version" => return Ok(Invocation::Version),
            "-l" | "--list" => list_only = true,
            "--hide-done" => hide_done = true,
            "--sort" => sort_on_load = true,
            "--no-config" => no_config = true,
            "-f" | "--file" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| ArgError(format!("{a} requires a path argument")))?;
                file_arg = Some(v.clone());
            }
            "-c" | "--config" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| ArgError(format!("{a} requires a path argument")))?;
                config_arg = Some(v.clone());
            }
            _ => {
                if let Some(v) = a.strip_prefix("--file=") {
                    file_arg = Some(v.to_string());
                } else if let Some(v) = a.strip_prefix("--config=") {
                    config_arg = Some(v.to_string());
                } else if a.starts_with('-') && a != "-" {
                    return Err(ArgError(format!(
                        "unknown option `{a}` (try `tooll --help`)"
                    )));
                } else if positional_seen {
                    return Err(ArgError(format!(
                        "unexpected extra argument `{a}`; only one task file may be given"
                    )));
                } else {
                    positional_seen = true;
                    // An explicit `--file` wins over a bare path.
                    if file_arg.is_none() {
                        file_arg = Some(a.to_string());
                    }
                }
            }
        }
        i += 1;
    }

    if let Some(f) = &file_arg {
        if f.is_empty() {
            return Err(ArgError("task file path must not be empty".into()));
        }
    }

    // Locate and read the config file.
    let mut config_path = None;
    let mut settings = ConfigFile::default();
    if !no_config {
        let candidate = match &config_arg {
            Some(p) => Some(PathBuf::from(p)),
            None => default_config_path(env),
        };
        if let Some(path) = candidate {
            match env.read_file(&path) {
                Some(text) => {
                    settings = ConfigFile::parse(&text).map_err(|e| {
                        ArgError(format!("{}: {}", path.display(), e))
                    })?;
                    config_path = Some(path);
                }
                None => {
                    // An explicitly requested config file must exist; the
                    // default location is allowed to be absent.
                    if config_arg.is_some() {
                        return Err(ArgError(format!(
                            "config file not found: {}",
                            path.display()
                        )));
                    }
                }
            }
        }
    }

    let (file, file_source) = if let Some(f) = file_arg {
        (PathBuf::from(f), FileSource::Argument)
    } else if let Some(f) = env.var(ENV_TODO_FILE) {
        (PathBuf::from(f), FileSource::Environment)
    } else if let Some(f) = settings.file.clone() {
        (PathBuf::from(f), FileSource::ConfigFile)
    } else {
        (PathBuf::from(DEFAULT_TODO_FILE), FileSource::Default)
    };

    let cfg = Config {
        file: expand_tilde(&file, env),
        file_source,
        config_path,
        hide_done: hide_done || settings.hide_done.unwrap_or(false),
        sort_on_load: sort_on_load || settings.sort_on_load.unwrap_or(false),
    };

    Ok(if list_only {
        Invocation::List(cfg)
    } else {
        Invocation::Run(cfg)
    })
}

/// Where a config file would be looked for when none is specified.
pub fn default_config_path(env: &dyn Env) -> Option<PathBuf> {
    if let Some(p) = env.var(ENV_CONFIG) {
        return Some(PathBuf::from(p));
    }
    if let Some(x) = env.var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(x).join("tooll").join("config.toml"));
    }
    env.home().map(|h| h.join(".config/tooll/config.toml"))
}

/// Expand a leading `~` so config files can use it.
fn expand_tilde(path: &Path, env: &dyn Env) -> PathBuf {
    let s = path.to_string_lossy();
    if s == "~" {
        if let Some(h) = env.home() {
            return h;
        }
    } else if let Some(rest) = s.strip_prefix("~/") {
        if let Some(h) = env.home() {
            return h.join(rest);
        }
    }
    path.to_path_buf()
}

/// The recognised config-file settings.
#[derive(Debug, Default, PartialEq, Eq)]
struct ConfigFile {
    file: Option<String>,
    hide_done: Option<bool>,
    sort_on_load: Option<bool>,
}

impl ConfigFile {
    /// Parse a minimal `key = value` format: `#`/`;` comments, optional quotes,
    /// and an optional `[tooll]` section header that is simply skipped.
    fn parse(text: &str) -> Result<ConfigFile, String> {
        let mut cfg = ConfigFile::default();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("line {}: expected `key = value`", n + 1))?;
            let key = key.trim().to_ascii_lowercase();
            let value = unquote(value.trim());
            match key.as_str() {
                "file" | "todo_file" | "todo-file" => cfg.file = Some(value.to_string()),
                "hide_done" | "hide-done" => {
                    cfg.hide_done = Some(parse_bool(value).ok_or_else(|| {
                        format!("line {}: `{key}` expects true or false", n + 1)
                    })?)
                }
                "sort_on_load" | "sort" => {
                    cfg.sort_on_load = Some(parse_bool(value).ok_or_else(|| {
                        format!("line {}: `{key}` expects true or false", n + 1)
                    })?)
                }
                other => return Err(format!("line {}: unknown setting `{other}`", n + 1)),
            }
        }
        Ok(cfg)
    }
}

fn unquote(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A scripted environment: no real env vars or filesystem involved.
    struct FakeEnv {
        vars: HashMap<String, String>,
        files: HashMap<PathBuf, String>,
        home: Option<PathBuf>,
    }

    impl FakeEnv {
        fn new() -> FakeEnv {
            FakeEnv {
                vars: HashMap::new(),
                files: HashMap::new(),
                home: Some(PathBuf::from("/root")),
            }
        }
        fn with_var(mut self, k: &str, v: &str) -> Self {
            self.vars.insert(k.into(), v.into());
            self
        }
        fn with_file(mut self, p: &str, body: &str) -> Self {
            self.files.insert(PathBuf::from(p), body.into());
            self
        }
    }

    impl Env for FakeEnv {
        fn var(&self, key: &str) -> Option<String> {
            self.vars.get(key).cloned()
        }
        fn home(&self) -> Option<PathBuf> {
            self.home.clone()
        }
        fn read_file(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
    }

    fn run(args: &[&str], env: &dyn Env) -> Config {
        match parse_args(args.iter().map(|s| s.to_string()), env).unwrap() {
            Invocation::Run(c) | Invocation::List(c) => c,
            other => panic!("expected a config, got {other:?}"),
        }
    }

    #[test]
    fn defaults_to_the_bench_data_path() {
        let cfg = run(&[], &FakeEnv::new());
        assert_eq!(cfg.file, PathBuf::from("/bench/data/todo.txt"));
        assert_eq!(cfg.file_source, FileSource::Default);
        assert!(!cfg.hide_done);
    }

    #[test]
    fn flag_and_positional_override_the_default() {
        let env = FakeEnv::new();
        assert_eq!(run(&["--file", "/tmp/a.txt"], &env).file, PathBuf::from("/tmp/a.txt"));
        assert_eq!(run(&["-f", "/tmp/b.txt"], &env).file, PathBuf::from("/tmp/b.txt"));
        assert_eq!(run(&["--file=/tmp/c.txt"], &env).file, PathBuf::from("/tmp/c.txt"));
        let cfg = run(&["/tmp/d.txt"], &env);
        assert_eq!(cfg.file, PathBuf::from("/tmp/d.txt"));
        assert_eq!(cfg.file_source, FileSource::Argument);
    }

    #[test]
    fn precedence_is_arg_then_env_then_config_then_default() {
        let env = FakeEnv::new()
            .with_var(ENV_TODO_FILE, "/env/todo.txt")
            .with_var(ENV_CONFIG, "/cfg/config.toml")
            .with_file("/cfg/config.toml", "file = /cfg/todo.txt\n");

        assert_eq!(run(&["-f", "/arg/todo.txt"], &env).file, PathBuf::from("/arg/todo.txt"));

        let no_arg = run(&[], &env);
        assert_eq!(no_arg.file, PathBuf::from("/env/todo.txt"));
        assert_eq!(no_arg.file_source, FileSource::Environment);

        let mut env2 = FakeEnv::new()
            .with_var(ENV_CONFIG, "/cfg/config.toml")
            .with_file("/cfg/config.toml", "file = /cfg/todo.txt\n");
        env2.vars.remove(ENV_TODO_FILE);
        let from_cfg = run(&[], &env2);
        assert_eq!(from_cfg.file, PathBuf::from("/cfg/todo.txt"));
        assert_eq!(from_cfg.file_source, FileSource::ConfigFile);
        assert_eq!(from_cfg.config_path, Some(PathBuf::from("/cfg/config.toml")));
    }

    #[test]
    fn explicit_file_flag_beats_a_bare_path() {
        let cfg = run(&["/pos/todo.txt", "--file", "/flag/todo.txt"], &FakeEnv::new());
        assert_eq!(cfg.file, PathBuf::from("/flag/todo.txt"));
    }

    #[test]
    fn config_file_settings_are_applied() {
        let env = FakeEnv::new().with_var(ENV_CONFIG, "/c.toml").with_file(
            "/c.toml",
            "# comment\n[tooll]\nfile = \"/x/todo.txt\"\nhide_done = yes\nsort = 1\n",
        );
        let cfg = run(&[], &env);
        assert_eq!(cfg.file, PathBuf::from("/x/todo.txt"));
        assert!(cfg.hide_done);
        assert!(cfg.sort_on_load);
    }

    #[test]
    fn no_config_skips_the_file() {
        let env = FakeEnv::new()
            .with_var(ENV_CONFIG, "/c.toml")
            .with_file("/c.toml", "file = /x/todo.txt\n");
        let cfg = run(&["--no-config"], &env);
        assert_eq!(cfg.file, PathBuf::from("/bench/data/todo.txt"));
        assert_eq!(cfg.config_path, None);
    }

    #[test]
    fn xdg_and_home_config_locations() {
        let env = FakeEnv::new().with_var("XDG_CONFIG_HOME", "/xdg");
        assert_eq!(
            default_config_path(&env),
            Some(PathBuf::from("/xdg/tooll/config.toml"))
        );
        let env = FakeEnv::new();
        assert_eq!(
            default_config_path(&env),
            Some(PathBuf::from("/root/.config/tooll/config.toml"))
        );
    }

    #[test]
    fn tilde_is_expanded() {
        let env = FakeEnv::new();
        assert_eq!(run(&["-f", "~/todo.txt"], &env).file, PathBuf::from("/root/todo.txt"));
        assert_eq!(run(&["-f", "~"], &env).file, PathBuf::from("/root"));
    }

    #[test]
    fn missing_default_config_is_fine_but_explicit_one_must_exist() {
        let env = FakeEnv::new();
        assert!(parse_args(["--no-op-check"; 0], &env).is_ok());
        let err = parse_args(["-c", "/nope.toml"], &env).unwrap_err();
        assert!(err.0.contains("config file not found"), "{}", err.0);
    }

    #[test]
    fn help_and_version_short_circuit() {
        let env = FakeEnv::new();
        assert_eq!(parse_args(["--help"], &env).unwrap(), Invocation::Help);
        assert_eq!(parse_args(["-h"], &env).unwrap(), Invocation::Help);
        assert_eq!(parse_args(["-V"], &env).unwrap(), Invocation::Version);
        // Even alongside other arguments.
        assert_eq!(parse_args(["-f", "/a", "--help"], &env).unwrap(), Invocation::Help);
    }

    #[test]
    fn list_mode_is_selected() {
        let env = FakeEnv::new();
        assert!(matches!(
            parse_args(["--list"], &env).unwrap(),
            Invocation::List(_)
        ));
    }

    #[test]
    fn rejects_bad_usage() {
        let env = FakeEnv::new();
        assert!(parse_args(["--file"], &env).is_err());
        assert!(parse_args(["--nope"], &env).is_err());
        assert!(parse_args(["a.txt", "b.txt"], &env).is_err());
        assert!(parse_args(["-f", ""], &env).is_err());
    }

    #[test]
    fn config_parse_errors_are_reported_with_line_numbers() {
        let env = FakeEnv::new()
            .with_var(ENV_CONFIG, "/c.toml")
            .with_file("/c.toml", "file = /a\nbogus\n");
        let err = parse_args(["--no-op"; 0], &env).unwrap_err();
        assert!(err.0.contains("line 2"), "{}", err.0);

        let env = FakeEnv::new()
            .with_var(ENV_CONFIG, "/c.toml")
            .with_file("/c.toml", "hide_done = maybe\n");
        let err = parse_args(["--no-op"; 0], &env).unwrap_err();
        assert!(err.0.contains("true or false"), "{}", err.0);

        let env = FakeEnv::new()
            .with_var(ENV_CONFIG, "/c.toml")
            .with_file("/c.toml", "colour = blue\n");
        let err = parse_args(["--no-op"; 0], &env).unwrap_err();
        assert!(err.0.contains("unknown setting"), "{}", err.0);
    }

    #[test]
    fn parses_booleans_and_quotes() {
        assert_eq!(parse_bool("TRUE"), Some(true));
        assert_eq!(parse_bool("off"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
        assert_eq!(unquote("\"x\""), "x");
        assert_eq!(unquote("'x'"), "x");
        assert_eq!(unquote("x"), "x");
        assert_eq!(unquote("\""), "\"");
    }
}
