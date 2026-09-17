//! `pato-asset://` URI scheme handler.
//!
//! Custom widgets reference bundled images as `pato-asset://<plugin>/<path>`
//! (the core rewrites a plugin's bare `pato-asset:` URLs into this scoped form
//! during validation). This serves those files from the owning plugin's
//! `assets/` directory and nothing else — path traversal is rejected and an
//! unknown plugin 404s.

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

use tauri::http::{self, Request, Response, StatusCode, Uri};
use tauri::{UriSchemeContext, Wry};

use crate::wasm;

pub fn handle(_ctx: UriSchemeContext<'_, Wry>, request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    match resolve(request.uri()) {
        Ok((bytes, mime)) => Response::builder()
            .header(http::header::CONTENT_TYPE, mime)
            .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(http::header::CACHE_CONTROL, "no-cache")
            .body(Cow::Owned(bytes))
            .expect("valid asset response"),
        Err(status) => Response::builder()
            .status(status)
            .body(Cow::Borrowed(&b""[..]))
            .expect("valid error response"),
    }
}

fn resolve(uri: &Uri) -> Result<(Vec<u8>, &'static str), StatusCode> {
    // pato-asset://<plugin>/<path...>  — on some platforms the authority is
    // folded into the path (pato-asset://localhost/<plugin>/<path>), so accept
    // both shapes.
    let host = uri.host().unwrap_or_default();
    let raw_path = uri.path().trim_start_matches('/');
    let (plugin, rel) = if host.is_empty() || host == "localhost" {
        raw_path.split_once('/').ok_or(StatusCode::BAD_REQUEST)?
    } else {
        (host, raw_path)
    };
    if plugin.is_empty() || rel.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let assets = wasm::plugin_root(plugin)
        .ok_or(StatusCode::NOT_FOUND)?
        .join("assets");

    // Only plain path components — no `..`, no absolute, no prefixes.
    let mut safe = PathBuf::new();
    for component in Path::new(rel).components() {
        match component {
            Component::Normal(part) => safe.push(part),
            _ => return Err(StatusCode::BAD_REQUEST),
        }
    }

    let full = assets.join(&safe);
    let canon_assets = assets.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    let canon_full = full.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    if !canon_full.starts_with(&canon_assets) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let bytes = std::fs::read(&canon_full).map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((bytes, mime_for(&canon_full)))
}

fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("json") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    }
}
