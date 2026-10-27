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

/// The fixed routes; anything else is `None`.
pub fn lookup(path: &str) -> Option<Asset> {
    let (content_type, body): (&'static str, &'static [u8]) = match path {
        "/" | "/index.html" => (HTML, include_bytes!("../assets/index.html")),
        "/app.js" => (JS, include_bytes!("../assets/app.js")),
        "/look.js" => (JS, include_bytes!("../assets/look.js")),
        "/room.js" => (JS, include_bytes!("../assets/room.js")),
        "/audio.js" => (JS, include_bytes!("../assets/audio.js")),
        "/controls.js" => (JS, include_bytes!("../assets/controls.js")),
        "/nav.js" => (JS, include_bytes!("../assets/nav.js")),
        "/style.css" => (CSS, include_bytes!("../assets/style.css")),
        "/eksplode_re_render.wasm" => (WASM, include_bytes!("../assets/eksplode_re_render.wasm")),
        _ => return None,
    };
    Some(Asset { content_type, body })
}
