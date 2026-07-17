use super::info::{format_search_event, format_search_outcome};
use super::options::EngineOpt;
use super::search::{SearchEngine, SearchEvent, SearchHandle, SearchLimit};
use super::time::TimingMode;
use super::uci::{self, Command};
use core::position::Position;

use crossbeam_channel::{select, unbounded, Receiver};

use std::io::{self, BufRead, BufReader, Read, Write};
use std::thread;
use std::time::Duration;

enum Input {
    Command(Command),
    ParseError(String),
    Closed,
}

enum DriverEvent {
    Input(Result<Input, crossbeam_channel::RecvError>),
    Search(Result<SearchEvent, crossbeam_channel::RecvError>),
}

/// Authoritative engine identity used for UCI `id` responses and human
/// diagnostics. All fields are supplied by the host binary so that the UCI
/// `id name`, the command-line `--version`, and any startup banner derive from
/// a single package version rather than drifting hardcoded strings.
#[derive(Clone, Copy, Debug)]
pub struct EngineInfo {
    /// Engine name, e.g. `seaborg`.
    pub name: &'static str,
    /// Package version, typically `env!("CARGO_PKG_VERSION")`.
    pub version: &'static str,
    /// Human-facing author string.
    pub author: &'static str,
    /// Full Git commit hash, typically `env!("GIT_HASH")`.
    pub commit: &'static str,
}

impl EngineInfo {
    /// Trimmed, human-facing form of the commit hash for diagnostics.
    fn short_commit(&self) -> &str {
        const SHORT_LEN: usize = 12;
        match self.commit.char_indices().nth(SHORT_LEN) {
            Some((idx, _)) => &self.commit[..idx],
            None => self.commit,
        }
    }
}

/// Launch the engine process.
pub fn launch(info: EngineInfo) {
    run(info, io::stdin(), io::stdout(), io::stderr());
}

fn run<R, W, E>(info: EngineInfo, input: R, mut output: W, mut errors: E)
where
    R: Read + Send,
    W: Write,
    E: Write,
{
    core::init::init_globals();

    let mut hash_size_mb = 16;
    let mut search_engine = SearchEngine::new(hash_size_mb);
    let mut active_search: Option<SearchHandle> = None;
    let mut pos = Position::start_pos();

    thread::scope(|scope| {
        let (uci_tx, uci_rx) = unbounded();
        scope.spawn(move || read_commands(BufReader::new(input), uci_tx));

        // Protocol stdout must contain only valid UCI traffic, so the human
        // banner (including trimmed commit metadata) goes to the diagnostic
        // channel and never precedes the `uci` handshake on stdout.
        let _ = writeln!(
            errors,
            "{} {} by {} (commit {})",
            info.name,
            info.version,
            info.author,
            info.short_commit()
        );

        loop {
            let event = next_event(&uci_rx, active_search.as_ref());
            match event {
                DriverEvent::Input(Ok(Input::Command(Command::Quit)))
                | DriverEvent::Input(Ok(Input::Closed))
                | DriverEvent::Input(Err(_)) => {
                    if let Some(search) = active_search.take() {
                        stop_search(search, &mut output);
                    }
                    break;
                }
                DriverEvent::Input(Ok(Input::ParseError(err))) => {
                    let _ = writeln!(errors, "error: {err}");
                }
                DriverEvent::Input(Ok(Input::Command(Command::Stop))) => {
                    if let Some(search) = active_search.take() {
                        stop_search(search, &mut output);
                    }
                }
                DriverEvent::Input(Ok(Input::Command(Command::SetOption(option)))) => {
                    if let Some(search) = active_search.take() {
                        stop_search(search, &mut output);
                    }
                    if let EngineOpt::Hash(size) = option {
                        hash_size_mb = size;
                        search_engine = SearchEngine::new(hash_size_mb);
                    }
                }
                DriverEvent::Input(Ok(Input::Command(Command::Go(timing)))) => {
                    if let Some(search) = active_search.take() {
                        stop_search(search, &mut output);
                    }

                    let limit = match timing {
                        TimingMode::Depth(depth) => SearchLimit::Depth(depth),
                        TimingMode::Infinite => SearchLimit::Infinite,
                        TimingMode::Timed(tc) => {
                            let move_time = tc.to_move_time(pos.move_number(), pos.turn());
                            SearchLimit::Time(Duration::from_millis(move_time))
                        }
                        TimingMode::MoveTime(time) => {
                            SearchLimit::Time(Duration::from_millis(time))
                        }
                    };
                    active_search = Some(search_engine.start(pos.clone(), limit));
                }
                DriverEvent::Input(Ok(Input::Command(Command::UciNewGame))) => {
                    if let Some(search) = active_search.take() {
                        stop_search(search, &mut output);
                    }
                    search_engine.new_game();
                }
                DriverEvent::Input(Ok(Input::Command(command))) => {
                    handle_command(&info, command, &mut pos, &mut output, &mut errors);
                }
                DriverEvent::Search(Ok(event)) => {
                    let _ = writeln!(output, "{}", format_search_event(&event));
                }
                DriverEvent::Search(Err(_)) => {
                    finish_search(active_search.take().unwrap(), &mut output);
                }
            }
        }
    });
}

