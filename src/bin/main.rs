use rchess::bb::{
    Bitboard, WHITE_LEFTWARD_PROMOTION_MASK, WHITE_LEFT_PAWN_CAPTURE_MASK,
    WHITE_RIGHT_PAWN_CAPTURE_MASK,
};

use rchess::mov::Move;
use rchess::movegen::MoveGen;
use rchess::position::{Position, Square};

use std::sync::{Once, ONCE_INIT};
use std::time::Instant;

use rchess::precalc::boards::{init_boards, king_moves, knight_moves};

static INITALIZED: Once = ONCE_INIT;

fn init_globals() {
    INITALIZED.call_once(|| {
        init_boards();
    })
}

fn main() {
    init_globals();
    let start_position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    let other_position = "rn3rk1/1bq2ppp/p3p3/1pnp2B1/3N1P2/2b3Q1/PPP3PP/2KRRB2 w - - 0 17";
    let position3 = "2r1b2k/3P4/8/8/8/8/8/7K w - - 0 1";
    let position4 = "7k/8/8/1PpP4/8/8/8/7K w - c6 0 2";
    let position5 = "7k/8/8/3Rnr2/3pKb2/3rpp2/8/8 w - - 0 1";

    let now = Instant::now();
    let pos = Position::from_fen(other_position);
    let elapsed = now.elapsed().as_micros();

    println!("{}", WHITE_LEFTWARD_PROMOTION_MASK);

    match pos {
        Ok(pos) => {
            println!("{:?}", pos);
            let Bitboard(x) = pos.occupied();
            let attacks = sliding_attack(&[-7, 7, -9, 9], 50, x);
            let bb = Bitboard::new(attacks);
            println!("{}", bb);
        }
        Err(fen_error) => {
            println!("{}", fen_error.msg);
        }
    }

    // println!("FEN string took {}μs to parse", elapsed);
}

/// Returns a bitboards of sliding attacks given an array of 4 deltas.
/// Does not include the origin square.
/// Includes occupied bits if it runs into them, but stops before going further.
// TODO: move this to a magic bitboards module, and use it to generate the magic
// tables.
fn sliding_attack(deltas: &[i8; 4], sq: u8, occupied: u64) -> u64 {
    assert!(sq < 64);
    let mut attack: u64 = 0;
    let square: i16 = sq as i16;
    for delta in deltas.iter().take(4 as usize) {
        let mut s: u8 = ((square as i16) + (*delta as i16)) as u8;
        'inner: while s < 64
            && Square(s as u8).distance(Square(((s as i16) - (*delta as i16)) as u8)) == 1
        {
            attack |= (1 as u64).wrapping_shl(s as u32);
            if occupied & (1 as u64).wrapping_shl(s as u32) != 0 {
                break 'inner;
            }
            s = ((s as i16) + (*delta as i16)) as u8;
        }
    }
    attack
}
