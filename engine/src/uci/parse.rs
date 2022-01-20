use super::Uci;

/// Represents a UCI message sent by the GUI to the engine.
#[derive(Clone, Debug, PartialEq)]
pub enum Req {
    /// Put the engine into UCI communication mode.
    Uci,
    /// Ask the engine if it is ready to receive further commands.
    IsReady,
    /// Tell the engine that we are analysing a position from a different game.
    UciNewGame,
    /// Set the given position on the internal board.
    SetPosition(Pos),
    /// Commence the search process on the internal board.
    Go,
    /// Halt the search process, but don't quit the engine.
    Stop,
    /// Stop the search process and quit the engine.
    Quit,
    /// Represents a request we have decided to ignore, likely because it
    /// does not conform to the UCI protocol, or we just don't implement that
    /// command.
    Ignored,
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
    /// Represents a situation where the `position` keyword was sent,
    /// but no further information on which position to set the board to.
    NoPosition,
    /// Represents an error reading from stdin.
    Io(String),
    /// Represents a situation where there was no input string received when
    /// reading from stdin.
    NoInput,
}

impl ParseError {
    pub fn emit(&self) {
        eprintln!("{}", self);
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
            ParseError::Io(err) => writeln!(f, "io error: {}", err),
            ParseError::NoInput => writeln!(f, "no input"),
            ParseError::NoPosition => writeln!(f, "no position provided"),
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
            Some(tok) => self.parse_keyword(tok),
            None => Err(ParseError::NoInput),
        }
    }

    fn parse_keyword(&mut self, tok: Token) -> ParseResult {
        match tok {
            Token::Keyword(Keyword::Uci) => Ok(Req::Uci),
            Token::Keyword(Keyword::Debug) => todo!(),
            Token::Keyword(Keyword::On) => todo!(),
            Token::Keyword(Keyword::Off) => todo!(),
            Token::Keyword(Keyword::IsReady) => Ok(Req::IsReady),
            Token::Keyword(Keyword::SetOption) => todo!(),
            Token::Keyword(Keyword::UciNewGame) => Ok(Req::UciNewGame),
            Token::Keyword(Keyword::Position) => self.parse_position(),
            Token::Keyword(Keyword::Fen) => todo!(),
            Token::Keyword(Keyword::Startpos) => Ok(Req::Ignored),
            Token::Keyword(Keyword::Go) => self.parse_go(),
            Token::Keyword(Keyword::SearchMoves) => todo!(),
            Token::Keyword(Keyword::Ponder) => todo!(),
            Token::Keyword(Keyword::Wtime) => todo!(),
            Token::Keyword(Keyword::Btime) => todo!(),
            Token::Keyword(Keyword::Winc) => todo!(),
            Token::Keyword(Keyword::Binc) => todo!(),
            Token::Keyword(Keyword::MovesToGo) => todo!(),
            Token::Keyword(Keyword::Depth) => todo!(),
            Token::Keyword(Keyword::Nodes) => todo!(),
            Token::Keyword(Keyword::Mate) => todo!(),
            Token::Keyword(Keyword::MoveTime) => todo!(),
            Token::Keyword(Keyword::Infinite) => todo!(),
            Token::Keyword(Keyword::Stop) => Ok(Req::Stop),
            Token::Keyword(Keyword::PonderHit) => todo!(),
            Token::Keyword(Keyword::Quit) => Ok(Req::Quit),
            _ => self.unexpected_token("expected a uci keyword"),
        }
    }

    fn parse_position(&mut self) -> ParseResult {
        match self.advance() {
            Some(tok) => match tok {
                Token::Keyword(Keyword::Startpos) => Ok(Req::SetPosition(Pos::Startpos)),
                Token::Keyword(Keyword::Fen) => self.parse_fen(),
                _ => self.unexpected_token("expected a fen string or `startpos`"),
            },
            None => Err(ParseError::NoPosition),
        }
    }

    fn parse_fen(&mut self) -> ParseResult {
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

        return Ok(Req::SetPosition(Pos::Fen(fen_vec.join(" "))));
    }

    fn parse_go(&mut self) -> ParseResult {
        // TODO: this needs to parse all the possible `go` strings from the UCI
        // protocol. Currently, we just accept the word `go` on its own.
        Ok(Req::Go)
    }

    fn unexpected_token(&mut self, msg: &str) -> ParseResult {
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
}
