use super::{Board, CastlingRights, Piece, Player, Position, Square, State, Zobrist};

use crate::bb::Bitboard;

pub const START_POSITION: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Debug)]
pub struct FenError {
    ty: FenErrorType,
    pub msg: String,
}

impl std::fmt::Display for FenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.ty, self.msg)
    }
}

#[derive(Debug)]
pub enum FenErrorType {
    IncorrectNumberOfFields,
    SideToMoveInvalid,
    MoveNumberFieldNotPositiveInteger,
    HalfMoveCounterNotNonNegInteger,
    EnPassantSquareInvalid,
    CastlingRightsInvalid,
    PiecePositionsNotEightRows,
    PiecePositionsConsecutiveNumbers,
    PiecePositionsInvalidPiece,
    PiecePositionsInvalidNumber,
    PiecePositionsRowTooLong,
    PiecePositionsRowTooShort,
    PiecePositionsInvalidKings,
}

impl std::fmt::Display for FenErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FenErrorType::IncorrectNumberOfFields => write!(f, "incorrect number of fields"),
            FenErrorType::SideToMoveInvalid => write!(f, "side to move invalid"),
            FenErrorType::MoveNumberFieldNotPositiveInteger => {
                write!(f, "move number field not positive integer")
            }
            FenErrorType::HalfMoveCounterNotNonNegInteger => {
                write!(f, "half move counter not a non-negative integer")
            }
            FenErrorType::EnPassantSquareInvalid => write!(f, "en passant square invalid"),
            FenErrorType::CastlingRightsInvalid => write!(f, "castling rights invalid"),
            FenErrorType::PiecePositionsNotEightRows => {
                write!(f, "piece positions string does not have eight ranks")
            }
            FenErrorType::PiecePositionsConsecutiveNumbers => {
                write!(f, "piece positions consecutive numbers")
            }
            FenErrorType::PiecePositionsInvalidPiece => {
                write!(f, "piece positions has an invalid piece")
            }
            FenErrorType::PiecePositionsInvalidNumber => {
                write!(f, "piece positions has invalid number")
            }
            FenErrorType::PiecePositionsRowTooLong => write!(f, "piece positions row is too long"),
            FenErrorType::PiecePositionsRowTooShort => {
                write!(f, "piece positions row is too short")
            }
            FenErrorType::PiecePositionsInvalidKings => {
                write!(f, "piece positions must contain exactly one king per side")
            }
        }
    }
}

impl Position {
    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        let [piece_positions, side_to_move, castling_rights, ep_square, half_move_clock, move_number] =
            Self::split_fen_fields(fen)?;

        let (bbs, player_occ, board) = Self::parse_piece_position_string(piece_positions)?;
        let turn = Self::parse_side_to_move(side_to_move)?;
        let castling_rights = Self::parse_castling_rights(castling_rights)?;
        let ep_square = Self::parse_ep_square(ep_square)?;
        let half_move_clock = Self::parse_half_move_clock(half_move_clock)?;
        let move_number = Self::parse_move_number(move_number)?;

        let mut pos = Self {
            board,
            turn,
            castling_rights,
            ep_square,
            half_move_clock,
            move_number,
            bbs,
            player_occ,
            state: State::blank(), // Temporary. The real `State` is generated below.
            history: Vec::new(),
            zobrist: Zobrist(0),
        };

        pos.set_state();
        pos.canonicalize_ep_square();
        pos.set_zobrist();

