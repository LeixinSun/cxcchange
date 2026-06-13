mod app;
mod cli;
mod config;
mod fsutil;
mod profiles;
mod prompt;

use std::process;

fn main() {
    if let Err(err) = app::run() {
        eprintln!("Error: {err}");
        process::exit(1);
    }
}
