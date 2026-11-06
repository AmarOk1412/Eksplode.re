//! The files of the web page, embedded in the binary.

pub const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self' ws: wss:; img-src 'self' data:";

/// One embedded file.
#[derive(Clone, Copy)]
pub struct Asset {
    pub content_type: &'static str,
    pub body: &'static [u8],
}

const HTML: &str = "text/html; charset=utf-8";
const JS: &str = "text/javascript; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";
const WASM: &str = "application/wasm";
const TEXT: &str = "text/plain; charset=utf-8";

/// `<package version>-<short git commit>`, shown in the page's corner.
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-", env!("EKSPLODE_RE_GIT_SHA"));

/// The complete static page inventory, shared by routing and the page-size gate.
pub const PAGE_ASSETS: &[(&str, Asset)] = &[
    (
        "/index.html",
        Asset {
            content_type: HTML,
            body: include_bytes!("../assets/index.html"),
        },
    ),
    (
        "/app.js",
        Asset {
            content_type: JS,
            body: include_bytes!("../assets/app.js"),
        },
    ),
    (
        "/look.js",
        Asset {
            content_type: JS,
            body: include_bytes!("../assets/look.js"),
        },
    ),
    (
        "/room.js",
        Asset {
            content_type: JS,
            body: include_bytes!("../assets/room.js"),
        },
    ),
    (
        "/audio.js",
        Asset {
            content_type: JS,
            body: include_bytes!("../assets/audio.js"),
        },
    ),
    (
        "/controls.js",
        Asset {
            content_type: JS,
            body: include_bytes!("../assets/controls.js"),
        },
    ),
    (
        "/nav.js",
        Asset {
            content_type: JS,
            body: include_bytes!("../assets/nav.js"),
        },
    ),
    (
        "/touch.js",
        Asset {
            content_type: JS,
            body: include_bytes!("../assets/touch.js"),
        },
    ),
    (
        "/style.css",
        Asset {
            content_type: CSS,
            body: include_bytes!("../assets/style.css"),
        },
    ),
    (
        "/lobby-scene.png",
        Asset {
            content_type: "image/png",
            body: include_bytes!("../assets/lobby-scene.png"),
        },
    ),
    (
        "/eksplode_re_render.wasm",
        Asset {
            content_type: WASM,
            body: include_bytes!("../assets/eksplode_re_render.wasm"),
        },
    ),
];

/// The fixed routes; anything else is `None`.
pub fn lookup(path: &str) -> Option<Asset> {
    if path == "/version" {
        return Some(Asset {
            content_type: TEXT,
            body: VERSION.as_bytes(),
        });
    }
    let path = if path == "/" { "/index.html" } else { path };
    PAGE_ASSETS
        .iter()
        .find(|(route, _)| *route == path)
        .map(|(_, asset)| *asset)
}
