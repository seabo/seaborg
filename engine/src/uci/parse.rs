use super::Uci;
use crate::search::search::SearchMode;
use crate::time::TimeControl;

use log::info;

/// Represents a UCI message sent by the GUI to the engine.
#[derive(Clone, Debug)]
pub enum Req {
    /// Put the engine into UCI communication mode.
    Uci,
    /// Ask the engine if it is ready to receive further commands.
    IsReady,
    /// Tell the engine that we are analysing a position from a different game.
    UciNewGame,
    /// Set the given position on the internal board and advances that
    /// position by playing any additional moves given in the second slot
    /// of the tuple.
    SetPosition((Pos, Option<Vec<String>>)),
    /// Commence the search process on the internal board.
    Go(SearchMode),
    /// Halt the search process, but don't quit the engine.
    Stop,
    /// Stop the search process and quit the engine.
    Quit,
}

/// Represents a position to be set on the internal board, either as the
/// `startpos` keyword, or a fen string.
#[derive(Clone, Debug, PartialEq)]
pub enum Pos {
    Fen(String),
    Startpos,
}

impl Uci {
    pub(super) fn parse(raw: String) -> ParseResult {
        Parser::parse(raw)
    }
}

/// The tokens which can be sent by the GUI. We will either have
/// a reserved keyword, a FEN string, or a number or some other
/// general string.
#[derive(Clone, Debug)]
enum Token<'a> {
    Keyword(Keyword),
    String(&'a str),
}

/// The reserved keywords which can be sent from the GUI to the
/// engine.
#[derive(Copy, Clone, Debug)]
enum Keyword {
    Uci,
    Debug,
    On,
    Off,
    IsReady,
    SetOption,
    UciNewGame,
    Position,
    Fen,
    Startpos,
    Moves,
    Go,
    SearchMoves,
    Ponder,
    Wtime,
    Btime,
    Winc,
    Binc,
    MovesToGo,
    Depth,
    Nodes,
    Mate,
    MoveTime,
    Infinite,
    Stop,
    PonderHit,
    Quit,
}

#[derive(Debug)]
pub enum ParseError {
    /// Represents a token which should not have appeared where it did.
    UnexpectedToken(String),
    /// Expected more tokens but reach the end of the input.
    ExpectedMore(String),
    /// Represents a situation where the `position` keyword was sent,
    /// but no further information on which position to set the board to.
    NoPosition,
    /// Represents an error reading from stdin.
    Io(String),
    /// Represents a situation where there was no input string received when
    /// reading from stdin.
    NoInput,
    /// Unsupported time control. This error is used for the time control
    /// formats which are legitimate within the UCI protocol but which we do
    /// not yet support.
    /// TODO: support all formats and delete this.
    UnsupportedTimeControl,
    /// A set of time control parameters was expected, but this was incomplete
    /// in the command.
    IncompleteTimeControl,
}

impl ParseError {
    pub fn emit(&self) {
        info!("parse error: {}", self);
    }
}

impl std::convert::From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> ParseError {
        ParseError::Io(format!("{}", err))
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedToken(tok) => writeln!(f, "unexpected token: {}", tok),
            ParseError::ExpectedMore(err) => writeln!(f, "expected further input: {}", err),
            ParseError::Io(err) => writeln!(f, "io error: {}", err),
            ParseError::NoInput => writeln!(f, "expected command but received no input"),
            ParseError::NoPosition => writeln!(f, "no position provided"),
            ParseError::UnsupportedTimeControl => writeln!(f, "unsupported time control"),
            ParseError::IncompleteTimeControl => writeln!(f, "incomplete time control provided"),
        }
    }
}

/// The result of attempting to parse a new string from stdin.
pub type ParseResult = Result<Req, ParseError>;

/// A struct for parsing a command sent by the GUI via stdin.
struct Parser<'a> {
    toks: Vec<Token<'a>>,
    cursor: usize,
}

impl<'a> Parser<'a> {
    pub fn parse(raw: String) -> ParseResult {
        let mut parser = Parser::new(&raw);
        parser.parse_command()
    }

    /// Read the next command from stdin using the provided handle,
    /// parse that command and return a `ParseResult` containing an
    /// `EngineCommand` or a `ParseError`.
    fn new(buf: &'a str) -> Self {
        let mut toks = Vec::new();
        for tok in buf.split_whitespace() {
            toks.push(Parser::scan_token(tok));
        }

        Parser { toks, cursor: 0 }
    }

