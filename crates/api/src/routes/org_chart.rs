use crate::app_state::AppState;
use axum::{Router, 
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use models::{OrgChartOptions, OrgChartStyle};
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

    let options = OrgChartOptions {
        style,
        company_name: Some("Parrot Agent".to_string()),
        stats: Some("Agents: 6".to_string()),
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
}
