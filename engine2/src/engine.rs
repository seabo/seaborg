use super::score::Score;
use super::search::{Master, Search, Worker};
use super::time::TimingMode;
use super::tt::Table;
use super::uci::{self, Command, Error};
use core::position::Position;

use crossbeam_channel::unbounded;

use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    io,
    thread::{self, Scope},
};

/// Launch the engine process.
pub fn launch() {
    core::init::init_globals();

    let stop_flag = AtomicBool::new(false);
    let flag = &stop_flag;

    let tt = Table::new(16);

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

        loop {
            match uci_rx.try_recv() {
                Ok(Command::Quit) => {
                    stop_flag.store(true, Ordering::Relaxed);
                    break;
                }
                Ok(Command::Stop) => {
                    stop_flag.store(true, Ordering::Relaxed);
                }
                Ok(Command::Go(d)) => match d {
                    TimingMode::Depth(depth) => {
                        stop_flag.store(false, Ordering::Relaxed);
                        launch_search(s, flag, 1, depth, pos.clone(), &tt);
                    }
                    _ => todo!(),
                },
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
                Ok(cmd) => println!("{:?}", cmd),
                Err(_err) => {}
            }
        }
    });
}

fn launch_search<'scope, 'engine>(
    s: &'scope Scope<'scope, 'engine>,
    flag: &'engine AtomicBool,
    num_threads: u8,
    depth: u8,
    pos: Position,
    tt: &'engine Table,
) {
    for i in 0..num_threads {
        let thread_pos = pos.clone();
        s.spawn(move || {
            let mut search = Search::new(thread_pos, flag, tt);
            if i == 0 {
                search.start_search::<Master>(depth);
            } else {
                search.start_search::<Worker>(depth);
            }
        });
    }
}
