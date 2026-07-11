use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 组织架构响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgChartResponse {
    /// 公司 ID
    pub company_id: Uuid,

    /// 根节点（通常是 CEO）
    pub root: OrgNodeResponse,

    /// 统计信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<OrgChartStats>,
}

/// 组织节点响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgNodeResponse {
    /// Agent ID
    pub id: Uuid,

    /// Agent 名称
    pub name: String,

    /// Agent 角色
    pub role: String,

    /// Agent 状态
    pub status: String,

    /// 标题（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// 图标（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// 直接下属
    #[serde(default)]
    pub reports: Vec<OrgNodeResponse>,

    /// 折叠的下属（用于大型组织）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collapsed_reports: Option<Vec<OrgNodeResponse>>,
}

/// 组织架构统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgChartStats {
    /// 总 Agent 数量
    pub total_agents: usize,

    /// 活跃 Agent 数量
    pub active_agents: usize,

    /// 暂停 Agent 数量
    pub paused_agents: usize,

    /// 最大层级深度
    pub max_depth: usize,

    /// 按角色统计
    pub by_role: std::collections::HashMap<String, usize>,
}

/// 组织架构查询参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgChartQueryParams {
    /// 是否包含已暂停的 Agent
    #[serde(default)]
    pub include_paused: bool,

    /// 是否包含已终止的 Agent
    #[serde(default)]
    pub include_terminated: bool,

    /// 最大深度（用于限制树的深度）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,

    /// 起始节点（用于查询子树）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_agent_id: Option<Uuid>,
}

/// Agent 在组织中的位置信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentOrgPosition {
    /// Agent ID
    pub agent_id: Uuid,

    /// 上级 Agent ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reports_to: Option<Uuid>,

    /// 角色
    pub role: String,

    /// 层级（0 = CEO, 1 = VP, 2 = Manager, etc.）
    pub level: usize,

    /// 到根节点的路径
    pub path_to_root: Vec<Uuid>,

    /// 直接下属数量
    pub direct_reports_count: usize,

    /// 所有下属数量（递归）
    pub total_reports_count: usize,
}

/// 更新组织关系请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOrgRelationRequest {
    /// 新的上级 Agent ID（None 表示设为根节点）
    pub reports_to: Option<Uuid>,
}

/// 组织架构样式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrgChartStyle {
    Warmth,
    Professional,
    Dark,
    Minimal,
}

impl Default for OrgChartStyle {
    fn default() -> Self {
        Self::Warmth
    }
}

/// 组织架构渲染选项
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgChartRenderOptions {
    /// 视觉样式
    #[serde(default)]
    pub style: OrgChartStyle,

    /// 公司名称（用于标题）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_name: Option<String>,

    /// 是否显示统计信息
    #[serde(default)]
    pub show_stats: bool,

    /// 输出格式（svg, png, json）
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "json".to_string()
}

impl Default for OrgChartRenderOptions {
    fn default() -> Self {
        Self {
            style: OrgChartStyle::Warmth,
            company_name: None,
            show_stats: false,
            format: default_format(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_org_node_response_serialization() {
        let node = OrgNodeResponse {
            id: Uuid::nil(),
            name: "CEO Agent".to_string(),
            role: "ceo".to_string(),
            status: "active".to_string(),
            title: Some("Chief Executive Officer".to_string()),
            icon: None,
            reports: vec![],
            collapsed_reports: None,
        };

        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("CEO Agent"));
        assert!(json.contains("ceo"));
    }

    #[test]
    fn test_org_chart_query_params_defaults() {
        let params = OrgChartQueryParams {
            include_paused: false,
            include_terminated: false,
            max_depth: None,
            root_agent_id: None,
        };

        assert!(!params.include_paused);
        assert!(!params.include_terminated);
    }

    #[test]
    fn test_org_chart_style() {
        let style = OrgChartStyle::Warmth;
        let json = serde_json::to_string(&style).unwrap();
        assert_eq!(json, r#""warmth""#);

        let parsed: OrgChartStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, OrgChartStyle::Warmth);
    }
}
