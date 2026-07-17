use axum::{
    async_trait,
    extract::{FromRequestParts, Path},
    http::request::Parts,
};
use uuid::Uuid;
use crate::errors::AppError;

/// Agent ID 提取器 - 支持 UUID 或 shortname
#[derive(Debug, Clone)]
pub struct AgentIdOrShortname(pub Uuid);

#[async_trait]
impl<S> FromRequestParts<S> for AgentIdOrShortname
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(id_str): Path<String> = Path::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::BadRequest("Invalid agent ID parameter".to_string()))?;

        // 尝试直接解析为 UUID
        if let Ok(uuid) = Uuid::parse_str(&id_str) {
            return Ok(AgentIdOrShortname(uuid));
        }

        // 尝试作为 shortname 解析（格式: "ag_" + base62编码）
        if id_str.starts_with("ag_") {
            if let Some(uuid) = decode_shortname(&id_str) {
                return Ok(AgentIdOrShortname(uuid));
            }
        }

        Err(AppError::BadRequest(format!(
            "Invalid agent identifier: {}. Expected UUID or shortname (ag_*)",
            id_str
        )))
    }
}

/// Company ID 提取器
#[derive(Debug, Clone)]
pub struct CompanyIdOrShortname(pub Uuid);

#[async_trait]
impl<S> FromRequestParts<S> for CompanyIdOrShortname
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(id_str): Path<String> = Path::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::BadRequest("Invalid company ID parameter".to_string()))?;

        // 尝试直接解析为 UUID
        if let Ok(uuid) = Uuid::parse_str(&id_str) {
            return Ok(CompanyIdOrShortname(uuid));
        }

        // 尝试作为 shortname 解析（格式: "co_" + base62编码）
        if id_str.starts_with("co_") {
            if let Some(uuid) = decode_shortname(&id_str) {
                return Ok(CompanyIdOrShortname(uuid));
            }
        }

        Err(AppError::BadRequest(format!(
            "Invalid company identifier: {}. Expected UUID or shortname (co_*)",
            id_str
        )))
    }
}

/// Revision ID 提取器
#[derive(Debug, Clone)]
pub struct RevisionId(pub Uuid);

#[async_trait]
impl<S> FromRequestParts<S> for RevisionId
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(id_str): Path<String> = Path::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::BadRequest("Invalid revision ID parameter".to_string()))?;

        Uuid::parse_str(&id_str)
            .map(RevisionId)
            .map_err(|_| AppError::BadRequest(format!("Invalid revision UUID: {}", id_str)))
    }
}

// ========== Shortname 编解码实现 ==========

const BASE62_CHARSET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// 将 UUID 编码为 shortname (base62)
pub fn encode_shortname(prefix: &str, uuid: Uuid) -> String {
    let bytes = uuid.as_bytes();
    let mut num: u128 = 0;

    for &byte in bytes {
        num = (num << 8) | byte as u128;
    }

    let encoded = encode_base62(num);
    format!("{}{}", prefix, encoded)
}

/// 将 shortname 解码为 UUID
fn decode_shortname(shortname: &str) -> Option<Uuid> {
    // 移除前缀（ag_、co_ 等）
    let encoded = shortname.strip_prefix("ag_")
        .or_else(|| shortname.strip_prefix("co_"))
        .or_else(|| shortname.strip_prefix("rv_"))?;

    let num = decode_base62(encoded)?;

    // 将 u128 转换回 UUID bytes
    let mut bytes = [0u8; 16];
    for i in (0..16).rev() {
        bytes[i] = (num >> ((15 - i) * 8)) as u8;
    }

    Some(Uuid::from_bytes(bytes))
}

/// Base62 编码
fn encode_base62(mut num: u128) -> String {
    if num == 0 {
        return "0".to_string();
    }

    let mut result = Vec::new();
    while num > 0 {
        let remainder = (num % 62) as usize;
        result.push(BASE62_CHARSET[remainder]);
        num /= 62;
    }

    result.reverse();
    String::from_utf8(result).unwrap()
}

/// Base62 解码
fn decode_base62(s: &str) -> Option<u128> {
    let mut num: u128 = 0;

    for ch in s.bytes() {
        let digit = BASE62_CHARSET.iter().position(|&c| c == ch)?;
        num = num.checked_mul(62)?.checked_add(digit as u128)?;
    }

    Some(num)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shortname_roundtrip() {
        let uuid = Uuid::new_v4();
        let shortname = encode_shortname("ag_", uuid);

        assert!(shortname.starts_with("ag_"));

        let decoded = decode_shortname(&shortname);
        assert_eq!(decoded, Some(uuid));
    }

    #[test]
    fn test_base62_encoding() {
        assert_eq!(encode_base62(0), "0");
        assert_eq!(encode_base62(61), "z");
        assert_eq!(encode_base62(62), "10");
        assert_eq!(encode_base62(100), "1C");
    }

    #[test]
    fn test_base62_decoding() {
        assert_eq!(decode_base62("0"), Some(0));
        assert_eq!(decode_base62("z"), Some(61));
        assert_eq!(decode_base62("10"), Some(62));
        assert_eq!(decode_base62("1C"), Some(100));
    }

    #[test]
    fn test_invalid_shortname() {
        assert_eq!(decode_shortname("invalid"), None);
        assert_eq!(decode_shortname("ag_!!!"), None);
    }

    #[test]
    fn test_company_shortname() {
        let uuid = Uuid::new_v4();
        let shortname = encode_shortname("co_", uuid);

        assert!(shortname.starts_with("co_"));

        let decoded = decode_shortname(&shortname);
        assert_eq!(decoded, Some(uuid));
    }
}
