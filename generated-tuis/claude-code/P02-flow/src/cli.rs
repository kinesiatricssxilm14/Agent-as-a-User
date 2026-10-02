//! Command line parsing.
//!
//! Hand-rolled rather than pulling in an argument-parsing crate: the surface is four flags and
//! one positional, and this keeps the dependency set to the TUI stack the specification asks for.

use std::path::PathBuf;

pub const USAGE: &str = "\
toolb - kanban board TUI

USAGE:
    toolb [OPTIONS] [BOARD_ROOT]

ARGS:
    <BOARD_ROOT>          Board root directory (same as --board)

OPTIONS:
    -b, --board <PATH>    Board root directory to open
    -c, --config <PATH>   Configuration file to read and write
        --init            Create the board directory (with default columns) and exit
    -h, --help            Print this help and exit
    -V, --version         Print version and exit

BOARD ROOT RESOLUTION (highest priority first):
    1. --board / positional argument
    2. $TOOLB_BOARD
    3. board_root in the configuration file
    4. /bench/data/board (built-in default)

CONFIGURATION FILE RESOLUTION (highest priority first):
    1. --config
    2. $TOOLB_CONFIG
    3. $XDG_CONFIG_HOME/toolb/config.conf
    4. $HOME/.config/toolb/config.conf
    5. /root/.config/toolb/config.conf

BOARD LAYOUT:
    <root>/board.txt                  lines of: col <column_id> \"<display name>\"
    <root>/cols/<column_id>/order.txt one card id per line
    <root>/cols/<column_id>/<id>.md   line 1 is '# <title>', the rest is the body

Press '?' inside the application for the full list of key bindings.";

/// What the user asked us to do.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    pub board_root: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    /// Create the board scaffolding and exit without starting the UI.
    pub init: bool,
    pub show_help: bool,
    pub show_version: bool,
}

/// Parse arguments (excluding argv[0]).
///
/// Returns a human-readable message on error; `main` prints it to stderr and exits non-zero.
pub fn parse<I, S>(args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = Args::default();
    let mut iter = args.into_iter().map(|s| s.as_ref().to_string()).peekable();
    let mut positional_seen = false;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => out.show_help = true,
            "-V" | "--version" => out.show_version = true,
            "--init" => out.init = true,
            "-b" | "--board" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("{arg} requires a path argument"))?;
                out.board_root = Some(PathBuf::from(value));
            }
            "-c" | "--config" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("{arg} requires a path argument"))?;
                out.config_path = Some(PathBuf::from(value));
            }
            // `--board=/path` form.
            _ if arg.starts_with("--board=") => {
                out.board_root = Some(PathBuf::from(arg.trim_start_matches("--board=")));
            }
            _ if arg.starts_with("--config=") => {
                out.config_path = Some(PathBuf::from(arg.trim_start_matches("--config=")));
            }
            // A bare "-" or unknown dash-prefixed token is a mistake, not a path: report it
            // rather than silently opening a board directory named "--baord".
            _ if arg.starts_with('-') && arg.len() > 1 => {
                return Err(format!("unknown option: {arg}\n\nRun 'toolb --help' for usage."));
            }
            _ => {
                if positional_seen {
                    return Err(format!("unexpected extra argument: {arg}"));
                }
                positional_seen = true;
                // An explicit --board earlier wins; ignoring the positional would hide the clash.
                if out.board_root.is_none() {
                    out.board_root = Some(PathBuf::from(arg));
                } else {
                    return Err(format!(
                        "board root given twice (--board and positional '{arg}')"
                    ));
                }
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_of(args: &[&str]) -> Option<PathBuf> {
        parse(args).unwrap().board_root
    }

    #[test]
    fn no_arguments_yields_defaults() {
        assert_eq!(parse(Vec::<String>::new()).unwrap(), Args::default());
    }

    #[test]
    fn accepts_positional_and_flag_forms() {
        assert_eq!(root_of(&["/tmp/b"]), Some(PathBuf::from("/tmp/b")));
        assert_eq!(root_of(&["--board", "/tmp/b"]), Some(PathBuf::from("/tmp/b")));
        assert_eq!(root_of(&["-b", "/tmp/b"]), Some(PathBuf::from("/tmp/b")));
        assert_eq!(root_of(&["--board=/tmp/b"]), Some(PathBuf::from("/tmp/b")));
    }

    #[test]
    fn accepts_config_forms() {
        assert_eq!(
            parse(["--config", "/x.conf"]).unwrap().config_path,
            Some(PathBuf::from("/x.conf"))
        );
        assert_eq!(
            parse(["--config=/x.conf"]).unwrap().config_path,
            Some(PathBuf::from("/x.conf"))
        );
        assert_eq!(parse(["-c", "/x.conf"]).unwrap().config_path, Some(PathBuf::from("/x.conf")));
    }

    #[test]
    fn recognises_simple_flags() {
        assert!(parse(["--help"]).unwrap().show_help);
        assert!(parse(["-h"]).unwrap().show_help);
        assert!(parse(["--version"]).unwrap().show_version);
        assert!(parse(["-V"]).unwrap().show_version);
        assert!(parse(["--init"]).unwrap().init);
    }

    #[test]
    fn paths_with_spaces_and_unicode_survive() {
        assert_eq!(root_of(&["/tmp/my board"]), Some(PathBuf::from("/tmp/my board")));
        assert_eq!(root_of(&["/tmp/English-only text"]), Some(PathBuf::from("/tmp/English-only text")));
    }

    #[test]
    fn missing_flag_values_are_errors() {
        assert!(parse(["--board"]).is_err());
        assert!(parse(["--config"]).is_err());
    }

    #[test]
    fn typos_are_reported_not_treated_as_paths() {
        let err = parse(["--baord", "/tmp/b"]).unwrap_err();
        assert!(err.contains("unknown option"), "got: {err}");
    }

    #[test]
    fn conflicting_and_extra_positionals_are_errors() {
        assert!(parse(["--board", "/a", "/b"]).is_err());
        assert!(parse(["/a", "/b"]).is_err());
    }

    #[test]
    fn flags_combine_with_paths_in_any_order() {
        let args = parse(["--init", "/tmp/b", "--config", "/c.conf"]).unwrap();
        assert!(args.init);
        assert_eq!(args.board_root, Some(PathBuf::from("/tmp/b")));
        assert_eq!(args.config_path, Some(PathBuf::from("/c.conf")));
    }
}
