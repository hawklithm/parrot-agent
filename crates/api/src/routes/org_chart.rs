use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use models::{OrgChartOptions, OrgChartStyle};
use services::OrgChartService;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrgChartQuery {
    #[serde(default)]
    style: Option<String>,
}

/// GET /companies/:companyId/org - 获取组织树JSON
pub async fn get_org_tree(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn OrgChartService>>,
) -> Response {
    match service.get_org_tree(company_id).await {
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
    State(service): State<Arc<dyn OrgChartService>>,
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

    match service.generate_org_chart_svg(company_id, options).await {
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
pub fn org_chart_routes(service: Arc<dyn OrgChartService>) -> axum::Router {
    axum::Router::new()
        .route(
            "/companies/:companyId/org",
            axum::routing::get(get_org_tree),
        )
        .route(
            "/companies/:companyId/org-chart.svg",
            axum::routing::get(generate_org_chart_svg),
        )
        .with_state(service)
}
