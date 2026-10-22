use std::path::PathBuf;
use std::process::Command;

/// Runs the JavaScript tests (the page logic and the wasm module in Node) when node is installed.
#[test]
fn the_javascript_tests_pass() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("js-tests");
    let Ok(out) = Command::new("node")
        .arg("--test")
        .current_dir(&dir)
        .output()
    else {
        eprintln!("skipped: node is not installed (run `cd crates/web/js-tests && node --test`)");
        return;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "node tests failed:\n{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
