//! 静态文件嵌入（rust-embed）
//! 编译时将 assets/web/ 打包进二进制

use axum::{
    body::Body,
    http::{StatusCode, header},
    response::Response,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets/web/"]
struct WebAssets;

/// 获取嵌入资源
fn get_asset(path: &str) -> Option<(rust_embed::EmbeddedFile, &'static str)> {
    let file = WebAssets::get(path)?;
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    // 泄漏 mime 字符串以获得 'static 生命周期
    let mime_static: &'static str = Box::leak(mime.into_boxed_str());
    Some((file, mime_static))
}

/// 构建 HTTP 响应
fn asset_response(path: &str) -> Response<Body> {
    match get_asset(path) {
        Some((file, mime)) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .body(Body::from(file.data))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap(),
    }
}

pub async fn index_html() -> Response<Body> {
    asset_response("index.html")
}

pub async fn style_css() -> Response<Body> {
    asset_response("style.css")
}

pub async fn app_js() -> Response<Body> {
    asset_response("app.js")
}
