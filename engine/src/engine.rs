use super::info::{format_search_event, format_search_outcome};
use super::nnue::Network;
use super::options::{advertised_uci_options, EngineConfig, EngineOpt};
use super::search::{SearchEngine, SearchEvent, SearchHandle, SearchLimit};
use super::time::{self, TimingMode};
use super::uci::{self, Command};
use chess::position::Position;

use crossbeam_channel::{select, unbounded, Receiver};

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

enum Input {
    Command(Command),
    ParseError(String),
    Closed,
}

enum DriverEvent {
    Input(Result<Input, crossbeam_channel::RecvError>),
    /// A typed update from a still-running search.
    SearchProgress(SearchEvent),
    /// The active search's worker thread has finished, however that was observed.
    SearchComplete,
}

/// How often [`next_event`] rechecks a running search's liveness directly.
///
/// This is only a backstop. Completion is normally observed the instant the worker
/// signals it; the poll exists so that a lost channel wakeup costs at most this much
/// latency instead of deadlocking the driver forever.
const SEARCH_LIVENESS_POLL: Duration = Duration::from_millis(50);

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
    run_detached(info, io::stdin(), io::stdout(), io::stderr());
}

/// Run the production driver with a detached input reader.
///
/// A UCI host normally keeps stdin open for the engine's lifetime, so the reader can be blocked in
/// `read_line` when the driver panics. It must not be a scoped thread: unwinding a scope joins the
/// reader and turns the panic into a permanent hang. A detached reader is harmless during normal
/// shutdown and is terminated by the operating system if the main/driver thread panics.
fn run_detached<R, W, E>(info: EngineInfo, input: R, output: W, errors: E)
where
    R: Read + Send + 'static,
    W: Write,
    E: Write,
{
    chess::init::init_globals();

    let (uci_tx, uci_rx) = unbounded();
    thread::spawn(move || read_commands(BufReader::new(input), uci_tx));
    drive(info, uci_rx, output, errors);
}

#[cfg(test)]
fn run<R, W, E>(info: EngineInfo, input: R, mut output: W, mut errors: E)
where
    R: Read + Send,
    W: Write,
    E: Write,
{
    chess::init::init_globals();

    thread::scope(|scope| {
        let (uci_tx, uci_rx) = unbounded();
        scope.spawn(move || read_commands(BufReader::new(input), uci_tx));
        drive(info, uci_rx, &mut output, &mut errors);
    });
}

