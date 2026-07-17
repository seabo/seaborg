#[path = "../build_metadata.rs"]
mod build_metadata;

fn main() {
    println!("cargo:rustc-env=GIT_HASH={}", build_metadata::git_hash());
}