        Ok(pos)
    }

    /// Drops an en-passant target that no legal move can use.
    ///
    /// `make_move_unchecked` only records an en-passant square when the capture is genuinely
    /// available, but FEN input is accepted verbatim, and the notation is routinely written with
    /// the target filled in after every double push regardless. Without this, a position parsed
    /// from such a FEN and the identical position reached by playing moves hash differently,
    /// splitting one position across two transposition-table identities (TASK-58).
    ///
    /// Both paths go through the same predicate deliberately. Canonicalizing here against a weaker
    /// test than the one `make_move_unchecked` applies would reopen the very split this closes.
    ///
    /// This must run before `set_zobrist`, so that the canonical square is the one hashed.
    fn canonicalize_ep_square(&mut self) {
        let Some(ep) = self.ep_square else {
            return;
        };

        // `turn` is the side that may capture; the pawn that double-pushed belongs to the other.
        if !self.has_legal_ep_capture(ep, self.turn) {
            self.ep_square = None;
        }
    }

    pub fn start_pos() -> Self {
        Self::from_fen(START_POSITION).unwrap()
    }

    pub fn split_fen_fields(fen: &str) -> Result<[&str; 6], FenError> {
        let fields: Vec<&str> = fen.split(' ').collect();
        if fields.len() != 6 {
            Err(FenError {
                ty: FenErrorType::IncorrectNumberOfFields,
                msg: format!(
                    "{} space-delimited fields in fen string; expected 6",
                    fields.len()
                ),
            })
        } else {
            Ok([
                fields[0], fields[1], fields[2], fields[3], fields[4], fields[5],
            ])
        }
    }

    pub fn parse_piece_position_string(
        piece_positions: &str,
    ) -> Result<([Bitboard; 13], [Bitboard; 2], Board), FenError> {
        let mut board: [Piece; 64] = [Piece::None; 64];
        let mut white_pawns: Bitboard = Bitboard::new(0);
        let mut white_knights: Bitboard = Bitboard::new(0);
        let mut white_bishops: Bitboard = Bitboard::new(0);
        let mut white_rooks: Bitboard = Bitboard::new(0);
        let mut white_queens: Bitboard = Bitboard::new(0);
        let mut white_king: Bitboard = Bitboard::new(0);
        let mut black_pawns: Bitboard = Bitboard::new(0);
        let mut black_knights: Bitboard = Bitboard::new(0);
        let mut black_bishops: Bitboard = Bitboard::new(0);
        let mut black_rooks: Bitboard = Bitboard::new(0);
        let mut black_queens: Bitboard = Bitboard::new(0);
        let mut black_king: Bitboard = Bitboard::new(0);

        let rows: Vec<&str> = piece_positions.split('/').collect();
        if rows.len() != 8 {
            return Err(FenError {
                ty: FenErrorType::PiecePositionsNotEightRows,
                msg: format!("fen string has {} rows; expected 8", rows.len()),
            });
        }

        let mut file_counter = 0;
        let mut rank_counter = 0;
        let mut last_was_number = false;

        for c in piece_positions.chars() {
            if file_counter >= 8 {
                match c {
                    '/' if file_counter == 8 => {
                        last_was_number = false;
                        file_counter = 0;
                        rank_counter += 1;
                        continue;
                    }
                    _ => {
                        return Err(FenError {
                            ty: FenErrorType::PiecePositionsRowTooLong,
                            msg: format!(
                                "row {} ({}) too long",
                                rank_counter, rows[rank_counter as usize]
                            ),
                        });
                    }
                }
            }

            let idx = rank_file_to_idx(rank_counter, file_counter);
            match c {
                'P' => {
                    white_pawns |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::WhitePawn;
                    last_was_number = false;
                    file_counter += 1;
                }
                'N' => {
                    white_knights |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::WhiteKnight;
                    last_was_number = false;
                    file_counter += 1;
                }
                'B' => {
                    white_bishops |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::WhiteBishop;
                    last_was_number = false;
                    file_counter += 1;
                }
                'R' => {
                    white_rooks |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::WhiteRook;
                    last_was_number = false;
                    file_counter += 1;
                }
                'Q' => {
                    white_queens |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::WhiteQueen;
                    last_was_number = false;
                    file_counter += 1;
                }
                'K' => {
                    white_king |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::WhiteKing;
                    last_was_number = false;
                    file_counter += 1;
                }
                'p' => {
                    black_pawns |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::BlackPawn;
                    last_was_number = false;
                    file_counter += 1;
                }
                'n' => {
                    black_knights |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::BlackKnight;
                    last_was_number = false;
                    file_counter += 1;
                }
                'b' => {
                    black_bishops |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::BlackBishop;
                    last_was_number = false;
                    file_counter += 1;
                }
                'r' => {
                    black_rooks |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::BlackRook;
                    last_was_number = false;
                    file_counter += 1;
                }
                'q' => {
                    black_queens |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::BlackQueen;
                    last_was_number = false;
                    file_counter += 1;
                }
                'k' => {
                    black_king |= Bitboard::from_sq_idx(idx);
                    board[idx as usize] = Piece::BlackKing;
                    last_was_number = false;
                    file_counter += 1;
                }
                '/' => {
                    // `/` should only appear when expected at the end of row, and is dealt
                    // with separately above. PiecePositionsRowTooLongan error.
                    return Err(FenError {
                        ty: FenErrorType::PiecePositionsRowTooShort,
                        msg: format!(
                            "row {} ({}) too short; represents {} files - expected 8",
                            rank_counter, rows[rank_counter as usize], file_counter
                        ),
                    });
                }

                '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' => {
                    if last_was_number {
                        return Err(FenError {
                            ty: FenErrorType::PiecePositionsConsecutiveNumbers,
                            msg: "fen string contained consecutive numbers".to_string(),
                        });
                    } else {
                        let skip = c.to_digit(10).unwrap_or_else(|| panic!("{} matched as a number char ('1' to '8'), but didn't parse to a number",
                            c));

                        if !(1..=8).contains(&skip) {
                            return Err(FenError {
                                ty: FenErrorType::PiecePositionsInvalidNumber,
                                msg: format!("invalid number {} found in piece position string; should be between 1-8", skip),
                            });
                        }

                        last_was_number = true;
                        file_counter += skip as u8;
                    }
                }
                _ => {
                    return Err(FenError {
                        ty: FenErrorType::PiecePositionsInvalidPiece,
                        msg: format!("unexpected character: {} in fen string", c),
                    })
                }
            }
        }

        if file_counter != 8 {
            return Err(FenError {
                ty: if file_counter < 8 {
                    FenErrorType::PiecePositionsRowTooShort
                } else {
                    FenErrorType::PiecePositionsRowTooLong
                },
                msg: format!(
                    "row {} ({}) represents {} files; expected 8",
                    rank_counter, rows[rank_counter as usize], file_counter
                ),
            });
        }

        if white_king.popcnt() != 1 || black_king.popcnt() != 1 {
            return Err(FenError {
                ty: FenErrorType::PiecePositionsInvalidKings,
                msg: format!(
                    "found {} white kings and {} black kings; expected exactly one of each",
                    white_king.popcnt(),
                    black_king.popcnt()
                ),
            });
        }

        let white_pieces =
            white_pawns | white_knights | white_bishops | white_rooks | white_queens | white_king;
        let black_pieces =
            black_pawns | black_knights | black_bishops | black_rooks | black_queens | black_king;
        let no_piece = !(white_pieces | black_pieces);

        Ok((
            [
                no_piece,
                white_pawns,
                white_knights,
                white_bishops,
                white_rooks,
                white_queens,
                white_king,
                black_pawns,
                black_knights,
                black_bishops,
                black_rooks,
                black_queens,
                black_king,
            ],
            [white_pieces, black_pieces],
            Board::from_array(board),
        ))
    }

    fn parse_side_to_move(side_to_move: &str) -> Result<Player, FenError> {
        match side_to_move {
            "w" => Ok(Player::WHITE),
            "b" => Ok(Player::BLACK),
            _ => Err(FenError {
                ty: FenErrorType::SideToMoveInvalid,
                msg: format!(
                    "{} is an invalid string for the side to move; should be `w` or `b`",
                    side_to_move
                ),
            }),
        }
    }

    fn parse_castling_rights(castling_rights: &str) -> Result<CastlingRights, FenError> {
        if castling_rights.len() > 4 {
            return Err(FenError {
                ty: FenErrorType::CastlingRightsInvalid,
                msg: format!(
                    "castling rights field ({}) invalid; should contain 4 flags or fewer",
                    castling_rights
                ),
            });
        }

        if castling_rights == "-" {
            return Ok(CastlingRights::empty());
        }

        let mut white_kingside = false;
        let mut white_queenside = false;
        let mut black_kingside = false;
        let mut black_queenside = false;

        for c in castling_rights.chars() {
            match c {
                'K' => {
                    if white_kingside {
                        return Err(FenError {
                            ty: FenErrorType::CastlingRightsInvalid,
                            msg: "invalid castling rights; white kingside castling was set more than once".to_string(),
                        });
                    } else {
                        white_kingside = true;
                    }
                }
                'Q' => {
                    if white_queenside {
                        return Err(FenError {
                            ty: FenErrorType::CastlingRightsInvalid,
                            msg: "invalid castling rights; white queenside castling was set more than once".to_string(),
                    });
                    } else {
                        white_queenside = true;
                    }
                }
                'k' => {
                    if black_kingside {
                        return Err(FenError {
                            ty: FenErrorType::CastlingRightsInvalid,
                            msg: "invalid castling rights; black kingside castling was set more than once".to_string(),
                        });
                    } else {
                        black_kingside = true;
                    }
                }
                'q' => {
                    if black_queenside {
                        return Err(FenError {
                            ty: FenErrorType::CastlingRightsInvalid,
                            msg: "invalid castling rights; black queenside castling was set more than once".to_string(),
                        });
                    } else {
                        black_queenside = true;
                    }
                }
                _ => {
                    return Err(FenError {
                        ty: FenErrorType::CastlingRightsInvalid,
                        msg: format!("unexpected character {} in castling rights field", c),
                    })
                }
            }
        }

        Ok(CastlingRights::new(
            white_kingside,
            white_queenside,
            black_kingside,
            black_queenside,
        ))
    }

    fn parse_ep_square(ep_square: &str) -> Result<Option<Square>, FenError> {
        if ep_square == "-" {
            return Ok(None);
        }

        if ep_square.len() != 2 {
            return Err(FenError {
                ty: FenErrorType::EnPassantSquareInvalid,
                msg: format!(
                    "`{}` not a valid square name for the en passant square",
                    ep_square
                ),
            });
        }

        // TODO: can also run a check to ensure that the en passant square reconciles with the
        // side to move

        match ep_square {
            "a3" => Ok(Some(Square(16))),
            "b3" => Ok(Some(Square(17))),
            "c3" => Ok(Some(Square(18))),
            "d3" => Ok(Some(Square(19))),
            "e3" => Ok(Some(Square(20))),
            "f3" => Ok(Some(Square(21))),
            "g3" => Ok(Some(Square(22))),
            "h3" => Ok(Some(Square(23))),
            "a6" => Ok(Some(Square(40))),
            "b6" => Ok(Some(Square(41))),
            "c6" => Ok(Some(Square(42))),
            "d6" => Ok(Some(Square(43))),
            "e6" => Ok(Some(Square(44))),
            "f6" => Ok(Some(Square(45))),
            "g6" => Ok(Some(Square(46))),
            "h6" => Ok(Some(Square(47))),
            _ => Err(FenError {
                ty: FenErrorType::EnPassantSquareInvalid,
                msg: format!("invalid en passant square `{}`; must be a valid algebraic notation square on the 3rd or 6th rank", ep_square),
            })
        }
    }

    fn parse_half_move_clock(hmc: &str) -> Result<u32, FenError> {
        match hmc.to_string().parse::<i32>() {
            Ok(i) => {
                if i < 0 {
                    Err(FenError {
                        ty: FenErrorType::HalfMoveCounterNotNonNegInteger,
                        msg: format!(
                            "half move clock should be a non-negative integer; found {}",
                            hmc
                        ),
                    })
                } else {
                    Ok(i as u32)
                }
            }
            Err(err) => Err(FenError {
                ty: FenErrorType::HalfMoveCounterNotNonNegInteger,
                msg: format!("half move clock value of `{}` is invalid; {}", hmc, err),
            }),
        }
    }

    fn parse_move_number(mn: &str) -> Result<u32, FenError> {
        match mn.to_string().parse::<i32>() {
            Ok(i) => {
                if i < 1 {
                    Err(FenError {
                        ty: FenErrorType::MoveNumberFieldNotPositiveInteger,
                        msg: format!("move number should be a positive integer; found {}", mn),
                    })
                } else {
                    Ok(i as u32)
                }
            }
            Err(err) => Err(FenError {
                ty: FenErrorType::HalfMoveCounterNotNonNegInteger,
                msg: format!("move number value of `{}` is invalid; {}", mn, err),
            }),
        }
    }

    pub fn to_fen(&self) -> String {
        let mut s = String::with_capacity(120);

        // 1. Board
        let mut squares: [[Piece; 8]; 8] = [[Piece::None; 8]; 8];

        for i in 0..64 {
            let rank = i / 8;
            let file = i % 8;
            squares[rank][file] = self.board.arr[i]
        }

        let mut board_rows: Vec<String> = Vec::with_capacity(8);

        for row in squares.iter().rev() {
            let mut fen_row = String::with_capacity(8);
            let mut c = 0; // counter of consecutive empty squares

            for square in row.iter() {
                if square.is_none() {
                    c += 1;
                } else {
                    if c != 0 {
                        fen_row.push_str(&format!("{}", c));
                    }

                    fen_row.push_str(&format!("{}", square));
                    c = 0;
                }
            }

            if c != 0 {
                fen_row.push_str(&format!("{}", c));
            }

            board_rows.push(fen_row);
        }

        s.push_str(&board_rows.join("/"));
        s.push(' ');

        // 2. Turn
        s.push_str(match self.turn() {
            Player::WHITE => "w",
            Player::BLACK => "b",
        });
        s.push(' ');

        // 3. Castling rights
        s.push_str(&format!("{}", self.castling_rights()));
        s.push(' ');

        // 4. En passant square
        match self.ep_square {
            Some(ep) => s.push_str(&format!("{}", ep)),
            None => s.push('-'),
        }
        s.push(' ');

        // 5. Halfmove clock
        s.push_str(&format!("{}", self.half_move_clock));
        s.push(' ');

        // 6. Fullmove number
        s.push_str(&format!("{}", self.move_number));

        s
    }
}

