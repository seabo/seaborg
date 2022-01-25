use crate::dev::dev;
use crate::perft::perft;
use clap::Parser;
use engine::sess::Session;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    /// Run the engine in UCI mode
    #[clap(short, long)]
    uci: bool,

    /// Run the dev mode loop
    #[clap(short, long)]
    dev: bool,

    /// Run perft
    #[clap(short, long)]
    perft: Option<u8>,
}

pub fn cmdline() {
    let args = Args::parse();

    if args.uci {
        let mut engine_sess = Session::new();
        engine_sess.main_loop();
    } else if args.dev {
        dev();
    } else if let Some(depth) = args.perft {
        perft(depth);
    }
}
