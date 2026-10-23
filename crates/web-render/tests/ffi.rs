#![allow(unsafe_code, clippy::unwrap_used)]

mod common;

use bomberust_web_render::ffi::*;
use common::*;

fn load(bytes: &[u8]) -> i32 {
    let size = bytes.len().max(1);
    let ptr = bomberust_alloc(size);
    assert!(!ptr.is_null());
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
    let code = unsafe { bomberust_load(ptr, bytes.len()) };
    unsafe { bomberust_free(ptr, size) };
    code
}

fn fixture_json() -> Vec<u8> {
    serde_json::to_vec(&obs(&standard())).unwrap()
}

#[test]
fn allocation_limits() {
    assert!(bomberust_alloc(0).is_null());
    assert!(bomberust_alloc(2_000_000).is_null());
    let p = bomberust_alloc(64);
    assert!(!p.is_null());
    unsafe {
        for i in 0..64 {
            p.add(i).write(i as u8);
        }
        bomberust_free(p, 64);
    }
    unsafe { bomberust_free(std::ptr::null_mut(), 10) };
}

#[test]
fn the_stride_is_sixteen() {
    assert_eq!(bomberust_stride(), 16);
}

#[test]
fn nothing_is_drawn_before_a_load() {
    assert_eq!(bomberust_frame(800.0, 600.0, 0.5), 0);
}

#[test]
fn loading_the_fixture_works_and_frames_have_commands() {
    assert_eq!(load(&fixture_json()), 0);
    let n = bomberust_frame(800.0, 600.0, 1.0);
    assert!(n > 100, "{n}");
    let ptr = bomberust_output_ptr();
    assert!(!ptr.is_null());
    let first = unsafe { std::slice::from_raw_parts(ptr, 16) };
    assert_eq!(first[0], 0.0, "first command is a quad");
    // calling again with the same arguments gives the same count
    assert_eq!(bomberust_frame(800.0, 600.0, 1.0), n);
}

#[test]
fn bad_input_gives_error_codes_not_panics() {
    assert_eq!(load(&[0xff, 0xfe, 0xfd]), 1, "not UTF-8");
    assert_eq!(load(b"{}"), 2);
    assert_eq!(load(b"not json"), 2);
    assert_eq!(load(b""), 2);

    let mut big = serde_json::to_value(obs(&standard())).unwrap();
    let tiles = big["arena"]["tiles"].as_array().unwrap().clone();
    big["arena"]["width"] = serde_json::json!(65);
    big["arena"]["height"] = serde_json::json!(65);
    big["arena"]["tiles"] =
        serde_json::Value::Array(tiles.iter().cycle().take(65 * 65).cloned().collect());
    assert_eq!(
        load(big.to_string().as_bytes()),
        3,
        "arena larger than 64x64"
    );

    let mut short = serde_json::to_value(obs(&standard())).unwrap();
    short["arena"]["tiles"] = serde_json::json!(["Floor", "Wall"]);
    assert_eq!(
        load(short.to_string().as_bytes()),
        2,
        "tile list does not match the size"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x1234_5678_9abc_def0;
    for len in 0..300usize {
        let bytes: Vec<u8> = (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x & 0xff) as u8
            })
            .collect();
        let code = load(&bytes);
        assert!(code == 1 || code == 2, "len {len} gave {code}");
    }
}

#[test]
fn odd_frame_arguments_are_handled() {
    assert_eq!(load(&fixture_json()), 0);
    for (w, h, a) in [
        (0.0, 0.0, 0.5),
        (-5.0, 100.0, 0.5),
        (f32::NAN, 100.0, 0.5),
        (800.0, 600.0, f32::NAN),
    ] {
        let n = bomberust_frame(w, h, a);
        assert!(n < 100_000);
    }
}

#[test]
fn picking_a_tile_finds_the_tile_under_a_point() {
    assert_eq!(load(&fixture_json()), 0);
    bomberust_frame(800.0, 600.0, 1.0);
    // the centre of the arena is the centre of the canvas
    let t = bomberust_pick_tile(400.0, 300.0);
    assert!(t >= 0, "{t}");
    assert_eq!(bomberust_pick_tile(-100.0, -100.0), -1);
}

#[test]
fn portraits_work_without_a_loaded_game() {
    let n = bomberust_portrait(1, 160.0, 226.0, 8);
    assert!(n > 10, "{n}");
    let ptr = bomberust_output_ptr();
    assert!(!ptr.is_null());
    let first = unsafe { std::slice::from_raw_parts(ptr, 16) };
    assert_eq!(first[0], 0.0, "a portrait is made of quads");
    assert!(
        bomberust_portrait(0, 160.0, 226.0, 0) > n,
        "the whole figure has more pixels"
    );
    assert_eq!(bomberust_portrait(0, 0.0, 10.0, 0), 0);
    assert_eq!(bomberust_portrait(u32::MAX, f32::NAN, 10.0, u32::MAX), 0);
    assert!(
        bomberust_portrait(7, 50.0, 50.0, u32::MAX) > 0,
        "slot and rows are clamped"
    );
}

