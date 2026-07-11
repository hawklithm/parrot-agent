use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use services::{DefaultOrgChartService, OrgChartService};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::AppError;

/// 组织架构路由状态
#[derive(Clone)]
pub struct OrgRouteState {
    pub org_chart_service: Arc<dyn OrgChartService>,
}

impl OrgRouteState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            org_chart_service: Arc::new(DefaultOrgChartService::new(pool)),
        }
    }
}

/// 创建组织架构路由
pub fn org_routes(pool: PgPool) -> Router {
    let state = OrgRouteState::new(pool);

    Router::new()
        .route("/companies/:company_id/org", get(get_org_tree))
        .route("/companies/:company_id/org.svg", get(get_org_svg))
        .route("/companies/:company_id/org.png", get(get_org_png))
        .with_state(state)
}

/// GET /companies/:company_id/org - 获取组织架构树（JSON）
async fn get_org_tree(
    Path(company_id): Path<Uuid>,
    State(state): State<OrgRouteState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tree = state
        .org_chart_service
        .build_org_tree(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    Ok(Json(serde_json::json!({ "data": tree })))
}

/// GET /companies/:company_id/org.svg - 生成 SVG 组织架构图
async fn get_org_svg(
    Path(company_id): Path<Uuid>,
    State(state): State<OrgRouteState>,
) -> Result<Response, AppError> {
    let tree = state
        .org_chart_service
        .build_org_tree(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    // TODO: 实现 SVG 渲染逻辑
    // 当前返回占位符响应
    let svg_placeholder = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="600">
            <text x="400" y="300" text-anchor="middle" font-size="20" fill="#666">
                Org Chart SVG (Company: {})
            </text>
            <text x="400" y="330" text-anchor="middle" font-size="14" fill="#999">
                {} agents in tree
            </text>
        </svg>"##,
        company_id,
        count_nodes(&tree)
    );

    Ok((
        [("content-type", "image/svg+xml")],
        svg_placeholder,
    ).into_response())
}

/// GET /companies/:company_id/org.png - 生成 PNG 组织架构图
async fn get_org_png(
    Path(company_id): Path<Uuid>,
    State(state): State<OrgRouteState>,
) -> Result<Response, AppError> {
    let _tree = state
        .org_chart_service
        .build_org_tree(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    // TODO: 实现 PNG 渲染逻辑（需要 resvg 或 image 库）
    // 当前返回 501 Not Implemented
    Err(AppError::NotImplemented(
        "PNG rendering not yet implemented".to_string(),
    ))
}

/// 辅助函数：递归统计组织架构树中的节点总数
fn count_nodes(nodes: &[services::OrgNode]) -> usize {
    nodes.iter().map(|node| 1 + count_nodes(&node.reports)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_nodes() {
        use services::OrgNode;

        let nodes = vec![
            OrgNode {
                id: Uuid::new_v4(),
                name: "CEO".into(),
                role: "Chief Executive".into(),
                status: "active".into(),
                reports: vec![
                    OrgNode {
                        id: Uuid::new_v4(),
                        name: "CTO".into(),
                        role: "Technology".into(),
                        status: "active".into(),
                        reports: vec![],
                    },
                    OrgNode {
                        id: Uuid::new_v4(),
                        name: "CFO".into(),
                        role: "Finance".into(),
                        status: "active".into(),
                        reports: vec![],
                    },
                ],
            },
        ];

        assert_eq!(count_nodes(&nodes), 3);
    }
}
