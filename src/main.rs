mod cmdline;
mod dev;
mod perft;

use log::{info, LevelFilter};
use simple_logger::SimpleLogger;

fn main() {
    // Set up the logger.
    SimpleLogger::new()
        .with_level(LevelFilter::Error)
        .env()
        .init()
        .unwrap();

    info!("logger initialized");

    // Parse command line arguments.
    cmdline::cmdline();
}
