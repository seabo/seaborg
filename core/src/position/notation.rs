use super::{PieceType, Position, Square};
use crate::mono_traits::{All, Legal};
use crate::mov::{Move, MoveType};
use crate::movelist::BasicMoveList;
use std::iter::Peekable;

use unicode_segmentation::{Graphemes, UnicodeSegmentation};

#[derive(Debug)]
enum ParseError {
    Invalid,
    InvalidPieceType,
    UnexpectedEndOfString,
    ExpectedFile,
    ExpectedRank,
    InvalidPromotionOnNonPawnMove,
}

type PResult<T> = Result<T, ParseError>;

#[derive(Debug)]
struct MoveDetails {
    piece_type: PieceType,
    from_file: Option<usize>,
    from_rank: Option<usize>,
    to_square: Option<Square>,
    is_capture: bool,
    promo_piece: Option<PieceType>,
    is_ks_castle: bool,
    is_qs_castle: bool,
}

impl MoveDetails {
    /// Test whether the passed `Move` matches.
    pub fn matches(&self, pos: &Position, mov: Move) -> bool {
        if self.to_square.is_none() {
            if self.is_ks_castle
                && mov.move_type().contains(MoveType::CASTLE)
                && mov.dest().0 > mov.orig().0
            {
                return true;
            } else if self.is_qs_castle
                && mov.move_type().contains(MoveType::CASTLE)
                && mov.dest().0 < mov.orig().0
            {
                return true;
            }
            return false;
        }

        if !(self.to_square.unwrap() == mov.dest()) {
            return false;
        }

        let board_piece = pos.piece_at_sq(mov.orig()).type_of();
        if !(board_piece == self.piece_type) {
            return false;
        }

        // We don't check that these actually match, because we want to allow a situation where the
        // user omits the 'x' capture character. So if the move is really a capture but the
        // notation says 'Be3', this should actually be considered valid.
        //
        // On the other hand, if the user writes 'x' but it isn't a capture, then we want this to
        // fail, because they actively asserted that it's a capture when it isn't.
        if self.is_capture && !mov.is_capture() {
            return false;
        }

        if let Some(f) = self.from_file {
            if mov.orig().file() != f as u8 {
                return false;
            }
        }

        if let Some(r) = self.from_rank {
            if mov.orig().rank() != r as u8 {
                return false;
            }
        }

        if !(self.promo_piece == mov.promo_piece_type()) {
            return false;
        }

        true
    }
}

struct SanParser<'a> {
    san: Peekable<Graphemes<'a>>,
    piece_type: Option<PieceType>,
    from_file: Option<usize>,
    from_rank: Option<usize>,
    to_square: Option<Square>,
    is_capture: bool,
    promo_piece: Option<PieceType>,
    is_ks_castle: bool,
    is_qs_castle: bool,
}

impl<'a> SanParser<'a> {
    fn parse(san: &'a str) -> PResult<MoveDetails> {
        let mut parser = Self::new(san);

        match san {
            "O-O" | "o-o" => {
                parser.is_ks_castle = true;
                parser.piece_type = Some(PieceType::King);
            }
            "O-O-O" | "o-o-o" => {
                parser.is_qs_castle = true;
                parser.piece_type = Some(PieceType::King);
            }
            _ => parser.parse_san()?,
        }

        Ok(parser.finish())
    }

    fn new(san: &'a str) -> Self {
        Self {
            san: UnicodeSegmentation::graphemes(san, true).peekable(),
            piece_type: None,
            from_file: None,
            from_rank: None,
            to_square: None,
            is_capture: false,
            promo_piece: None,
            is_ks_castle: false,
            is_qs_castle: false,
        }
    }

    fn parse_san(&mut self) -> PResult<()> {
        self.parse_piece_type()
    }

    fn finish(&mut self) -> MoveDetails {
        MoveDetails {
            piece_type: self.piece_type.unwrap(),
            from_file: self.from_file,
            from_rank: self.from_rank,
            to_square: self.to_square,
            is_capture: self.is_capture,
            promo_piece: self.promo_piece,
            is_ks_castle: self.is_ks_castle,
            is_qs_castle: self.is_qs_castle,
        }
    }

