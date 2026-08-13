use crate::org_chart_service::{get_role_label, OrgChartError, OrgChartService};
use models::{OrgChartOptions, OrgNode};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// 默认组织架构服务实现
pub struct DefaultOrgChartService {
    pool: PgPool,
}

impl DefaultOrgChartService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const CARD_W: f32 = 180.0;
const CARD_H: f32 = 72.0;
const GAP_X: f32 = 28.0;
const GAP_Y: f32 = 64.0;
const PADDING: f32 = 40.0;

fn svg_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn subtree_width(node: &OrgNode) -> f32 {
    if node.reports.is_empty() {
        return CARD_W;
    }
    let children_width = node.reports.iter().map(subtree_width).sum::<f32>()
        + GAP_X * (node.reports.len().saturating_sub(1) as f32);
    CARD_W.max(children_width)
}

fn render_org_tree_svg(tree: &[OrgNode], options: &OrgChartOptions) -> String {
    let (background, card_background, border, text) = match options.style {
        models::OrgChartStyle::Dark => ("#0f172a", "#1e293b", "#475569", "#e2e8f0"),
        models::OrgChartStyle::Minimal => ("#ffffff", "#ffffff", "#d1d5db", "#1f2937"),
        models::OrgChartStyle::Professional => ("#f0f9ff", "#ffffff", "#0ea5e9", "#0c4a6e"),
        models::OrgChartStyle::Warmth => ("#fffbeb", "#ffffff", "#f59e0b", "#78350f"),
    };
    let forest_width = tree.iter().map(subtree_width).sum::<f32>()
        + GAP_X * (tree.len().saturating_sub(1) as f32);
    let max_depth = fn_depth(tree);
    let width = (forest_width + PADDING * 2.0).max(640.0);
    let height = PADDING * 2.0 + (max_depth as f32 * (CARD_H + GAP_Y));
    let mut body = String::new();
    let mut x = PADDING;
    for root in tree {
        render_node_svg(root, x, PADDING, &mut body, border, card_background, text);
        x += subtree_width(root) + GAP_X;
    }
    let title = options
        .company_name
        .as_deref()
        .map(svg_escape)
        .unwrap_or_default();
    let stats = options.stats.as_deref().map(svg_escape).unwrap_or_default();
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.0} {height:.0}"><rect width="100%" height="100%" fill="{background}"/><text x="{PADDING}" y="24" fill="{text}" font-family="system-ui,sans-serif" font-size="16" font-weight="600">{title}</text><text x="{PADDING}" y="42" fill="{text}" opacity=".7" font-family="system-ui,sans-serif" font-size="11">{stats}</text>{body}</svg>"#
    )
}

fn fn_depth(nodes: &[OrgNode]) -> usize {
    nodes.iter().map(|node| 1 + fn_depth(&node.reports)).max().unwrap_or(1)
}

fn render_node_svg(
    node: &OrgNode,
    x: f32,
    y: f32,
    output: &mut String,
    border: &str,
    card_background: &str,
    text: &str,
) {
    let width = subtree_width(node);
    let card_x = x + (width - CARD_W) / 2.0;
    let cx = card_x + CARD_W / 2.0;
    let bottom = y + CARD_H;
    output.push_str(&format!(
        r#"<rect x="{card_x:.1}" y="{y:.1}" width="{CARD_W}" height="{CARD_H}" rx="8" fill="{card_background}" stroke="{border}" stroke-width="1.5"/><text x="{cx:.1}" y="{:.1}" text-anchor="middle" fill="{text}" font-family="system-ui,sans-serif" font-size="14" font-weight="600">{}</text><text x="{cx:.1}" y="{:.1}" text-anchor="middle" fill="{text}" opacity=".72" font-family="system-ui,sans-serif" font-size="11">{}</text>"#,
        y + 31.0,
        svg_escape(&node.name),
        y + 51.0,
        svg_escape(&node.role)
    ));
    if node.reports.is_empty() {
        return;
    }
    let children_width = node.reports.iter().map(subtree_width).sum::<f32>()
        + GAP_X * (node.reports.len().saturating_sub(1) as f32);
    let mut child_x = x + (width - children_width) / 2.0;
    let mid_y = bottom + GAP_Y / 2.0;
    output.push_str(&format!(
        r#"<path d="M {cx:.1} {bottom:.1} V {mid_y:.1}" fill="none" stroke="{border}"/>"#
    ));
    for child in &node.reports {
        let child_width = subtree_width(child);
        let child_cx = child_x + child_width / 2.0;
        output.push_str(&format!(
            r#"<path d="M {cx:.1} {mid_y:.1} H {child_cx:.1} V {next_y:.1}" fill="none" stroke="{border}"/>"#,
            next_y = bottom + GAP_Y
        ));
        render_node_svg(child, child_x, bottom + GAP_Y, output, border, card_background, text);
        child_x += child_width + GAP_X;
    }
}

