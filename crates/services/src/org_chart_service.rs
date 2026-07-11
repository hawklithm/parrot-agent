use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 组织架构节点，表示 Agent 在组织架构中的位置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrgNode {
    /// Agent ID (对应 agents.id)
    pub id: Uuid,
    /// Agent 名称
    pub name: String,
    /// 角色标签 (如 "CEO", "Engineer" 等)
    pub role: String,
    /// Agent 状态 (如 "active", "paused")
    pub status: String,
    /// 直接下属列表
    pub reports: Vec<OrgNode>,
}

/// 组织架构服务 trait，用于构建和查询组织架构树
#[async_trait::async_trait]
pub trait OrgChartService: Send + Sync {
    /// 根据公司 ID 构建完整的组织架构树
    ///
    /// 该方法从数据库读取 company_id 下所有 agents，
    /// 根据 reports_to_agent_id 字段构建树形结构。
    ///
    /// 返回值：根节点列表（可能有多个顶级节点）
    async fn build_org_tree(&self, company_id: Uuid) -> Result<Vec<OrgNode>, OrgChartError>;

    /// 根据 Agent ID 查找其直接下属
    async fn get_direct_reports(&self, agent_id: Uuid) -> Result<Vec<OrgNode>, OrgChartError>;

    /// 根据 Agent ID 查找其完整下属树
    async fn get_subtree(&self, agent_id: Uuid) -> Result<OrgNode, OrgChartError>;
}

#[derive(Debug, thiserror::Error)]
pub enum OrgChartError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),
    #[error("Circular reporting structure detected involving agent {0}")]
    CircularDependency(Uuid),
}

/// 角色标签映射表（从数据库的 role 值到显示标签）
pub const ROLE_LABELS: &[(&str, &str)] = &[
    ("ceo", "Chief Executive"),
    ("cto", "Technology"),
    ("cmo", "Marketing"),
    ("cfo", "Finance"),
    ("coo", "Operations"),
    ("vp", "VP"),
    ("manager", "Manager"),
    ("engineer", "Engineer"),
    ("agent", "Agent"),
];

/// 获取角色显示标签，如果没有对应映射则返回原始值
pub fn get_role_label(role: &str) -> String {
    ROLE_LABELS
        .iter()
        .find(|(key, _)| *key == role)
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| role.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_labels() {
        assert_eq!(get_role_label("ceo"), "Chief Executive");
        assert_eq!(get_role_label("engineer"), "Engineer");
        assert_eq!(get_role_label("unknown"), "unknown");
    }

    #[test]
    fn test_org_node_serialization() {
        let node = OrgNode {
            id: Uuid::new_v4(),
            name: "Alice".to_string(),
            role: "CEO".to_string(),
            status: "active".to_string(),
            reports: vec![],
        };
        let json = serde_json::to_string(&node).unwrap();
        let deserialized: OrgNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node, deserialized);
    }
}
