mod cmdline;
mod dev;

use log::info;
use simple_logger::SimpleLogger;

fn main() {
    // Set up the logger.
    SimpleLogger::new().init().unwrap();

    info!("logger initialized");

    // Parse command line arguments.
    cmdline::cmdline();
}
