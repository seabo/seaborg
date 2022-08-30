use core::position::Position;

fn main() {
    let position =
        Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
    println!("{}", position);
    // let moves = position.generate_moves();
    // println!("{}", moves);
}
