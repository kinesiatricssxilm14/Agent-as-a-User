//! Command-line parsing. The first positional argument is the working
//! directory, defaulting to `/bench/data/src` as specified.

use std::path::PathBuf;

/// Directory opened when no argument is given.
pub const DEFAULT_DIR: &str = "/bench/data/src";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    pub dir: PathBuf,
    pub show_hidden: bool,
}

/// What `parse` decided the process should do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Run(Cli),
    Help,
    Version,
    Error(String),
}

pub const USAGE: &str = "\
toolc — dual-pane file manager TUI

USAGE:
    toolc [OPTIONS] [DIRECTORY]

ARGS:
    <DIRECTORY>    Directory to open on launch [default: /bench/data/src]

OPTIONS:
    -a, --all      Show hidden entries (dotfiles) from the start
    -h, --help     Print this help and exit
    -V, --version  Print version and exit

KEYS (also shown in the status bar, and in the side panel via `?`):
    Navigate   up/down or j/k      move            Enter, Right, l   open directory
               Left, u, Backspace  parent          g                 go to a path
               Tab                 switch pane     PgUp/PgDn         page
    Operate    c copy    m move    r rename    n new directory    d delete
    View       / filter  H hidden  s sort  S reverse  w wrap  L activity log
               F5 reload   ? help   q quit
";

/// Parse pre-split arguments (excluding argv[0]).
pub fn parse<I, S>(args: I) -> Action
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut dir: Option<PathBuf> = None;
    let mut show_hidden = false;
    let mut positional_seen = false;
    let mut iter = args.into_iter().peekable();

    while let Some(raw) = iter.next() {
        let arg = raw.as_ref();
        match arg {
            "-h" | "--help" => return Action::Help,
            "-V" | "--version" => return Action::Version,
            "-a" | "--all" | "--hidden" => show_hidden = true,
            // Everything after `--` is positional.
            "--" => {
                if let Some(next) = iter.next() {
                    if positional_seen {
                        return Action::Error("more than one directory given".into());
                    }
                    dir = Some(PathBuf::from(next.as_ref()));
                    positional_seen = true;
                }
            }
            other if other.starts_with('-') && other.len() > 1 => {
                return Action::Error(format!("unknown option `{other}`"));
            }
            other => {
                if positional_seen {
                    return Action::Error(format!(
                        "unexpected extra argument `{other}` (only one directory is accepted)"
                    ));
                }
                dir = Some(PathBuf::from(other));
                positional_seen = true;
            }
        }
    }

    Action::Run(Cli {
        dir: dir.unwrap_or_else(|| PathBuf::from(DEFAULT_DIR)),
        show_hidden,
    })
}

/// Turn the requested directory into one that can actually be opened.
///
/// A path pointing at a file opens its parent with that file selected; the
/// returned `Option<String>` is the name to select.
pub fn prepare_dir(input: &PathBuf) -> Result<(PathBuf, Option<String>), String> {
    let meta =
        std::fs::metadata(input).map_err(|e| format!("cannot open {}: {e}", input.display()))?;
    if meta.is_dir() {
        // Canonicalize so the header shows a real absolute path and `..` works.
        let dir = std::fs::canonicalize(input).unwrap_or_else(|_| absolutize(input));
        Ok((dir, None))
    } else {
        let name = input.file_name().map(|n| n.to_string_lossy().into_owned());
        let parent = input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let dir = std::fs::canonicalize(&parent).unwrap_or_else(|_| absolutize(&parent));
        Ok((dir, name))
    }
}

fn absolutize(path: &PathBuf) -> PathBuf {
    if path.is_absolute() {
        crate::fs_ops::normalize(path)
    } else {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        crate::fs_ops::normalize(&cwd.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_uses_the_default_directory() {
        let args: [&str; 0] = [];
        assert_eq!(
            parse(args),
            Action::Run(Cli {
                dir: PathBuf::from(DEFAULT_DIR),
                show_hidden: false
            })
        );
    }

    #[test]
    fn positional_argument_overrides_the_default() {
        assert_eq!(
            parse(["/tmp/work"]),
            Action::Run(Cli {
                dir: PathBuf::from("/tmp/work"),
                show_hidden: false
            })
        );
    }

    #[test]
    fn flags_are_recognised_in_any_order() {
        assert_eq!(
            parse(["-a", "/data"]),
            Action::Run(Cli {
                dir: PathBuf::from("/data"),
                show_hidden: true
            })
        );
        assert_eq!(
            parse(["/data", "--all"]),
            Action::Run(Cli {
                dir: PathBuf::from("/data"),
                show_hidden: true
            })
        );
        assert_eq!(parse(["--help"]), Action::Help);
        assert_eq!(parse(["-V"]), Action::Version);
    }

    #[test]
    fn unknown_flags_and_extra_paths_are_errors() {
        assert!(matches!(parse(["--nope"]), Action::Error(_)));
        assert!(matches!(parse(["/a", "/b"]), Action::Error(_)));
    }

    #[test]
    fn double_dash_allows_dash_prefixed_paths() {
        assert_eq!(
            parse(["--", "-weird-dir"]),
            Action::Run(Cli {
                dir: PathBuf::from("-weird-dir"),
                show_hidden: false
            })
        );
    }

    #[test]
    fn prepare_dir_reports_missing_paths() {
        let err = prepare_dir(&PathBuf::from("/no/such/place/toolc")).unwrap_err();
        assert!(err.contains("cannot open"));
    }

    #[test]
    fn prepare_dir_selects_a_file_inside_its_parent() {
        let mut root = std::env::temp_dir();
        root.push(format!("toolc-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("target.txt");
        std::fs::write(&file, b"x").unwrap();

        let (dir, selected) = prepare_dir(&file).unwrap();
        assert_eq!(dir, std::fs::canonicalize(&root).unwrap());
        assert_eq!(selected.as_deref(), Some("target.txt"));

        let (dir, selected) = prepare_dir(&root).unwrap();
        assert_eq!(dir, std::fs::canonicalize(&root).unwrap());
        assert_eq!(selected, None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn usage_documents_the_core_operations() {
        for needle in [
            "c copy",
            "m move",
            "r rename",
            "n new directory",
            "d delete",
        ] {
            assert!(USAGE.contains(needle), "usage missing {needle}");
        }
        assert!(USAGE.contains(DEFAULT_DIR));
    }
}
