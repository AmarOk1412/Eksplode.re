//! Embeds the short git commit of the build as `EKSPLODE_RE_GIT_SHA` ("unknown" outside git).

use std::path::Path;
use std::process::Command;

fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|text| text.trim().to_string())
        .filter(|sha| !sha.is_empty() && sha.chars().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=EKSPLODE_RE_GIT_SHA={sha}");
    println!("cargo:rerun-if-changed=build.rs");
    // a new commit moves the branch ref; a missing path would rerun the script on every build
    for path in [
        "../../.git/HEAD",
        "../../.git/refs/heads",
        "../../.git/packed-refs",
    ] {
        if Path::new(path).exists() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}
