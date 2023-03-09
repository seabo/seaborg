/// Traits to allow for compile-time monomorphization of functions like movegen.

/// Defines a player, allowing for specific functions in relation to a certain
/// player. This trait is applied to dummy structs `WhiteType` and `BlackType`
/// which are then used in turbofishes in the movegen module - the compiler
/// monomorphizes these functions over the two types of player and we get better
/// code reuse. We also inline all the functions in the implementation for speed
/// in the compiled monomorphized code.
use crate::bb::Bitboard;
use crate::movegen::{GenType, LegalityType};
use crate::position::{PieceType, Player, Square};

pub trait PlayerTrait {
    /// Return the current `Player`.
    fn player() -> Player;

    /// Return the opposing `Player`.
    fn opp_player() -> Player;

    /// Given a `Square`, return a square that is down relative to the current player.
    fn down(sq: Square) -> Square;

    /// Given a `Square`, return a square that is up relative to the current player.
    fn up(sq: Square) -> Square;

    /// Given a `Square`, return a square that is left relative to the current player.
    fn left(sq: Square) -> Square;

    /// Given a `Square`, return a square that is right relative to the current player.
    fn right(sq: Square) -> Square;

    /// Given a `Square`, return a square that is down-left relative to the current player.
    fn down_left(sq: Square) -> Square;

    /// Given a `Square`, return a square that is down-right relative to the current player.
    fn down_right(sq: Square) -> Square;

    /// Given a `Square`, return a square that is up-left relative to the current player.
    fn up_left(sq: Square) -> Square;

    /// Given a `Square`, return a square that is up-right relative to the current player.
    fn up_right(sq: Square) -> Square;

    /// Return the same BitBoard shifted down relative to the current player.
    fn shift_down(bb: Bitboard) -> Bitboard;

    /// Return the same BitBoard shifted up relative to the current player.
    fn shift_up(bb: Bitboard) -> Bitboard;

    /// Return the same BitBoard shifted left relative to the current player.
    fn shift_left(bb: Bitboard) -> Bitboard;

    /// Return the same BitBoard shifted right relative to the current player.
    fn shift_right(bb: Bitboard) -> Bitboard;

    /// Return the same BitBoard shifted down left relative to the current player.
    fn shift_down_left(bb: Bitboard) -> Bitboard;

    /// Return the same BitBoard shifted down right relative to the current player.
    fn shift_down_right(bb: Bitboard) -> Bitboard;

    /// Return the same BitBoard shifted up left relative to the current player.
    fn shift_up_left(bb: Bitboard) -> Bitboard;

    /// Return the same BitBoard shifted up right relative to the current player.
    fn shift_up_right(bb: Bitboard) -> Bitboard;
}

/// Dummy type to represent a `Player::White` which implements `PlayerTrait`.
pub struct WhiteType {}

/// Dummy type to represent a `Player::Black` which implements `PlayerTrait`.
pub struct BlackType {}

impl PlayerTrait for WhiteType {
    #[inline(always)]
    fn player() -> Player {
        Player::WHITE
    }

    #[inline(always)]
    fn opp_player() -> Player {
        Player::BLACK
    }

    #[inline(always)]
    fn down(sq: Square) -> Square {
        sq - Square(8)
    }

    #[inline(always)]
    fn up(sq: Square) -> Square {
        sq + Square(8)
    }

    #[inline(always)]
    fn left(sq: Square) -> Square {
        sq - Square(1)
    }

    #[inline(always)]
    fn right(sq: Square) -> Square {
        sq + Square(1)
    }

    #[inline(always)]
    fn down_left(sq: Square) -> Square {
        sq - Square(9)
    }

    #[inline(always)]
    fn down_right(sq: Square) -> Square {
        sq - Square(7)
    }
    #[inline(always)]
    fn up_left(sq: Square) -> Square {
        sq + Square(7)
    }
    #[inline(always)]
    fn up_right(sq: Square) -> Square {
        sq + Square(9)
    }

    #[inline(always)]
    fn shift_down(bb: Bitboard) -> Bitboard {
        bb >> 8
    }

    #[inline(always)]
    fn shift_up(bb: Bitboard) -> Bitboard {
        bb << 8
    }

    #[inline(always)]
    fn shift_left(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_A) >> 1
    }

    #[inline(always)]
    fn shift_right(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_H) << 1
    }

    #[inline(always)]
    fn shift_down_left(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_A) >> 9
    }

    #[inline(always)]
    fn shift_down_right(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_H) << 7
    }

    #[inline(always)]
    fn shift_up_left(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_A) << 7
    }

    #[inline(always)]
    fn shift_up_right(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_H) << 9
    }
}

impl PlayerTrait for BlackType {
    #[inline(always)]
    fn player() -> Player {
        Player::BLACK
    }

    #[inline(always)]
    fn opp_player() -> Player {
        Player::WHITE
    }

    #[inline(always)]
    fn down(sq: Square) -> Square {
        sq + Square(8)
    }

    #[inline(always)]
    fn up(sq: Square) -> Square {
        sq - Square(8)
    }

    #[inline(always)]
    fn left(sq: Square) -> Square {
        sq + Square(1)
    }

    #[inline(always)]
    fn right(sq: Square) -> Square {
        sq - Square(1)
    }

