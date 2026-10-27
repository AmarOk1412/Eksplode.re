use std::path::PathBuf;
use std::process::Command;

/// Runs scripts/pty-enter-check.py against the freshly built CLI (skipped without python3).
#[test]
fn us3_as4_every_enter_variant_starts_the_match_in_a_real_terminal() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_eksplode-re-cli"));
    // `cargo test --workspace` builds the server too; the script skips its checks otherwise
    let server = bin.with_file_name("eksplode-re-server");
    let Ok(out) = Command::new("python3")
        .args(["scripts/pty-enter-check.py", "--attempts", "3"])
        .env("EKSPLODE_RE_CLI", &bin)
        .env("EKSPLODE_RE_SERVER", &server)
        .current_dir(&root)
        .output()
    else {
        eprintln!("skipped: python3 is not available");
        return;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    println!("{text}");
    assert!(
        out.status.success(),
        "pty check failed:\n{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
