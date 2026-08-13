//! 附件 / 资产的内容类型与大小策略 —— 对齐 Paperclip `server/src/attachment-types.ts`
//!
//! 该模块只包含纯函数与进程级配置读取，便于单测覆盖：
//! - 允许的 MIME 类型白名单（支持 `image/*`、`application/vnd...*` 通配）
//! - 上传大小上限（`PARROT_ATTACHMENT_MAX_BYTES`，默认 10MiB）
//! - Office 文件在 `application/octet-stream` 时按扩展名回推真实类型
//! - 响应头 `Content-Disposition` 的 inline / attachment 判定
//! - `Range` 请求头解析（附件 content 支持断点续传/视频拖动）
//! - SVG 危险构造检测（Paperclip 用 DOMPurify 做 sanitize，这里做保守拒绝）

use std::sync::OnceLock;

/// 默认允许的内容类型，与 Paperclip `DEFAULT_ALLOWED_TYPES` 一致。
pub const DEFAULT_ALLOWED_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/jpg",
    "image/webp",
    "image/gif",
    "application/pdf",
    "application/zip",
    "text/markdown",
    "text/plain",
    "application/json",
    "text/csv",
    "text/html",
    "application/msword",
    "application/vnd.ms-excel",
    "application/vnd.ms-powerpoint",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "video/mp4",
    "video/webm",
    "video/quicktime",
    "video/x-m4v",
];

pub const DEFAULT_ATTACHMENT_CONTENT_TYPE: &str = "application/octet-stream";
pub const SVG_CONTENT_TYPE: &str = "image/svg+xml";

/// 这些"泛型"类型说明浏览器/客户端没识别出真实类型，需要按文件名回推。
pub const GENERIC_ATTACHMENT_CONTENT_TYPES: &[&str] = &[
    "application/octet-stream",
    "binary/octet-stream",
    "application/x-binary",
];

/// 可以直接 inline 展示的类型（其余一律走 `attachment` 下载）。
pub const INLINE_ATTACHMENT_TYPES: &[&str] = &[
    "image/*",
    "application/pdf",
    "text/plain",
    "text/markdown",
    "application/json",
    "text/csv",
    "video/mp4",
    "video/webm",
    "video/quicktime",
    "video/x-m4v",
];

/// 公司 Logo 允许的类型，对齐 Paperclip `ALLOWED_COMPANY_LOGO_CONTENT_TYPES`。
pub const ALLOWED_COMPANY_LOGO_CONTENT_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/jpg",
    "image/webp",
    "image/gif",
    SVG_CONTENT_TYPE,
];

/// 默认上限 10MiB，与 Paperclip 一致。
pub const DEFAULT_MAX_ATTACHMENT_BYTES: i64 = 10 * 1024 * 1024;
/// 公司级 `attachment_max_bytes` 的硬上限（防止把实例打爆）。
pub const MAX_COMPANY_ATTACHMENT_MAX_BYTES: i64 = 512 * 1024 * 1024;

/// 解析逗号分隔的 MIME 模式列表；为空时回落到默认白名单。
pub fn parse_allowed_types(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw else {
        return DEFAULT_ALLOWED_TYPES.iter().map(|s| s.to_string()).collect();
    };
    let parsed: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if parsed.is_empty() {
        DEFAULT_ALLOWED_TYPES.iter().map(|s| s.to_string()).collect()
    } else {
        parsed
    }
}

/// 判断 `content_type` 是否命中任一模式。支持精确匹配与 `xxx/*`、`xxx.*` 前缀通配。
pub fn matches_content_type(content_type: &str, allowed_patterns: &[String]) -> bool {
    let ct = content_type.trim().to_ascii_lowercase();
    allowed_patterns.iter().any(|pattern| {
        if pattern == "*" {
            return true;
        }
        if pattern.ends_with("/*") || pattern.ends_with(".*") {
            return ct.starts_with(&pattern[..pattern.len() - 1]);
        }
        ct == *pattern
    })
}