fn drive<W, E>(info: EngineInfo, uci_rx: Receiver<Input>, mut output: W, mut errors: E)
where
    W: Write,
    E: Write,
{
    let mut config = EngineConfig::new();
    let mut search_engine = SearchEngine::new(config.hash_mb());
    let mut active_search: Option<SearchHandle> = None;
    let mut pos = Position::start_pos();

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
            DriverEvent::Input(Ok(Input::Command(Command::SetOption(option)))) => match option {
                EngineOpt::Hash(size) => {
                    // Resizing the hash reallocates the table the search shares, so the active
                    // search must be cancelled and joined first: replacing the allocation while a
                    // worker still holds it is exactly what `SearchEngine::set_hash_size` refuses to
                    // do. The config is updated in step with the resource so the two never disagree,
                    // and a value the engine cannot apply leaves both untouched.
                    if let Some(search) = active_search.take() {
                        stop_search(search, &mut output);
                    }
                    match config.set_hash_mb(size) {
                        Ok(()) => search_engine.set_hash_size(config.hash_mb()),
                        Err(err) => {
                            let _ = writeln!(errors, "error: {err}");
                        }
                    }
                }
                // Debug is a mode flag rather than a resource: it allocates nothing and must not
                // disturb a running search, so it is recorded without a quiescent boundary.
                EngineOpt::DebugMode(on) => config.set_debug(on),
                // Selecting a network changes what every subsequent leaf evaluates to. A running
                // search keeps the handle it started with, but the change is applied at a quiescent
                // boundary anyway — stopping first — so the switch is unambiguous and the loaded
                // weights never race a worker mid-search. A file that will not load leaves the
                // current selection untouched and reports the reason, exactly as a rejected hash
                // does.
                EngineOpt::EvalFile(path) => {
                    if let Some(search) = active_search.take() {
                        stop_search(search, &mut output);
                    }
                    // The transposition table caches each position's static evaluation, which is a
                    // property of the evaluation function, not the position. Swapping evaluators
                    // therefore invalidates those cached values, so a successful change clears the
                    // hash — as `ucinewgame` does — before the next search can read a stale eval. A
                    // file that fails to load changes nothing and leaves the hash intact.
                    match path {
                        None => {
                            search_engine.set_network(None);
                            search_engine.new_game();
                        }
                        Some(path) => match load_network(&path) {
                            Ok(network) => {
                                search_engine.set_network(Some(Arc::new(network)));
                                search_engine.new_game();
                            }
                            Err(err) => {
                                let _ = writeln!(
                                    errors,
                                    "error: could not load EvalFile {}: {err}",
                                    path.display()
                                );
                            }
                        },
                    }
                }
            },
            DriverEvent::Input(Ok(Input::Command(Command::Go(timing)))) => {
                if let Some(search) = active_search.take() {
                    stop_search(search, &mut output);
                }

                let limit = match timing {
                    TimingMode::Depth(depth) => SearchLimit::Depth(depth),
                    TimingMode::Nodes(nodes) => SearchLimit::Nodes(nodes),
                    TimingMode::Infinite => SearchLimit::Infinite,
                    TimingMode::Timed(tc) => {
                        SearchLimit::Time(tc.to_move_budget(pos.move_number(), pos.turn()).into())
                    }
                    TimingMode::MoveTime(time) => {
                        SearchLimit::Time(time::move_time_budget(time).into())
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
                handle_command(&info, command, &config, &mut pos, &mut output, &mut errors);
            }
            DriverEvent::SearchProgress(event) => {
                let _ = writeln!(output, "{}", format_search_event(&event));
            }
            DriverEvent::SearchComplete => {
                // `next_event` only reports completion for a search it was given,
                // so a handle is necessarily active here.
                if let Some(search) = active_search.take() {
                    finish_search(search, &mut output);
                }
            }
        }
    }
}

/// Load and validate an `SBNN` network file for the `EvalFile` option.
///
/// Buffered because the loader reads the header and parameter blob in many small reads. Both a
/// filesystem failure and a rejected file are surfaced as one error so the driver can report either
/// cause without inspecting which happened.
fn load_network(path: &Path) -> Result<Network, Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(File::open(path)?);
    Ok(Network::read(&mut reader)?)
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
    let Some(search) = search else {
        return DriverEvent::Input(commands.recv());
    };

    loop {
        select! {
            recv(commands) -> command => return DriverEvent::Input(command),
            recv(search.events()) -> event => match event {
                Ok(event) => return DriverEvent::SearchProgress(event),
                // The worker dropped its event `Sender`. This used to be the only
                // completion signal; it is now merely the earliest of three.
                Err(_) => return DriverEvent::SearchComplete,
            },
            recv(search.finished()) -> _ => return DriverEvent::SearchComplete,
            default(SEARCH_LIVENESS_POLL) => {
                // Nothing woke us. Ask the thread itself rather than trusting that a
                // wakeup would have arrived, so a finished search is always noticed.
                if search.is_finished() {
                    return DriverEvent::SearchComplete;
                }
            }
        }
    }
}