/// 从数据库查询的 Agent 记录
#[derive(Debug, sqlx::FromRow)]
struct AgentRecord {
    id: Uuid,
    name: String,
    role: String,
    status: String,
    reports_to_agent_id: Option<Uuid>,
}

#[async_trait::async_trait]
impl OrgChartService for DefaultOrgChartService {
    async fn build_org_tree(&self, company_id: Uuid) -> Result<Vec<OrgNode>, OrgChartError> {
        // Paperclip's org endpoint is a view of the live organization: terminated
        // agents are omitted, and a manager reference which does not resolve to a
        // live agent makes the agent a root node.  Keep that normalization here so
        // the JSON endpoint and generated exports have identical semantics.
        let agents = sqlx::query_as::<_, AgentRecord>(
            r#"
            SELECT id, name, role, status, reports_to AS reports_to_agent_id
            FROM agents
            WHERE company_id = $1
              AND status <> 'terminated'
            ORDER BY name
            "#,
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OrgChartError::Database(e.to_string()))?;

        if agents.is_empty() {
            return Ok(vec![]);
        }

        let live_ids: std::collections::HashSet<Uuid> =
            agents.iter().map(|agent| agent.id).collect();

        // Normalize stale/cross-company manager references to the root.  The
        // foreign key normally prevents these, but imported legacy data can
        // still contain them and Paperclip deliberately keeps those nodes visible.
        let agents: Vec<AgentRecord> = agents
            .into_iter()
            .map(|mut agent| {
                if !agent
                    .reports_to_agent_id
                    .is_some_and(|manager_id| live_ids.contains(&manager_id))
                {
                    agent.reports_to_agent_id = None;
                }
                agent
            })
            .collect();

        // Detect cycles after normalization so an invalid stale reference cannot
        // make the complete organization endpoint fail.
        detect_circular_dependencies(&agents)?;

        // 构建 parent -> children 映射
        let mut children_map: HashMap<Option<Uuid>, Vec<AgentRecord>> = HashMap::new();
        for agent in agents {
            children_map
                .entry(agent.reports_to_agent_id)
                .or_default()
                .push(agent);
        }

        // 递归构建树
        fn build_subtree(
            parent_id: Option<Uuid>,
            children_map: &HashMap<Option<Uuid>, Vec<AgentRecord>>,
        ) -> Vec<OrgNode> {
            let Some(children) = children_map.get(&parent_id) else {
                return vec![];
            };

            children
                .iter()
                .map(|agent| OrgNode {
                    id: agent.id.to_string(),
                    name: agent.name.clone(),
                    role: get_role_label(&agent.role),
                    status: agent.status.clone(),
                    reports: build_subtree(Some(agent.id), children_map),
                    collapsed_reports: None,
                })
                .collect()
        }

        // 从根节点（reports_to_agent_id = NULL）开始构建
        Ok(build_subtree(None, &children_map))
    }