    fn parse_piece_type(&mut self) -> PResult<()> {
        if Self::is_file(self.peek().ok_or(ParseError::UnexpectedEndOfString)?) {
            // if this happens, then we should have a pawn move; continue to next step
            self.piece_type = Some(PieceType::Pawn);
        } else {
            let piece_type = match self.eat()? {
                "R" => PieceType::Rook,
                "N" => PieceType::Knight,
                "B" => PieceType::Bishop,
                "Q" => PieceType::Queen,
                "K" => PieceType::King,
                "P" => PieceType::Pawn,
                _ => {
                    return Err(ParseError::InvalidPieceType);
                }
            };

            self.piece_type = Some(piece_type);
        }
        return self.parse_remainder_after_piece();
    }

    fn parse_remainder_after_piece(&mut self) -> PResult<()> {
        if self.parse_orig_file().is_err() {
            // we had no file identifier immediately after the piece, which can only occur if we
            // have a rank identifier as part of disambiguating a move
            self.parse_orig_rank()?;
            self.parse_middle()
        } else {
            // we successfully parsed a file, but we still don't know whether we are looking at the
            // origin or destination
            if self.parse_orig_rank().is_err() {
                // we didn't get a rank character after the file character, so the file character
                // was a disambiguator; we can continue to parse the middle
                self.parse_middle()
            } else {
                // we did get a rank character after the file character, so we _still_ don't know
                // if this was an origin square or a destination square
                if self.did_parse_dest()? {
                    // we now know that the square we just parsed was the destination, so we can
                    // transfer the details and continue onwards to parse for promotions
                    let file = self.from_file.expect("error parsing destination square");
                    let rank = self.from_rank.expect("error parsing destination square");
                    self.from_file = None;
                    self.from_rank = None;
                    self.to_square = Some(Square::from_rank_file(rank, file));

                    self.parse_promo()
                } else {
                    // we now know that the square we just parsed was the origin square, so we can
                    // proceed to parse the middle
                    self.parse_middle()
                }
            }
        }
    }

    fn parse_middle(&mut self) -> PResult<()> {
        // here, we parse potential characters in the middle of the string, such as an optional
        // hyphen "-", or a capture identifier "x"
        let middle_char = self.peek().ok_or(ParseError::UnexpectedEndOfString)?;
        match middle_char {
            "-" => {
                self.eat()?;
            }
            "x" => {
                self.eat()?;
                self.is_capture = true;
            }
            _ => {}
        }

        self.parse_dest()
    }

    fn parse_dest(&mut self) -> PResult<()> {
        // we only arrive here if it is known for certain that we expect to parse a destination
        // square next

        let file = self.parse_file()?;
        let rank = self.parse_rank()?;
        self.to_square = Some(Square::from_rank_file(rank, file));

        self.parse_promo()
    }

    fn parse_promo(&mut self) -> PResult<()> {
        // here, we parse for a possible "=" and then for a valid promotion piece type
        // if we end up finding this stuff, we bail immediately with an error if we didn't already
        // determine that the moving piece is a pawn
        match self.peek() {
            Some(c) => match c {
                "=" => {
                    // note: this theoretically means that a promotion string with loads of equals
                    // signs would parse correctly, e.g. e8========Q; ...this is probably fine
                    self.eat()?;
                    self.parse_promo()
                }
                "Q" | "R" | "N" | "B" => {
                    if self.piece_type != Some(PieceType::Pawn) {
                        Err(ParseError::InvalidPromotionOnNonPawnMove)
                    } else {
                        self.eat()?;
                        self.promo_piece = Some(Self::promo_piece_from_letter(c));
                        self.parse_check_markers()
                    }
                }
                _ => self.parse_check_markers(),
            },
            None => self.parse_check_markers(),
        }
    }

