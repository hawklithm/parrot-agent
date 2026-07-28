use async_trait::async_trait;
use models::{
    CompanyLogo, InviteOnboardingManifest, InviteSkillDetails, InviteSkillIndex,
};
use sqlx::{PgPool, Row};
use std::path::PathBuf;

#[async_trait]
pub trait InviteResourceService: Send + Sync {
    /// GET /api/invites/:token/logo - 返回公司Logo
    async fn get_company_logo(&self, token: &str) -> Result<CompanyLogo, String>;

    /// GET /api/invites/:token/onboarding - 返回onboarding文档（Markdown）
    async fn get_onboarding(&self, token: &str) -> Result<InviteOnboardingManifest, String>;

    /// GET /api/invites/:token/onboarding.txt - 返回纯文本版本
    async fn get_onboarding_text(&self, token: &str) -> Result<String, String>;

    /// GET /api/invites/:token/skills/index - 邀请范围内的技能索引
    async fn get_skills_index(&self, token: &str) -> Result<InviteSkillIndex, String>;

    /// GET /api/invites/:token/skills/:skillName - 技能详情
    async fn get_skill_details(&self, token: &str, skill_name: &str) -> Result<InviteSkillDetails, String>;
}

pub struct PgInviteResourceService { pool: PgPool, asset_root: PathBuf }
impl PgInviteResourceService {
    pub fn new(pool: PgPool) -> Self { Self { pool, asset_root: std::env::var_os("PARROT_ASSET_STORAGE_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("data/assets")) } }
    async fn company(&self, token: &str) -> Result<(uuid::Uuid, String), String> {
        let row = sqlx::query("SELECT i.company_id, c.name FROM invites i JOIN companies c ON c.id=i.company_id WHERE i.token=$1 AND i.accepted=false AND i.expires_at > NOW()").bind(token).fetch_optional(&self.pool).await.map_err(|e| e.to_string())?.ok_or_else(|| String::from("invite is invalid or expired"))?;
        Ok((row.get("company_id"), row.get("name")))
    }
}

#[async_trait]
impl InviteResourceService for PgInviteResourceService {
    async fn get_company_logo(&self, token: &str) -> Result<CompanyLogo, String> {
        let (company_id, _) = self.company(token).await?;
        let row = sqlx::query("SELECT a.provider, a.object_key, a.content_type FROM companies c JOIN assets a ON a.id=c.logo_asset_id WHERE c.id=$1").bind(company_id).fetch_optional(&self.pool).await.map_err(|e| e.to_string())?.ok_or_else(|| "company logo not found".to_string())?;
        if row.get::<String,_>("provider") != "local" { return Err("unsupported logo asset provider".into()); }
        let data = tokio::fs::read(self.asset_root.join(row.get::<String,_>("object_key"))).await.map_err(|e| e.to_string())?;
        Ok(CompanyLogo { content_type: row.get("content_type"), data })
    }
    async fn get_onboarding(&self, token: &str) -> Result<InviteOnboardingManifest, String> {
        let (company_id, name) = self.company(token).await?;
        let description: Option<String> = sqlx::query_scalar("SELECT description FROM companies WHERE id=$1").bind(company_id).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        let markdown = description.map(|d| format!("# {name}\n\n{d}"));
        let plain = markdown.as_ref().map(|m| m.replace('#', ""));
        Ok(InviteOnboardingManifest { has_onboarding_doc: markdown.is_some(), markdown, plain_text: plain })
    }
    async fn get_onboarding_text(&self, token: &str) -> Result<String, String> { Ok(self.get_onboarding(token).await?.plain_text.unwrap_or_default()) }
    async fn get_skills_index(&self, token: &str) -> Result<InviteSkillIndex, String> {
        let (company_id, _) = self.company(token).await?;
        let rows = sqlx::query("SELECT name, description, is_paperclip_managed FROM company_skills WHERE company_id=$1 AND status='active' ORDER BY name").bind(company_id).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(InviteSkillIndex { skills: rows.into_iter().map(|r| models::InviteScopedSkill { name:r.get("name"), description:r.get("description"), is_paperclip_managed:r.get("is_paperclip_managed") }).collect() })
    }
    async fn get_skill_details(&self, token: &str, skill_name: &str) -> Result<InviteSkillDetails, String> {
        let (company_id, _) = self.company(token).await?;
        let row = sqlx::query("SELECT name, slug, description, category, version, tags, is_paperclip_managed, config, created_at FROM company_skills WHERE company_id=$1 AND status='active' AND (name=$2 OR slug=$2)").bind(company_id).bind(skill_name).fetch_optional(&self.pool).await.map_err(|e| e.to_string())?.ok_or_else(|| "skill not found".to_string())?;
        Ok(InviteSkillDetails { name:row.get("name"), slug:row.get("slug"), description:row.get("description"), parameters:None, examples:None, usage_notes:row.get::<Option<serde_json::Value>,_>("config").and_then(|v| v.get("usageNotes").and_then(|x| x.as_str()).map(str::to_string)), })
    }
}

pub struct MockInviteResourceService;

#[async_trait]
impl InviteResourceService for MockInviteResourceService {
    async fn get_company_logo(&self, _token: &str) -> Result<CompanyLogo, String> {
        // Mock返回一个简单的1x1 PNG图片（透明像素）
        let png_data = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1 dimensions
            0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89,
            0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, // IDAT chunk
            0x78, 0x9C, 0x63, 0x00, 0x01, 0x0, 0x05, 0x00, 0x01,
            0x0D, 0x0A, 0x2D, 0xB4,
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND chunk
            0xAE, 0x42, 0x60, 0x82,
        ];

        Ok(CompanyLogo {
            content_type: "image/png".to_string(),
            data: png_data,
        })
    }

    async fn get_onboarding(&self, _token: &str) -> Result<InviteOnboardingManifest, String> {
        Ok(InviteOnboardingManifest {
            has_onboarding_doc: true,
            markdown: Some(
                r#"# Welcome to Parrot Agent

## Getting Started

This is your onboarding guide for joining the team.

### What you'll need

- Access to the company repository
- Development environment setup
- API credentials

### Next steps

1. Clone the repository
2. Install dependencies
3. Run the test suite
4. Join the team Slack channel

For questions, contact your team lead.
"#
                .to_string(),
            ),
            plain_text: Some(
                "Welcome to Parrot Agent\n\nGetting Started\n\nThis is your onboarding guide for joining the team.\n\nWhat you'll need:\n- Access to the company repository\n- Development environment setup\n- API credentials\n\nNext steps:\n1. Clone the repository\n2. Install dependencies\n3. Run the test suite\n4. Join the team Slack channel\n\nFor questions, contact your team lead.".to_string(),
            ),
        })
    }

    async fn get_onboarding_text(&self, _token: &str) -> Result<String, String> {
        Ok("Welcome to Parrot Agent\n\nGetting Started\n\nThis is your onboarding guide for joining the team.\n\nWhat you'll need:\n- Access to the company repository\n- Development environment setup\n- API credentials\n\nNext steps:\n1. Clone the repository\n2. Install dependencies\n3. Run the test suite\n4. Join the team Slack channel\n\nFor questions, contact your team lead.".to_string())
    }

    async fn get_skills_index(&self, _token: &str) -> Result<InviteSkillIndex, String> {
        use models::InviteScopedSkill;

        Ok(InviteSkillIndex {
            skills: vec![
                InviteScopedSkill {
                    name: "code-review".to_string(),
                    description: "Automated code review with best practices".to_string(),
                    is_paperclip_managed: true,
                },
                InviteScopedSkill {
                    name: "test-generator".to_string(),
                    description: "Generate unit tests for functions".to_string(),
                    is_paperclip_managed: true,
                },
                InviteScopedSkill {
                    name: "documentation".to_string(),
                    description: "Generate API documentation".to_string(),
                    is_paperclip_managed: false,
                },
            ],
        })
    }

    async fn get_skill_details(
        &self,
        _token: &str,
        skill_name: &str,
    ) -> Result<InviteSkillDetails, String> {
        use models::{InviteSkillExample, InviteSkillParameter};

        match skill_name {
            "code-review" => Ok(InviteSkillDetails {
                name: "code-review".to_string(),
                slug: "code-review".to_string(),
                description: "Automated code review with best practices and security checks"
                    .to_string(),
                parameters: Some(vec![
                    InviteSkillParameter {
                        name: "file_path".to_string(),
                        description: "Path to the file to review".to_string(),
                        required: true,
                        default_value: None,
                    },
                    InviteSkillParameter {
                        name: "severity".to_string(),
                        description: "Minimum severity level (info|warning|error)".to_string(),
                        required: false,
                        default_value: Some("warning".to_string()),
                    },
                ]),
                examples: Some(vec![InviteSkillExample {
                    title: "Review a TypeScript file".to_string(),
                    code: "code-review --file src/api/users.ts".to_string(),
                }]),
                usage_notes: Some(
                    "This skill analyzes code for common issues, security vulnerabilities, and style violations.".to_string(),
                ),
            }),
            "test-generator" => Ok(InviteSkillDetails {
                name: "test-generator".to_string(),
                slug: "test-generator".to_string(),
                description: "Generate comprehensive unit tests for functions".to_string(),
                parameters: Some(vec![InviteSkillParameter {
                    name: "function_name".to_string(),
                    description: "Name of the function to test".to_string(),
                    required: true,
                    default_value: None,
                }]),
                examples: Some(vec![InviteSkillExample {
                    title: "Generate tests for a function".to_string(),
                    code: "test-generator --function calculateTotal".to_string(),
                }]),
                usage_notes: Some("Generates Jest/Vitest compatible test cases.".to_string()),
            }),
            _ => Err(format!("Skill '{}' not found", skill_name)),
        }
    }
}
