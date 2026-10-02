//! `toolc` entry point: parse arguments, then hand off to the library.

use std::io::{self, Write};

use toolc::cli::{self, Action};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(&args) {
        Action::Help => {
            print!("{}", cli::USAGE);
            let _ = io::stdout().flush();
        }
        Action::Version => println!("toolc {}", cli::VERSION),
        Action::Error(message) => {
            eprintln!("toolc: {message}\n");
            eprint!("{}", cli::USAGE);
            std::process::exit(2);
        }
        Action::Run(options) => {
            let (dir, preselect) = match cli::prepare_dir(&options.dir) {
                Ok(pair) => pair,
                Err(message) => {
                    eprintln!("toolc: {message}");
                    eprintln!("hint: pass a directory, e.g. `toolc {}`", cli::DEFAULT_DIR);
                    std::process::exit(1);
                }
            };
            if let Err(err) = toolc::run(dir, preselect, options.show_hidden) {
                eprintln!("toolc: {err}");
                std::process::exit(1);
            }
        }
    }
}
