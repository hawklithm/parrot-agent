//! MCP transport/domain types shared by the HTTP gateway.
//!
//! The route layer still owns Axum extraction and persistence, but these
//! types make the run-scoped contract explicit instead of passing unrelated
//! UUIDs through individual handlers.

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpInvocationContext {
    pub session_id: Uuid,
    pub company_id: Uuid,
    pub agent_id: Uuid,
    pub run_id: Uuid,
    pub issue_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpRequestKind {
    Request,
    Notification,
}

pub fn request_kind(has_id: bool) -> McpRequestKind {
    if has_id {
        McpRequestKind::Request
    } else {
        McpRequestKind::Notification
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_json_rpc_notifications() {
        assert_eq!(request_kind(true), McpRequestKind::Request);
        assert_eq!(request_kind(false), McpRequestKind::Notification);
    }
}
