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
    /// Display the board in a Lichess analysis window with the default browser.
    DisplayLichess,
    /// Make a move directly on the internal board in its current position.
    ///
    /// The UCI protocol is theoretically supposed to be stateless, so that the GUI manages states
    /// and tells the engine exactly what position to search every time it sends a command, even if
    /// a game is being played. In reality, because of pondering and `ponderhit`, this doesn't even
    /// happen in the protocol anyway.
    ///
    /// It's an annoying faff when working directly with the CLI to have to retype position strings
    /// and long strings of consecutive moves, so this additional command allows a move to be
    /// played directly.
    Move(String),
    /// Display the current engine configuration.
    Config,
    /// Run perft to the given depth.
    Perft(usize),
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
    /// Appears after the display keyword to open a board position in Lichess.
    Lichess,
    /// Short form keyword to open the current internal board position in a Lichess Analysis board.
    DisplayLichess,
    /// Make a move on the internal board.
    Move,
    /// Display the current config of the engine.
    Config,
    /// Run a perft test.
    Perft,
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
    toks: Vec<Token<'a>>,
    cursor: usize,
}

impl<'a> Parser<'a> {
    pub fn parse(input: &'a str) -> PResult {
        Parser::new(input).parse_command()
    }

    fn new(input: &'a str) -> Parser<'a> {
        Parser {
            toks: input
                .split_whitespace()
                .map(|t| Token::scan(t))
                .collect::<Vec<Token>>(),
            cursor: 0,
        }
    }

    fn advance(&mut self) -> Option<&Token<'a>> {
        let next = self.toks.get(self.cursor)?;
        self.cursor += 1;
        Some(next)
    }

    fn peek(&mut self) -> Option<&Token<'a>> {
        self.toks.get(self.cursor)
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

