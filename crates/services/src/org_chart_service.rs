use async_trait::async_trait;
use models::{OrgChartOptions, OrgChartStyle, OrgNode};
use uuid::Uuid;

/// 组织架构服务错误
#[derive(Debug, thiserror::Error)]
pub enum OrgChartError {
    #[error("database error: {0}")]
    Database(String),
    #[error("agent not found: {0}")]
    AgentNotFound(Uuid),
    #[error("circular dependency detected at agent: {0}")]
    CircularDependency(Uuid),
}

/// 角色标签映射表
pub const ROLE_LABELS: &[(&str, &str)] = &[
    ("ceo", "CEO"),
    ("vp", "VP"),
    ("manager", "Manager"),
    ("researcher", "Researcher"),
    ("general", "General Agent"),
    ("engineer", "Engineer"),
    ("director", "Director"),
    ("product", "Product"),
    ("pm", "Product Manager"),
    ("admin", "Admin"),
];

/// 将内部角色标识转换为可读标签
pub fn get_role_label(role: &str) -> String {
    let normalized = role.to_lowercase();
    for (key, label) in ROLE_LABELS {
        if normalized.contains(key) {
            return (*label).to_string();
        }
    }
    // 未匹配则做简单的标题化
    let mut chars = normalized.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &chars.collect::<String>(),
        None => normalized,
    }
}

#[async_trait]
pub trait OrgChartService: Send + Sync {
    /// GET /companies/:companyId/org - 获取组织树结构
    async fn get_org_tree(&self, company_id: Uuid) -> Result<Vec<OrgNode>, String>;

    /// GET /companies/:companyId/org-chart.svg - 生成SVG组织架构图
    async fn generate_org_chart_svg(
        &self,
        company_id: Uuid,
        options: OrgChartOptions,
    ) -> Result<String, String>;

    /// 从数据库构建组织树（含循环依赖检测）
    async fn build_org_tree(&self, company_id: Uuid) -> Result<Vec<OrgNode>, OrgChartError>;

    /// 获取 Agent 的直接下属
    async fn get_direct_reports(&self, agent_id: Uuid) -> Result<Vec<OrgNode>, OrgChartError>;

    /// 获取以指定 Agent 为根的子树
    async fn get_subtree(&self, agent_id: Uuid) -> Result<OrgNode, OrgChartError>;
}

pub struct MockOrgChartService;

impl MockOrgChartService {
    /// 构建mock组织树（检测循环引用）
    fn build_org_tree_mock() -> Vec<OrgNode> {
        // CEO -> 2 directors -> agents
        vec![OrgNode {
            id: "agent-ceo".to_string(),
            name: "Alice".to_string(),
            role: "CEO".to_string(),
            status: "active".to_string(),
            reports: vec![
                OrgNode {
                    id: "agent-eng-dir".to_string(),
                    name: "Bob".to_string(),
                    role: "Engineering Director".to_string(),
                    status: "active".to_string(),
                    reports: vec![
                        OrgNode {
                            id: "agent-backend-1".to_string(),
                            name: "Charlie".to_string(),
                            role: "Backend Engineer".to_string(),
                            status: "active".to_string(),
                            reports: vec![],
                            collapsed_reports: None,
                        },
                        OrgNode {
                            id: "agent-frontend-1".to_string(),
                            name: "Diana".to_string(),
                            role: "Frontend Engineer".to_string(),
                            status: "active".to_string(),
                            reports: vec![],
                  collapsed_reports: None,
                        },
                    ],
                    collapsed_reports: None,
                },
                OrgNode {
                    id: "agent-product-dir".to_string(),
                    name: "Eve".to_string(),
                    role: "Product Director".to_string(),
                    status: "active".to_string(),
                    reports: vec![OrgNode {
                        id: "agent-pm-1".to_string(),
                        name: "Frank".to_string(),
                        role: "Product Manager".to_string(),
                        status: "active".to_string(),
                        reports: vec![],
                        collapsed_reports: None,
                    }],
                    collapsed_reports: None,
                },
            ],
            collapsed_reports: None,
        }]
    }

