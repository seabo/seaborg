use std::io::Write;
use std::path::PathBuf;

use crate::dev::dev;
use crate::perft::{perft, PerftArgs};
use clap::{Parser, Subcommand};
use engine::ui;

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
    /// Play on Lichess as a bot, or upgrade the account to a bot
    Lichess(LichessArgs),
}

/// Arguments for Lichess bot play.
#[derive(Debug, clap::Args)]
pub struct LichessArgs {
    /// Load bot configuration from this TOML file instead of the default path
    #[clap(long, value_name = "PATH")]
    config: Option<PathBuf>,

    #[clap(subcommand)]
    command: Option<LichessCommand>,
}

/// Subcommands under `seaborg lichess`.
#[derive(Debug, Subcommand)]
enum LichessCommand {
    /// Upgrade the authenticated account to a BOT account (irreversible)
    Upgrade,
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
        Commands::Lichess(lichess_args) => run_lichess(&lichess_args),
    }
}

/// Dispatch the `lichess` subcommand, exiting non-zero on any failure so the
/// shell and any supervising process see the error.
fn run_lichess(args: &LichessArgs) {
    let result = match args.command {
        None => lichess::run::run(args.config.as_deref()),
        Some(LichessCommand::Upgrade) => {
            lichess::run::upgrade(confirm_upgrade).map(|outcome| match outcome {
                lichess::run::UpgradeOutcome::Upgraded => {
                    println!("Account upgraded to a BOT account.");
                }
                lichess::run::UpgradeOutcome::AlreadyBot => {
                    println!("Account is already a BOT account; nothing to do.");
                }
                lichess::run::UpgradeOutcome::Cancelled => {
                    println!("Upgrade cancelled.");
                }
            })
        }
    };

    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

/// Prompt on the terminal for confirmation before the irreversible bot upgrade.
///
/// Returns whether the operator typed an affirmative answer; any other input, or
/// a failure to read the terminal, is treated as a refusal so the account is
/// never upgraded by accident.
fn confirm_upgrade(account: &lichess::account::Account) -> bool {
    println!(
        "This will irreversibly convert account '{}' into a BOT account.",
        account.username
    );
    println!("A BOT account can only play through the Bot API and cannot be reverted.");
    print!("Type 'yes' to continue: ");
    if std::io::stdout().flush().is_err() {
        return false;
    }

    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    answer.trim().eq_ignore_ascii_case("yes")
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
