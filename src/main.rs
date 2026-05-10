#![deny(unsafe_code)]

mod app;
mod app_constants;
mod app_state;
mod cli;
mod logging;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();

    logging::init(cli.debug);
    app::run(cli.debug)
}
