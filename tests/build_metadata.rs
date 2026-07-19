#[path = "../build_metadata.rs"]
#[allow(dead_code)]
mod build_metadata;

use build_metadata::{head_ref, override_git_hash, resolve_git_hash, GitOutput, UNKNOWN_GIT_HASH};

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
