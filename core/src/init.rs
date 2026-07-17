/// Compatibility entry point for callers that previously initialized global
/// engine tables such as:
/// - magic bitboard tables
/// - precalculated piece movements
/// - zobrist hash keys.
///
/// These tables are now computed at compile time, so this function has no runtime work.
pub fn init_globals() {}
