use crate::dev::dev;
use crate::perft::{perft, PerftArgs};
use clap::{Parser, Subcommand};
// Leading `::` names the crate: importing `engine::engine` otherwise shadows the crate name for
// the remaining imports.
use ::engine::{engine, ui};

// This is a plain comment because a doc comment here would replace the crate description in
// `--help`.
//
// Each run mode is its own subcommand so that per-mode arguments stay isolated (for example the
// UI's port only makes sense under `ui`). Bare `seaborg` with no subcommand starts UCI, which is
// what a chess GUI expects when it launches the executable directly.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run the engine in UCI mode (the default when no subcommand is given)
    Uci,
    /// Play in a local browser UI served on the loopback interface
    Ui(UiArgs),
    /// Run perft on a given FEN position
    Perft(PerftArgs),
    /// Run the dev mode loop
    Dev,
    /// Print the notices for third-party material embedded in this executable
    Licenses,
    // A `lichess` subcommand for online play is expected here; it dispatches like the peers above.
}

/// Arguments for the browser UI mode.
#[derive(Debug, clap::Args)]
pub struct UiArgs {
    /// Serve the browser UI on a fixed port instead of an available one
    #[clap(long, value_name = "PORT")]
    port: Option<u16>,

    /// Do not open a browser when starting the UI
    #[clap(long)]
    no_open: bool,
}

pub fn cmdline() {
    let args = Args::parse();

    // No subcommand means UCI, so a chess GUI can launch `seaborg` directly.
    match args.command.unwrap_or(Commands::Uci) {
        Commands::Uci => engine::launch(engine::EngineInfo {
            name: "seaborg",
            version: env!("CARGO_PKG_VERSION"),
            author: "George Seabridge",
            commit: env!("GIT_HASH"),
        }),
        Commands::Ui(ui_args) => run_ui(&ui_args),
        Commands::Perft(perft_args) => perft(&perft_args),
        Commands::Dev => dev(),
        Commands::Licenses => {
            // The embedded piece artwork is permissively licensed on the one condition that its
            // notice reaches whoever receives the binary. Someone who only ever runs the executable
            // never sees the source tree, so the notice has to be printable from the executable
            // itself.
            print!("{}", ui::PIECE_ARTWORK_LICENSE);
        }
    }
}

/// Serve the browser UI until the process is interrupted.
fn run_ui(args: &UiArgs) {
    let config = ui::UiConfig {
        port: args.port,
        open_browser: !args.no_open,
        ..ui::UiConfig::default()
    };

    let server = match ui::bind(&config) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    let url = server.url();
    println!("Seaborg UI listening on {url}");
    println!("Use Quit Seaborg in the browser, or press Ctrl-C, to stop.");

    // Serve before announcing the URL to the browser, so the first request cannot outrun the
    // accept loop. The listener already accepts connections at this point either way.
    let serving = std::thread::spawn(move || server.run());

    if config.open_browser {
        // A browser that will not launch is not a reason to stop serving; the URL is on stdout.
        if let Err(error) = ui::open_browser(&url) {
            eprintln!("{error}");
        }
    }

    // A panicking server thread must not look like a clean shutdown to whatever started Seaborg.
    if serving.join().is_err() {
        eprintln!("the Seaborg UI server stopped unexpectedly");
        std::process::exit(1);
    }

    // Reached when the browser asked the server to quit: the accept loop returned on its own.
    println!("Seaborg UI stopped.");
}
