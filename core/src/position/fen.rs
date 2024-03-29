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
        pos.set_zobrist();

        Ok(pos)
    }

    pub fn start_pos() -> Self {
        Self::from_fen(START_POSITION).unwrap()
    }

    pub fn split_fen_fields(fen: &str) -> Result<[&str; 6], FenError> {
        let fields: Vec<&str> = fen.split(' ').collect();
        if fields.len() != 6 {
            return Err(FenError {
                ty: FenErrorType::IncorrectNumberOfFields,
                msg: format!(
                    "{} space-delimited fields in fen string; expected 6",
                    fields.len()
                ),
            });
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
            if file_counter == 8 {
                match c {
                    '/' => {
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
                        let skip = c.to_digit(10).expect(&format!(
                            "{} matched as a number char ('1' to '8'), but didn't parse to a number",
                            c
                        ));

                        if skip < 1 || skip > 8 {
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
                    if white_kingside == true {
                        return Err(FenError {
                            ty: FenErrorType::CastlingRightsInvalid,
                            msg: format!("invalid castling rights; white kingside castling was set more than once"),
                        });
                    } else {
                        white_kingside = true;
                    }
                }
                'Q' => {
                    if white_queenside == true {
                        return Err(FenError {
                            ty: FenErrorType::CastlingRightsInvalid,
                            msg: format!("invalid castling rights; white queenside castling was set more than once"),
                    });
                    } else {
                        white_queenside = true;
                    }
                }
                'k' => {
                    if black_kingside == true {
                        return Err(FenError {
                            ty: FenErrorType::CastlingRightsInvalid,
                            msg: format!("invalid castling rights; black kingside castling was set more than once"),
                        });
                    } else {
                        black_kingside = true;
                    }
                }
                'q' => {
                    if black_queenside == true {
                        return Err(FenError {
                            ty: FenErrorType::CastlingRightsInvalid,
                            msg: format!("invalid castling rights; black queenside castling was set more than once"),
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
