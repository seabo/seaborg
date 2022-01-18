// uci::cmd - handles the cmdline interaction, which includes:
//             - reading in commands on demand
//             - parsing of UCI commands
//             - dispatch of responses to the GUI on demand
use super::{EngineCommand, SessionCommand};
use std::io::{self, Stdin};

/// The tokens which can be sent by the GUI. We will either have
/// a reserved keyword, a FEN string, or a number or some other
/// general string.
#[derive(Clone, Debug)]
enum GuiToken<'a> {
    Keyword(GuiKeyword),
    String(&'a str),
    /// Special token used to initialise the parser
    Init,
}

/// The reserved keywords which can be sent from the GUI to the
/// engine.
#[derive(Copy, Clone, Debug)]
enum GuiKeyword {
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
    UnexpectedToken(String),
    Io(String),
    NoInput,
}

impl std::convert::From<io::Error> for ParseError {
    fn from(err: io::Error) -> ParseError {
        ParseError::Io(format!("{}", err))
    }
}

/// The result of attempting to parse a new string from stdin.
pub type ParseResult = Result<SessionCommand, ParseError>;

/// A struct for parsing a command sent by the GUI via stdin.
pub struct UciParser<'a> {
    toks: Vec<GuiToken<'a>>,
    cursor: usize,
}

impl<'a> UciParser<'a> {
    /// Read the next command from stdin using the provided handle,
    /// parse that command and return a `ParseResult` containing an
    /// `EngineCommand` or a `ParseError`.
    pub fn next_command(stdin: &Stdin) -> ParseResult {
        let mut buf = String::new();
        stdin.read_line(&mut buf)?;

        let mut uci_parser = UciParser::new(&buf);

        uci_parser.parse_command()
    }
    fn new(buf: &'a str) -> Self {
        let mut toks = Vec::new();
        for tok in buf.split_whitespace() {
            toks.push(UciParser::scan_token(tok));
        }

        Self { toks, cursor: 0 }
    }

    fn advance(&mut self) -> Option<GuiToken<'a>> {
        if self.cursor < self.toks.len() {
            let next = self.toks[self.cursor].clone();
            self.cursor += 1;
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

    fn parse_keyword(&mut self, tok: GuiToken) -> ParseResult {
        match tok {
            GuiToken::Keyword(GuiKeyword::Uci) => Ok(SessionCommand::Uci),
            GuiToken::Keyword(GuiKeyword::Debug) => todo!(),
            GuiToken::Keyword(GuiKeyword::On) => todo!(),
            GuiToken::Keyword(GuiKeyword::Off) => todo!(),
            GuiToken::Keyword(GuiKeyword::IsReady) => Ok(SessionCommand::IsReady),
            GuiToken::Keyword(GuiKeyword::SetOption) => todo!(),
            GuiToken::Keyword(GuiKeyword::UciNewGame) => todo!(),
            GuiToken::Keyword(GuiKeyword::Position) => todo!(),
            GuiToken::Keyword(GuiKeyword::Fen) => todo!(),
            GuiToken::Keyword(GuiKeyword::Startpos) => todo!(),
            GuiToken::Keyword(GuiKeyword::Go) => todo!(),
            GuiToken::Keyword(GuiKeyword::SearchMoves) => todo!(),
            GuiToken::Keyword(GuiKeyword::Ponder) => todo!(),
            GuiToken::Keyword(GuiKeyword::Wtime) => todo!(),
            GuiToken::Keyword(GuiKeyword::Btime) => todo!(),
            GuiToken::Keyword(GuiKeyword::Winc) => todo!(),
            GuiToken::Keyword(GuiKeyword::Binc) => todo!(),
            GuiToken::Keyword(GuiKeyword::MovesToGo) => todo!(),
            GuiToken::Keyword(GuiKeyword::Depth) => todo!(),
            GuiToken::Keyword(GuiKeyword::Nodes) => todo!(),
            GuiToken::Keyword(GuiKeyword::Mate) => todo!(),
            GuiToken::Keyword(GuiKeyword::MoveTime) => todo!(),
            GuiToken::Keyword(GuiKeyword::Infinite) => todo!(),
            GuiToken::Keyword(GuiKeyword::Stop) => todo!(),
            GuiToken::Keyword(GuiKeyword::PonderHit) => todo!(),
            GuiToken::Keyword(GuiKeyword::Quit) => todo!(),
            _ => Err(ParseError::UnexpectedToken(
                "expected a uci keyword".to_string(),
            )),
        }
    }

    fn scan_token(str: &str) -> GuiToken {
        match str {
            "uci" => GuiToken::Keyword(GuiKeyword::Uci),
            "debug" => GuiToken::Keyword(GuiKeyword::Debug),
            "on" => GuiToken::Keyword(GuiKeyword::On),
            "off" => GuiToken::Keyword(GuiKeyword::Off),
            "isready" => GuiToken::Keyword(GuiKeyword::IsReady),
            "setoption" => GuiToken::Keyword(GuiKeyword::SetOption),
            "ucinewgame" => GuiToken::Keyword(GuiKeyword::UciNewGame),
            "position" => GuiToken::Keyword(GuiKeyword::Position),
            "fen" => GuiToken::Keyword(GuiKeyword::Fen),
            "startpos" => GuiToken::Keyword(GuiKeyword::Startpos),
            "go" => GuiToken::Keyword(GuiKeyword::Go),
            "searchmoves" => GuiToken::Keyword(GuiKeyword::SearchMoves),
            "ponder" => GuiToken::Keyword(GuiKeyword::Ponder),
            "wtime" => GuiToken::Keyword(GuiKeyword::Wtime),
            "btime" => GuiToken::Keyword(GuiKeyword::Btime),
            "winc" => GuiToken::Keyword(GuiKeyword::Winc),
            "binc" => GuiToken::Keyword(GuiKeyword::Binc),
            "movestogo" => GuiToken::Keyword(GuiKeyword::MovesToGo),
            "depth" => GuiToken::Keyword(GuiKeyword::Depth),
            "nodes" => GuiToken::Keyword(GuiKeyword::Nodes),
            "mate" => GuiToken::Keyword(GuiKeyword::Mate),
            "movetime" => GuiToken::Keyword(GuiKeyword::MoveTime),
            "infinite" => GuiToken::Keyword(GuiKeyword::Infinite),
            "stop" => GuiToken::Keyword(GuiKeyword::Stop),
            "ponderhit" => GuiToken::Keyword(GuiKeyword::PonderHit),
            "quit" => GuiToken::Keyword(GuiKeyword::Quit),
            _ => GuiToken::String(str),
        }
    }
}
