mod cmdline;
mod dev;
mod perft;

use log::info;
use simple_logger::SimpleLogger;

fn main() {
    // Set up the logger.
    SimpleLogger::new().init().unwrap();

    info!("logger initialized");

    // Parse command line arguments.
    cmdline::cmdline();
}
