/// Engine configuration.
#[derive(Debug)]
pub struct Config {
    /// Debug mode.
    debug_mode: bool,
    /// Transposition table configuration.
    hash: HashConfig,
}

impl Config {
    pub fn debug_mode(&self) -> bool {
        self.debug_mode
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            debug_mode: false,
            hash: Default::default(),
        }
    }
}

/// Transposition table configuration.
#[derive(Debug, Default)]
pub struct HashConfig {
    /// Size of the hash table
    size: usize,
}

impl HashConfig {
    pub fn set_size(&mut self, size: usize) {
        self.size = size;
    }
}

/// Move ordering configuration.
#[derive(Debug, Default)]
pub struct MoveOrderingConfig;

/// The configuration of halting criteria.
#[derive(Debug, Default)]
pub enum HaltingConfig {
    #[default]
    Infinite,
    MoveTime(u32),
    FixedDepth(u8),
}

impl Config {
    pub fn set_option(&mut self, o: EngineOpt) {
        match o {
            EngineOpt::Hash(v) => self.hash.set_size(v),
            EngineOpt::DebugMode(b) => self.debug_mode = b,
        }
    }
}

/// Possible options which can be set via the UCI protocol.
#[derive(Clone, Debug)]
pub enum EngineOpt {
    /// The size in MB of the hash table.
    Hash(usize),
    /// Whether debug mode is turned on.
    DebugMode(bool),
}
