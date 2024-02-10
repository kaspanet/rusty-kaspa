pub use clap::Parser;
// use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// port to listen on
    #[arg(short, long, default_value = "127.0.0.1:8888")]
    pub listen: String,
    // // / Optional name to operate on
    // name: Option<String>,

    // // / Sets a custom config file
    // #[arg(short, long, value_name = "FILE")]
    // config: Option<PathBuf>,

    // // / Turn debugging information on
    // #[arg(short, long, action = clap::ArgAction::Count)]
    // debug: u8,

    // #[command(subcommand)]
    // command: Option<Commands>,
}
