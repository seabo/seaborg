//! Revision metadata embedded into the binary at build time.
//!
//! This file is the single authoritative implementation. It is included directly by the build
//! script rather than being a crate of its own, because a build script cannot depend on a member
//! of the workspace it builds. Any crate that wants the revision must therefore receive it from
//! the binary at runtime instead of growing a second copy of this logic.

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Value embedded when the revision cannot be determined: no Git, no repository, or a checkout
/// with no commits yet. Builds must succeed in all of those cases, so this is a normal outcome
/// and not an error.
pub const UNKNOWN_GIT_HASH: &str = "unknown";

/// Environment variable that overrides revision discovery entirely.
///
/// Source archives and distribution packaging build outside a repository, where Git either is not
/// installed or would resolve some unrelated enclosing repository. Setting this pins the embedded
/// revision to a known value and makes the build reproducible.
pub const GIT_HASH_OVERRIDE_VAR: &str = "SEABORG_GIT_HASH";

/// Raw result of running a Git command.
pub struct GitOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
}

/// Resolve the revision and print every Cargo directive the build depends on.
pub fn emit() {
    println!("cargo:rerun-if-env-changed={GIT_HASH_OVERRIDE_VAR}");

    let hash = match override_git_hash(env::var(GIT_HASH_OVERRIDE_VAR).ok()) {
        // A pinned revision does not depend on repository state, so watching it would only cause
        // rebuilds that cannot change the embedded value.
        Some(pinned) => pinned,
        None => {
            for path in revision_watch_paths() {
                // A path that does not exist is still worth declaring: Cargo reruns the script
                // when it appears, which is what happens when the first commit creates a loose
                // ref.
                println!("cargo:rerun-if-changed={}", path.display());
            }

            resolve_git_hash(|| run_git(["rev-parse", "HEAD"]))
        }
    };

    println!("cargo:rustc-env=GIT_HASH={hash}");
}

/// Apply the environment override, if it carries a usable value.
///
/// An empty or whitespace-only setting is treated as absent rather than as a request to embed the
/// empty string, so `SEABORG_GIT_HASH=` behaves like not setting it at all.
pub fn override_git_hash(value: Option<String>) -> Option<String> {
    value
        .map(|hash| hash.trim().to_owned())
        .filter(|hash| !hash.is_empty())
}

/// Interpret the output of `git rev-parse HEAD`.
///
/// Every failure mode collapses to [`UNKNOWN_GIT_HASH`]: Git absent or not executable, a non-zero
/// exit, output that is not UTF-8, or empty output.
pub fn resolve_git_hash(run_git: impl FnOnce() -> Option<GitOutput>) -> String {
    git_line(run_git).unwrap_or_else(|| UNKNOWN_GIT_HASH.to_owned())
}

/// Extract the ref name from the contents of `HEAD`, or `None` when HEAD is detached.
///
/// A detached HEAD holds the commit itself, so there is no further file to watch.
pub fn head_ref(contents: Option<&str>) -> Option<String> {
    contents?
        .lines()
        .next()?
        .strip_prefix("ref:")
        .map(|reference| reference.trim().to_owned())
        .filter(|reference| !reference.is_empty())
}

/// Files whose contents determine the revision, so Cargo knows when to regenerate it.
///
/// Without these the build script declares no dependencies at all, and Cargo falls back to
/// watching the package directory. Repository metadata lives outside the package, so the embedded
/// hash would silently keep the value it had when the crate was last recompiled.
fn revision_watch_paths() -> Vec<PathBuf> {
    let Some(git_dir) = git_line(|| run_git(["rev-parse", "--git-dir"])).map(PathBuf::from) else {
        return Vec::new();
    };

    let head = git_dir.join("HEAD");
    let mut paths = vec![head.clone()];

    // A branch checkout moves when its ref file changes, and the ref may be loose or packed. Both
    // candidates are declared because whether a ref is currently packed is invisible from here.
    //
    // Refs live in the *common* directory rather than the per-worktree Git directory: in a linked
    // worktree `HEAD` is worktree-local, but the branch it names is shared with the main checkout,
    // so joining the worktree directory would watch a path that never exists.
    if let Some(reference) = head_ref(std::fs::read_to_string(&head).ok().as_deref()) {
        let common_dir = git_line(|| run_git(["rev-parse", "--git-common-dir"]))
            .map(PathBuf::from)
            .unwrap_or_else(|| git_dir.clone());

        paths.push(common_dir.join(reference));
        paths.push(common_dir.join("packed-refs"));
    }

    paths
}

/// Run a Git command, treating an unavailable Git as a plain absence of output.
fn run_git<const N: usize>(args: [&str; N]) -> Option<GitOutput> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .map(|output| GitOutput {
            success: output.status.success(),
            stdout: output.stdout,
        })
}

/// Trimmed output of a successful Git command, or `None` if the command gave nothing usable.
fn git_line(run_git: impl FnOnce() -> Option<GitOutput>) -> Option<String> {
    run_git()
        .filter(|output| output.success)
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
}