fn handle_command<W: Write, E: Write>(
    info: &EngineInfo,
    command: Command,
    config: &EngineConfig,
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
            let _ = writeln!(output, "{}", advertised_uci_options());
            let _ = writeln!(output, "uciok");
        }
        Command::IsReady => {
            let _ = writeln!(output, "readyok");
        }
        Command::Config => {
            let _ = writeln!(output, "{config}");
        }
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
    use std::process::{Command as ProcessCommand, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

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

    fn bestmove_from(output: &str) -> &str {
        let mut bestmoves = output
            .lines()
            .filter_map(|line| line.strip_prefix("bestmove "));
        let bestmove = bestmoves.next().expect("driver must emit a bestmove");
        assert!(
            bestmoves.next().is_none(),
            "driver emitted multiple bestmoves"
        );
        bestmove
    }

    fn assert_eof_returns_legal_move(mut position: Position, position_command: &str) {
        let (output, errors) = run_script(&format!("{position_command}go infinite\n"));
        let bestmove = bestmove_from(&output);

        assert_ne!(bestmove, "0000", "non-terminal EOF returned a null move");
        assert!(
            position.make_uci_move(bestmove).is_some(),
            "EOF returned illegal move {bestmove} for {position_command:?}"
        );
        assert_eq!(diagnostics_after_banner(&errors), "");
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
    fn stdin_eof_during_search_emits_a_legal_bestmove() {
        assert_eof_returns_legal_move(Position::start_pos(), "");
    }

    #[test]
    fn stdin_eof_emits_null_bestmove_only_for_terminal_positions() {
        // Keep a non-terminal control in this boundary test: besides distinguishing terminal
        // positions, it makes the test sensitive to removal of the minimum-search guarantee.
        assert_eof_returns_legal_move(Position::start_pos(), "position startpos\n");

        for fen in [
            "7k/5QQ1/8/8/8/8/8/7K b - - 0 1",
            "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1",
        ] {
            let (output, errors) = run_script(&format!("position fen {fen}\ngo infinite\n"));
            assert_eq!(bestmove_from(&output), "0000", "terminal FEN: {fen}");
            assert_eq!(diagnostics_after_banner(&errors), "");
        }
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
        // `ucinewgame` is handled silently: beyond the startup banner on the
        // diagnostic channel, no error diagnostics are produced.
        assert_eq!(diagnostics_after_banner(&errors), "");
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
    fn hash_change_while_searching_applies_at_a_quiescent_boundary() {
        // The resize reallocates the shared table, which `SearchEngine::set_hash_size` refuses to do
        // while any worker still holds it. If the driver did not stop and join the active search
        // first, the resize would panic the driver thread; a clean `readyok` after it proves the
        // search was quiesced before the table was replaced.
        let (output, errors) =
            run_script("go infinite\nsetoption name Hash value 1\nisready\nquit\n");
        assert!(output.contains("bestmove "));
        assert!(output.contains("readyok"));
        assert_eq!(diagnostics_after_banner(&errors), "");
    }

    #[test]
    fn repeated_hash_changes_are_each_applied_cleanly() {
        let (output, errors) = run_script(
            "setoption name Hash value 1\n\
             setoption name Hash value 256\n\
             setoption name Hash value 16\n\
             isready\nquit\n",
        );
        assert!(output.contains("readyok"));
        assert_eq!(diagnostics_after_banner(&errors), "");
    }

    #[test]
    fn eval_file_option_loads_a_valid_network_and_keeps_searching() {
        // The committed golden network is a valid SBNN file. Selecting it must be accepted silently
        // (no diagnostics beyond the banner) and leave the engine able to search and report a move,
        // proving the load reached the search path rather than merely being parsed. The path is
        // relative to the package directory, the working directory of a cargo test binary.
        let (output, errors) = run_script(
            "setoption name EvalFile value tests/fixtures/golden_v1.sbnn\n\
             go depth 4\nisready\nquit\n",
        );
        assert!(output.contains("bestmove "));
        assert!(output.contains("readyok"));
        assert_eq!(diagnostics_after_banner(&errors), "");
    }

    #[test]
    fn eval_file_load_failure_is_reported_without_stopping_the_engine() {
        // A path that does not resolve to a loadable network reports the reason on the diagnostic
        // channel and changes nothing: the engine stays on its previous evaluation and keeps
        // running, so a later `isready` still answers and a search still returns a move.
        let (output, errors) = run_script(
            "setoption name EvalFile value does/not/exist.sbnn\n\
             go depth 2\nisready\nquit\n",
        );
        assert!(output.contains("bestmove "));
        assert!(output.contains("readyok"));
        assert!(diagnostics_after_banner(&errors).contains("could not load EvalFile"));
    }

    #[test]
    fn config_command_reflects_the_authoritative_configuration() {
        // The `config` command reads the one owner of the runtime settings, so it must show the
        // defaults first and then the values a `setoption`/`debug` sequence applied — evidence that
        // those commands feed a single authoritative config rather than ad hoc state.
        let (output, errors) =
            run_script("config\nsetoption name Hash value 256\ndebug on\nconfig\nquit\n");
        assert!(output.contains("hash 16 MB, threads 1, debug off"));
        assert!(output.contains("hash 256 MB, threads 1, debug on"));
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
             option name EvalFile type string default <empty>\n\
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

    /// Completion must be observed through the explicit signal, never only through
    /// the events channel disconnecting when the worker drops its `Sender`.
    ///
    /// The test pins that channel open for the whole search, which is exactly the
    /// state a lost disconnect wakeup leaves the driver in. Without the explicit
    /// signal `next_event` has no other way to learn the worker has exited, so this
    /// parks forever; the watchdog turns that hang into a failure instead of a
    /// wedged test binary.
    #[test]
    fn search_completion_is_observed_without_an_events_disconnect() {
        const ITERATIONS: usize = 20;
        const PER_SEARCH_TIMEOUT: Duration = Duration::from_secs(30);

        let (done_tx, done_rx) = unbounded::<usize>();
        let searcher = thread::spawn(move || {
            chess::init::init_globals();
            let engine = SearchEngine::new(1);
            // Never written to, but kept alive so the command arm of the select
            // stays open and completion is the only way out of `next_event`.
            let (_command_tx, command_rx) = unbounded::<Input>();

            for iteration in 0..ITERATIONS {
                // Alternate the two ways a search ends: running to its depth limit,
                // and being cancelled while unbounded.
                let cancelled = iteration % 2 == 1;
                let limit = if cancelled {
                    SearchLimit::Infinite
                } else {
                    SearchLimit::Depth(2)
                };
                let (handle, retained_events) =
                    engine.start_retaining_events(Position::start_pos(), limit);
                if cancelled {
                    handle.cancel();
                }

                loop {
                    match next_event(&command_rx, Some(&handle)) {
                        DriverEvent::SearchProgress(_) => {}
                        DriverEvent::SearchComplete => break,
                        DriverEvent::Input(_) => unreachable!("no commands are sent"),
                    }
                }

                let outcome = handle.wait();
                assert_eq!(outcome.was_cancelled(), cancelled);
                drop(retained_events);
                if done_tx.send(iteration).is_err() {
                    break;
                }
            }
        });

        for iteration in 0..ITERATIONS {
            match done_rx.recv_timeout(PER_SEARCH_TIMEOUT) {
                Ok(observed) => assert_eq!(observed, iteration),
                Err(_) => panic!(
                    "search {iteration} completed but the driver never observed it; \
                     completion still depends on the events channel disconnecting"
                ),
            }
        }
        searcher.join().unwrap();
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

    struct PanickingWriter;

    impl Write for PanickingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            panic!("injected UCI driver panic")
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn driver_panic_does_not_join_a_blocked_input_reader() {
        const TIMEOUT: Duration = Duration::from_secs(2);

        let (input_tx, input_rx) = unbounded::<Vec<u8>>();
        let (done_tx, done_rx) = unbounded();
        thread::spawn(move || {
            let panicked = std::panic::catch_unwind(|| {
                run_detached(
                    TEST_INFO,
                    ChannelReader(input_rx),
                    PanickingWriter,
                    io::sink(),
                )
            })
            .is_err();
            let _ = done_tx.send(panicked);
        });

        input_tx.send(b"isready\n".to_vec()).unwrap();
        assert_eq!(
            done_rx.recv_timeout(TIMEOUT),
            Ok(true),
            "driver unwind waited for the still-open input reader"
        );
    }

    /// Subprocess probe used by `driver_panic_exits_the_process_nonzero`. The sender is deliberately
    /// leaked so the detached reader remains blocked exactly as it does under a UCI runner.
    #[test]
    #[ignore = "launched explicitly by the subprocess regression"]
    fn driver_panic_process_probe() {
        let (input_tx, input_rx) = unbounded::<Vec<u8>>();
        input_tx.send(b"isready\n".to_vec()).unwrap();
        std::mem::forget(input_tx);
        run_detached(
            TEST_INFO,
            ChannelReader(input_rx),
            PanickingWriter,
            io::sink(),
        );
    }

    #[test]
    fn driver_panic_exits_the_process_nonzero() {
        const TIMEOUT: Duration = Duration::from_secs(2);

        let mut child = ProcessCommand::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "engine::tests::driver_panic_process_probe",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + TIMEOUT;

        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(
                    !status.success(),
                    "driver panic must produce a non-zero exit"
                );
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("panicking driver process remained blocked on its stdin reader");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
