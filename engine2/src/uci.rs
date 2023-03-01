use super::options::EngineOpt;
use super::time::{TimeControl, TimingMode};

/// A UCI message sent by the GUI to the engine.
#[derive(Clone, Debug)]
pub enum Command {
    /// Instruct engine to enter UCI mode.
    Uci,
    /// Request confirmation of engine readiness for further instructions.
    IsReady,
    /// Tell the engine that we are considering a position from a new game, unrelated to previous
    /// searches.
    UciNewGame,
    /// Set the given position on the internal board, and advance that position by playing any
    /// additional moves included in the second slot of the tuple.
    SetPosition((String, Vec<String>)),
    /// Set an engine configuration option.
    SetOption(EngineOpt),
    /// Commence the search process.
    Go(TimingMode),
    /// Halt the search process, but don't quit the engine.
    Stop,
    /// Stop the search process and quit the engine.
    Quit,
    /// Display the board in ascii format.
    Display,
    /// Display the current engine configuration.
    Config,
}

/// The reserved keywords which can be sent from the GUI to the engine.
#[derive(PartialEq)]
enum Keyword {
    Uci,
    Debug,
    On,
    Off,
    IsReady,
    SetOption,
    Name,
    Value,
    Register,
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

    // Additional commands
    /// Display the current internal board position.
    Display,
    /// Display the current config of the engine.
    Config,
}

/// A parsing error.
#[derive(Debug)]
pub enum Error {
    /// There were no tokens to parse in the input stream.
    NoInput,
    /// Unexpected additional input tokens were found at the end of an otherwise valid command.
    ExpectedEnd,
    /// Expected a number value.
    ExpectedNumber,
    /// Expected a string, but got e.g. a reserved keyword.
    ExpectedString,
    /// Expected a boolean value "true" or "false".
    ExpectedBool,
    /// The input stream ended unexpectedly.
    UnexpectedEnd,
    /// Unexpected token in input.
    UnexpectedToken,
    /// An attempt was made to set an option with an invalid name.
    InvalidOption,
    /// No position was defined after the `position` keyword.
    NoPosition,
    /// The position provided was invalid.
    InvalidPosition(core::position::FenError),
    /// A move provided as part of setting the position is invalid.
    InvalidMove,
    /// A go command was issued with an unsupported time control.
    UnsupportedTimeControl,
    /// A go comannd was issued with a time control that was incomplete.
    IncompleteTimeControl,
}

pub type PResult = Result<Command, Error>;

pub struct Parser<'a> {
    raw: &'a str,
    toks: Vec<Token<'a>>,
    cursor: usize,
}

impl<'a> Parser<'a> {
    pub fn parse(input: &'a str) -> PResult {
        Parser::new(input).parse_command()
    }

    fn new(input: &'a str) -> Parser<'a> {
        Parser {
            raw: input,
            toks: input
                .split_whitespace()
                .map(|t| Token::scan(t))
                .collect::<Vec<Token>>(),
            cursor: 0,
        }
    }

    fn advance(&mut self) -> Option<&Token<'a>> {
        if self.cursor < self.toks.len() {
            let next = unsafe { self.toks.get_unchecked(self.cursor) };
            self.cursor += 1;
            Some(next)
        } else {
            None
        }
    }

