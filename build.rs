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

    // get used spec version
    let output = Command::new("cargo")
        .args(["pkgid", "-p", "openapi"])
        .output()
        .expect("fetching openapi crate version failed");
    let openapi_version = String::from_utf8(output.stdout).expect("Fetching from stdout failed");
    let openapi_version = openapi_version
        .split('@')
        .nth(1)
        .expect("malformed openapi version string");
    println!("cargo:rustc-env=OPENAPI_SERVER_VERSION={openapi_version}");
}