    fn parse_u64(&mut self) -> Result<u64, Error> {
        let s = self.parse_string()?;
        s.parse::<u64>().map_err(|_| Error::ExpectedNumber)
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
                Token::Kw(Keyword::Stop) => self.parse_stop(),
                Token::Kw(Keyword::Quit) => self.parse_quit(),
                Token::Kw(Keyword::Display) => self.parse_display(),
                Token::Kw(Keyword::DisplayLichess) => self.parse_display_lichess(),
                Token::Kw(Keyword::Move) => self.parse_move(),
                Token::Kw(Keyword::Config) => self.parse_config(),
                Token::Kw(Keyword::Perft) => self.parse_perft(),
                Token::String(_) | Token::Kw(_) => self.unexpected_token(),
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
                Token::Kw(Keyword::MoveTime) => self.parse_movetime(),
                Token::Kw(Keyword::Infinite) => self.parse_infinite(),
                Token::Kw(Keyword::PonderHit) => self.unsupported_time_control(),
                _ => Err(Error::UnexpectedToken),
            },
            None => self.unexpected_end(),
        }
    }

    fn parse_stop(&mut self) -> PResult {
        self.expect_end(Ok(Command::Stop))
    }

    fn parse_quit(&mut self) -> PResult {
        self.expect_end(Ok(Command::Quit))
    }

    fn parse_time_control(&mut self) -> PResult {
        let mut wtime: Option<u64> = None;
        let mut btime: Option<u64> = None;
        let mut winc: u64 = 0;
        let mut binc: u64 = 0;
        let mut moves_to_go: Option<u64> = None;

        while let Some(token) = self.advance() {
            match token {
                Token::Kw(Keyword::Wtime) => {
                    wtime = Some(self.parse_u64()?);
                }
                Token::Kw(Keyword::Btime) => {
                    btime = Some(self.parse_u64()?);
                }
                Token::Kw(Keyword::Winc) => {
                    winc = self.parse_u64()?;
                }
                Token::Kw(Keyword::Binc) => {
                    binc = self.parse_u64()?;
                }
                Token::Kw(Keyword::MovesToGo) => moves_to_go = Some(self.parse_u64()?),
                _ => {
                    return Err(Error::UnexpectedToken);
                }
            }
        }

        let (Some(wtime), Some(btime)) = (wtime, btime) else {
            return Err(Error::IncompleteTimeControl);
        };

        Ok(Command::Go(TimingMode::Timed(TimeControl::new(
            wtime,
            btime,
            winc,
            binc,
            moves_to_go,
        ))))
    }

    fn parse_depth(&mut self) -> PResult {
        self.advance().ok_or(Error::UnexpectedEnd)?;

        let depth = u8::try_from(self.parse_integer()?).map_err(|_| Error::ExpectedNumber)?;
        if depth == 0 {
            return Err(Error::ExpectedNumber);
        }
        self.expect_end(Ok(Command::Go(TimingMode::Depth(depth))))
    }

    fn parse_infinite(&mut self) -> PResult {
        self.advance().ok_or(Error::UnexpectedEnd)?;
        self.expect_end(Ok(Command::Go(TimingMode::Infinite)))
    }

    fn parse_movetime(&mut self) -> PResult {
        self.advance().ok_or(Error::UnexpectedEnd)?;

        let movetime = self.parse_u64()?;
        self.expect_end(Ok(Command::Go(TimingMode::MoveTime(movetime))))
    }

    fn unsupported_time_control(&mut self) -> PResult {
        Err(Error::UnsupportedTimeControl)
    }

    fn parse_debug(&mut self) -> PResult {
        let command = match self.advance() {
            Some(Token::Kw(Keyword::On)) => Command::SetOption(EngineOpt::DebugMode(true)),
            Some(Token::Kw(Keyword::Off)) => Command::SetOption(EngineOpt::DebugMode(false)),
            Some(_) => return self.unexpected_token(),
            None => return self.unexpected_end(),
        };
        self.expect_end(Ok(command))
    }

    fn parse_setoption(&mut self) -> PResult {
        self.expect_kw(Keyword::Name)?;

        match self.parse_string()? {
            "Hash" => self.parse_hash(),
            _ => Err(Error::InvalidOption),
        }
    }

    fn parse_hash(&mut self) -> PResult {
        self.expect_kw(Keyword::Value)?;

        let v = self.parse_integer()?;
        if !(1..=1024).contains(&v) {
            return Err(Error::ExpectedNumber);
        }

        self.expect_end(Ok(Command::SetOption(EngineOpt::Hash(v))))
    }

    fn parse_display(&mut self) -> PResult {
        if let Some(token) = self.advance() {
            match token {
                Token::Kw(Keyword::Lichess) => self.parse_display_lichess(),
                _ => self.unexpected_token(),
            }
        } else {
            self.expect_end(Ok(Command::Display))
        }
    }

    fn parse_display_lichess(&mut self) -> PResult {
        self.expect_end(Ok(Command::DisplayLichess))
    }

    fn parse_move(&mut self) -> PResult {
        let mov = self.parse_string()?;
        self.expect_end(Ok(Command::Move(mov.to_string())))
    }

    fn parse_config(&mut self) -> PResult {
        self.expect_end(Ok(Command::Config))
    }

    fn parse_perft(&mut self) -> PResult {
        let d = self.parse_integer()?;
        self.expect_end(Ok(Command::Perft(d)))
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
            "register" => Token::Kw(Keyword::Register),
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
            "dl" => Token::Kw(Keyword::DisplayLichess),
            "lichess" => Token::Kw(Keyword::Lichess),
            "display" => Token::Kw(Keyword::Display),
            "move" => Token::Kw(Keyword::Move),
            "config" => Token::Kw(Keyword::Config),
            "perft" => Token::Kw(Keyword::Perft),
            _ => Token::String(t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::position::Player;

    #[test]
    fn reserved_standalone_tokens_return_errors_without_panicking() {
        let reserved = [
            "debug",
            "on",
            "off",
            "setoption",
            "name",
            "value",
            "register",
            "ucinewgame extra",
            "position",
            "fen",
            "startpos",
            "moves",
            "go",
            "searchmoves",
            "ponder",
            "wtime",
            "btime",
            "winc",
            "binc",
            "movestogo",
            "depth",
            "nodes",
            "mate",
            "movetime",
            "infinite",
            "ponderhit",
            "lichess",
        ];

        for input in reserved {
            let result = std::panic::catch_unwind(|| Parser::parse(input));
            assert!(result.is_ok(), "parser panicked for {input:?}");
            assert!(result.unwrap().is_err(), "parser accepted {input:?}");
        }
    }

    #[test]
    fn oversized_and_invalid_numeric_values_are_rejected() {
        for input in [
            "go depth 0",
            "go depth 256",
            "go depth 999999999999999999999999999999999999",
            "go movetime 999999999999999999999999999999999999",
            "setoption name Hash value 0",
            "setoption name Hash value 1025",
            "perft 999999999999999999999999999999999999",
        ] {
            assert!(Parser::parse(input).is_err(), "parser accepted {input:?}");
        }
    }

    #[test]
    fn commands_reject_trailing_tokens_consistently() {
        for input in [
            "uci extra",
            "isready extra",
            "ucinewgame extra",
            "stop extra",
            "quit extra",
            "debug on extra",
            "setoption name Hash value 16 extra",
            "go depth 1 extra",
            "go infinite extra",
            "go movetime 1 extra",
            "move e2e4 extra",
            "perft 1 extra",
        ] {
            assert!(Parser::parse(input).is_err(), "parser accepted {input:?}");
        }
    }

    #[test]
    fn parses_setoption_and_ucinewgame() {
        assert!(matches!(
            Parser::parse("setoption name Hash value 32"),
            Ok(Command::SetOption(EngineOpt::Hash(32)))
        ));
        assert!(matches!(
            Parser::parse("ucinewgame"),
            Ok(Command::UciNewGame)
        ));
    }

    #[test]
    fn parses_move_time_above_u32_max_without_narrowing() {
        let command = Parser::parse("go movetime 4294967296").unwrap();

        assert!(matches!(
            command,
            Command::Go(TimingMode::MoveTime(4_294_967_296))
        ));
    }

    #[test]
    fn parses_large_timed_control_values_without_narrowing() {
        let command =
            Parser::parse("go wtime 85899345900 btime 1000 winc 4294967296 binc 0 movestogo 20")
                .unwrap();
        let Command::Go(TimingMode::Timed(control)) = command else {
            panic!("expected a timed go command");
        };

        // (85899345900 - 30) / 20 + 4294967296, well clear of the share cap. The point is that
        // neither the clock nor the increment narrows to u32 on the way through.
        let move_time = control.to_move_time(1, Player::WHITE);
        assert_eq!(move_time, 8_589_934_589);
        assert!(move_time > u64::from(u32::MAX));
    }
}
