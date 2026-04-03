use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // get git hash
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap();
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);

    // get used spec version
    let output = Command::new("cargo")
        .args(["pkgid", "-p", "openapi"])
        .output()
        .unwrap();
    let openapi_version = String::from_utf8(output.stdout).unwrap();
    let openapi_version = openapi_version.split("@").nth(1).unwrap();
    println!("cargo:rustc-env=OPENAPI_SERVER_VERSION={}", openapi_version);
}
