use axum::{
    body::Body,
    http::{HeaderName, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::LazyLock;

/// 安全相关的响应头集合。
///
/// 这些头在每次响应时注入，用于缓解常见的 Web 攻击
/// （MIME 嗅探、点击劫持、XSS 反射等）。
static SECURITY_HEADERS: LazyLock<Vec<(HeaderName, HeaderValue)>> = LazyLock::new(|| {
    let mut headers = Vec::new();

    // 阻止浏览器对响应内容类型进行 MIME 嗅探
    headers.push((
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    ));

    // 防止页面被嵌入到 <frame>/<iframe> 中（点击劫持防护）
    headers.push((
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    ));

    // 启用浏览器内建的 XSS 过滤器（旧版浏览器仍有效）
    headers.push((
        HeaderName::from_static("x-xss-protection"),
        HeaderValue::from_static("1; mode=block"),
    ));

    // 限制 Referer 头仅发送源（不泄露完整路径）
    headers.push((
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    ));

    // 显式禁用浏览器特性/API 的访问（防指纹与非常规注入）
    headers.push((
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
    ));

    headers
});

/// Axum 中间件：为每个响应注入基础安全头。
///
/// 用法：
/// ```rust,ignore
/// router.layer(axum::middleware::from_fn(security_headers_middleware));
/// ```
pub async fn security_headers_middleware(
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let mut response = next.run(request).await;

    let headers = response.headers_mut();
    for (name, value) in SECURITY_HEADERS.iter() {
        // 若上游已显式设置该头，则保留原值，不覆盖。
        if !headers.contains_key(name) {
            headers.insert(name.clone(), value.clone());
        }
    }

    Ok(response)
}
