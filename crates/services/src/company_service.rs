use models::{AppResult, Company, CompanyStats, CreateCompanyInput, UpdateCompanyInput};
use repositories::CompanyRepository;
use uuid::Uuid;

pub struct CompanyService {
    company_repo: CompanyRepository,
}

impl CompanyService {
    pub fn new(company_repo: CompanyRepository) -> Self {
        Self { company_repo }
    }

    pub async fn create(&self, input: CreateCompanyInput, creator_user_id: Uuid) -> AppResult<Company> {
        // Create company with owner membership in a transaction
        let company = self.company_repo.create(input, creator_user_id).await?;

        // TODO: Call AccessService.ensure_role_default_grants() when implemented
        // TODO: Call BudgetService.upsert_policy() when budget is set

        Ok(company)
    }

    pub async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Company>> {
        Ok(self.company_repo.get_by_id(id).await?)
    }

    pub async fn list(&self, limit: i64, offset: i64) -> AppResult<Vec<Company>> {
        Ok(self.company_repo.list(limit, offset).await?)
    }

    pub async fn list_by_user(&self, user_id: Uuid) -> AppResult<Vec<Company>> {
        Ok(self.company_repo.list_by_user(user_id).await?)
    }

    pub async fn update(&self, id: Uuid, input: UpdateCompanyInput) -> AppResult<Company> {
        Ok(self.company_repo.update(id, input).await?)
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<bool> {
        Ok(self.company_repo.delete(id).await?)
    }

    pub async fn archive(&self, id: Uuid) -> AppResult<Company> {
        let input = UpdateCompanyInput {
            name: None,
            description: None,
            status: Some(models::CompanyStatus::Archived),
            pause_reason: None,
            budget_monthly_cents: None,
            attachment_max_bytes: None,
            default_responsible_user_id: None,
            require_board_approval_for_new_agents: None,
        };
        Ok(self.company_repo.update(id, input).await?)
    }

    pub async fn get_stats(&self, company_id: Uuid) -> AppResult<CompanyStats> {
        Ok(self.company_repo.get_stats(company_id).await?)
    }

    pub async fn increment_issue_counter(&self, company_id: Uuid) -> AppResult<i32> {
        Ok(self.company_repo.increment_issue_counter(company_id).await?)
    }

    pub async fn update_branding(&self, id: Uuid, brand_color: Option<String>, logo_asset_id: Option<Uuid>) -> AppResult<Company> {
        let input = UpdateCompanyInput {
            name: None,
            description: None,
            status: None,
            pause_reason: None,
            budget_monthly_cents: None,
            attachment_max_bytes: None,
            default_responsible_user_id: None,
            require_board_approval_for_new_agents: None,
        };
        // Note: This is simplified - in production you'd have specific fields for branding
        Ok(self.company_repo.update(id, input).await?)
    }
}