    async fn get_org_tree(&self, company_id: Uuid) -> Result<Vec<OrgNode>, String> {
        self.build_org_tree(company_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn generate_org_chart_svg(
        &self,
        company_id: Uuid,
        options: OrgChartOptions,
    ) -> Result<String, String> {
        let tree = self.build_org_tree(company_id).await.map_err(|e| e.to_string())?;
        Ok(render_org_tree_svg(&tree, &options))
    }

    async fn get_direct_reports(&self, agent_id: Uuid) -> Result<Vec<OrgNode>, OrgChartError> {
        let agents = sqlx::query_as::<_, AgentRecord>(
            r#"
            SELECT id, name, role, status, reports_to AS reports_to_agent_id
            FROM agents
            WHERE reports_to = $1
            ORDER BY name
            "#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OrgChartError::Database(e.to_string()))?;

        Ok(agents
            .into_iter()
            .map(|agent| OrgNode {
                id: agent.id.to_string(),
                name: agent.name,
                role: get_role_label(&agent.role),
                status: agent.status,
                reports: vec![],
                collapsed_reports: None,
            })
            .collect())
    }

    async fn get_subtree(&self, agent_id: Uuid) -> Result<OrgNode, OrgChartError> {
        // 查询根节点
        let root = sqlx::query_as::<_, AgentRecord>(
            r#"
            SELECT id, name, role, status, reports_to AS reports_to_agent_id
            FROM agents
            WHERE id = $1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OrgChartError::Database(e.to_string()))?
        .ok_or(OrgChartError::AgentNotFound(agent_id))?;

        // 查询所有可能的下属（用于递归构建）
        let all_agents = sqlx::query_as::<_, AgentRecord>(
            r#"
            SELECT id, name, role, status, reports_to AS reports_to_agent_id
            FROM agents
            WHERE company_id = (SELECT company_id FROM agents WHERE id = $1)
            "#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OrgChartError::Database(e.to_string()))?;

        // 构建 children_map
        let mut children_map: HashMap<Uuid, Vec<AgentRecord>> = HashMap::new();
        for agent in all_agents {
            if let Some(parent_id) = agent.reports_to_agent_id {
                children_map.entry(parent_id).or_default().push(agent);
            }
        }

        // 递归构建子树
        fn build_reports(
            parent_id: Uuid,
            children_map: &HashMap<Uuid, Vec<AgentRecord>>,
        ) -> Vec<OrgNode> {
            let Some(children) = children_map.get(&parent_id) else {
                return vec![];
            };

            children
                .iter()
                .map(|agent| OrgNode {
                    id: agent.id.to_string(),
                    name: agent.name.clone(),
                    role: get_role_label(&agent.role),
                    status: agent.status.clone(),
                    reports: build_reports(agent.id, children_map),
                    collapsed_reports: None,
                })
                .collect()
        }

        Ok(OrgNode {
            id: root.id.to_string(),
            name: root.name,
            role: get_role_label(&root.role),
            status: root.status,
            reports: build_reports(root.id, &children_map),
            collapsed_reports: None,
        })
    }
}

/// 检测循环依赖（使用 DFS + visited 标记）
fn detect_circular_dependencies(agents: &[AgentRecord]) -> Result<(), OrgChartError> {
    let mut parent_map: HashMap<Uuid, Option<Uuid>> = HashMap::new();
    for agent in agents {
        parent_map.insert(agent.id, agent.reports_to_agent_id);
    }

    for agent in agents {
        let mut visited = std::collections::HashSet::new();
        let mut current = agent.id;

        loop {
            if visited.contains(&current) {
                return Err(OrgChartError::CircularDependency(current));
            }
            visited.insert(current);

            let Some(&parent) = parent_map.get(&current) else {
                break;
            };
            let Some(parent_id) = parent else {
                break;
            };
            current = parent_id;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circular_dependency_detection() {
        // 正常情况：A -> B -> C
        let agents = vec![
            AgentRecord {
                id: Uuid::from_u128(1),
                name: "A".into(),
                role: "ceo".into(),
                status: "active".into(),
                reports_to_agent_id: None,
            },
            AgentRecord {
                id: Uuid::from_u128(2),
                name: "B".into(),
                role: "manager".into(),
                status: "active".into(),
                reports_to_agent_id: Some(Uuid::from_u128(1)),
            },
            AgentRecord {
                id: Uuid::from_u128(3),
                name: "C".into(),
                role: "engineer".into(),
                status: "active".into(),
                reports_to_agent_id: Some(Uuid::from_u128(2)),
            },
        ];
        assert!(detect_circular_dependencies(&agents).is_ok());

        // 循环：A -> B -> A
        let circular_agents = vec![
            AgentRecord {
                id: Uuid::from_u128(1),
                name: "A".into(),
                role: "ceo".into(),
                status: "active".into(),
                reports_to_agent_id: Some(Uuid::from_u128(2)),
            },
            AgentRecord {
                id: Uuid::from_u128(2),
                name: "B".into(),
                role: "manager".into(),
                status: "active".into(),
                reports_to_agent_id: Some(Uuid::from_u128(1)),
            },
        ];
        let result = detect_circular_dependencies(&circular_agents);
        assert!(result.is_err());
        match result {
            Err(OrgChartError::CircularDependency(_)) => (),
            _ => panic!("Expected CircularDependency error"),
        }
    }
}
