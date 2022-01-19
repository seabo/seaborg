use crate::dev::dev;
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
}

pub fn cmdline() {
    let args = Args::parse();

    if args.uci {
        let mut engine_sess = Session::new();
        engine_sess.main_loop();
    }

    if args.dev {
        dev();
    }
}
