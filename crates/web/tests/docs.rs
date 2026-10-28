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
    assert!(readme.contains("cargo run -p eksplode-re-web"));
    assert!(readme.contains("cargo run -p eksplode-re-cli -- join"));
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
    let server = flags(eksplode_re_web::args::USAGE);
    let cli = flags(eksplode_re_cli::args::USAGE);
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
    for line in usage
        .lines()
        .filter(|line| line.trim_start().starts_with('-'))
    {
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
    let expected = options_table(eksplode_re_web::args::USAGE);
    assert_eq!(
        committed, expected,
        "the options table of docs/WEB.md is out of date; replace the text between the markers with:\n{expected}"
    );
}

/// The quoted names of `export const <name> = [ ... ];` in `audio.js`.
fn js_list(source: &str, name: &str) -> Vec<String> {
    let start = source
        .find(&format!("export const {name} = ["))
        .unwrap_or_else(|| panic!("{name} not found in audio.js"));
    let rest = &source[start..];
    let body = &rest[rest.find('[').unwrap() + 1..rest.find(']').unwrap()];
    body.split('"')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, s)| s.to_string())
        .collect()
}

#[test]
fn f3_sc009_the_audio_guide_lists_every_sound_and_tune_as_generated() {
    let js = read("crates/web/assets/audio.js");
    let guide = read("docs/AUDIO.md");
    assert!(
        guide.contains("generated"),
        "the guide must say that the audio is generated"
    );
    for name in ["SOUND_IDS", "TUNE_NAMES"] {
        let list = js_list(&js, name);
        assert!(list.len() >= 2, "{name}: {list:?}");
        for id in list {
            let row = guide
                .lines()
                .find(|l| l.starts_with('|') && l.contains(&format!("`{id}`")))
                .unwrap_or_else(|| panic!("docs/AUDIO.md has no table row for `{id}`"));
            let cells: Vec<&str> = row.split('|').map(str::trim).collect();
            assert!(
                cells.iter().filter(|c| !c.is_empty()).count() >= 2
                    && cells.last().is_some_and(|_| row.len() > id.len() + 12),
                "`{id}` needs a recipe in docs/AUDIO.md"
            );
        }
    }
}

#[test]
fn f3_sc009_no_audio_file_is_referenced_or_embedded() {
    let extensions = [".mp3", ".ogg", ".wav", ".flac", ".m4a", ".aac"];
    for file in [
        "crates/web/assets/audio.js",
        "crates/web/assets/app.js",
        "crates/web/assets/index.html",
        "crates/web/assets/style.css",
        "crates/web/src/assets.rs",
    ] {
        let text = read(file).to_lowercase();
        for ext in extensions {
            assert!(!text.contains(ext), "{file} mentions {ext}");
        }
    }
    let assets = fs::read_dir(root().join("crates/web/assets")).unwrap();
    for entry in assets {
        let name = entry.unwrap().file_name().to_string_lossy().to_lowercase();
        assert!(
            !extensions.iter().any(|e| name.ends_with(e)),
            "audio file {name}"
        );
    }
    assert!(
        !read("crates/web/assets/audio.js").contains("fetch("),
        "audio.js must not fetch anything"
    );
}

#[test]
fn f4_the_guides_describe_every_mode_setting_option_and_sound_of_game_modes() {
    use eksplode_re_core::{BoardSize, Mode, MINUTES};
    let gameplay = read("docs/GAMEPLAY.md");
    let readme = read("README.md");
    let protocol = read("docs/PROTOCOL.md");
    let web = read("docs/WEB.md");
    for mode in Mode::ALL {
        let label = mode.label();
        assert!(
            gameplay.contains(&format!("{label} mode")),
            "docs/GAMEPLAY.md has no section for {label} mode"
        );
        assert!(readme.contains(label), "README.md does not mention {label}");
        assert!(
            protocol.contains(&format!("`{}`", mode.name())),
            "docs/PROTOCOL.md lacks `{}`",
            mode.name()
        );
    }
    for size in BoardSize::ALL {
        let (w, h) = size.dimensions();
        assert!(
            gameplay.contains(&format!("{w} x {h}")),
            "docs/GAMEPLAY.md lacks {w} x {h}"
        );
    }
    for m in MINUTES {
        assert!(gameplay.contains(&m.to_string()), "round time {m}");
    }
    for flag in [
        "--room-name",
        "--mode",
        "--size",
        "--time",
        "--friendly-fire",
        "--no-setup",
    ] {
        assert!(readme.contains(flag), "README.md lacks {flag}");
    }
    for needle in [
        "end pattern",
        "friendly fire",
        "hearts",
        "RoomQuery",
        "SetRoom",
        "eksplode.re.room.v1",
    ] {
        assert!(
            [&gameplay, &readme, &protocol, &web]
                .iter()
                .any(|d| d.contains(needle)),
            "no guide mentions {needle}"
        );
    }
}
