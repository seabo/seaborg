//! Logger setup for the executable.
//!
//! The bot logs its lifecycle at `Info` so an operator sees connections,
//! challenges, and games without configuring anything. Everything more detailed —
//! notably the per-request transport trace — sits at `Debug`, selected through
//! `RUST_LOG`.
//!
//! `simple_logger` supplies the output format but reads only a bare level word
//! from `RUST_LOG` (`debug`), discarding the `target=level` form (`RUST_LOG=lichess::transport=debug`)
//! without complaint. That form is the only practical way to ask for one
//! subsystem's detail, so the directives are parsed here and handed to
//! `with_module_level`.
//!
//! Turning the level up globally is the other way to reach that detail, and on
//! its own it is unusable: the HTTP and TLS stacks emit several `Debug` lines per
//! request, which is enough to bury the bot's own output entirely. They are held
//! at `Info` unless `RUST_LOG` names them.

use log::LevelFilter;
use simple_logger::SimpleLogger;

/// Dependencies whose `Debug` output is voluminous enough to drown the bot's.
///
/// Matching is by target prefix, so each entry also covers that crate's
/// submodules. An explicit `RUST_LOG` directive for one of these still wins.
const VERBOSE_DEPENDENCIES: [&str; 3] = ["ureq", "ureq_proto", "rustls"];

/// Install the process logger.
///
/// # Panics
///
/// Panics if a logger is already installed, which can only happen by calling
/// this twice.
pub fn init() {
    let directives = std::env::var("RUST_LOG").unwrap_or_default();
    let mut logger = SimpleLogger::new()
        // The default when `RUST_LOG` says nothing, and the level every target
        // without a directive of its own keeps.
        .with_level(default_level(&directives).unwrap_or(LevelFilter::Info));

    // Operator directives are registered before the dependency defaults so that
    // naming a dependency explicitly overrides its default. `simple_logger`
    // resolves a target by the longest matching prefix and breaks ties by
    // registration order, so equal-length entries are decided by this ordering.
    for (target, level) in module_directives(&directives) {
        logger = logger.with_module_level(target, level);
    }
    for target in VERBOSE_DEPENDENCIES {
        logger = logger.with_module_level(target, LevelFilter::Info);
    }

    logger.init().expect("no logger installed yet");
}

/// The level from a bare `RUST_LOG` level word, if one is present.
///
/// Only an entry with no `=` sets the default level; `RUST_LOG=debug` and
/// `RUST_LOG=lichess::transport=debug,info` both reach here, and the second
/// means "everything at info, the transport at debug".
fn default_level(directives: &str) -> Option<LevelFilter> {
    directives
        .split(',')
        .filter(|entry| !entry.contains('='))
        .filter_map(parse_level)
        .next_back()
}

/// The `target=level` directives in `RUST_LOG`, in the order written.
///
/// An entry that names no recognizable level is skipped rather than rejected,
/// matching how `RUST_LOG` is treated elsewhere: a malformed directive should
/// cost detail in the log, never the run.
fn module_directives(directives: &str) -> Vec<(&str, LevelFilter)> {
    directives
        .split(',')
        .filter_map(|entry| entry.split_once('='))
        .filter_map(|(target, level)| {
            let target = target.trim();
            match (target.is_empty(), parse_level(level)) {
                (false, Some(level)) => Some((target, level)),
                _ => None,
            }
        })
        .collect()
}

/// Parse one level word, case-insensitively. `off` is included so a single
/// dependency can be silenced outright.
fn parse_level(word: &str) -> Option<LevelFilter> {
    match word.trim().to_ascii_lowercase().as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_level_word_sets_the_default() {
        assert_eq!(default_level("debug"), Some(LevelFilter::Debug));
        assert_eq!(default_level("TRACE"), Some(LevelFilter::Trace));
        assert_eq!(default_level(""), None);
        assert_eq!(default_level("nonsense"), None);
    }

    #[test]
    fn a_module_directive_does_not_move_the_default_level() {
        // The whole point of the `target=level` form is to raise one subsystem
        // without raising everything; treating it as a bare level would restore
        // exactly the flood it exists to avoid.
        assert_eq!(default_level("lichess::transport=debug"), None);
        assert_eq!(
            module_directives("lichess::transport=debug"),
            vec![("lichess::transport", LevelFilter::Debug)]
        );
    }

    #[test]
    fn the_two_forms_combine() {
        // The documented way to read a transport trace without the HTTP stack's
        // own debug output.
        const RUST_LOG: &str = "info,lichess::transport=debug";
        assert_eq!(default_level(RUST_LOG), Some(LevelFilter::Info));
        assert_eq!(
            module_directives(RUST_LOG),
            vec![("lichess::transport", LevelFilter::Debug)]
        );
    }

    #[test]
    fn several_directives_are_kept_in_order() {
        // Order is what resolves ties between equal-length targets, so it has to
        // survive parsing rather than being incidental.
        assert_eq!(
            module_directives("ureq=trace,lichess=debug,rustls=off"),
            vec![
                ("ureq", LevelFilter::Trace),
                ("lichess", LevelFilter::Debug),
                ("rustls", LevelFilter::Off),
            ]
        );
    }

    #[test]
    fn a_malformed_directive_is_skipped_not_fatal() {
        // A typo in RUST_LOG must cost log detail, never the bot's run.
        assert_eq!(
            module_directives("lichess=verbose,=debug,lichess::game=trace"),
            vec![("lichess::game", LevelFilter::Trace)]
        );
    }

    #[test]
    fn whitespace_around_a_directive_is_tolerated() {
        assert_eq!(
            module_directives(" lichess::transport = debug "),
            vec![("lichess::transport", LevelFilter::Debug)]
        );
    }
}
