#[derive(Copy, Clone, PartialEq)]
struct Bitboard(u64);

impl Bitboard {
    fn print(self) {
        let mut squares: [[u8; 8]; 8] = [[0; 8]; 8];

        for i in 0..64 {
            let rank = i / 8;
            let file = i % 8;
            let x: Bitboard = Bitboard(1 << i);
            if x & self != Bitboard(0) {
                squares[rank][file] = 1;
            }
        }

        println!("");
        println!("   ┌────────────────────────┐");
        for (i, row) in squares.iter().rev().enumerate() {
            print!(" {} │", 8 - i);
            for square in row {
                print!(" {} ", square);
            }
            print!("│\n");
        }
        println!("   └────────────────────────┘");
        println!("     a  b  c  d  e  f  g  h ");
        println!("")
    }
}

impl std::ops::BitAnd for Bitboard {
    type Output = Bitboard;

    fn bitand(self, other: Bitboard) -> Bitboard {
        match (self, other) {
            (Bitboard(left), Bitboard(right)) => Bitboard(left & right),
        }
    }
}

fn main() {
    let bb: Bitboard = Bitboard(0xFF00000100000000);
    bb.print();
}
