//! The look options are written down in several places; they must never drift apart.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use eksplode_re_net::look::{Colour, Face, Gender, Head};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The strings of `export const NAME = ["A", "B"];` in look.js.
fn js_list(source: &str, name: &str) -> Vec<String> {
    let start = source
        .find(&format!("export const {name} = ["))
        .unwrap_or_else(|| panic!("{name} not found in look.js"));
    let rest = &source[start..];
    let open = rest.find('[').unwrap() + 1;
    let close = rest.find(']').unwrap();
    rest[open..close]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn names<T: Copy>(all: &[T], name: impl Fn(T) -> &'static str) -> Vec<String> {
    all.iter().map(|c| name(*c).to_string()).collect()
}

#[test]
fn the_look_options_of_the_server_the_renderer_the_page_and_the_docs_agree() {
    let js = read("crates/web/assets/look.js");
    let docs = read("docs/PROTOCOL.md");
    let lists: [(&str, Vec<String>, Vec<&str>); 4] = [
        (
            "COLOUR_NAMES",
            names(Colour::ALL, Colour::name),
            eksplode_re_web_render::look::COLOUR_NAMES.to_vec(),
        ),
        (
            "GENDERS",
            names(Gender::ALL, Gender::name),
            eksplode_re_web_render::look::GENDER_NAMES.to_vec(),
        ),
        (
            "HEADS",
            names(Head::ALL, Head::name),
            eksplode_re_web_render::look::HEAD_NAMES.to_vec(),
        ),
        (
            "FACES",
            names(Face::ALL, Face::name),
            eksplode_re_web_render::look::FACE_NAMES.to_vec(),
        ),
    ];
    for (js_name, server, render) in lists {
        assert_eq!(
            js_list(&js, js_name),
            server,
            "look.js {js_name} differs from net::look"
        );
        assert_eq!(
            render, server,
            "web-render {js_name} differs from net::look"
        );
        for value in &server {
            assert!(
                docs.contains(&format!("`{value}`")),
                "docs/PROTOCOL.md does not list the look value `{value}`"
            );
        }
    }
}

/// The strings of `export const NAME = ["A", "B"];` in room.js.
fn room_list(source: &str, name: &str) -> Vec<String> {
    let start = source
        .find(&format!("export const {name} = ["))
        .unwrap_or_else(|| panic!("{name} not found in room.js"));
    let rest = &source[start..];
    let open = rest.find('[').unwrap() + 1;
    let close = rest.find(']').unwrap();
    rest[open..close]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[test]
fn the_room_options_of_the_core_the_page_and_the_docs_agree() {
    use eksplode_re_core::{BoardSize, Mode, MINUTES, PLAYABLE};
    let js = read("crates/web/assets/room.js");
    let docs = read("docs/PROTOCOL.md");
    let lists: [(&str, Vec<String>); 4] = [
        (
            "MODES",
            Mode::ALL.iter().map(|m| m.name().to_string()).collect(),
        ),
        (
            "PLAYABLE_MODES",
            PLAYABLE.iter().map(|m| m.name().to_string()).collect(),
        ),
        (
            "SIZES",
            BoardSize::ALL
                .iter()
                .map(|s| s.name().to_string())
                .collect(),
        ),
        ("MINUTES", MINUTES.iter().map(|m| m.to_string()).collect()),
    ];
    for (name, core) in lists {
        assert_eq!(
            room_list(&js, name),
            core,
            "room.js {name} differs from core::room"
        );
        for value in &core {
            assert!(
                docs.contains(&format!("`{value}`")),
                "docs/PROTOCOL.md must list `{value}` ({name})"
            );
        }
    }
    for size in BoardSize::ALL {
        let (w, h) = size.dimensions();
        assert!(
            js.contains(&format!("{}: [{w}, {h}]", size.name())),
            "room.js SIZE_DIMENSIONS differs for {}",
            size.name()
        );
    }
}