    fn promo_piece_from_letter(c: &str) -> PieceType {
        match c {
            "Q" => PieceType::Queen,
            "R" => PieceType::Rook,
            "B" => PieceType::Bishop,
            "N" => PieceType::Knight,
            _ => unreachable!(),
        }
    }

    fn parse_check_markers(&mut self) -> PResult<()> {
        // here, we just check to see if the move was marked as one of "+" or "#" or some
        // combination of "!" / "?" characters before ending

        match self.eat() {
            Ok(c) => match c {
                "+" | "#" => self.parse_annotations(),
                _ => Err(ParseError::Invalid),
            },
            Err(_) => Ok(()),
        }
    }

    fn parse_annotations(&mut self) -> PResult<()> {
        match self.eat() {
            Ok(c) => match c {
                "!" | "?" => self.parse_annotations(),
                _ => Err(ParseError::Invalid),
            },
            Err(_) => Ok(()),
        }
    }

    fn parse_orig_file(&mut self) -> PResult<()> {
        match self.peek() {
            Some(c) => {
                if Self::is_file(c) {
                    let file = self.parse_file()?;
                    self.from_file = Some(file);
                    Ok(())
                } else {
                    Err(ParseError::ExpectedFile)
                }
            }
            None => Err(ParseError::UnexpectedEndOfString),
        }
    }

    fn parse_orig_rank(&mut self) -> PResult<()> {
        match self.peek() {
            Some(c) => {
                if Self::is_rank(c) {
                    let rank = self.parse_rank()?;
                    self.from_rank = Some(rank);
                    Ok(())
                } else {
                    Err(ParseError::ExpectedRank)
                }
            }
            None => Err(ParseError::UnexpectedEndOfString),
        }
    }

    fn parse_file(&mut self) -> PResult<usize> {
        match self.eat()? {
            "a" => Ok(0),
            "b" => Ok(1),
            "c" => Ok(2),
            "d" => Ok(3),
            "e" => Ok(4),
            "f" => Ok(5),
            "g" => Ok(6),
            "h" => Ok(7),
            _ => Err(ParseError::ExpectedFile),
        }
    }

    fn parse_rank(&mut self) -> PResult<usize> {
        match self.eat()? {
            "1" => Ok(0),
            "2" => Ok(1),
            "3" => Ok(2),
            "4" => Ok(3),
            "5" => Ok(4),
            "6" => Ok(5),
            "7" => Ok(6),
            "8" => Ok(7),
            _ => Err(ParseError::ExpectedRank),
        }
    }

    fn did_parse_dest(&mut self) -> PResult<bool> {
        // We invoke this after successfully parsing a piece type and then immediately afterwards,
        // a file and a rank. It's possible that we only had an origin square and there's more to
        // come, so we have to disambiguate that. In most case, we just parsed something like "Be3"
        // and the square was the destination.
        //
        // We peek the next character:
        // 1) If it's "+" or "#", then it was the destination.
        // 2) If it's "=" then it looks like a promotion move, so it was the destination
        // 3) If it's a "Q", "R", "B", "N" then it looks like a promotion move, so it was
        //    the destination
        // 4) If it was a hyphen "-" then it was the origin square
        // 5) If it was another file then we had an origin square
        // 6) If it was end of string, then it was the destination square
        // 7) If it was anything else then it's an error
        match self.peek() {
            Some("+") | Some("#") | Some("=") => Ok(true),
            Some("Q") | Some("R") | Some("B") | Some("N") => Ok(true),
            Some("-") => Ok(false),
            Some(c) => {
                if SanParser::is_file(c) {
                    Ok(false)
                } else {
                    // The next character was somethign other than "+", "#" or a file,
                    // which doesn't make sense.
                    return Err(ParseError::Invalid);
                }
            }
            None => Ok(true),
        }
    }

    fn eat(&mut self) -> PResult<&'a str> {
        self.san.next().ok_or(ParseError::UnexpectedEndOfString)
    }

    fn peek(&mut self) -> Option<&'a str> {
        self.san.peek().map(|s| *s)
    }

    fn is_file(c: &str) -> bool {
        match c {
            "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" => true,
            _ => false,
        }
    }

    fn is_rank(c: &str) -> bool {
        match c {
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" => true,
            _ => false,
        }
    }
}