    fn peek(&mut self) -> Option<&Token<'a>> {
        if self.cursor < self.toks.len() {
            let next = unsafe { self.toks.get_unchecked(self.cursor) };
            Some(next)
        } else {
            None
        }
    }

    fn expect_kw(&mut self, kw: Keyword) -> Result<(), Error> {
        if self.advance() != Some(&Token::Kw(kw)) {
            Err(Error::UnexpectedToken)
        } else {
            Ok(())
        }
    }

    fn expect_end(&mut self, p: PResult) -> PResult {
        if self.cursor == self.toks.len() {
            p
        } else {
            Err(Error::ExpectedEnd)
        }
    }

    fn unexpected_end(&mut self) -> PResult {
        Err(Error::UnexpectedEnd)
    }

    fn unexpected_token(&self) -> PResult {
        Err(Error::UnexpectedToken)
    }

    fn parse_string(&mut self) -> Result<&'a str, Error> {
        match self.advance() {
            Some(Token::String(s)) => Ok(*s),
            _ => Err(Error::ExpectedString),
        }
    }

    fn parse_integer(&mut self) -> Result<usize, Error> {
        let s = self.parse_string()?;
        s.parse::<usize>().map_err(|_| Error::ExpectedNumber)
    }

    fn parse_bool(&mut self) -> Result<bool, Error> {
        let b = self.parse_string()?;
        match b {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(Error::ExpectedBool),
        }
    }

    fn parse_command(&mut self) -> PResult {
        match self.advance() {
            Some(tok) => match tok {
                Token::Kw(Keyword::Uci) => self.parse_uci(),
                Token::Kw(Keyword::Debug) => self.parse_debug(),
                Token::Kw(Keyword::IsReady) => self.parse_isready(),
                Token::Kw(Keyword::SetOption) => self.parse_setoption(),
                Token::Kw(Keyword::UciNewGame) => self.parse_ucinewgame(),
                Token::Kw(Keyword::Position) => self.parse_position_and_moves(),
                Token::Kw(Keyword::Go) => self.parse_go(),
                Token::Kw(Keyword::Stop) => todo!(),
                Token::Kw(Keyword::Quit) => todo!(),
                Token::Kw(Keyword::Display) => self.parse_display(),
                Token::Kw(Keyword::Config) => self.parse_config(),
                Token::String(s) => self.unexpected_token(),

                _ => todo!(),
            },
            None => Err(Error::NoInput),
        }
    }

    fn parse_uci(&mut self) -> PResult {
        self.expect_end(Ok(Command::Uci))
    }

    fn parse_isready(&mut self) -> PResult {
        self.expect_end(Ok(Command::IsReady))
    }

    fn parse_ucinewgame(&mut self) -> PResult {
        self.expect_end(Ok(Command::UciNewGame))
    }

    fn parse_position_and_moves(&mut self) -> PResult {
        let pos = self.parse_position()?;
        let moves = self.parse_moves()?;

        Ok(Command::SetPosition((pos, moves)))
    }

    fn parse_position(&mut self) -> Result<String, Error> {
        match self.advance() {
            Some(tok) => match tok {
                Token::Kw(Keyword::Startpos) => Ok(core::position::START_POSITION.to_string()),
                Token::Kw(Keyword::Fen) => self.parse_fen(),
                _ => Err(Error::UnexpectedToken),
            },
            None => Err(Error::NoPosition),
        }
    }

    fn parse_fen(&mut self) -> Result<String, Error> {
        // A fen string should have 6 whitespace-separated fields, so we collect the next 6 tokens
        // in advance.
        let mut fen_vec = Vec::new();

        for _ in 0..6 {
            match self.advance() {
                Some(Token::String(field)) => {
                    fen_vec.push(field.to_string());
                }
                Some(_) => {
                    return Err(Error::UnexpectedToken);
                }
                None => {
                    return Err(Error::NoPosition);
                }
            }
        }

        let fen = fen_vec.join(" ");
        Ok(fen)
    }

    fn parse_moves(&mut self) -> Result<Vec<String>, Error> {
        match self.peek() {
            Some(tok) => match tok {
                Token::Kw(Keyword::Moves) => {
                    self.advance();
                    self.parse_move_list()
                }
                _ => Err(Error::UnexpectedToken),
            },
            None => Ok(vec![]),
        }
    }

    fn parse_move_list(&mut self) -> Result<Vec<String>, Error> {
        let mut moves = Vec::new();
        while let Some(tok) = self.advance() {
            match *tok {
                Token::String(mov) => moves.push(mov.to_string()),
                _ => return Err(Error::UnexpectedToken),
            }
        }

        Ok(moves)
    }

    fn parse_go(&mut self) -> PResult {
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
        // We only support time control, depth and infinite for now. We'll match on the next
        // token, and handle the legitimate UCI commands with a panic saying that we don't support
        // that time control (or we could just return `go infinite` and at least not crash).

        match self.peek() {
            Some(tok) => match *tok {
                Token::Kw(Keyword::SearchMoves) => self.unsupported_time_control(),
                Token::Kw(Keyword::Ponder) => self.unsupported_time_control(),
                Token::Kw(Keyword::Wtime) => self.parse_time_control(),
                Token::Kw(Keyword::Btime) => self.parse_time_control(),
                Token::Kw(Keyword::Winc) => self.parse_time_control(),
                Token::Kw(Keyword::Binc) => self.parse_time_control(),
                Token::Kw(Keyword::MovesToGo) => self.parse_time_control(),
                Token::Kw(Keyword::Depth) => self.parse_depth(),
                Token::Kw(Keyword::Nodes) => self.unsupported_time_control(),
                Token::Kw(Keyword::Mate) => self.unsupported_time_control(),
                Token::Kw(Keyword::MoveTime) => self.unsupported_time_control(),
                Token::Kw(Keyword::Infinite) => self.unsupported_time_control(),
                Token::Kw(Keyword::PonderHit) => self.unsupported_time_control(),
                _ => Err(Error::UnexpectedToken),
            },
            None => self.unexpected_end(),
        }
    }

    fn parse_time_control(&mut self) -> PResult {
        let mut wtime: Option<usize> = None;
        let mut btime: Option<usize> = None;
        let mut winc: usize = 0;
        let mut binc: usize = 0;
        let mut moves_to_go: Option<usize> = None;

        while self.peek().is_some() {
            match self.advance().unwrap() {
                Token::Kw(Keyword::Wtime) => {
                    wtime = Some(self.parse_integer()?);
                }
                Token::Kw(Keyword::Btime) => {
                    btime = Some(self.parse_integer()?);
                }
                Token::Kw(Keyword::Winc) => {
                    winc = self.parse_integer()?;
                }
                Token::Kw(Keyword::Binc) => {
                    binc = self.parse_integer()?;
                }
                Token::Kw(Keyword::MovesToGo) => moves_to_go = Some(self.parse_integer()?),
                _ => {
                    return Err(Error::UnexpectedToken);
                }
            }
        }

        if wtime.is_none() || btime.is_none() {
            return Err(Error::IncompleteTimeControl);
        }

        Ok(Command::Go(TimingMode::Timed(TimeControl::new(
            wtime.expect("should not be None"),
            btime.expect("should not be None"),
            winc,
            binc,
            moves_to_go,
        ))))
    }

    fn parse_depth(&mut self) -> PResult {
        self.advance().ok_or(Error::UnexpectedEnd)?;

        let depth = self.parse_integer()? as u8;
        Ok(Command::Go(TimingMode::Depth(depth)))
    }

    fn unsupported_time_control(&mut self) -> PResult {
        Err(Error::UnsupportedTimeControl)
    }

    fn parse_debug(&mut self) -> PResult {
        match self.advance() {
            Some(Token::Kw(Keyword::On)) => Ok(Command::SetOption(EngineOpt::DebugMode(true))),
            Some(Token::Kw(Keyword::Off)) => Ok(Command::SetOption(EngineOpt::DebugMode(false))),
            Some(_) => self.unexpected_token(),
            None => self.unexpected_end(),
        }
    }

    fn parse_setoption(&mut self) -> PResult {
        self.expect_kw(Keyword::Name)?;

        match self.parse_string()? {
            "Hash" => self.parse_hash(),
            "Iterative_Deepening" => self.parse_iterative_deepening(),
            _ => Err(Error::InvalidOption),
        }
    }

    fn parse_hash(&mut self) -> PResult {
        self.expect_kw(Keyword::Value)?;

        let v = self.parse_integer()?;

        Ok(Command::SetOption(EngineOpt::Hash(v)))
    }

    fn parse_iterative_deepening(&mut self) -> PResult {
        self.expect_kw(Keyword::Value)?;

        let b = self.parse_bool()?;

        Ok(Command::SetOption(EngineOpt::IterativeDeepening(b)))
    }

    fn parse_display(&mut self) -> PResult {
        self.expect_end(Ok(Command::Display))
    }

    fn parse_config(&mut self) -> PResult {
        self.expect_end(Ok(Command::Config))
    }
}

