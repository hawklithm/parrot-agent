use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use crate::errors::AppError;

/// 权限检查结果
#[derive(Debug)]
pub enum AuthCheck {
    /// 允许访问
    Allowed,
    /// 拒绝访问
    Denied(String),
}

/// 实例管理员权限检查（对应 Paperclip 的 assertInstanceAdmin）
/// 
/// 检查请求是否具有实例级别的管理员权限。
/// 在 Paperclip 中，这用于：
/// - POST /adapters/install（安装适配器）
/// - DELETE /adapters/:type（删除适配器）
/// - POST /adapters/:type/reinstall（重新安装）
pub fn check_instance_admin(_req: &Request) -> AuthCheck {
    // TODO: 实现真实的权限检查
    // 当前实现：允许所有请求（开发模式）
    AuthCheck::Allowed
}

/// Board/Org 访问权限检查（对应 Paperclip 的 assertBoardOrgAccess）
/// 
/// 检查请求是否具有 Board 或组织级别的访问权限。
/// 在 Paperclip 中，这用于：
/// - GET /adapters（列出适配器）
/// - GET /adapters/:type（查看适配器详情）
/// - PATCH /adapters/:type（更新配置）
/// - POST /adapters/:type/reload（重新加载）
pub fn check_board_org_access(_req: &Request) -> AuthCheck {
    // TODO: 实现真实的权限检查
    // 当前实现：允许所有请求（开发模式）
    AuthCheck::Allowed
}

/// 实例管理员中间件
pub async fn require_instance_admin(
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    match check_instance_admin(&req) {
        AuthCheck::Allowed => Ok(next.run(req).await),
        AuthCheck::Denied(reason) => Err(AppError::Forbidden(format!(
            "Instance admin access required: {}",
            reason
        ))),
    }
}

/// Board/Org 访问中间件
pub async fn require_board_org_access(
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    match check_board_org_access(&req) {
        AuthCheck::Allowed => Ok(next.run(req).await),
        AuthCheck::Denied(reason) => Err(AppError::Forbidden(format!(
            "Board or organization access required: {}",
            reason
        ))),
    }
}

/// 直接返回 403 For的辅助函数
pub fn forbidden_response(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        format!("{{\"error\":\"{}\"}}", message),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    
    #[test]
    fn test_check_instance_admin() {
        let req = HttpRequest::builder()
            .uri("/adapters/install")
            .body(Body::empty())
            .unwrap();
        
        match check_instance_admin(&req) {
            AuthCheck::Allowed => assert!(true),
            AuthCheck::Denied(_) => panic!("Expected Allowed"),
        }
    }
    
    #[test]
    fn test_check_board_org_access() {
        let req = HttpRequest::builder()
            .uri("/adapters")
            .body(Body::empty())
            .unwrap();
        
        match check_board_org_access(&req) {
            AuthCheck::Allowed => assert!(true),
            AuthCheck::Denied(_) => panic!("Expected Allowed"),
        }
    }
}
