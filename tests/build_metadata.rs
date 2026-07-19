#[path = "../build_metadata.rs"]
#[allow(dead_code)]
mod build_metadata;

use build_metadata::{
    head_ref, override_git_hash, resolve_git_hash, watch_paths, GitOutput, UNKNOWN_GIT_HASH,
};
use std::fs;
use std::path::{Path, PathBuf};

/// A throwaway directory tree, removed when the test ends.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    /// `name` only has to be unique across tests; the process id separates concurrent runs.
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("seaborg-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create scratch root");
        Self { root }
    }

    /// Create `relative` as a directory, including any missing parents.
    fn dir(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(&path).expect("create scratch directory");
        path
    }

    /// Create `relative` as a file, including any missing parents.
    fn file(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create scratch parent");
        }
        fs::write(&path, b"scratch").expect("write scratch file");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Assert the set of watched paths, order-insensitively, with readable output on mismatch.
fn assert_watches(actual: &[PathBuf], expected: &[&Path]) {
    let mut actual: Vec<_> = actual
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let mut expected: Vec<_> = expected
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    actual.sort();
    expected.sort();

    assert_eq!(actual, expected);
}

#[test]
fn trims_successful_git_output() {
    let hash = resolve_git_hash(|| {
        Some(GitOutput {
            success: true,
            stdout: b"abc123\n".to_vec(),
        })
    });

    assert_eq!(hash, "abc123");
}

#[test]
fn falls_back_when_git_cannot_run() {
    assert_eq!(resolve_git_hash(|| None), UNKNOWN_GIT_HASH);
}

#[test]
fn falls_back_when_git_returns_an_error() {
    let hash = resolve_git_hash(|| {
        Some(GitOutput {
            success: false,
            stdout: Vec::new(),
        })
    });

    assert_eq!(hash, UNKNOWN_GIT_HASH);
}

#[test]
fn falls_back_for_invalid_utf8() {
    let hash = resolve_git_hash(|| {
        Some(GitOutput {
            success: true,
            stdout: vec![0xff],
        })
    });

    assert_eq!(hash, UNKNOWN_GIT_HASH);
}

#[test]
fn falls_back_for_empty_output() {
    let hash = resolve_git_hash(|| {
        Some(GitOutput {
            success: true,
            stdout: b" \n".to_vec(),
        })
    });

    assert_eq!(hash, UNKNOWN_GIT_HASH);
}

/// The override exists so builds outside a repository can pin a revision, so its value is taken
/// verbatim apart from surrounding whitespace.
#[test]
fn accepts_a_trimmed_override() {
    assert_eq!(
        override_git_hash(Some("  abc123\n".to_owned())),
        Some("abc123".to_owned())
    );
}

/// An exported-but-empty variable is a common shell accident. Treating it as a pin would embed an
/// empty revision, so it must fall through to discovery instead.
#[test]
fn ignores_a_blank_override() {
    assert_eq!(override_git_hash(Some("   ".to_owned())), None);
    assert_eq!(override_git_hash(Some(String::new())), None);
    assert_eq!(override_git_hash(None), None);
}

/// A branch checkout names the ref backing it; that file is what has to be watched for rebuilds.
#[test]
fn reads_the_branch_a_symbolic_head_points_at() {
    assert_eq!(
        head_ref(Some("ref: refs/heads/master\n")),
        Some("refs/heads/master".to_owned())
    );
}

/// A detached HEAD stores the commit directly, so there is no second file to watch.
#[test]
fn reports_no_ref_for_a_detached_head() {
    assert_eq!(head_ref(Some("7449461f0a\n")), None);
}

/// A freshly initialised repository can have an unreadable or malformed HEAD; watching nothing
/// extra is correct, and must not panic.
#[test]
fn reports_no_ref_for_missing_or_malformed_head() {
    assert_eq!(head_ref(None), None);
    assert_eq!(head_ref(Some("")), None);
    assert_eq!(head_ref(Some("ref:   \n")), None);
}

const SYMBOLIC_HEAD: &str = "ref: refs/heads/master\n";

