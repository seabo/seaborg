mod cmdline;
mod datagen;
mod dev;
mod perft;

use log::{debug, LevelFilter};
use simple_logger::SimpleLogger;

fn main() {
    // Default to `Info` so the Lichess bot's lifecycle events (connection,
    // challenges, games) are visible on stderr without the operator having to
    // discover `RUST_LOG`. UCI mode emits no logs of its own, so this stays
    // silent there. `.env()` lets `RUST_LOG` still override the level.
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .env()
        .init()
        .unwrap();

    // Kept below the default level: this is a startup diagnostic, not operator
    // output, so it must not appear in UCI mode where any unexpected line on
    // stderr is noise. Raise the level via `RUST_LOG` to see it.
    debug!("logger initialized");

    // Parse command line arguments.
    cmdline::cmdline();
}