impl Position {
    /// Determine the `Move` which corresponds to the passed string in the current position.
    ///
    /// This function is deliberately designed to be somewhat forgiving of lax notation. For
    /// example, the passed string does not need to accurately report check ('+') or checkmate ('#')
    /// for the `Move` to be correctly identified.
    ///
    /// Additionally, if the string is overspecified (e.g. `Nd4e4` instead of `Ne4` when there is
    /// no ambiguity) we correctly identify the move.
    ///
    /// However, ambiguity is not allowed. If more than one move in the position could conceivably
    /// match the string, then `None` is returned.
    ///
    /// If no move is found which matches the string, `None` is returned.
    pub fn move_from_san(&self, mov: &str) -> Option<Move> {
        // Algorithm:
        //
        // 1. Parse the following from the text: (Piece, FileFrom, RankFrom, SquareTo, IsCapture,
        //    PromoPiece)
        // 2. Generate the legal moves in the position
        // 3. Iterate the legal moves, and test if any of them match
        //    - here, we cannot return early as soon as we find something which matches, because
        //    there may be ambiguity and another move will also match
        //    - we have to continue iterating and if we find a second move which matches, we return
        //    `None`; if we reach the end of the list and only one matched, then we return that.
        match SanParser::parse(mov) {
            Ok(move_details) => {
                let legal_moves = self.generate::<BasicMoveList, All, Legal>();
                let mut matched: bool = false;
                let mut res: Option<Move> = None;
                for mov in &legal_moves {
                    if move_details.matches(self, *mov) {
                        if !matched {
                            matched = true;
                            res = Some(*mov);
                        } else {
                            return None;
                        }
                    }
                }
                res
            }
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::init::init_globals;
    use crate::position::Position;

    #[test]
    fn basic_moves_in_startpos() {
        init_globals();

        let pos = Position::start_pos();

        assert!(pos.move_from_san("e4").is_some());
        assert!(pos.move_from_san("d4").is_some());
        assert!(pos.move_from_san("Nf3").is_some());
        assert!(pos.move_from_san("Nb1-a3").is_some());
        assert!(pos.move_from_san("Nb1c3").is_some());
        assert!(!pos.move_from_san("e5").is_some());
    }

    #[test]
    fn weird_position() {
        init_globals();

        let pos =
            Position::from_fen("rnbqkbnr/pPpppp1p/8/5Pp1/Q6Q/8/P1P1P1PP/QNB1KBNR w Kkq g6 0 1")
                .unwrap();

        // Promotions
        assert!(pos.move_from_san("bxa8R").is_some());
        assert!(pos.move_from_san("bxa8=R").is_some());
        assert!(pos.move_from_san("bxa8=N").is_some());
        assert!(pos.move_from_san("ba8B").is_some());
        assert!(pos.move_from_san("bxa8Q").is_some());
        assert!(pos.move_from_san("bxc8Q").is_some());
        assert!(pos.move_from_san("bxc8=Q").is_some());
        assert!(pos.move_from_san("bxc8=R").is_some());
        assert!(pos.move_from_san("bxc8=B").is_some());
        assert!(pos.move_from_san("bxc8=N").is_some());
        assert!(pos.move_from_san("bxc8=b").is_none());

        // Disambiguations
        assert!(pos.move_from_san("Qd4").is_none());
        assert!(pos.move_from_san("Qad4").is_none());
        assert!(pos.move_from_san("Q1d4").is_some());
        assert!(pos.move_from_san("Qhd4").is_some());
        assert!(pos.move_from_san("Qa1d4").is_some());
        assert!(pos.move_from_san("Qa4d4").is_some());
        assert!(pos.move_from_san("Qg4").is_none());
        assert!(pos.move_from_san("Qag4").is_some());

        // En passant
        assert!(pos.move_from_san("fxg6").is_some());
        assert!(pos.move_from_san("fxg5").is_none());
    }
}