/// 归一化内容类型：去空白、转小写、空值回落到 `application/octet-stream`。
pub fn normalize_content_type(content_type: Option<&str>) -> String {
    let normalized = content_type.unwrap_or("").trim().to_ascii_lowercase();
    if normalized.is_empty() {
        DEFAULT_ATTACHMENT_CONTENT_TYPE.to_string()
    } else {
        normalized
    }
}

/// 按扩展名回推 Office 类型（客户端常常只给 `application/octet-stream`）。
pub fn infer_office_content_type_from_filename(filename: Option<&str>) -> Option<&'static str> {
    let lower = filename.unwrap_or("").trim().to_ascii_lowercase();
    if lower.ends_with(".docx") {
        return Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document");
    }
    if lower.ends_with(".xlsx") {
        return Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
    }
    if lower.ends_with(".pptx") {
        return Some("application/vnd.openxmlformats-officedocument.presentationml.presentation");
    }
    if lower.ends_with(".doc") {
        return Some("application/msword");
    }
    if lower.ends_with(".xls") {
        return Some("application/vnd.ms-excel");
    }
    if lower.ends_with(".ppt") {
        return Some("application/vnd.ms-powerpoint");
    }
    None
}

/// 上传时的内容类型归一化：泛型类型 + 已知 Office 扩展名 → 回推真实类型。
pub fn normalize_upload_content_type(
    content_type: Option<&str>,
    original_filename: Option<&str>,
    allowed_patterns: &[String],
) -> String {
    let normalized = normalize_content_type(content_type);
    if !GENERIC_ATTACHMENT_CONTENT_TYPES.contains(&normalized.as_str()) {
        return normalized;
    }
    let Some(inferred) = infer_office_content_type_from_filename(original_filename) else {
        return normalized;
    };
    if !matches_content_type(inferred, allowed_patterns) {
        return normalized;
    }
    inferred.to_string()
}

/// 该类型是否适合 inline 展示。
pub fn is_inline_attachment_content_type(content_type: &str) -> bool {
    let patterns: Vec<String> = INLINE_ATTACHMENT_TYPES.iter().map(|s| s.to_string()).collect();
    matches_content_type(content_type, &patterns)
}

// ---------- 进程级单例（启动时读取一次 env） ----------

fn allowed_patterns() -> &'static Vec<String> {
    static PATTERNS: OnceLock<Vec<String>> = OnceLock::new();
    PATTERNS.get_or_init(|| parse_allowed_types(std::env::var("PARROT_ALLOWED_ATTACHMENT_TYPES").ok().as_deref()))
}

/// 使用进程级白名单判断类型是否允许上传。
pub fn is_allowed_content_type(content_type: &str) -> bool {
    matches_content_type(content_type, allowed_patterns())
}

/// 使用进程级白名单做上传类型归一化。
pub fn normalize_upload_attachment_content_type(
    content_type: Option<&str>,
    original_filename: Option<&str>,
) -> String {
    normalize_upload_content_type(content_type, original_filename, allowed_patterns())
}

/// 进程级上传大小上限（字节）。
pub fn max_attachment_bytes() -> i64 {
    static MAX: OnceLock<i64> = OnceLock::new();
    *MAX.get_or_init(|| {
        std::env::var("PARROT_ATTACHMENT_MAX_BYTES")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_MAX_ATTACHMENT_BYTES)
    })
}

/// 归一化公司级附件上限：非法值回落到默认值，并且不得超过实例上限。
pub fn normalize_issue_attachment_max_bytes(value: Option<i64>, instance_max: i64) -> i64 {
    match value {
        Some(v) if v > 0 => v.min(MAX_COMPANY_ATTACHMENT_MAX_BYTES).min(instance_max),
        _ => DEFAULT_MAX_ATTACHMENT_BYTES.min(instance_max),
    }
}

// ---------- 响应头相关 ----------

/// 去掉文件名里的引号与控制字符，避免污染 `Content-Disposition`。
pub fn sanitize_header_filename(filename: Option<&str>, fallback: &str) -> String {
    let raw = filename.unwrap_or("").trim();
    let cleaned: String = raw
        .chars()
        .filter(|c| *c != '"' && *c != '\\' && !c.is_control())
        .collect();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned
    }
}

