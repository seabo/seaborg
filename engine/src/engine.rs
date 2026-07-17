use super::info::{format_search_event, format_search_outcome};
use super::search::{SearchEngine, SearchHandle, SearchLimit};
use super::time::TimingMode;
use super::uci::{self, Command};
use core::position::Position;

use crossbeam_channel::unbounded;

use std::time::Duration;
use std::{io, thread};

/// Launch the engine process.
pub fn launch() {
    core::init::init_globals();

    let search_engine = SearchEngine::new(16);
    let mut active_search: Option<SearchHandle> = None;

    let mut pos = Position::start_pos();

    // Everything happens inside a global thread scope.
    thread::scope(|s| {
        let (uci_tx, uci_rx) = unbounded::<uci::Command>();

        // Launch the UCI thread.
        s.spawn(move || {
            let mut buf: String = String::with_capacity(256);
            loop {
                buf.clear();
                io::stdin()
                    .read_line(&mut buf)
                    .expect("couldn't read from stdin");

                match uci::Parser::parse(&buf.clone()) {
                    Ok(cmd @ Command::Quit) => {
                        let _ = uci_tx.send(cmd);
                        break;
                    }
                    Ok(cmd) => {
                        let _ = uci_tx.send(cmd);
                    }
                    Err(err) => {
                        eprintln!("error: {:?}", err);
                    }
                }
            }
        });

        println!("seaborg 0.0.2 by George Seabridge");
        println!("commit: {}", env!("GIT_HASH"));

        loop {
            match uci_rx.try_recv() {
                Ok(Command::Quit) => {
                    if let Some(search) = active_search.take() {
                        search.cancel();
                        finish_search(search);
                    }
                    break;
                }
                Ok(Command::Stop) => {
                    if let Some(search) = &active_search {
                        search.cancel();
                    }
                }
                Ok(Command::Go(timing)) => {
                    if let Some(search) = active_search.take() {
                        search.cancel();
                        finish_search(search);
                    }

                    let limit = match timing {
                        TimingMode::Depth(depth) => SearchLimit::Depth(depth),
                        TimingMode::Infinite => SearchLimit::Infinite,
                        TimingMode::Timed(tc) => {
                            let move_time = tc.to_move_time(pos.move_number(), pos.turn());
                            SearchLimit::Time(Duration::from_millis(move_time.into()))
                        }
                        TimingMode::MoveTime(time) => {
                            SearchLimit::Time(Duration::from_millis(time as u64))
                        }
                    };
                    active_search = Some(search_engine.start(pos.clone(), limit));
                }
                Ok(Command::SetPosition((fen, moves))) => match Position::from_fen(&fen) {
                    Ok(mut p) => {
                        for mov in moves {
                            if p.make_uci_move(&mov).is_none() {
                                println!("invalid move {}", mov);
                            }
                        }
                        pos = p;
                    }
                    Err(err) => println!("invalid position; {}", err),
                },
                Ok(Command::Display) => println!("{}", pos),
                Ok(Command::DisplayLichess) => {
                    let fen_url_safe = pos.to_fen().replace(" ", "_");
                    let lichess_url =
                        format!("https://lichess.org/analysis/standard/{}", fen_url_safe);

                    let _ = open::that(lichess_url);
                }
                Ok(Command::Move(mov)) => match pos.make_uci_move(&mov) {
                    Some(_) => {}
                    None => {
                        // If the move wasn't valid uci, try to see if it was SAN.
                        match pos.move_from_san(&mov) {
                            Some(mov) => pos.make_move(&mov),
                            None => println!("illegal move: {}", mov),
                        }
                    }
                },
                Ok(Command::Perft(d)) => {
                    super::perft::Perft::divide(&mut pos, d, true, false);
                }
                Ok(Command::Uci) => {
                    println!("id name seaborg 0.0.2");
                    println!("id author George Seabridge");
                    println!("uciok");
                }
                Ok(Command::IsReady) => {
                    println!("readyok");
                }
                Ok(cmd) => println!("{:?}: not yet implemented", cmd),
                Err(_err) => {}
            }

            if let Some(search) = &active_search {
                report_search_events(search);
            }
            if active_search
                .as_ref()
                .is_some_and(SearchHandle::is_finished)
            {
                finish_search(active_search.take().unwrap());
            }
        }
    });
}

fn report_search_events(search: &SearchHandle) {
    for event in search.events().try_iter() {
        println!("{}", format_search_event(&event));
    }
}

fn finish_search(search: SearchHandle) {
    let events = search.events().clone();
    let outcome = search.wait();
    for event in events.try_iter() {
        println!("{}", format_search_event(&event));
    }
    println!("{}", format_search_outcome(&outcome));
}