#[derive(PartialEq)]
enum Token<'a> {
    Kw(Keyword),
    String(&'a str),
}

impl<'a> Token<'a> {
    fn scan(t: &'a str) -> Token<'a> {
        match t {
            "uci" => Token::Kw(Keyword::Uci),
            "debug" => Token::Kw(Keyword::Debug),
            "on" => Token::Kw(Keyword::On),
            "off" => Token::Kw(Keyword::Off),
            "isready" => Token::Kw(Keyword::IsReady),
            "setoption" => Token::Kw(Keyword::SetOption),
            "ucinewgame" => Token::Kw(Keyword::UciNewGame),
            "position" => Token::Kw(Keyword::Position),
            "fen" => Token::Kw(Keyword::Fen),
            "startpos" => Token::Kw(Keyword::Startpos),
            "moves" => Token::Kw(Keyword::Moves),
            "go" => Token::Kw(Keyword::Go),
            "name" => Token::Kw(Keyword::Name),
            "value" => Token::Kw(Keyword::Value),
            "searchmoves" => Token::Kw(Keyword::SearchMoves),
            "ponder" => Token::Kw(Keyword::Ponder),
            "wtime" => Token::Kw(Keyword::Wtime),
            "btime" => Token::Kw(Keyword::Btime),
            "winc" => Token::Kw(Keyword::Winc),
            "binc" => Token::Kw(Keyword::Binc),
            "movestogo" => Token::Kw(Keyword::MovesToGo),
            "depth" => Token::Kw(Keyword::Depth),
            "nodes" => Token::Kw(Keyword::Nodes),
            "mate" => Token::Kw(Keyword::Mate),
            "movetime" => Token::Kw(Keyword::MoveTime),
            "infinite" => Token::Kw(Keyword::Infinite),
            "stop" => Token::Kw(Keyword::Stop),
            "ponderhit" => Token::Kw(Keyword::PonderHit),
            "quit" => Token::Kw(Keyword::Quit),
            "d" => Token::Kw(Keyword::Display),
            "display" => Token::Kw(Keyword::Display),
            "config" => Token::Kw(Keyword::Config),
            _ => Token::String(t),
        }
    }
}