/// 生成 `Content-Disposition` 头。`force_download=true` 时强制 attachment。
pub fn content_disposition(content_type: &str, filename: Option<&str>, force_download: bool, fallback: &str) -> String {
    let disposition = if force_download || !is_inline_attachment_content_type(content_type) {
        "attachment"
    } else {
        "inline"
    };
    format!("{}; filename=\"{}\"", disposition, sanitize_header_filename(filename, fallback))
}

/// SVG 需要额外的 CSP 沙箱头，避免内联脚本执行。
pub const SVG_CONTENT_SECURITY_POLICY: &str =
    "sandbox; default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'";

// ---------- Range 解析 ----------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeSpec {
    /// 没有 Range 头，或者是可以忽略的形式 —— 返回完整内容。
    Full,
    /// 合法的单段 Range，闭区间 `[start, end]`。
    Range { start: i64, end: i64 },
    /// 非法/越界 Range —— 调用方应返回 416。
    Invalid,
}

/// 解析 `Range: bytes=start-end`。只支持单段，符合 Paperclip 行为。
pub fn parse_range_header(raw: Option<&str>, content_length: i64) -> RangeSpec {
    let Some(raw) = raw else { return RangeSpec::Full };
    let raw = raw.trim();
    if raw.is_empty() {
        return RangeSpec::Full;
    }
    let Some(spec) = raw.strip_prefix("bytes=") else {
        return RangeSpec::Invalid;
    };
    if spec.contains(',') {
        // 多段 Range 不支持，退化为整体返回。
        return RangeSpec::Full;
    }
    let Some((start_raw, end_raw)) = spec.split_once('-') else {
        return RangeSpec::Invalid;
    };
    let start_raw = start_raw.trim();
    let end_raw = end_raw.trim();
    if content_length <= 0 {
        return RangeSpec::Invalid;
    }

    if start_raw.is_empty() {
        // suffix range: bytes=-N  → 最后 N 字节
        let Ok(suffix) = end_raw.parse::<i64>() else {
            return RangeSpec::Invalid;
        };
        if suffix <= 0 {
            return RangeSpec::Invalid;
        }
        let start = (content_length - suffix).max(0);
        return RangeSpec::Range { start, end: content_length - 1 };
    }

    let Ok(start) = start_raw.parse::<i64>() else {
        return RangeSpec::Invalid;
    };
    if start < 0 || start >= content_length {
        return RangeSpec::Invalid;
    }
    let end = if end_raw.is_empty() {
        content_length - 1
    } else {
        match end_raw.parse::<i64>() {
            Ok(v) => v.min(content_length - 1),
            Err(_) => return RangeSpec::Invalid,
        }
    };
    if end < start {
        return RangeSpec::Invalid;
    }
    RangeSpec::Range { start, end }
}

// ---------- SVG 安全检查 ----------

/// Paperclip 用 DOMPurify 做 sanitize；Rust 侧不引入 DOM 依赖，改为保守拒绝：
/// 一旦发现 `<script>`、`<foreignObject>`、`on*=` 事件属性或外部 href，直接判定不安全。
/// 返回 `true` 表示该 SVG 可以安全存储/回放。
pub fn is_safe_svg(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else { return false };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("<svg") && !lower.starts_with("<?xml") && !lower.starts_with("<!doctype") {
        return false;
    }
    if !lower.contains("<svg") {
        return false;
    }
    for needle in ["<script", "</script", "<foreignobject", "javascript:", "<iframe", "<embed", "<object"] {
        if lower.contains(needle) {
            return false;
        }
    }
    if contains_event_handler_attribute(&lower) {
        return false;
    }
    if contains_external_href(&lower) {
        return false;
    }
    true
}