    #[inline(always)]
    fn down_left(sq: Square) -> Square {
        sq + Square(9)
    }

    #[inline(always)]
    fn down_right(sq: Square) -> Square {
        sq + Square(7)
    }
    #[inline(always)]
    fn up_left(sq: Square) -> Square {
        sq - Square(7)
    }
    #[inline(always)]
    fn up_right(sq: Square) -> Square {
        sq - Square(9)
    }

    #[inline(always)]
    fn shift_down(bb: Bitboard) -> Bitboard {
        bb << 8
    }

    #[inline(always)]
    fn shift_up(bb: Bitboard) -> Bitboard {
        bb >> 8
    }

    #[inline(always)]
    fn shift_left(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_H) << 1
    }

    #[inline(always)]
    fn shift_right(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_A) >> 1
    }

    #[inline(always)]
    fn shift_down_left(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_H) << 9
    }

    #[inline(always)]
    fn shift_down_right(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_A) >> 7
    }

    #[inline(always)]
    fn shift_up_left(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_H) >> 7
    }

    #[inline(always)]
    fn shift_up_right(bb: Bitboard) -> Bitboard {
        (bb & !Bitboard::FILE_A) >> 9
    }
}

/// The `PieceTrait` allows for reusing movegen code by monomorphizing
/// over different piece types. This trait provides common functionality
/// across each piece type.
pub trait PieceTrait {
    /// Returns the `PieceType`.
    fn piece_type() -> PieceType;
}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct PawnType {}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct KnightType {}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct BishopType {}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct RookType {}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct QueenType {}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct KingType {}

impl PieceTrait for PawnType {
    #[inline(always)]
    fn piece_type() -> PieceType {
        PieceType::Pawn
    }
}

impl PieceTrait for KnightType {
    #[inline(always)]
    fn piece_type() -> PieceType {
        PieceType::Knight
    }
}

impl PieceTrait for BishopType {
    #[inline(always)]
    fn piece_type() -> PieceType {
        PieceType::Bishop
    }
}

impl PieceTrait for RookType {
    #[inline(always)]
    fn piece_type() -> PieceType {
        PieceType::Rook
    }
}

impl PieceTrait for QueenType {
    #[inline(always)]
    fn piece_type() -> PieceType {
        PieceType::Queen
    }
}

impl PieceTrait for KingType {
    #[inline(always)]
    fn piece_type() -> PieceType {
        PieceType::King
    }
}

/// The `GenTypeTrait` allows for reusing movegen code by monomorphizing
/// over different 'generation types', such as 'captures-only', 'evasions-only'
/// 'quiet moves only' etc.
pub trait GenTypeTrait {
    /// Returns the `GenType`.
    fn gen_type() -> GenType;
}

/// Dummy type to represent a `GenType::All` which implements `GenTypeTrait`.
pub struct AllGenType {}
/// Dummy type to represent a `GenType::Captures` which implements `GenTypeTrait`.
pub struct CapturesGenType {}
/// Dummy type to represent a `GenType::Quiets` which implements `GenTypeTrait`.
pub struct QuietsGenType {}
/// Dummy type to represent a `GenType::QuietChecks` which implements `GenTypeTrait`.
pub struct QuietChecksGenType {}
/// Dummy type to represent a `GenType::Evasions` which implements `GenTypeTrait`.
pub struct EvasionsGenType {}
/// Dummy type to represent a `GenType::NonEvasions` which implements `GenTypeTrait`.
pub struct NonEvasionsGenType {}

impl GenTypeTrait for AllGenType {
    #[inline(always)]
    fn gen_type() -> GenType {
        GenType::All
    }
}

impl GenTypeTrait for CapturesGenType {
    #[inline(always)]
    fn gen_type() -> GenType {
        GenType::Captures
    }
}

impl GenTypeTrait for QuietsGenType {
    #[inline(always)]
    fn gen_type() -> GenType {
        GenType::Quiets
    }
}

impl GenTypeTrait for QuietChecksGenType {
    #[inline(always)]
    fn gen_type() -> GenType {
        GenType::QuietChecks
    }
}

impl GenTypeTrait for EvasionsGenType {
    #[inline(always)]
    fn gen_type() -> GenType {
        GenType::Evasions
    }
}

impl GenTypeTrait for NonEvasionsGenType {
    #[inline(always)]
    fn gen_type() -> GenType {
        GenType::NonEvasions
    }
}

/// The `Legality` allows for monomorphizing movegen code to different version based on
/// whether we want to generate just legal moves, or include pseudolegal moves as well.
pub trait Legality {
    /// Returns the `LegalityType`.
    fn legality_type() -> LegalityType;
}

/// Dummy type to represent a `LegalityType::Legal` which implements `Legality`.
pub struct LegalType {}

/// Dummy type to represent a `LegalityType::Pseudolegal` which implements `Legality`.
pub struct PseudolegalType {}

impl Legality for LegalType {
    #[inline(always)]
    fn legality_type() -> LegalityType {
        LegalityType::Legal
    }
}

impl Legality for PseudolegalType {
    #[inline(always)]
    fn legality_type() -> LegalityType {
        LegalityType::Pseudolegal
    }
}
