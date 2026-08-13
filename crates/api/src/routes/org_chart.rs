use crate::app_state::AppState;
use crate::errors::AppError;
use axum::{Router,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use models::{OrgChartOptions, OrgChartStyle};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgChartQuery {
    #[serde(default)]
    style: Option<String>,
}

/// GET /companies/:companyId/org - 获取组织树JSON
pub async fn get_org_tree(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    match state.org_chart_service.get_org_tree(company_id).await {
        Ok(tree) => (StatusCode::OK, Json(tree)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// GET /companies/:companyId/org-chart.svg - 生成SVG组织架构图
pub async fn generate_org_chart_svg(
    Path(company_id): Path<Uuid>,
    Query(query): Query<OrgChartQuery>,
    State(state): State<AppState>,
) -> Response {
    let style = match query.style.as_deref() {
        Some("professional") => OrgChartStyle::Professional,
        Some("dark") => OrgChartStyle::Dark,
        Some("minimal") => OrgChartStyle::Minimal,
        _ => OrgChartStyle::Warmth,
    };

    let (company_name, agent_count) = sqlx::query_as::<_, (String, i64)>(
        "SELECT c.name, (SELECT COUNT(*) FROM agents a WHERE a.company_id=c.id) FROM companies c WHERE c.id=$1")
        .bind(company_id).fetch_optional(&state.pool).await.ok().flatten()
        .unwrap_or_else(|| ("Company".to_string(), 0));
    let options = OrgChartOptions {
        style,
        company_name: Some(company_name),
        stats: Some(format!("Agents: {agent_count}")),
    };

    match state.org_chart_service.generate_org_chart_svg(company_id, options).await {
        Ok(svg) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "image/svg+xml")],
            svg,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// 创建组织架构图路由器
pub fn org_chart_routes() -> Router<AppState> {
    axum::Router::new()
        .route(
            "/companies/:companyId/org",
            axum::routing::get(get_org_tree),
        )
        .route(
            "/companies/:companyId/org-chart.svg",
            axum::routing::get(generate_org_chart_svg),
        )
        // 对齐 Paperclip `GET /companies/:companyId/org.svg`（同 handler 别名）
        .route(
            "/companies/:companyId/org.svg",
            axum::routing::get(generate_org_chart_svg),
        )
        .route(
            "/companies/:companyId/org.png",
            axum::routing::get(generate_org_png),
        )
}

/// GET /companies/:companyId/org.png - 生成组织架构图。
///
/// The renderer is SVG-based in both the web UI and Paperclip. Keep this
/// endpoint useful for clients that historically requested `.png` by returning
/// the same standards-compliant SVG bytes instead of a fake/501 response.
async fn generate_org_png(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let (company_name, agent_count) = sqlx::query_as::<_, (String, i64)>(
        "SELECT c.name, (SELECT COUNT(*) FROM agents a WHERE a.company_id=c.id) FROM companies c WHERE c.id=$1")
        .bind(company_id).fetch_optional(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Company not found".to_string()))?;
    let svg = state.org_chart_service.generate_org_chart_svg(company_id, OrgChartOptions {
        style: OrgChartStyle::Warmth,
        company_name: Some(company_name),
        stats: Some(format!("Agents: {agent_count}")),
    }).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(&svg, &options)
        .map_err(|e| AppError::InternalServerError(format!("Invalid org chart SVG: {e}")))?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| AppError::InternalServerError("Unable to allocate org chart image".to_string()))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
    let png = pixmap
        .encode_png()
        .map_err(|e| AppError::InternalServerError(format!("Unable to encode org chart PNG: {e}")))?;
    Ok(([(header::CONTENT_TYPE, "image/png"), (header::CONTENT_DISPOSITION, "inline")], png).into_response())
}

#[cfg(test)]
/// 辅助函数：递归统计组织架构树中的节点总数
fn count_nodes(nodes: &[services::OrgNode]) -> usize {
    nodes.iter().map(|node| 1 + count_nodes(&node.reports)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_count_nodes() {
        use services::OrgNode;

        let id1 = Uuid::new_v4().to_string();
        let id2 = Uuid::new_v4().to_string();
        let id3 = Uuid::new_v4().to_string();

        let nodes = vec![
            OrgNode {
                id: id1,
                name: "CEO".into(),
                role: "Chief Executive".into(),
                status: "active".into(),
                collapsed_reports: None,
                reports: vec![
                    OrgNode {
                        id: id2,
                        name: "CTO".into(),
                        role: "Technology".into(),
                        status: "active".into(),
                        collapsed_reports: None,
                        reports: vec![],
                    },
                    OrgNode {
                        id: id3,
                        name: "CFO".into(),
                        role: "Finance".into(),
                        status: "active".into(),
                        collapsed_reports: None,
                        reports: vec![],
                    },
                ],
            },
        ];

        assert_eq!(count_nodes(&nodes), 3);
    }
}
