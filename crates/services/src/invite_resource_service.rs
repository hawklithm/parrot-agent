use async_trait::async_trait;
use models::{
    CompanyLogo, InviteOnboardingManifest, InviteSkillDetails, InviteSkillIndex, InviteToken,
};
use std::sync::Arc;
use uuid::Uuid;

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