/// Cargo does not read a missing `rerun-if-changed` path as "unchanged": it holds the unit dirty
/// while the path is absent, and the build script keeps re-declaring it, so the crate would
/// recompile on every build forever. No layout may produce a path that is not there.
#[test]
fn never_watches_a_path_that_does_not_exist() {
    let scratch = Scratch::new("watch-existence");
    let git = scratch.dir(".git");

    // Every combination of the parts that may independently be present or absent.
    for head in [None, Some(SYMBOLIC_HEAD), Some("cafebabe\n")] {
        for refs_present in [false, true] {
            for packed_present in [false, true] {
                let _ = fs::remove_dir_all(git.join("refs"));
                let _ = fs::remove_file(git.join("packed-refs"));
                if refs_present {
                    scratch.dir(".git/refs/heads");
                }
                if packed_present {
                    scratch.file(".git/packed-refs");
                }

                for path in watch_paths(&git, &git, head) {
                    assert!(path.exists(), "declared a missing path: {}", path.display());
                }
            }
        }
    }
}

/// The ordinary layout: the branch is loose, so its directory is watched and any commit,
/// checkout, or pack of that ref changes the directory.
#[test]
fn watches_the_ref_directory_for_a_loose_branch() {
    let scratch = Scratch::new("watch-loose");
    let git = scratch.dir(".git");
    let head = scratch.file(".git/HEAD");
    let heads = scratch.file(".git/refs/heads/master");

    assert_watches(
        &watch_paths(&git, &git, Some(SYMBOLIC_HEAD)),
        &[&head, heads.parent().expect("refs/heads")],
    );
}

/// After `git pack-refs` or `git gc` the loose file is gone. Naming it anyway was the defect this
/// guards: the surviving directory is watched instead, so the ref reappearing as a loose file —
/// which is what the next commit does — is still observed.
#[test]
fn watches_the_surviving_directory_when_the_branch_is_packed() {
    let scratch = Scratch::new("watch-packed");
    let git = scratch.dir(".git");
    let head = scratch.file(".git/HEAD");
    let heads = scratch.dir(".git/refs/heads");
    let packed = scratch.file(".git/packed-refs");

    assert_watches(
        &watch_paths(&git, &git, Some(SYMBOLIC_HEAD)),
        &[&head, &heads, &packed],
    );
}

/// A packed hierarchical branch name can leave several directory levels missing, so the climb to
/// the nearest surviving ancestor has to skip more than one.
#[test]
fn climbs_past_several_missing_levels_of_a_hierarchical_branch() {
    let scratch = Scratch::new("watch-hierarchical");
    let git = scratch.dir(".git");
    let head = scratch.file(".git/HEAD");
    let refs = scratch.dir(".git/refs");

    assert_watches(
        &watch_paths(&git, &git, Some("ref: refs/heads/feature/deep/work\n")),
        &[&head, &refs],
    );
}

/// The climb stops at `refs`. The Git directory above it holds the object store, which is
/// expensive to scan and changes for reasons that cannot affect the resolved revision.
#[test]
fn never_climbs_above_the_refs_directory() {
    let scratch = Scratch::new("watch-refs-floor");
    let git = scratch.dir(".git");
    let head = scratch.file(".git/HEAD");

    assert_watches(&watch_paths(&git, &git, Some(SYMBOLIC_HEAD)), &[&head]);
}

/// A linked worktree keeps HEAD to itself but shares the branch with the main checkout, so the two
/// halves of the watch set come from different directories.
#[test]
fn resolves_refs_against_the_common_directory_in_a_linked_worktree() {
    let scratch = Scratch::new("watch-worktree");
    let common = scratch.dir(".git");
    let git = scratch.dir(".git/worktrees/task");
    let head = scratch.file(".git/worktrees/task/HEAD");
    let heads = scratch.dir(".git/refs/heads");

    assert_watches(
        &watch_paths(&git, &common, Some(SYMBOLIC_HEAD)),
        &[&head, &heads],
    );
}

/// A detached HEAD stores the commit itself, so no ref file backs it and watching the refs tree
/// would only cause rebuilds that cannot change the embedded revision.
#[test]
fn watches_only_head_when_detached() {
    let scratch = Scratch::new("watch-detached");
    let git = scratch.dir(".git");
    let head = scratch.file(".git/HEAD");
    scratch.dir(".git/refs/heads");
    scratch.file(".git/packed-refs");

    assert_watches(&watch_paths(&git, &git, Some("cafebabe\n")), &[&head]);
}
