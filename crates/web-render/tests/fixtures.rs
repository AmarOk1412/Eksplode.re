#![allow(clippy::unwrap_used)]

mod common;

use std::path::PathBuf;

/// The committed observation used by the node tests; regenerate with UPDATE_FIXTURES=1.
#[test]
fn the_observation_fixture_is_up_to_date() {
    let json = serde_json::to_string_pretty(&common::obs(&common::standard())).unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/observation.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() || !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{json}\n")).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        committed.trim_end(),
        json,
        "run with UPDATE_FIXTURES=1 to refresh"
    );
}
