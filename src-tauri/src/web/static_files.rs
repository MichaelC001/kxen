//! dist/ 静态托管（rust-embed 编译期内嵌，release 嵌入二进制 / debug 运行时读盘）+ SPA 回退 + 安全响应头。

use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use super::WebContext;
use std::sync::atomic::Ordering;

/// vite 产物（pnpm build -> 仓库根 dist/；路径相对 src-tauri/Cargo.toml）。
/// rust-embed 宏编译期展开该目录：干净 checkout 需先 `pnpm build` 再跑 cargo（见 docs/web-mode/design.md）。
#[derive(RustEmbed)]
#[folder = "../dist"]
struct Assets;

const CSP: &str = "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; connect-src 'self' ws: wss:";

pub(super) async fn serve(State(ctx): State<WebContext>, uri: Uri) -> Response {
    let enabled = ctx.static_enabled.load(Ordering::Relaxed);
    lookup(enabled, uri.path())
}

/// 纯函数主体（可测）：未命中回退 index.html（SPA），关闭时整棵 404。
fn lookup(static_enabled: bool, request_path: &str) -> Response {
    if !static_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = normalize_path(request_path);
    match Assets::get(&path).or_else(|| Assets::get("index.html")) {
        Some(file) => respond(&path, file),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `/` 与目录形态落到 index.html；其余去前导斜杠对齐 embed key。
fn normalize_path(path: &str) -> String {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() || trimmed.ends_with('/') { format!("{trimmed}index.html") } else { trimmed.to_string() }
}

fn respond(path: &str, file: rust_embed::EmbeddedFile) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, file.metadata.mimetype())
        .header(header::CONTENT_SECURITY_POLICY, CSP)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    // vite hash 产物（/assets/*）内容寻址，可永久缓存
    if path.starts_with("assets/") && !path.ends_with("index.html") {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
    }
    builder.body(Body::from(file.data.into_owned())).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_maps_root_and_spa_routes() {
        assert_eq!(normalize_path("/"), "index.html");
        assert_eq!(normalize_path("/settings"), "settings");
        assert_eq!(normalize_path("/assets/app-abc123.js"), "assets/app-abc123.js");
    }

    #[test]
    fn static_disabled_returns_404() {
        assert_eq!(lookup(false, "/").status(), StatusCode::NOT_FOUND);
        assert_eq!(lookup(false, "/assets/anything.js").status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn index_served_with_security_headers_and_spa_fallback() {
        let response = lookup(true, "/");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "text/html");
        assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], CSP);
        assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(response.headers().get(header::CACHE_CONTROL).is_none(), "index.html 不做 immutable 缓存");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(!body.is_empty(), "dist/index.html 必须随编译产物存在");

        // SPA 回退：未命中路径也回 index.html
        let fallback = lookup(true, "/settings/workspace/abc");
        assert_eq!(fallback.status(), StatusCode::OK);
        assert_eq!(fallback.headers()[header::CONTENT_TYPE], "text/html");
    }

    #[test]
    fn hashed_assets_get_immutable_cache() {
        // 用真实 embed 清单里的任一 hash 产物断言缓存策略（清单来自 dist 编译期快照）
        let Some(asset) = Assets::iter().find(|name| name.starts_with("assets/")) else {
            panic!("dist/assets/ 必须存在 hash 产物");
        };
        let response = lookup(true, &format!("/{asset}"));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
    }
}
