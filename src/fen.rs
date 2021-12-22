use crate::bb::Bitboard;
use crate::position::{Board, CastlingRights, Color, Piece, Position, Square};
use std::fmt;

pub struct FenError {
    ty: FenErrorType,
    pub msg: String,
}

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
    PiecePositionsRowTooLarge,
}

impl Position {
    // TODO: remove this function once FEN parsing works properly
    pub fn new() -> Self {
        Self {
            board: Board::new(),
            turn: Color::White,
            castling_rights: CastlingRights::none(),
            ep_square: None,
            half_move_clock: 0,
            move_number: 0,
            no_piece: Bitboard::new(0xFFFFFFFFFFFFFFFF),
            white_pawns: Bitboard::new(0x0),
            white_knights: Bitboard::new(0x0),
            white_bishops: Bitboard::new(0x0),
            white_rooks: Bitboard::new(0x0),
            white_queens: Bitboard::new(0x0),
            white_king: Bitboard::new(0x0),
            black_pawns: Bitboard::new(0x0),
            black_knights: Bitboard::new(0x0),
            black_bishops: Bitboard::new(0x0),
            black_rooks: Bitboard::new(0x0),
            black_queens: Bitboard::new(0x0),
            black_king: Bitboard::new(0x0),
            white_pieces: Bitboard::new(0x0),
            black_pieces: Bitboard::new(0x0),
            white_piece_count: 0,
            black_piece_count: 0,
        }
    }

    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        let [piece_positions, side_to_move, castling_rights, ep_square, half_move_clock, move_number] =
            Self::split_fen_fields(fen)?;

        let (bbs, board) = Self::parse_piece_position_string(piece_positions)?;
        let turn = Self::parse_side_to_move(side_to_move)?;
        let castling_rights = Self::parse_castling_rights(castling_rights)?;
        let ep_square = Self::parse_ep_square(ep_square)?;
        let half_move_clock = Self::parse_half_move_clock(half_move_clock)?;
        let move_number = Self::parse_move_number(move_number)?;

        Ok(Self {
            board,
            turn,
            castling_rights,
            ep_square,
            half_move_clock,
            move_number,
            no_piece: bbs[0],
            white_pawns: bbs[1],
            white_knights: bbs[2],
            white_bishops: bbs[3],
            white_rooks: bbs[4],
            white_queens: bbs[5],
            white_king: bbs[6],
            black_pawns: bbs[7],
            black_knights: bbs[8],
            black_bishops: bbs[9],
            black_rooks: bbs[10],
            black_queens: bbs[11],
            black_king: bbs[12],
            white_pieces: bbs[13],
            black_pieces: bbs[14],
            white_piece_count: bbs[13].popcnt() as u8,
            black_piece_count: bbs[14].popcnt() as u8,
        })
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
    ) -> Result<([Bitboard; 15], Board), FenError> {
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
                    last_was_number = false;
                    file_counter = 0;
                    rank_counter += 1;
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
                        last_was_number = true;
                        file_counter += (skip - 1) as u8;

                        if file_counter > 7 {
                            return Err(FenError {
                                ty: FenErrorType::PiecePositionsRowTooLarge,
                                msg: format!(
                                    "row {} ({}) represents {} files; expected 8",
                                    rank_counter, rows[rank_counter as usize], file_counter
                                ),
                            });
                        }
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
                white_pieces,
                black_pieces,
            ],
            Board::from_array(board),
        ))
    }

    fn parse_side_to_move(side_to_move: &str) -> Result<Color, FenError> {
        match side_to_move {
            "w" => Ok(Color::White),
            "b" => Ok(Color::Black),
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
            return Ok(CastlingRights::none());
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

        Ok(CastlingRights {
            white_kingside,
            white_queenside,
            black_kingside,
            black_queenside,
        })
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

        match ep_square {
            "a3" => Ok(Some(Square::A3)),
            "b3" => Ok(Some(Square::B3)),
            "c3" => Ok(Some(Square::C3)),
            "d3" => Ok(Some(Square::D3)),
            "e3" => Ok(Some(Square::E3)),
            "f3" => Ok(Some(Square::F3)),
            "g3" => Ok(Some(Square::G3)),
            "h3" => Ok(Some(Square::H3)),
            "a6" => Ok(Some(Square::A6)),
            "b6" => Ok(Some(Square::B6)),
            "c6" => Ok(Some(Square::C6)),
            "d6" => Ok(Some(Square::D6)),
            "e6" => Ok(Some(Square::E6)),
            "f6" => Ok(Some(Square::F6)),
            "g6" => Ok(Some(Square::G6)),
            "h6" => Ok(Some(Square::H6)),
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
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "")?;
        writeln!(f, "BITBOARDS\n=========\n")?;
        writeln!(f, "No Pieces:\n {}", self.no_piece)?;
        writeln!(f, "White Pawns:\n {}", self.white_pawns)?;
        writeln!(f, "White Knights:\n {}", self.white_knights)?;
        writeln!(f, "White Bishops:\n {}", self.white_bishops)?;
        writeln!(f, "White Rooks:\n {}", self.white_rooks)?;
        writeln!(f, "White Queens:\n {}", self.white_queens)?;
        writeln!(f, "White King:\n {}", self.white_king)?;
        writeln!(f, "Black Pawns:\n {}", self.black_pawns)?;
        writeln!(f, "Black Knights:\n {}", self.black_knights)?;
        writeln!(f, "Black Bishops:\n {}", self.black_bishops)?;
        writeln!(f, "Black Rooks:\n {}", self.black_rooks)?;
        writeln!(f, "Black Queens:\n {}", self.black_queens)?;
        writeln!(f, "Black King:\n {}", self.black_king)?;
        writeln!(f, "White Pieces:\n {}", self.white_pieces)?;
        writeln!(f, "Black Pieces:\n {}", self.black_pieces)?;

        writeln!(f, "BOARD ARRAY\n===========\n")?;
        writeln!(f, "{}", self.board)?;

        writeln!(f, "PIECE COUNTS\n============\n")?;
        writeln!(f, "White: {}", self.white_piece_count)?;
        writeln!(f, "Black: {}", self.black_piece_count)?;
        writeln!(f)?;

        writeln!(f, "STATE DATA\n==========\n")?;
        writeln!(f, "Turn: {}", self.turn)?;
        writeln!(f, "Castling Rights: {}", self.castling_rights)?;
        writeln!(
            f,
            "En Passant Square: {}",
            match self.ep_square {
                Some(sq) => sq.to_string(),
                None => "none".to_string(),
            }
        )?;
        writeln!(f, "Half move clock: {}", self.half_move_clock)?;
        writeln!(f, "Move number: {}", self.move_number)

        // TODO: display all the other Position struct data
    }
}

fn rank_file_to_idx(rank: u32, file: u8) -> u8 {
    // `rank` and `file` here have indices based on iterating through the
    // fen string, so `rank` = 0 means the rank usually labelled as 8 in
    // algebraic chess notation
    (7 - rank as u8) * 8 + file
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut squares: [[Piece; 8]; 8] = [[Piece::None; 8]; 8];

        for i in 0..64 {
            let rank = i / 8;
            let file = i % 8;
            squares[rank][file] = self.arr[i]
        }

        writeln!(f, "   ┌────────────────────────┐")?;
        for (i, row) in squares.iter().rev().enumerate() {
            write!(f, " {} │", 8 - i)?;
            for square in row {
                write!(f, " {} ", square)?;
            }
            write!(f, "│\n")?;
        }
        writeln!(f, "   └────────────────────────┘")?;
        writeln!(f, "     a  b  c  d  e  f  g  h ")
    }
}
