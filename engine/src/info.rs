//! Formatting typed search reports for the UCI protocol.

use super::search::{SearchEvent, SearchOutcome};

/// Format a typed search event as a UCI `info` line.
pub fn format_search_event(event: &SearchEvent) -> String {
    match event {
        SearchEvent::Progress(progress) => {
            let pv = progress
                .principal_variation
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "info depth {} multipv 1 score {} nodes {} nps {} hashfull {} time {} pv {}",
                progress.depth,
                progress.score,
                progress.nodes,
                progress.nps,
                progress.hashfull,
                progress.elapsed.as_millis(),
                pv
            )
        }
        SearchEvent::CurrentMove(current) => format!(
            "info depth {} currmove {} currmovenumber {}",
            current.depth, current.current_move, current.number
        ),
    }
}

/// Format a typed final search outcome as a UCI `bestmove` line.
pub fn format_search_outcome(outcome: &SearchOutcome) -> String {
    format!("bestmove {}", outcome.result().best_move)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::Score;
    use crate::search::{CurrentMove, SearchProgress, SearchResult};
    use core::mov::Move;
    use core::position::Position;
    use std::time::Duration;

    #[test]
    fn formats_progress_as_uci_info() {
        let mut position = Position::start_pos();
        let best_move = position.make_uci_move("e2e4").unwrap();
        let event = SearchEvent::Progress(SearchProgress {
            depth: 4,
            score: Score::cp(23),
            elapsed: Duration::from_millis(17),
            nodes: 1200,
            nps: 70_588,
            hashfull: 12,
            principal_variation: vec![best_move],
        });

        assert_eq!(
            format_search_event(&event),
            "info depth 4 multipv 1 score cp 23 nodes 1200 nps 70588 hashfull 12 time 17 pv e2e4"
        );
    }

    #[test]
    fn formats_current_move_as_uci_info() {
        let mut position = Position::start_pos();
        let current_move = position.make_uci_move("g1f3").unwrap();
        let event = SearchEvent::CurrentMove(CurrentMove {
            depth: 8,
            current_move,
            number: 3,
        });

        assert_eq!(
            format_search_event(&event),
            "info depth 8 currmove g1f3 currmovenumber 3"
        );
    }

    #[test]
    fn formats_outcome_as_uci_bestmove() {
        let outcome = SearchOutcome::Cancelled(SearchResult {
            score: Score::zero(),
            best_move: Move::null(),
            depth: 0,
        });

        assert_eq!(format_search_outcome(&outcome), "bestmove Null");
    }
}
