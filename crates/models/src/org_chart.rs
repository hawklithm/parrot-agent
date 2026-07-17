use serde::{Deserialize, Serialize};

/// Organization node (recursive tree structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgNode {
    pub id: String,
    pub name: String,
    pub role: String,
    pub status: String, // "active" | "paused" | "terminated"
    #[serde(default)]
    pub reports: Vec<OrgNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collapsed_reports: Option<Vec<OrgNode>>,
}

/// Layout node with position information
#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub node: OrgNode,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub children: Vec<LayoutNode>,
}

/// SVG generation options
#[derive(Debug, Clone)]
pub struct OrgChartOptions {
    pub style: OrgChartStyle,
    pub company_name: Option<String>,
    pub stats: Option<String>,
}

/// Chart visual style
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

impl Default for OrgChartOptions {
    fn default() -> Self {
        Self {
            style: OrgChartStyle::Warmth,
            company_name: None,
            stats: None,
        }
    }
}