fn set_looks(bytes: &[u8]) -> i32 {
    let size = bytes.len().max(1);
    let ptr = bomberust_alloc(size);
    assert!(!ptr.is_null());
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
    let code = unsafe { bomberust_set_looks(ptr, bytes.len()) };
    unsafe { bomberust_free(ptr, size) };
    code
}

#[test]
fn set_looks_codes_and_leniency() {
    assert_eq!(set_looks(br#"[{"colour":"Green","head":"Crown"}]"#), 0);
    assert_eq!(set_looks(b"[]"), 0, "no entries means defaults");
    assert_eq!(
        set_looks(b"[1,null,\"x\",{\"colour\":5}]"),
        0,
        "bad entries become defaults"
    );
    assert_eq!(set_looks(&[0xff, 0xfe, 0xfd]), 1, "not UTF-8");
    assert_eq!(set_looks(b"{}"), 2, "not an array");
    assert_eq!(set_looks(b"not json"), 2);
    assert_eq!(set_looks(b""), 2);
    let mut x: u64 = 0x2545_f491_4f6c_dd1d;
    for len in 0..200usize {
        let bytes: Vec<u8> = (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x & 0xff) as u8
            })
            .collect();
        let code = set_looks(&bytes);
        assert!((0..=2).contains(&code), "len {len} gave {code}");
    }
}

#[test]
fn f3_us1_as5_frames_and_portraits_use_the_loaded_looks() {
    use bomberust_web_render::look::CROWN;
    let crowns = |n: usize| {
        let data = unsafe { std::slice::from_raw_parts(bomberust_output_ptr(), n * 16) };
        data.chunks(16)
            .filter(|c| c[0] == 0.0 && c[1..4] == CROWN[..] && c[4] == 1.0)
            .count()
    };
    assert_eq!(load(&fixture_json()), 0);
    assert_eq!(set_looks(b"[]"), 0);
    let n = bomberust_frame(800.0, 600.0, 1.0);
    assert_eq!(crowns(n), 0);
    assert_eq!(
        set_looks(br#"[{"head":"Crown"},{},{},{"head":"Crown"}]"#),
        0
    );
    let n = bomberust_frame(800.0, 600.0, 1.0);
    assert!(crowns(n) > 0, "players in the match wear the crown");
    let p = bomberust_portrait(3, 160.0, 226.0, 7);
    assert!(crowns(p) > 0, "portraits use the looks too");
    let p = bomberust_portrait(1, 160.0, 226.0, 7);
    assert_eq!(crowns(p), 0);
}

#[test]
fn f3_us1_as3_the_3d_portrait_export_draws_quads_and_rejects_bad_sizes() {
    let n = bomberust_portrait3d(1, 160.0, 226.0, 0.0);
    assert!(n > 20, "{n}");
    let ptr = bomberust_output_ptr();
    let first = unsafe { std::slice::from_raw_parts(ptr, 16) };
    assert_eq!(first[0], 0.0, "made of quads");
    assert_eq!(bomberust_portrait3d(0, 0.0, 10.0, 0.0), 0);
    assert_eq!(bomberust_portrait3d(0, -1.0, 10.0, 0.0), 0);
    assert_eq!(bomberust_portrait3d(u32::MAX, f32::NAN, 10.0, f32::NAN), 0);
    assert!(
        bomberust_portrait3d(7, 50.0, 50.0, f32::NAN) > 0,
        "slot is clamped and a bad phase is harmless"
    );
}

/// A small deterministic generator for the random-input tests.
fn noise(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[test]
fn f3_random_bytes_and_floats_never_break_set_looks_or_portrait3d() {
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    for _ in 0..500 {
        let len = (noise(&mut seed) % 200) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| (noise(&mut seed) & 0xff) as u8).collect();
        let code = set_looks(&bytes);
        assert!((0..=4).contains(&code), "{code}");
        let float = |seed: &mut u64| match noise(seed) % 8 {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            3 => f32::MAX,
            4 => -1.0,
            5 => 0.0,
            _ => (noise(seed) % 100_000) as f32 / 7.0,
        };
        let slot = noise(&mut seed) as u32;
        let (w, h, phase) = (float(&mut seed), float(&mut seed), float(&mut seed));
        let n = bomberust_portrait3d(slot, w, h, phase);
        let finite_size = w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0;
        assert_eq!(n > 0, finite_size, "{w} x {h}");
    }
    assert_eq!(set_looks(b"[]"), 0);
}
