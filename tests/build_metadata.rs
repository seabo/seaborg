#[path = "../build_metadata.rs"]
#[allow(dead_code)]
mod build_metadata;

use build_metadata::{resolve_git_hash, GitOutput, UNKNOWN_GIT_HASH};

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
