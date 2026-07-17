use std::process::Command;

/// Stable metadata used when the current Git revision cannot be determined.
pub const UNKNOWN_GIT_HASH: &str = "unknown";

pub struct GitOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
}

pub fn git_hash() -> String {
    resolve_git_hash(|| {
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .map(|output| GitOutput {
                success: output.status.success(),
                stdout: output.stdout,
            })
    })
}

pub fn resolve_git_hash(run_git: impl FnOnce() -> Option<GitOutput>) -> String {
    run_git()
        .filter(|output| output.success)
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|hash| hash.trim().to_owned())
        .filter(|hash| !hash.is_empty())
        .unwrap_or_else(|| UNKNOWN_GIT_HASH.to_owned())
}
