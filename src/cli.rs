use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "set-desto")]
#[command(version)]
#[command(about = "Set Desto desktop utility", long_about = None)]
pub struct Cli {
    /// Enable debug mode with verbose application logging
    #[arg(long, global = true)]
    pub debug: bool,
}
