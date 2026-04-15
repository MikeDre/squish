mod cli;
mod walker;

use clap::Parser;

fn main() -> std::process::ExitCode {
    let args = cli::Args::parse();
    eprintln!("parsed: {args:?}");
    std::process::ExitCode::SUCCESS
}
