use models::{
    AgentMembership, AppResult, CreateProjectInput, CreateWorkspaceInput, Project, ProjectMembership,
    ProjectWorkspace, ResourceMemberships, UpdateProjectInput, MembershipState,
};
use repositories::ProjectRepository;
use uuid::Uuid;

pub struct ProjectService {
    project_repo: ProjectRepository,
}

impl ProjectService {
    pub fn new(project_repo: ProjectRepository) -> Self {
        Self { project_repo }
    }

    pub async fn create(&self, input: CreateProjectInput) -> AppResult<Project> {
        // TODO: Call SecretService.normalize_env_bindings_for_persistence() when implemented
        let project = self.project_repo.create(input).await?;

        // TODO: Optionally create workspace and sync env bindings

        Ok(project)
    }

    pub async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Project>> {
        Ok(self.project_repo.get_by_id(id).await?)
    }

    pub async fn list_by_company(&self, company_id: Uuid) -> AppResult<Vec<Project>> {
        Ok(self.project_repo.list_by_company(company_id).await?)
    }

    pub async fn update(&self, id: Uuid, input: UpdateProjectInput) -> AppResult<Project> {
        Ok(self.project_repo.update(id, input).await?)
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<bool> {
        Ok(self.project_repo.delete(id).await?)
    }

    pub async fn archive(&self, id: Uuid) -> AppResult<Project> {
        Ok(self.project_repo.archive(id).await?)
    }

    // Workspace operations
    pub async fn create_workspace(&self, input: CreateWorkspaceInput) -> AppResult<ProjectWorkspace> {
        Ok(self.project_repo.create_workspace(input).await?)
    }

    pub async fn list_workspaces(&self, project_id: Uuid) -> AppResult<Vec<ProjectWorkspace>> {
        Ok(self.project_repo.list_workspaces(project_id).await?)
    }

    pub async fn get_primary_workspace(&self, project_id: Uuid) -> AppResult<Option<ProjectWorkspace>> {
        Ok(self.project_repo.get_primary_workspace(project_id).await?)
    }

    pub async fn delete_workspace(&self, id: Uuid) -> AppResult<bool> {
        Ok(self.project_repo.delete_workspace(id).await?)
    }

    // Resource membership operations
    pub async fn list_memberships_for_user(&self, company_id: Uuid, user_id: Uuid) -> AppResult<ResourceMemberships> {
        Ok(self.project_repo.list_memberships_for_user(company_id, user_id).await?)
    }

    pub async fn update_project_membership(
        &self,
        company_id: Uuid,
        project_id: Uuid,
        user_id: Uuid,
        state: MembershipState,
    ) -> AppResult<ProjectMembership> {
        // TODO: assert_mutation_allowed when AccessService is integrated
        Ok(self.project_repo.upsert_project_membership(company_id, project_id, user_id, state).await?)
    }

    pub async fn star_project(
        &self,
        company_id: Uuid,
        project_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<ProjectMembership> {
        Ok(self.project_repo.star_project(company_id, project_id, user_id).await?)
    }

    pub async fn unstar_project(
        &self,
        company_id: Uuid,
        project_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<ProjectMembership> {
        Ok(self.project_repo.unstar_project(company_id, project_id, user_id).await?)
    }

    /// Upsert an agent membership (joined/left + optional starred).
    ///
    /// Mirrors Paperclip `resourceMembershipService.updateAgent`. `starred`
    /// overrides `starred_at`: `true` sets NOW() (if not already starred),
    /// `false` clears it; `None` leaves it untouched on state-only updates.
    pub async fn update_agent_membership(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
        user_id: Uuid,
        state: MembershipState,
        starred: Option<bool>,
    ) -> AppResult<AgentMembership> {
        Ok(self
            .project_repo
            .upsert_agent_membership(company_id, agent_id, user_id, state, starred)
            .await?)
    }
}
