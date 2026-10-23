//! Top-down pixel-art graphics for the web page. Everything except `ffi` is safe Rust.
//!
//! The page loads this crate as a WebAssembly module: it hands over an `Observation` as JSON and
//! gets back a flat buffer of drawing commands (see `specs/002-web-client/contracts`).

pub mod commands;
pub mod depth;
pub mod ffi;
pub mod interp;
pub mod look;
pub mod sprites;
pub mod view;
