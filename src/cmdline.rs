use crate::dev::dev;
use crate::perft::{perft, PerftArgs};
use clap::{ArgGroup, Parser, Subcommand};
// Leading `::` names the crate: importing `engine::engine` otherwise shadows the crate name for
// the remaining imports.
use ::engine::{engine, ui};

// The run modes are grouped so clap rejects any combination of them: an `ArgGroup` is
// single-valued unless declared otherwise, and each mode takes over the process. This is a plain
// comment because a doc comment here would replace the crate description in `--help`.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(group(ArgGroup::new("mode")))]
pub struct Args {
    /// Run the engine in UCI mode
    #[clap(short, long, group = "mode")]
    uci: bool,

    /// Run the dev mode loop
    #[clap(short, long, group = "mode")]
    dev: bool,

    /// Play in a local browser UI served on the loopback interface
    #[clap(long, group = "mode")]
    ui: bool,

    /// Serve the browser UI on a fixed port instead of an available one
    #[clap(long, value_name = "PORT", requires = "ui")]
    ui_port: Option<u16>,

    /// Do not open a browser when starting the UI
    #[clap(long, requires = "ui")]
    no_open: bool,

    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Perft(PerftArgs),
}

pub fn cmdline() {
    let args = Args::parse();

    if args.uci {
        engine::launch(engine::EngineInfo {
            name: "seaborg",
            version: env!("CARGO_PKG_VERSION"),
            author: "George Seabridge",
            commit: env!("GIT_HASH"),
        })
    } else if args.dev {
        dev();
    } else if args.ui {
        run_ui(&args);
    } else {
        match &args.command {
            Some(Commands::Perft(perft_args)) => {
                perft(perft_args);
            }
            None => {}
        }
    }
}

/// Serve the browser UI until the process is interrupted.
fn run_ui(args: &Args) {
    let config = ui::UiConfig {
        port: args.ui_port,
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
    println!("Press Ctrl-C to stop.");

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
}
