use core::init::init_globals;
use core::position::Position;
use engine::search::perft::Perft;

use separator::Separatable;

use std::time::Instant;

/// Run perft on a given FEN position
#[derive(Debug, clap::Args)]
pub struct PerftArgs {
    /// Divide perft output
    #[clap(short, long, action, default_value_t = false)]
    divide: bool,
    /// Print extended output, including additional perft stats (captures, en passant, castles,
    /// promotions) & timing data
    #[clap(short, long, action, default_value_t = false)]
    verbose: bool,
    /// Depth to search
    #[clap(short = 'n', long, action, default_value_t = 1)]
    depth: u8,
    /// FEN string to run perft on; default to start position
    #[clap(default_value_t = String::from(core::position::START_POSITION))]
    fen: String,
}

pub fn perft(args: &PerftArgs) {
    init_globals();

    let mut pos = Position::from_fen(&args.fen);
    let depth = args.depth;

    match pos {
        Ok(ref mut pos) => {
            let start_zob = pos.zobrist().clone();
            let now = Instant::now();
            let perft_result = if args.divide {
                Perft::divide(pos, depth as usize, false, false)
            } else {
                Perft::perft(pos, depth as usize, false, false, true)
            };

            let elapsed = now.elapsed();

            println!(
                "{}µs to calculate perft {}",
                elapsed.as_micros().separated_string(),
                depth
            );
            println!(
                "{} nodes/sec",
                ((perft_result.nodes.unwrap() * 1_000_000_000) / (elapsed.as_nanos() as usize))
                    .separated_string()
            );
            let end_zob = pos.zobrist().clone();
            println!();
            println!("Start zob: {}", start_zob);
            println!("End zob:   {}", end_zob);
            println!(
                "Zobrist keys {}differ",
                if start_zob != end_zob { "" } else { "do not " }
            );
        }
        Err(fen_error) => {
            println!("{}", fen_error.msg);
        }
    }
}
