#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![deny(unsafe_code)]

mod app;
mod app_constants;
mod app_state;
mod cli;
mod domain;
mod eve;
mod logging;
mod platform;
mod storage;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

fn main() -> Result<()> {
    platform::attach_parent_console();

    let cli = Cli::parse();

    logging::init(cli.debug);
    app::run(cli.debug)
}