    fn advance(&mut self) -> Option<Token<'a>> {
        if self.cursor < self.toks.len() {
            let next = self.toks[self.cursor].clone();
            self.cursor += 1;
            Some(next)
        } else {
            None
        }
    }

    fn peek(&mut self) -> Option<Token<'a>> {
        if self.cursor < self.toks.len() {
            let next = self.toks[self.cursor].clone();
            Some(next)
        } else {
            None
        }
    }

    fn parse_command(&mut self) -> ParseResult {
        match self.advance() {
            Some(tok) => match tok {
                Token::Keyword(Keyword::Uci) => Ok(Req::Uci),
                Token::Keyword(Keyword::Debug) => todo!(),
                Token::Keyword(Keyword::IsReady) => Ok(Req::IsReady),
                Token::Keyword(Keyword::SetOption) => todo!(),
                Token::Keyword(Keyword::UciNewGame) => Ok(Req::UciNewGame),
                Token::Keyword(Keyword::Position) => self.parse_position_and_moves(),
                Token::Keyword(Keyword::Go) => self.parse_go(),
                Token::Keyword(Keyword::Stop) => Ok(Req::Stop),
                Token::Keyword(Keyword::Quit) => Ok(Req::Quit),
                _ => self.unexpected_token("expected a valid uci command"),
            },
            None => Err(ParseError::NoInput),
        }
    }

    fn parse_position_and_moves(&mut self) -> ParseResult {
        let pos = self.parse_position()?;
        let moves = self.parse_moves()?;

        Ok(Req::SetPosition((pos, moves)))
    }

    fn parse_position(&mut self) -> Result<Pos, ParseError> {
        match self.advance() {
            Some(tok) => match tok {
                Token::Keyword(Keyword::Startpos) => Ok(Pos::Startpos),
                Token::Keyword(Keyword::Fen) => self.parse_fen(),
                _ => self.unexpected_token("expected a fen string or `startpos`"),
            },
            None => Err(ParseError::NoPosition),
        }
    }

    fn parse_fen(&mut self) -> Result<Pos, ParseError> {
        // A fen string should have 6 whitespace-separate fields, so we collect
        // the next 6 tokens with advance.
        let mut fen_vec = Vec::new();

        for _ in 0..6 {
            match self.advance() {
                Some(Token::String(field)) => {
                    fen_vec.push(field);
                }
                Some(_) => {
                    return self.unexpected_token("expected a fen string");
                }
                None => {
                    return Err(ParseError::NoPosition);
                }
            }
        }

        let fen = fen_vec.join(" ");

        Ok(Pos::Fen(fen))
    }

    fn parse_moves(&mut self) -> Result<Option<Vec<String>>, ParseError> {
        match self.peek() {
            Some(tok) => match tok {
                Token::Keyword(Keyword::Moves) => {
                    self.advance();
                    self.parse_move_list()
                }
                _ => self.unexpected_token("expected `moves` keyword after position"),
            },
            _ => Ok(None),
        }
    }

    fn parse_move_list(&mut self) -> Result<Option<Vec<String>>, ParseError> {
        let mut moves = Vec::new();
        while let Some(tok) = self.advance() {
            match tok {
                Token::String(mov) => moves.push(mov.to_string()),
                _ => return self.unexpected_token("expected move, found keyword"),
            }
        }

        Ok(Some(moves))
    }

    fn parse_go(&mut self) -> ParseResult {
        // The next token will be one of:
        // - searchmoves
        // - ponder
        // - wtime, btime, winc, binc
        // - movestogo
        // - depth
        // - nodes
        // - mate
        // - movetime
        // - infinite
        //
        // We only want to support the time control version and infinite for now.
        // We'll match on the next token, and handle the legitimate UCI commands
        // with a panic saying that we don't support that time control (or we
        // could just return `go infinite` and at least not crash).

        match self.peek() {
            Some(tok) => match tok {
                Token::Keyword(Keyword::SearchMoves) => Parser::unsupported_time_control(),
                Token::Keyword(Keyword::Ponder) => Parser::unsupported_time_control(),
                Token::Keyword(Keyword::Wtime) => self.parse_time_control(),
                Token::Keyword(Keyword::Btime) => self.parse_time_control(),
                Token::Keyword(Keyword::Winc) => self.parse_time_control(),
                Token::Keyword(Keyword::Binc) => self.parse_time_control(),
                Token::Keyword(Keyword::MovesToGo) => self.parse_time_control(),
                Token::Keyword(Keyword::Depth) => Parser::unsupported_time_control(),
                Token::Keyword(Keyword::Nodes) => Parser::unsupported_time_control(),
                Token::Keyword(Keyword::Mate) => Parser::unsupported_time_control(),
                Token::Keyword(Keyword::MoveTime) => self.parse_move_time(),
                Token::Keyword(Keyword::Infinite) => {
                    self.advance();
                    self.expect_end(Ok(Req::Go(SearchMode::Infinite)))
                }
                Token::Keyword(Keyword::PonderHit) => Parser::unsupported_time_control(),
                _ => Err(ParseError::UnexpectedToken(
                    "did not recognise token after `go` command".to_string(),
                )),
            },
            None => Err(ParseError::ExpectedMore(
                "no time control information provided in `go` command".to_string(),
            )),
        }
    }

    fn parse_time_control(&mut self) -> ParseResult {
        let mut wtime: Option<u32> = None;
        let mut btime: Option<u32> = None;
        let mut winc: u32 = 0;
        let mut binc: u32 = 0;
        let mut moves_to_go: Option<u8> = None;

        if let Some(Token::Keyword(Keyword::Infinite)) = self.peek() {
            // We found `infnite` after the `go` command. We expect this to be the end
            // of input, so bail here immediately.
            self.advance();
            return self.expect_end(Ok(Req::Go(SearchMode::Infinite)));
        }

        while self.peek().is_some() {
            match self.advance().unwrap() {
                Token::Keyword(Keyword::Wtime) => {
                    wtime = Some(self.parse_number()?);
                }
                Token::Keyword(Keyword::Btime) => {
                    btime = Some(self.parse_number()?);
                }
                Token::Keyword(Keyword::Winc) => {
                    winc = self.parse_number()?;
                }
                Token::Keyword(Keyword::Binc) => {
                    binc = self.parse_number()?;
                }
                Token::Keyword(Keyword::MovesToGo) => {
                    moves_to_go = Some(self.parse_number()?);
                }
                _ => {
                    return Err(ParseError::UnexpectedToken(
                        "expected one of `wtime`, `btime`, `winc`, `binc` in time control"
                            .to_string(),
                    ));
                }
            }
        }

        if wtime.is_none() || btime.is_none() {
            return Err(ParseError::IncompleteTimeControl);
        }

        Ok(Req::Go(SearchMode::Timed(TimeControl::new(
            wtime.unwrap(),
            btime.unwrap(),
            winc,
            binc,
            moves_to_go,
        ))))
    }

    fn parse_move_time(&mut self) -> ParseResult {
        // Consume the `movetime` token.
        self.advance();

        let ms: u32 = self.parse_number()?;

        Ok(Req::Go(SearchMode::FixedTime(ms)))
    }

    fn parse_number<T: std::str::FromStr>(&mut self) -> Result<T, ParseError> {
        match self.advance() {
            Some(Token::String(tok)) => match str::parse::<T>(tok) {
                Ok(i) => Ok(i),
                Err(err) => Err(ParseError::UnexpectedToken("expected a number".to_string())),
            },
            Some(_) => Err(ParseError::UnexpectedToken("expected a number".to_string())),
            None => Err(ParseError::ExpectedMore("expected a number".to_string())),
        }
    }

    fn unsupported_time_control() -> ParseResult {
        Err(ParseError::UnsupportedTimeControl)
    }

    fn unexpected_token<T>(&mut self, msg: &str) -> Result<T, ParseError> {
        Err(ParseError::UnexpectedToken(msg.to_string()))
    }

    fn scan_token(str: &str) -> Token {
        match str {
            "uci" => Token::Keyword(Keyword::Uci),
            "debug" => Token::Keyword(Keyword::Debug),
            "on" => Token::Keyword(Keyword::On),
            "off" => Token::Keyword(Keyword::Off),
            "isready" => Token::Keyword(Keyword::IsReady),
            "setoption" => Token::Keyword(Keyword::SetOption),
            "ucinewgame" => Token::Keyword(Keyword::UciNewGame),
            "position" => Token::Keyword(Keyword::Position),
            "fen" => Token::Keyword(Keyword::Fen),
            "startpos" => Token::Keyword(Keyword::Startpos),
            "moves" => Token::Keyword(Keyword::Moves),
            "go" => Token::Keyword(Keyword::Go),
            "searchmoves" => Token::Keyword(Keyword::SearchMoves),
            "ponder" => Token::Keyword(Keyword::Ponder),
            "wtime" => Token::Keyword(Keyword::Wtime),
            "btime" => Token::Keyword(Keyword::Btime),
            "winc" => Token::Keyword(Keyword::Winc),
            "binc" => Token::Keyword(Keyword::Binc),
            "movestogo" => Token::Keyword(Keyword::MovesToGo),
            "depth" => Token::Keyword(Keyword::Depth),
            "nodes" => Token::Keyword(Keyword::Nodes),
            "mate" => Token::Keyword(Keyword::Mate),
            "movetime" => Token::Keyword(Keyword::MoveTime),
            "infinite" => Token::Keyword(Keyword::Infinite),
            "stop" => Token::Keyword(Keyword::Stop),
            "ponderhit" => Token::Keyword(Keyword::PonderHit),
            "quit" => Token::Keyword(Keyword::Quit),
            _ => Token::String(str),
        }
    }

    fn expect_end(&mut self, res: ParseResult) -> ParseResult {
        match self.advance() {
            Some(_) => Err(ParseError::UnexpectedToken(
                "should have reached end of input, but found more tokens".to_string(),
            )),
            None => res,
        }
    }
}
