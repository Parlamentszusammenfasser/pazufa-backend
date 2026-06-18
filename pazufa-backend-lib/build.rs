#![allow(clippy::disallowed_macros, reason = "build.rs works with println only")]
#![allow(clippy::expect_used, reason = "The build script can just panic")]
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // get git hash
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("Expected to find git, yielding last commit");
    let git_hash = String::from_utf8(output.stdout).expect("Fetching from stdout failed");
    println!("cargo:rustc-env=GIT_HASH={git_hash}");
}
