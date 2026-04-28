mod cli;
mod error;
mod parser;
mod path;
mod scanner;

use std::process;

use clap::Parser;

fn main() {
    let args = cli::Cli::parse();
    let exit_code = match args.command {
        cli::Command::Scan(_) => {
            eprintln!("error: scan is not yet implemented");
            1
        }
    };
    process::exit(exit_code);
}
