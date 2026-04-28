mod cli;
mod config;
mod error;
mod parser;
mod path;
mod scanner;

use std::process;

use clap::Parser;

fn main() {
    let args = cli::Cli::parse();
    let exit_code = match args.command {
        cli::Command::Scan(scan_args) => run_scan(scan_args),
    };
    process::exit(exit_code);
}

fn run_scan(args: cli::ScanArgs) -> i32 {
    if let Some(config_path) = &args.config {
        match config::load(config_path) {
            Ok(_) => {
                if args.verbose {
                    eprintln!("loaded config from {}", config_path.display());
                }
            }
            Err(err) => {
                eprintln!("error: {}", err);
                return 1;
            }
        }
    }
    eprintln!("error: scan is not yet implemented");
    1
}
