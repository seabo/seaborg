//! Revision metadata embedded into the binary at build time.
//!
//! This file is the single authoritative implementation. It is included directly by the build
//! script rather than being a crate of its own, because a build script cannot depend on a member
//! of the workspace it builds. Any crate that wants the revision must therefore receive it from
//! the binary at runtime instead of growing a second copy of this logic.

use std::env;
use std::path::{Path, PathBuf};
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

/// Paths whose contents determine the revision, so Cargo knows when to regenerate it.
///
/// Without these the build script declares no dependencies at all, and Cargo falls back to
/// watching the package directory. Repository metadata lives outside the package, so the embedded
/// hash would silently keep the value it had when the crate was last recompiled.
fn revision_watch_paths() -> Vec<PathBuf> {
    let Some(git_dir) = git_line(|| run_git(["rev-parse", "--git-dir"])).map(PathBuf::from) else {
        return Vec::new();
    };

    // Refs live in the *common* directory rather than the per-worktree Git directory: in a linked
    // worktree `HEAD` is worktree-local, but the branch it names is shared with the main checkout,
    // so joining the worktree directory would watch a path that never exists.
    let common_dir = git_line(|| run_git(["rev-parse", "--git-common-dir"]))
        .map(PathBuf::from)
        .unwrap_or_else(|| git_dir.clone());

    let head = git_dir.join("HEAD");
    let head_contents = std::fs::read_to_string(&head).ok();

    watch_paths(&git_dir, &common_dir, head_contents.as_deref())
}

/// Decide which paths to watch for a repository laid out around `git_dir` and `common_dir`.
///
/// Every returned path exists. This is a correctness requirement, not tidiness: Cargo does not
/// read a missing `rerun-if-changed` path as "unchanged", it marks the unit dirty for as long as
/// the path is absent. Since the build script re-emits the same path on every rerun, naming a file
/// that is merely *expected* to appear recompiles the crate on every single build, forever.
///
/// That rules out naming the loose ref file directly, because a branch ref is stored either loose
/// or packed and the absent form would be declared in each case. The containing directory is
/// watched instead. Cargo scans a watched directory recursively, so creating, deleting, renaming,
/// or rewriting the ref file all register — which covers a plain commit as well as both directions
/// of the loose/packed transition that `git pack-refs` and `git gc` perform.
pub fn watch_paths(git_dir: &Path, common_dir: &Path, head_contents: Option<&str>) -> Vec<PathBuf> {
    let mut paths = vec![git_dir.join("HEAD")];

    // A detached HEAD holds the commit itself, so no ref backs it and there is nothing further to
    // watch.
    if let Some(reference) = head_ref(head_contents) {
        let refs_root = common_dir.join("refs");

        // The ref's own directory may be absent while the ref is packed, and a hierarchical name
        // such as `refs/heads/feature/work` can leave several levels missing. Climbing to the
        // nearest surviving ancestor keeps a directory watched in every layout. The climb stops at
        // `refs`, because the Git directory above it holds the object store, which is expensive to
        // scan and churns for reasons unrelated to the revision.
        let ref_directory = common_dir.join(&reference).parent().map(Path::to_path_buf);
        if let Some(directory) = ref_directory
            .as_deref()
            .and_then(nearest_existing_ancestor)
            .filter(|directory| directory.starts_with(&refs_root))
        {
            paths.push(directory);
        }

        // A packed ref changes value without touching any directory, so this is watched in
        // addition to the tree above.
        let packed_refs = common_dir.join("packed-refs");
        if packed_refs.exists() {
            paths.push(packed_refs);
        }
    }

    paths.retain(|path| path.exists());
    paths
}

/// The deepest existing directory at or above `path`, or `None` if no ancestor of it exists.
fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| ancestor.is_dir())
        .map(Path::to_path_buf)
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
