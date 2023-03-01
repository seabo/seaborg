use crate::dev::dev;
use crate::perft::{perft, PerftArgs};
use clap::{Parser, Subcommand};
use engine2::session::Session;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    /// Run the engine in UCI mode
    #[clap(short, long)]
    uci: bool,

    /// Run the dev mode loop
    #[clap(short, long)]
    dev: bool,

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
        Session::new().launch();
        // let mut engine_sess = Session::new();
        // engine_sess.main_loop();
    } else if args.dev {
        dev();
    } else {
        match &args.command {
            Some(Commands::Perft(perft_args)) => {
                perft(perft_args);
            }
            None => {}
        }
    }
}
