/// Traits to allow for compile-time monomorphization of functions like movegen.

/// Defines a player, allowing for specific functions in relation to a certain
/// player. This trait is applied to dummy structs `WhiteType` and `BlackType`
/// which are then used in turbofishes in the movegen module - the compiler
/// monomorphizes these functions over the two types of player and we get better
/// code reuse. We also inline all the functions in the implementation for speed
/// in the compiled monomorphized code.
use crate::bb::Bitboard;
use crate::movegen::{Generation, LegalityKind};
use crate::position::{PieceType, Player, Square};

pub trait Side {
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

/// Dummy type to represent a `Player::White` which implements `Side`.
pub struct White {}

/// Dummy type to represent a `Player::Black` which implements `Side`.
pub struct Black {}

impl Side for White {
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

impl Side for Black {
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
    fn kind() -> PieceType;
}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct Pawn {}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct Knight {}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct Bishop {}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct Rook {}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct Queen {}

/// Dummy type to represent a pawn, which implements `PieceTrait`.
pub struct King {}

impl PieceTrait for Pawn {
    #[inline(always)]
    fn kind() -> PieceType {
        PieceType::Pawn
    }
}

impl PieceTrait for Knight {
    #[inline(always)]
    fn kind() -> PieceType {
        PieceType::Knight
    }
}

impl PieceTrait for Bishop {
    #[inline(always)]
    fn kind() -> PieceType {
        PieceType::Bishop
    }
}

impl PieceTrait for Rook {
    #[inline(always)]
    fn kind() -> PieceType {
        PieceType::Rook
    }
}

impl PieceTrait for Queen {
    #[inline(always)]
    fn kind() -> PieceType {
        PieceType::Queen
    }
}

impl PieceTrait for King {
    #[inline(always)]
    fn kind() -> PieceType {
        PieceType::King
    }
}

/// The `Generate` allows for reusing movegen code by monomorphizing
/// over different 'generation types', such as 'captures-only', 'evasions-only'
/// 'quiet moves only' etc.
pub trait Generate {
    /// Returns the `Generation`.
    fn kind() -> Generation;
}

/// Dummy type to represent a `Generation::All` which implements `Generate`.
pub struct All {}
/// Dummy type to represent a `Generation::Captures` which implements `Generate`.
pub struct Captures {}
/// Dummy type to represent a `Generation::Promomtions` which implements `Generate`.
pub struct Promotions {}
/// Dummy type to represent a `Generation::Quiets` which implements `Generate`.
pub struct Quiets {}

impl Generate for All {
    #[inline(always)]
    fn kind() -> Generation {
        Generation::All
    }
}

impl Generate for Captures {
    #[inline(always)]
    fn kind() -> Generation {
        Generation::Captures
    }
}

impl Generate for Promotions {
    #[inline(always)]
    fn kind() -> Generation {
        Generation::Promotions
    }
}

impl Generate for Quiets {
    #[inline(always)]
    fn kind() -> Generation {
        Generation::Quiets
    }
}

/// The `Legality` allows for monomorphizing movegen code to different version based on
/// whether we want to generate just legal moves, or include pseudolegal moves as well.
pub trait Legality {
    /// Returns the `LegalityKind`.
    fn legality_type() -> LegalityKind;
}

/// Dummy type to represent a `LegalityKind::Legal` which implements `Legality`.
pub struct Legal {}

/// Dummy type to represent a `LegalityKind::Pseudolegal` which implements `Legality`.
pub struct PseudoLegal {}

impl Legality for Legal {
    #[inline(always)]
    fn legality_type() -> LegalityKind {
        LegalityKind::Legal
    }
}

impl Legality for PseudoLegal {
    #[inline(always)]
    fn legality_type() -> LegalityKind {
        LegalityKind::Pseudolegal
    }
}
