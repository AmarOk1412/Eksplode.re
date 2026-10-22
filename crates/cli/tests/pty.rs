use std::path::PathBuf;
use std::process::Command;

/// Runs scripts/pty-enter-check.py against the freshly built CLI (skipped without python3).
#[test]
fn every_enter_variant_starts_the_match_in_a_real_terminal() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_bomberust-cli"));
    let Ok(out) = Command::new("python3")
        .arg("scripts/pty-enter-check.py")
        .env("BOMBERUST_CLI", &bin)
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