fn rank_file_to_idx(rank: u32, file: u8) -> u8 {
    // `rank` and `file` here have indices based on iterating through the
    // fen string, so `rank` = 0 means the rank usually labelled as 8 in
    // algebraic chess notation
    (7 - rank as u8) * 8 + file
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::init_globals;
    use crate::mono_traits::{All as AllGen, Legal};
    use crate::movelist::BasicMoveList;

    fn assert_invalid_fen(piece_positions: &str) {
        let fen = format!("{piece_positions} w - - 0 1");
        let result = std::panic::catch_unwind(|| Position::from_fen(&fen));

        assert!(result.is_ok(), "invalid FEN panicked: {fen}");
        assert!(result.unwrap().is_err(), "invalid FEN was accepted: {fen}");
    }

    #[test]
    fn rejects_short_and_long_final_ranks() {
        assert_invalid_fen("4k3/8/8/8/8/8/8/3K3");
        assert_invalid_fen("4k3/8/8/8/8/8/8/4K4");
    }

    #[test]
    fn rejects_empty_and_missing_king_boards() {
        assert_invalid_fen("8/8/8/8/8/8/8/8");
        assert_invalid_fen("4k3/8/8/8/8/8/8/8");
        assert_invalid_fen("8/8/8/8/8/8/8/4K3");
    }

    #[test]
    fn rejects_duplicate_kings() {
        assert_invalid_fen("4k3/8/8/8/8/8/4K3/4K3");
        assert_invalid_fen("4k3/4k3/8/8/8/8/8/4K3");
    }

    #[test]
    fn accepts_exactly_one_king_per_side() {
        init_globals();

        assert!(Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").is_ok());
    }

    /// AC#3. An en-passant target that no enemy pawn can capture onto cannot affect any legal move,
    /// so it must not split the position into a second transposition identity (TASK-58).
    #[test]
    fn an_uncapturable_en_passant_target_is_dropped() {
        init_globals();

        // White's only pawn is on a2, nowhere near the d6 target, so no capture onto d6 exists.
        let pos = Position::from_fen("4k3/8/8/3p4/8/8/P7/4K3 w - d6 0 1").unwrap();
        assert_eq!(pos.ep_square(), None);

        // The same board written without the redundant target must be the identical position.
        let without = Position::from_fen("4k3/8/8/3p4/8/8/P7/4K3 w - - 0 1").unwrap();
        assert_eq!(pos.zobrist(), without.zobrist());
    }

    /// AC#3. A target a pawn can actually capture onto changes the legal moves, so it must remain a
    /// distinguishing part of the position's identity.
    #[test]
    fn a_capturable_en_passant_target_is_retained_and_distinguishing() {
        init_globals();

        // The white e5 pawn attacks d6, so the target is legally relevant.
        let with = Position::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").unwrap();
        assert_eq!(with.ep_square(), Some(Square(43)));

        let without = Position::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(without.ep_square(), None);

        assert_ne!(
            with.zobrist(),
            without.zobrist(),
            "a usable en-passant right must remain distinguishable"
        );
    }

    /// AC#3. The canonical identity is the one `make_move_unchecked` produces. A FEN that names the
    /// target after a double push must hash the same as the position reached by playing that push.
    #[test]
    fn a_parsed_position_hashes_the_same_as_the_played_one() {
        init_globals();

        for (before, after_with_ep, ep_is_usable) in [
            // Black has no pawn able to take on e3, so the notated target is redundant.
            (
                "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",
                "4k3/8/8/8/4P3/8/8/4K3 b - e3 0 1",
                false,
            ),
            // Black's d4 pawn attacks e3, so the target is a real capturing right.
            (
                "4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1",
                "4k3/8/8/8/3pP3/8/8/4K3 b - e3 0 1",
                true,
            ),
        ] {
            let mut played = Position::from_fen(before).unwrap();
            let moves = played.generate::<BasicMoveList, AllGen, Legal>();
            let double_push = moves
                .iter()
                .find(|m| format!("{m}").contains("e2e4"))
                .copied()
                .expect("the double push must be available");
            played.make_move(&double_push);

            let parsed = Position::from_fen(after_with_ep).unwrap();

            assert_eq!(
                parsed.ep_square().is_some(),
                ep_is_usable,
                "canonical en-passant square wrong for {after_with_ep}"
            );
            assert_eq!(
                played.ep_square(),
                parsed.ep_square(),
                "en-passant square disagreed for {after_with_ep}"
            );
            assert_eq!(
                played.zobrist(),
                parsed.zobrist(),
                "played and parsed positions hashed differently for {after_with_ep}"
            );
        }
    }

    /// AC#3, REV-1-02. A pawn attacking the target is not enough to make the capture available. If
    /// the only capturer is pinned, or the capture would expose its own king, no legal move can use
    /// the target, so it must not split the position's identity.
    #[test]
    fn an_en_passant_target_no_legal_move_can_use_is_dropped() {
        init_globals();

        for (with_ep, without_ep, why) in [
            // The e5 pawn attacks d6, but taking clears the e-file and the e8 rook checks Ke1.
            (
                "k3r3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
                "k3r3/8/8/3pP3/8/8/8/4K3 w - - 0 1",
                "capturing exposes the king along the e-file",
            ),
            // The d5 pawn attacks e6, but it is pinned against Kd1 by the d8 rook.
            (
                "k2r4/8/8/3Pp3/8/8/8/3K4 w - e6 0 1",
                "k2r4/8/8/3Pp3/8/8/8/3K4 w - - 0 1",
                "the only capturer is pinned on the d-file",
            ),
        ] {
            let with = Position::from_fen(with_ep).unwrap();
            let without = Position::from_fen(without_ep).unwrap();

            assert_eq!(
                with.ep_square(),
                None,
                "target should have been dropped because {why}"
            );
            assert_eq!(
                with.zobrist(),
                without.zobrist(),
                "a target no legal move can use must not split identity ({why})"
            );

            // The predicate must agree with the moves actually generated, or the canonical identity
            // would disagree with the position's real capturing rights.
            let ep_moves = without
                .generate::<BasicMoveList, AllGen, Legal>()
                .iter()
                .filter(|m| m.is_en_passant())
                .count();
            assert_eq!(
                ep_moves, 0,
                "no legal en-passant capture should exist ({why})"
            );
        }
    }

    /// AC#3. FEN input can name a target with no double-pushed pawn behind it. There is nothing to
    /// capture, so the target must be dropped rather than hashed.
    #[test]
    fn an_en_passant_target_with_no_pawn_behind_it_is_dropped() {
        init_globals();

        // e5 attacks d6, but d5 is empty, so the named target describes a capture of nothing.
        let with = Position::from_fen("4k3/8/8/4P3/8/8/8/4K3 w - d6 0 1").unwrap();
        let without = Position::from_fen("4k3/8/8/4P3/8/8/8/4K3 w - - 0 1").unwrap();

        assert_eq!(with.ep_square(), None);
        assert_eq!(with.zobrist(), without.zobrist());
    }

    /// AC#3, REV-1-02. The played and parsed derivations must apply the *same* legality test. If
    /// only FEN input filtered illegal targets, a position reached by playing the double push would
    /// hash differently from the identical position parsed from FEN.
    #[test]
    fn a_played_double_push_drops_an_unusable_target_too() {
        init_globals();

        // Black plays d7d5. White's e5 pawn attacks d6, but exd6 would expose Ke1 to the e8 rook,
        // so the double push confers no en-passant right and must record no target.
        let mut played = Position::from_fen("k3r3/3p4/8/4P3/8/8/8/4K3 b - - 0 1").unwrap();
        let moves = played.generate::<BasicMoveList, AllGen, Legal>();
        let double_push = moves
            .iter()
            .find(|m| format!("{m}").contains("d7d5"))
            .copied()
            .expect("the double push must be available");
        played.make_move(&double_push);

        assert_eq!(
            played.ep_square(),
            None,
            "make_move recorded a target that no legal capture can use"
        );

        let parsed = Position::from_fen("k3r3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").unwrap();
        assert_eq!(
            played.zobrist(),
            parsed.zobrist(),
            "played and parsed positions hashed differently"
        );
    }
}