fn read_commands<R: BufRead>(mut input: R, sender: crossbeam_channel::Sender<Input>) {
    let mut buf = String::with_capacity(256);
    loop {
        buf.clear();
        match input.read_line(&mut buf) {
            Ok(0) | Err(_) => {
                let _ = sender.send(Input::Closed);
                break;
            }
            Ok(_) => {
                let (message, quitting) = match uci::Parser::parse(&buf) {
                    Ok(command) => {
                        let quitting = matches!(command, Command::Quit);
                        (Input::Command(command), quitting)
                    }
                    Err(err) => (Input::ParseError(format!("{err:?}")), false),
                };
                if sender.send(message).is_err() {
                    break;
                }
                if quitting {
                    break;
                }
            }
        }
    }
}

fn next_event(commands: &Receiver<Input>, search: Option<&SearchHandle>) -> DriverEvent {
    if let Some(search) = search {
        select! {
            recv(commands) -> command => DriverEvent::Input(command),
            recv(search.events()) -> event => DriverEvent::Search(event),
        }
    } else {
        DriverEvent::Input(commands.recv())
    }
}

fn handle_command<W: Write, E: Write>(
    info: &EngineInfo,
    command: Command,
    pos: &mut Position,
    output: &mut W,
    errors: &mut E,
) {
    match command {
        Command::SetPosition((fen, moves)) => match Position::from_fen(&fen) {
            Ok(mut new_pos) => {
                for mov in moves {
                    if new_pos.make_uci_move(&mov).is_none() {
                        let _ = writeln!(errors, "error: invalid move {mov}");
                    }
                }
                *pos = new_pos;
            }
            Err(err) => {
                let _ = writeln!(errors, "error: invalid position; {err}");
            }
        },
        Command::Display => {
            let _ = writeln!(output, "{pos}");
        }
        Command::DisplayLichess => {
            let fen_url_safe = pos.to_fen().replace(' ', "_");
            let _ = open::that(format!(
                "https://lichess.org/analysis/standard/{fen_url_safe}"
            ));
        }
        Command::Move(mov) => {
            if pos.make_uci_move(&mov).is_none() {
                match pos.move_from_san(&mov) {
                    Some(mov) => pos.make_move(&mov),
                    None => {
                        let _ = writeln!(output, "illegal move: {mov}");
                    }
                }
            }
        }
        Command::Perft(depth) => {
            super::perft::Perft::divide(pos, depth, true, false);
        }
        Command::Uci => {
            let _ = writeln!(output, "id name {} {}", info.name, info.version);
            let _ = writeln!(output, "id author {}", info.author);
            let _ = writeln!(
                output,
                "option name Hash type spin default 16 min 1 max 1024"
            );
            let _ = writeln!(output, "uciok");
        }
        Command::IsReady => {
            let _ = writeln!(output, "readyok");
        }
        Command::Config => {}
        Command::UciNewGame
        | Command::SetOption(_)
        | Command::Go(_)
        | Command::Stop
        | Command::Quit => {}
    }
}

fn stop_search<W: Write>(search: SearchHandle, output: &mut W) {
    search.cancel();
    finish_search(search, output);
}