/// 检测形如 ` onload=`、`\tonclick =` 的事件属性。
fn contains_event_handler_attribute(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    let mut idx = 0usize;
    while let Some(found) = lower[idx..].find("on") {
        let pos = idx + found;
        idx = pos + 2;
        // 前一个字符必须是空白或 `/`（属性起始位置）
        if pos == 0 {
            continue;
        }
        let prev = bytes[pos - 1] as char;
        if !prev.is_whitespace() && prev != '/' {
            continue;
        }
        // 向后找到 `=` 之前必须全是字母
        let mut cursor = pos + 2;
        let mut name_len = 0usize;
        while cursor < bytes.len() && (bytes[cursor] as char).is_ascii_alphabetic() {
            cursor += 1;
            name_len += 1;
        }
        if name_len == 0 {
            continue;
        }
        while cursor < bytes.len() && (bytes[cursor] as char).is_whitespace() {
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == b'=' {
            return true;
        }
    }
    false
}

/// 检测非片段引用的 `href` / `xlink:href`（只允许 `#anchor`）。
fn contains_external_href(lower: &str) -> bool {
    for key in ["href=\"", "href='"] {
        let mut idx = 0usize;
        while let Some(found) = lower[idx..].find(key) {
            let pos = idx + found + key.len();
            idx = pos;
            let quote = key.chars().last().unwrap();
            let Some(end) = lower[pos..].find(quote) else { return true };
            let value = lower[pos..pos + end].trim();
            if !value.is_empty() && !value.starts_with('#') {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_allowed_types_falls_back_to_defaults() {
        assert_eq!(parse_allowed_types(None).len(), DEFAULT_ALLOWED_TYPES.len());
        assert_eq!(parse_allowed_types(Some("  ")).len(), DEFAULT_ALLOWED_TYPES.len());
        assert_eq!(parse_allowed_types(Some("image/*, application/pdf")), vec!["image/*", "application/pdf"]);
    }

    #[test]
    fn matches_content_type_supports_exact_and_wildcard() {
        let patterns = parse_allowed_types(Some("image/*,application/pdf,application/vnd.openxmlformats-officedocument.*"));
        assert!(matches_content_type("image/png", &patterns));
        assert!(matches_content_type("IMAGE/WEBP", &patterns));
        assert!(matches_content_type("application/pdf", &patterns));
        assert!(matches_content_type(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            &patterns
        ));
        assert!(!matches_content_type("text/html", &patterns));
        assert!(matches_content_type("anything/here", &parse_allowed_types(Some("*"))));
    }

    #[test]
    fn normalize_content_type_handles_empty() {
        assert_eq!(normalize_content_type(None), DEFAULT_ATTACHMENT_CONTENT_TYPE);
        assert_eq!(normalize_content_type(Some("   ")), DEFAULT_ATTACHMENT_CONTENT_TYPE);
        assert_eq!(normalize_content_type(Some(" Image/PNG ")), "image/png");
    }

    #[test]
    fn office_types_are_inferred_from_filename() {
        assert_eq!(
            infer_office_content_type_from_filename(Some("Report.DOCX")),
            Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
        );
        assert_eq!(infer_office_content_type_from_filename(Some("a.xls")), Some("application/vnd.ms-excel"));
        assert_eq!(infer_office_content_type_from_filename(Some("a.png")), None);
        assert_eq!(infer_office_content_type_from_filename(None), None);
    }

    #[test]
    fn upload_content_type_only_rewrites_generic_types() {
        let patterns = parse_allowed_types(None);
        assert_eq!(
            normalize_upload_content_type(Some("application/octet-stream"), Some("plan.docx"), &patterns),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        // 非泛型类型不改写
        assert_eq!(normalize_upload_content_type(Some("image/png"), Some("plan.docx"), &patterns), "image/png");
        // 回推出的类型不在白名单里时保持原样
        let narrow = parse_allowed_types(Some("image/*"));
        assert_eq!(
            normalize_upload_content_type(Some("application/octet-stream"), Some("plan.docx"), &narrow),
            "application/octet-stream"
        );
    }

    #[test]
    fn inline_disposition_is_only_for_previewable_types() {
        assert!(is_inline_attachment_content_type("image/png"));
        assert!(is_inline_attachment_content_type("application/pdf"));
        assert!(!is_inline_attachment_content_type("application/zip"));
        assert_eq!(
            content_disposition("image/png", Some("logo.png"), false, "asset"),
            "inline; filename=\"logo.png\""
        );
        assert_eq!(
            content_disposition("image/png", Some("logo.png"), true, "asset"),
            "attachment; filename=\"logo.png\""
        );
        assert_eq!(
            content_disposition("application/zip", None, false, "attachment"),
            "attachment; filename=\"attachment\""
        );
    }

    #[test]
    fn header_filename_is_sanitized() {
        assert_eq!(sanitize_header_filename(Some("a\"b\\c.png"), "asset"), "abc.png");
        assert_eq!(sanitize_header_filename(Some("   "), "asset"), "asset");
        assert_eq!(sanitize_header_filename(None, "asset"), "asset");
    }

    #[test]
    fn company_max_bytes_is_clamped() {
        assert_eq!(normalize_issue_attachment_max_bytes(Some(1024), DEFAULT_MAX_ATTACHMENT_BYTES), 1024);
        assert_eq!(normalize_issue_attachment_max_bytes(Some(0), DEFAULT_MAX_ATTACHMENT_BYTES), DEFAULT_MAX_ATTACHMENT_BYTES);
        assert_eq!(normalize_issue_attachment_max_bytes(None, DEFAULT_MAX_ATTACHMENT_BYTES), DEFAULT_MAX_ATTACHMENT_BYTES);
        // 公司值超过实例上限时被裁剪
        assert_eq!(normalize_issue_attachment_max_bytes(Some(999_999_999), 4096), 4096);
    }

    #[test]
    fn range_header_parsing() {
        assert_eq!(parse_range_header(None, 100), RangeSpec::Full);
        assert_eq!(parse_range_header(Some(""), 100), RangeSpec::Full);
        assert_eq!(parse_range_header(Some("bytes=0-9"), 100), RangeSpec::Range { start: 0, end: 9 });
        assert_eq!(parse_range_header(Some("bytes=10-"), 100), RangeSpec::Range { start: 10, end: 99 });
        assert_eq!(parse_range_header(Some("bytes=-20"), 100), RangeSpec::Range { start: 80, end: 99 });
        // end 超界被裁剪
        assert_eq!(parse_range_header(Some("bytes=90-500"), 100), RangeSpec::Range { start: 90, end: 99 });
        // start 越界 → 416
        assert_eq!(parse_range_header(Some("bytes=100-"), 100), RangeSpec::Invalid);
        assert_eq!(parse_range_header(Some("bytes=50-10"), 100), RangeSpec::Invalid);
        assert_eq!(parse_range_header(Some("items=0-9"), 100), RangeSpec::Invalid);
        // 多段退化为整体
        assert_eq!(parse_range_header(Some("bytes=0-9,20-29"), 100), RangeSpec::Full);
    }

    #[test]
    fn svg_safety_rejects_dangerous_constructs() {
        assert!(is_safe_svg(br#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10"/></svg>"#));
        assert!(is_safe_svg(br##"<svg><use href="#icon"/></svg>"##));
        assert!(!is_safe_svg(br#"<svg><script>alert(1)</script></svg>"#));
        assert!(!is_safe_svg(br#"<svg onload="alert(1)"></svg>"#));
        assert!(!is_safe_svg(br#"<svg><a href="https://evil.example">x</a></svg>"#));
        assert!(!is_safe_svg(br#"<svg><foreignObject><body/></foreignObject></svg>"#));
        assert!(!is_safe_svg(b""));
        assert!(!is_safe_svg(b"<html><body/></html>"));
    }

    #[test]
    fn logo_allowlist_covers_common_image_types() {
        assert!(ALLOWED_COMPANY_LOGO_CONTENT_TYPES.contains(&"image/png"));
        assert!(ALLOWED_COMPANY_LOGO_CONTENT_TYPES.contains(&SVG_CONTENT_TYPE));
        assert!(!ALLOWED_COMPANY_LOGO_CONTENT_TYPES.contains(&"application/pdf"));
    }
}
