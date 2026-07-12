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
        // 查询公司下所有 agents
        let agents = sqlx::query_as::<_, AgentRecord>(
            r#"
            SELECT id, name, role, status, reports_to_agent_id
            FROM agents
            WHERE company_id = $1
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

        // 检测循环依赖
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
        _options: OrgChartOptions,
    ) -> Result<String, String> {
        let tree = self.build_org_tree(company_id).await.map_err(|e| e.to_string())?;
        // 简单的占位 SVG 渲染（与 MockOrgChartService 的 render_svg 保持一致可后续统一）
        let mut svg = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1280\" height=\"640\">");
        svg.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#f0f9ff\"/>");
        for (i, node) in tree.iter().enumerate() {
            let x = 40 + i * 160;
            svg.push_str(&format!(
                "<g><rect x=\"{}\" y=\"40\" width=\"140\" height=\"60\" rx=\"8\" fill=\"#ffffff\" stroke=\"#0ea5e9\"/><text x=\"{}\" y=\"70\" font-size=\"14\">{}</text><text x=\"{}\" y=\"88\" font-size=\"11\">{}</text></g>",
                x, x + 70, node.name, x + 70, node.role
            ));
        }
        svg.push_str("</svg>");
        Ok(svg)
    }

    async fn get_direct_reports(&self, agent_id: Uuid) -> Result<Vec<OrgNode>, OrgChartError> {
        let agents = sqlx::query_as::<_, AgentRecord>(
            r#"
            SELECT id, name, role, status, reports_to_agent_id
            FROM agents
            WHERE reports_to_agent_id = $1
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
            SELECT id, name, role, status, reports_to_agent_id
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
            SELECT id, name, role, status, reports_to_agent_id
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