fn finish_search<W: Write>(search: SearchHandle, output: &mut W) {
    let events = search.events().clone();
    let outcome = search.wait();
    for event in events.try_iter() {
        let _ = writeln!(output, "{}", format_search_event(&event));
    }
    let _ = writeln!(output, "{}", format_search_outcome(&outcome));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl SharedWriter {
        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("test read failure"))
        }
    }

    /// Deterministic identity used to assert exact protocol streams without
    /// depending on the crate's build-time version or commit hash.
    const TEST_INFO: EngineInfo = EngineInfo {
        name: "seaborg",
        version: "9.9.9",
        author: "George Seabridge",
        commit: "0123456789abcdef0123",
    };

    /// The exact stderr banner emitted at startup for [`TEST_INFO`].
    const TEST_BANNER: &str = "seaborg 9.9.9 by George Seabridge (commit 0123456789ab)\n";

    fn run_script(script: &str) -> (String, String) {
        let output = SharedWriter::default();
        let errors = SharedWriter::default();
        run(TEST_INFO, script.as_bytes(), output.clone(), errors.clone());
        (output.contents(), errors.contents())
    }

    /// Diagnostics emitted after the startup banner has been stripped.
    fn diagnostics_after_banner(errors: &str) -> &str {
        errors
            .strip_prefix(TEST_BANNER)
            .expect("stderr must begin with the startup banner")
    }

    #[test]
    fn startup_emits_no_stdout_and_a_trimmed_stderr_banner() {
        let (output, errors) = run_script("");
        // Acceptance #1: no unsolicited non-UCI stdout before the uci command.
        assert_eq!(output, "");
        // Acceptance #4: commit metadata is trimmed and lives on the
        // diagnostic channel, never on protocol stdout.
        assert_eq!(errors, TEST_BANNER);
        assert!(!errors.contains("0123456789abcdef"));
    }

    #[test]
    fn eof_and_read_failure_shutdown_cleanly() {
        let (output, errors) = run_script("");
        assert_eq!(output, "");
        assert_eq!(diagnostics_after_banner(&errors), "");

        let output = SharedWriter::default();
        let errors = SharedWriter::default();
        run(TEST_INFO, FailingReader, output.clone(), errors.clone());
        assert_eq!(output.contents(), "");
        assert_eq!(diagnostics_after_banner(&errors.contents()), "");
    }

    #[test]
    fn idle_driver_blocks_and_remains_ready() {
        let (input_tx, input_rx) = unbounded::<Vec<u8>>();
        let output = SharedWriter::default();
        let thread_output = output.clone();
        let driver = thread::spawn(move || {
            run(
                TEST_INFO,
                ChannelReader(input_rx),
                thread_output,
                io::sink(),
            )
        });

        thread::sleep(Duration::from_millis(25));
        assert!(!driver.is_finished());
        input_tx.send(b"isready\n".to_vec()).unwrap();
        for _ in 0..100 {
            if output.contents().contains("readyok") {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(output.contents().contains("readyok"));
        assert!(!driver.is_finished());
        input_tx.send(b"quit\n".to_vec()).unwrap();
        driver.join().unwrap();
    }

    #[test]
    fn replacement_stop_and_quit_are_serialized() {
        let (output, errors) = run_script("go infinite\ngo depth 1\nstop\ngo infinite\nquit\n");
        assert_eq!(output.matches("bestmove ").count(), 3);
        assert_eq!(diagnostics_after_banner(&errors), "");
    }

    #[test]
    fn uci_new_game_is_an_owner_handled_hash_boundary() {
        let (output, errors) = run_script("ucinewgame\nisready\nquit\n");
        assert!(output.contains("readyok"));
        assert!(!output.contains("UciNewGame: not yet implemented"));
        assert!(errors.is_empty());
    }

    #[test]
    fn standard_state_commands_are_silent_and_supported() {
        let (output, errors) =
            run_script("setoption name Hash value 1\ndebug on\nucinewgame\nisready\nquit\n");

        assert!(output.contains("readyok"));
        assert!(!output.contains("not yet implemented"));
        assert!(!output.contains("SetOption"));
        assert!(!output.contains("UciNewGame"));
        assert_eq!(diagnostics_after_banner(&errors), "");
    }

    #[test]
    fn malformed_and_unsupported_commands_only_write_to_stderr() {
        let (output, errors) = run_script(
            "register\nsetoption name Missing value 1\nposition startpos moves invalid\nquit\n",
        );

        // Acceptance #2: errors never surface as invalid protocol messages on
        // stdout; the whole error stream stays empty here.
        assert_eq!(output, "");
        assert!(errors.contains("UnexpectedToken"));
        assert!(errors.contains("InvalidOption"));
        assert!(errors.contains("invalid move"));
    }

    #[test]
    fn uci_handshake_stream_is_exact() {
        // Acceptance #3/#5: the id name derives from the authoritative version
        // and the handshake is the only stdout traffic produced.
        let (output, errors) = run_script("uci\nquit\n");
        assert_eq!(
            output,
            "id name seaborg 9.9.9\n\
             id author George Seabridge\n\
             option name Hash type spin default 16 min 1 max 1024\n\
             uciok\n"
        );
        assert_eq!(diagnostics_after_banner(&errors), "");
    }

    #[test]
    fn readiness_stream_is_exact() {
        // Acceptance #5: isready yields exactly readyok on stdout.
        let (output, errors) = run_script("isready\nquit\n");
        assert_eq!(output, "readyok\n");
        assert_eq!(diagnostics_after_banner(&errors), "");
    }

    #[test]
    fn completed_search_is_reported_while_input_remains_open() {
        let (input_tx, input_rx) = unbounded::<Vec<u8>>();
        let output = SharedWriter::default();
        let thread_output = output.clone();
        let driver = thread::spawn(move || {
            run(
                TEST_INFO,
                ChannelReader(input_rx),
                thread_output,
                io::sink(),
            )
        });

        input_tx.send(b"go depth 1\n".to_vec()).unwrap();
        for _ in 0..500 {
            if output.contents().contains("bestmove ") {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(output.contents().contains("info depth 1"));
        assert!(output.contents().contains("bestmove "));
        assert!(!driver.is_finished());

        input_tx.send(b"quit\n".to_vec()).unwrap();
        driver.join().unwrap();
    }

    struct ChannelReader(Receiver<Vec<u8>>);

    impl Read for ChannelReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let chunk = self
                .0
                .recv()
                .map_err(|_| io::Error::from(io::ErrorKind::UnexpectedEof))?;
            assert!(chunk.len() <= buf.len());
            buf[..chunk.len()].copy_from_slice(&chunk);
            Ok(chunk.len())
        }
    }
}