    /// 简化SVG生成（手动拼接SVG XML）
    fn render_svg(tree: &[OrgNode], options: &OrgChartOptions) -> String {
        let (bg_color, text_color, card_bg, card_border) = match options.style {
            OrgChartStyle::Warmth => ("#fef3c7", "#78350f", "#fffbeb", "#fbbf24"),
            OrgChartStyle::Professional => ("#f0f9ff", "#0c4a6e", "#ffffff", "#0ea5e9"),
            OrgChartStyle::Dark => ("#1e293b", "#e2e8f0", "#334155", "#475569"),
            OrgChartStyle::Minimal => ("#ffffff", "#1f2937", "#f9fafb", "#d1d5db"),
        };

        let card_width = 140;
        let card_height = 90;
        let gap_x = 40;
        let gap_y = 60;

        // 简单布局：扁平化树为层级列表
        let mut svg_content = String::new();
        let mut y_offset = 40;

        // 遍历根节点
        for (_idx, root) in tree.iter().enumerate() {
            let root_x = 640 - card_width / 2; // 居中
            svg_content.push_str(&Self::render_card(root, root_x as f32, y_offset as f32, card_width, card_height, card_bg, card_border, text_color));
            
            let root_cx = root_x + card_width / 2;
            let root_bottom = y_offset + card_height;
            
            // 渲染子节点（第二层）
            let children_count = root.reports.len();
            if children_count > 0 {
                let total_width = (children_count as i32) * card_width + ((children_count as i32) - 1) * gap_x;
                let start_x = 640 - total_width / 2;
                let child_y = root_bottom + gap_y;
                
                // 连接线
                let mid_y = root_bottom + gap_y / 2;
                svg_content.push_str(&format!(
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2"/>"#,
                    root_cx, root_bottom, root_cx, mid_y, card_border
                ));
                
                for (child_idx, child) in root.reports.iter().enumerate() {
                    let child_x = start_x + (child_idx as i32) * (card_width + gap_x);
                    let child_cx = child_x + card_width / 2;
                    
                    // 横向连接线
                    if child_idx == 0 {
                        svg_content.push_str(&format!(
                            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2"/>"#,
                            child_cx, mid_y, root_cx, mid_y, card_border
                        ));
                    }
                    
                    // 垂直连接线
                    svg_content.push_str(&format!(
                        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2"/>"#,
                        child_cx, mid_y, child_cx, child_y, card_border
                    ));
                    
                    svg_content.push_str(&Self::render_card(child, child_x as f32, child_y as f32, card_width, card_height, card_bg, card_border, text_color));
                    
                    // 渲染孙子节点（第三层）
                    if !child.reports.is_empty() {
                        let grandchild_y = child_y + card_height + gap_y;
                        let grandchild_mid_y = child_y + card_height + gap_y / 2;
                        svg_content.push_str(&format!(
                            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2"/>"#,
                            child_cx, child_y + card_height, child_cx, grandchild_mid_y, card_border
                        ));
                        
                        for (gc_idx, grandchild) in child.reports.iter().enumerate() {
                            let gc_x = child_x + (gc_idx as i32) * 80 - 40;
                            let gc_cx = gc_x + card_width / 2;
                            svg_content.push_str(&format!(
                                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2"/>"#,
                                gc_cx, grandchild_mid_y, gc_cx, grandchild_y, card_border
                            ));
                            svg_content.push_str(&Self::render_card(grandchild, gc_x as f32, grandchild_y as f32, 120, 80, card_bg, card_border, text_color));
                        }
                    }
                }
            }
            
            y_offset += card_height + gap_y * 3;
        }

        let company_name_svg = if let Some(name) = &options.company_name {
            format!(r#"<text x="20" y="30" font-family="sans-serif" font-size="22" font-weight="700" fill="{}">{}</text>"#, text_color, Self::escape_xml(name))
        } else {
            String::new()
        };

        let stats_svg = if let Some(stats) = &options.stats {
            format!(r#"<text x="1260" y="620" text-anchor="end" font-family="sans-serif" font-size="13" fill="{}">{}</text>"#, text_color, Self::escape_xml(stats))
        } else {
            String::new()
        };

        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="640" viewBox="0 0 1280 640">
  <rect width="100%" height="100%" fill="{}"/>
  {}
  {}
  {}
</svg>"#,
            bg_color, company_name_svg, stats_svg, svg_content
        )
    }

    fn render_card(node: &OrgNode, x: f32, y: f32, w: i32, h: i32, bg: &str, border: &str, text: &str) -> String {
        let name = Self::escape_xml(&node.name);
        let role = Self::escape_xml(&node.role);
        let cx = x + w as f32 / 2.0;
        let name_y = y + h as f32 / 2.0 - 10.0;
        let role_y = y + h as f32 / 2.0 + 10.0;

        format!(
            r#"<g>
  <rect x="{}" y="{}" width="{}" height="{}" rx="8" fill="{}" stroke="{}" stroke-width="1"/>
  <text x="{}" y="{}" text-anchor="middle" font-family="sans-serif" font-size="14" font-weight="600" fill="{}">{}</text>
  <text x="{}" y="{}" text-anchor="middle" font-family="sans-serif" font-size="11" fill="{}">{}</text>
</g>"#,
            x, y, w, h, bg, border, cx, name_y, text, name, cx, role_y, text, role
        )
    }

    fn escape_xml(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
}

#[async_trait]
impl OrgChartService for MockOrgChartService {
    async fn get_org_tree(&self, _company_id: Uuid) -> Result<Vec<OrgNode>, String> {
        Ok(Self::build_org_tree_mock())
    }

    async fn generate_org_chart_svg(
        &self,
        company_id: Uuid,
        options: OrgChartOptions,
    ) -> Result<String, String> {
        let tree = self.get_org_tree(company_id).await?;
        Ok(Self::render_svg(&tree, &options))
    }

    async fn build_org_tree(&self, _company_id: Uuid) -> Result<Vec<OrgNode>, OrgChartError> {
        Ok(Self::build_org_tree_mock())
    }

    async fn get_direct_reports(&self, agent_id: Uuid) -> Result<Vec<OrgNode>, OrgChartError> {
        let tree = Self::build_org_tree_mock();
        if let Some(node) = Self::find_node(&tree, agent_id.to_string()) {
            Ok(node.reports.clone())
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_subtree(&self, agent_id: Uuid) -> Result<OrgNode, OrgChartError> {
        let tree = Self::build_org_tree_mock();
        Self::find_node(&tree, agent_id.to_string())
            .cloned()
            .ok_or(OrgChartError::AgentNotFound(agent_id))
    }
}

impl MockOrgChartService {
    /// 在组织中递归查找指定 id 的节点
    fn find_node<'a>(nodes: &'a [OrgNode], id: String) -> Option<&'a OrgNode> {
        for node in nodes {
            if node.id == id {
                return Some(node);
            }
            if let Some(found) = Self::find_node(&node.reports, id.clone()) {
                return Some(found);
            }
        }
        None
    }
}
