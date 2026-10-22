//! The only `unsafe` code of the crate: memory exchange with JavaScript.
//!
//! WebAssembly exports cannot take slices, so the page allocates a block, copies the JSON in,
//! and reads the command buffer back through a pointer. Every pointer is checked for null and
//! every size is capped; nothing here panics on bad input.
#![allow(unsafe_code)]

use std::cell::RefCell;

use bomberust_core::Observation;

use crate::commands::{build, flatten, portrait, Command};
use crate::interp::Interp;
use crate::view::fit;

const MAX_ALLOC: usize = 1 << 20;
const MAX_ARENA: u8 = 64;

struct State {
    obs: Option<Observation>,
    interp: Interp,
    out: Vec<f32>,
    canvas: (f32, f32),
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        obs: None,
        interp: Interp::default(),
        out: Vec::new(),
        canvas: (0.0, 0.0),
    });
}

/// Reserves `len` bytes (1 byte to 1 MiB); returns null otherwise.
#[no_mangle]
pub extern "C" fn bomberust_alloc(len: usize) -> *mut u8 {
    if len == 0 || len > MAX_ALLOC {
        return std::ptr::null_mut();
    }
    Box::into_raw(vec![0u8; len].into_boxed_slice()).cast::<u8>()
}

/// Releases a block from [`bomberust_alloc`]. `len` must be the size it was allocated with.
///
/// # Safety
/// `ptr` must be null or come from `bomberust_alloc(len)` and not be freed twice.
#[no_mangle]
pub unsafe extern "C" fn bomberust_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 || len > MAX_ALLOC {
        return;
    }
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
}

/// Parses an `Observation` from JSON. Returns 0 on success, 1 for invalid UTF-8, 2 for
/// invalid JSON or an inconsistent arena, 3 for an arena larger than 64x64.
///
/// # Safety
/// `ptr` must point to `len` readable bytes (or `len` must be 0).
#[no_mangle]
pub unsafe extern "C" fn bomberust_load(ptr: *const u8, len: usize) -> i32 {
    let bytes: &[u8] = if ptr.is_null() || len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return 1;
    };
    let Ok(obs) = serde_json::from_str::<Observation>(text) else {
        return 2;
    };
    if !obs.arena.is_valid() {
        return 2;
    }
    if obs.arena.width() > MAX_ARENA || obs.arena.height() > MAX_ARENA {
        return 3;
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.interp.update(&obs);
        s.obs = Some(obs);
    });
    0
}

/// Builds the drawing commands for a canvas; returns how many (0 if nothing is loaded).
#[no_mangle]
pub extern "C" fn bomberust_frame(width: f32, height: f32, alpha: f32) -> usize {
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return 0;
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(obs) = s.obs.as_ref() else {
            return 0;
        };
        let list = build(obs, &s.interp, alpha, (width, height));
        s.out = flatten(&list);
        s.canvas = (width, height);
        list.len()
    })
}

/// Builds the commands of a character card: slot `slot` fitted into `width` x `height` pixels,
/// the whole figure for `rows == 0`, otherwise its first `rows` rows. Returns how many.
#[no_mangle]
pub extern "C" fn bomberust_portrait(slot: u32, width: f32, height: f32, rows: u32) -> usize {
    let list = portrait((slot % 4) as u8, (width, height), rows as usize);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.out = flatten(&list);
        list.len()
    })
}

/// Pointer to the command buffer, valid until the next [`bomberust_frame`] or portrait call.
#[no_mangle]
pub extern "C" fn bomberust_output_ptr() -> *const f32 {
    STATE.with(|s| s.borrow().out.as_ptr())
}

/// Floats per command.
#[no_mangle]
pub extern "C" fn bomberust_stride() -> usize {
    Command::STRIDE
}

/// The tile (`y * width + x`) under a canvas point of the last frame, or -1.
#[no_mangle]
pub extern "C" fn bomberust_pick_tile(x: f32, y: f32) -> i32 {
    STATE.with(|s| {
        let s = s.borrow();
        let Some(obs) = s.obs.as_ref() else {
            return -1;
        };
        let (w, h) = (obs.arena.width(), obs.arena.height());
        let view = fit(w, h, s.canvas);
        let (tx, ty) = view.unproject(x, y);
        if !(tx.is_finite() && ty.is_finite()) || tx < 0.0 || ty < 0.0 {
            return -1;
        }
        let (tx, ty) = (tx.floor(), ty.floor());
        if tx >= f32::from(w) || ty >= f32::from(h) {
            return -1;
        }
        (ty as i32) * i32::from(w) + tx as i32
    })
}
