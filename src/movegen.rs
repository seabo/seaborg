use crate::bb::Bitboard;
use crate::mono_traits::{
    BishopType, BlackType, KingType, KnightType, PawnType, PieceTrait, PlayerTrait, QueenType,
    RookType, WhiteType,
};
use crate::mov::{Move, SpecialMove};
use crate::movelist::{MVPushable, MoveList};
use crate::position::{PieceType, Player, Position, Square};
use crate::precalc::boards::{king_moves, knight_moves};

use std::mem;
use std::ops::Index;

pub struct MoveGen {}

impl MoveGen {
    #[inline]
    pub fn generate(position: &Position) -> MoveList {
        let mut movelist = MoveList::default();
        InnerMoveGen::<MoveList>::generate(position, &mut movelist);
        movelist
    }
}

pub struct InnerMoveGen<'a, MP: MVPushable + 'a> {
    movelist: &'a mut MP,
    position: &'a Position,
    /// All occupied squares on the board
    occ: Bitboard,
    /// Squares occupied by the player to move
    us_occ: Bitboard,
    /// Square occupied by the opponent
    them_occ: Bitboard,
}

impl<'a, MP: MVPushable> InnerMoveGen<'a, MP>
where
    <MP as Index<usize>>::Output: Sized,
{
    fn generate(position: &'a Position, movelist: &'a mut MP) -> &'a mut MP {
        match position.turn() {
            Player::White => InnerMoveGen::<MP>::generate_helper::<WhiteType>(position, movelist),
            Player::Black => InnerMoveGen::<MP>::generate_helper::<BlackType>(position, movelist),
        }
    }

    #[inline(always)]
    fn get_self(position: &'a Position, movelist: &'a mut MP) -> Self {
        InnerMoveGen {
            movelist,
            position,
            occ: position.occupied(),
            us_occ: position.get_occupied_player(position.turn()),
            them_occ: position.get_occupied_player(position.turn().other_player()),
        }
    }

    fn generate_helper<P: PlayerTrait>(position: &'a Position, movelist: &'a mut MP) -> &'a mut MP {
        let mut movegen = InnerMoveGen::<MP>::get_self(position, movelist);
        movegen.generate_all::<P>();
        movegen.movelist
    }

    fn generate_all<P: PlayerTrait>(&mut self) {
        self.moves_per_piece::<P, KnightType>(Bitboard::ALL);
        self.moves_per_piece::<P, KingType>(Bitboard::ALL);
    }

    fn moves_per_piece<PL: PlayerTrait, P: PieceTrait>(&mut self, target: Bitboard) {
        let piece_bb: Bitboard = self.position.piece_bb(PL::player(), P::piece_type());
        for orig in piece_bb {
            let moves_bb: Bitboard = self.moves_bb::<P>(orig) & !self.us_occ & target;
            let mut captures_bb: Bitboard = moves_bb & self.them_occ;
            let mut non_captures_bb: Bitboard = moves_bb & !self.them_occ;
            self.move_append_from_bb_flag(&mut captures_bb, orig, SpecialMove::Capture);
            self.move_append_from_bb_flag(&mut non_captures_bb, orig, SpecialMove::Quiet);
        }
    }

    fn moves_bb<P: PieceTrait>(&mut self, sq: Square) -> Bitboard {
        debug_assert!(sq.is_okay());
        debug_assert_ne!(P::piece_type(), PieceType::Pawn);
        match P::piece_type() {
            PieceType::None => panic!(), // TODO
            PieceType::Pawn => panic!(),
            PieceType::Knight => knight_moves(sq),
            PieceType::Bishop => panic!(), // TODO
            PieceType::Rook => panic!(),   // TODO
            PieceType::Queen => panic!(),  // TODO
            PieceType::King => king_moves(sq),
        }
    }

    #[inline]
    fn move_append_from_bb_flag(&mut self, bb: &mut Bitboard, orig: Square, flag: SpecialMove) {
        for dest in bb {
            let mov = Move::build(orig, dest, None, false, false);
            self.add_move(mov);
        }
    }

    fn add_move(&mut self, mv: Move) {
        self.movelist.push(mv);
    }
}
