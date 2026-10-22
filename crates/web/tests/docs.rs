//! The documentation is part of the product: these tests keep it complete and fresh.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Every `--flag` mentioned in a help text.
fn flags(usage: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in usage.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
        if word.starts_with("--") && word.len() > 2 && !out.iter().any(|w| w == word) {
            out.push(word.to_string());
        }
    }
    out
}

/// The variant names of `pub enum <name>` in `crates/net/src/protocol.rs`.
fn variants(source: &str, name: &str) -> Vec<String> {
    let start = source
        .find(&format!("pub enum {name} {{"))
        .unwrap_or_else(|| panic!("enum {name} not found"));
    source[start..]
        .lines()
        .skip(1)
        .take_while(|l| *l != "}")
        .filter(|l| l.starts_with("    ") && !l.starts_with("     "))
        .filter_map(|l| {
            let word: String = l
                .trim()
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect();
            word.chars()
                .next()
                .filter(|c| c.is_uppercase())
                .map(|_| word)
        })
        .collect()
}

#[test]
fn us4_as1_the_readme_has_a_quick_start_with_the_two_commands() {
    let readme = read("README.md");
    assert!(
        readme.contains("## Quick start"),
        "README needs a Quick start section"
    );
    assert!(readme.contains("cargo run -p bomberust-web"));
    assert!(readme.contains("cargo run -p bomberust-cli -- join"));
}

#[test]
fn us4_as2_the_architecture_guide_names_every_crate_and_has_a_diagram() {
    let arch = read("docs/ARCHITECTURE.md");
    let mut crates = 0;
    for entry in fs::read_dir(root().join("crates")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            crates += 1;
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(
                arch.contains(&format!("`{name}`")),
                "docs/ARCHITECTURE.md must describe the crate `{name}`"
            );
        }
    }
    assert!(crates >= 6, "found {crates} crates");
    assert!(
        arch.contains("```mermaid"),
        "ARCHITECTURE.md needs a mermaid diagram"
    );
}

#[test]
fn us4_as2_every_option_of_both_programs_is_documented() {
    let docs = format!("{}\n{}", read("README.md"), read("docs/WEB.md"));
    let server = flags(bomberust_web::args::USAGE);
    let cli = flags(bomberust_cli::args::USAGE);
    assert!(server.len() >= 8 && cli.len() >= 8, "{server:?} {cli:?}");
    for flag in server.iter().chain(&cli) {
        assert!(
            docs.contains(flag.as_str()),
            "{flag} is in a --help text but not in README.md or docs/WEB.md"
        );
    }
}

#[test]
fn us4_as2_the_protocol_guide_lists_every_message() {
    let protocol = read("docs/PROTOCOL.md");
    let source = read("crates/net/src/protocol.rs");
    for name in ["ClientMsg", "ServerMsg"] {
        let list = variants(&source, name);
        assert!(list.len() >= 5, "{name}: {list:?}");
        for variant in list {
            assert!(
                protocol.contains(&format!("`{variant}`")),
                "docs/PROTOCOL.md does not describe `{variant}` of {name}"
            );
        }
    }
}

#[test]
fn us4_as3_the_safety_words_are_in_the_security_and_web_guides() {
    for file in ["docs/SECURITY.md", "docs/WEB.md"] {
        let text = read(file);
        for word in ["Origin", "CORS", "encryption", "authentication"] {
            assert!(text.contains(word), "{file} must mention {word}");
        }
    }
}

#[test]
fn relative_links_point_to_existing_files() {
    let mut files = vec!["README.md".to_string()];
    for entry in fs::read_dir(root().join("docs")).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        if name.ends_with(".md") {
            files.push(format!("docs/{name}"));
        }
    }
    assert!(files.len() >= 6, "{files:?}");
    for file in files {
        let text = read(&file);
        let base = root().join(&file).parent().unwrap().to_path_buf();
        let mut rest = text.as_str();
        while let Some(i) = rest.find("](") {
            rest = &rest[i + 2..];
            let Some(end) = rest.find(')') else { break };
            let target = &rest[..end];
            let path = target.split('#').next().unwrap_or("");
            let external = target.starts_with("http") || target.starts_with("mailto:");
            if !external && !path.is_empty() {
                assert!(base.join(path).exists(), "{file}: broken link to {target}");
            }
        }
    }
}

/// The options table of `docs/WEB.md` as generated from the server's help text.
fn options_table(usage: &str) -> String {
    let mut rows = vec![
        "| Option | Meaning |".to_string(),
        "|--------|---------|".to_string(),
    ];
    for line in usage.lines().filter(|l| l.starts_with("  -")) {
        let (flag, meaning) = line.trim().split_once("  ").expect("flag and meaning");
        rows.push(format!("| `{flag}` | {} |", meaning.trim()));
    }
    rows.join("\n")
}

#[test]
fn us4_as2_the_options_table_in_the_web_guide_is_generated_from_the_usage_text() {
    let web = read("docs/WEB.md");
    let begin = "<!-- options:begin -->";
    let end = "<!-- options:end -->";
    let from = web.find(begin).expect("options:begin marker") + begin.len();
    let to = web.find(end).expect("options:end marker");
    let committed = web[from..to].trim();
    let expected = options_table(bomberust_web::args::USAGE);
    assert_eq!(
        committed, expected,
        "the options table of docs/WEB.md is out of date; replace the text between the markers with:\n{expected}"
    );
}
