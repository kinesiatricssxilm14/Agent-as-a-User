//! toolb - command line entry point.
//!
//! Argument handling and terminal setup live here; everything else is in the library so the
//! integration tests can exercise the same code paths the binary uses.

use std::process::ExitCode;

use toolb::app::App;
use toolb::config::{self, Config};
use toolb::store::{self, Store};
use toolb::{cli, ui};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match cli::parse(&args) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("toolb: {e}");
            return ExitCode::FAILURE;
        }
    };

    if args.show_help {
        println!("{}", cli::USAGE);
        return ExitCode::SUCCESS;
    }
    if args.show_version {
        println!("toolb {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    // Resolve the configuration file first: it may itself supply the board root.
    let config_path = config::resolve_config_path(args.config_path.as_deref());
    let config = match Config::load(config_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("toolb: {e}");
            return ExitCode::FAILURE;
        }
    };

    let (root, root_source) = config::resolve_board_root(args.board_root.as_deref(), &config);
    let store = Store::new(&root);

    // `--init` is a batch operation: create the scaffolding, report, and exit.
    if args.init {
        return match store.init_board() {
            Ok(()) => {
                println!("Initialised board at {}", root.display());
                println!("  {}", store.board_file_path().display());
                for (id, name) in store::DEFAULT_COLUMNS {
                    println!("  cols/{id}/  ({name})");
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("toolb: {e}");
                ExitCode::FAILURE
            }
        };
    }

    if let Err(e) = run(store, config, root_source) {
        eprintln!("toolb: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Set up the terminal, run the event loop, and restore the terminal afterwards.
///
/// `ratatui::init` enters the alternate screen, switches to raw mode and installs a panic hook
/// that restores the terminal, so a crash cannot leave the user with an unusable shell.
fn run(store: Store, config: Config, root_source: config::RootSource) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new(store, config, root_source);

    let result = (|| -> std::io::Result<()> {
        while !app.should_quit {
            terminal.draw(|frame| ui::draw(frame, &mut app))?;
            app.handle_next_event()?;
        }
        Ok(())
    })();

    // Always restore, whether the loop ended normally or with an error, so the shell is usable.
    ratatui::restore();
    result
}
