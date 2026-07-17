use crate::errors::ServiceResult;
use async_trait::async_trait;
use models::{
    AdminUserDirectoryEntry, AdminUserDirectoryResponse, CompanyUserDirectoryEntry,
    CompanyUserDirectoryResponse, UserDirectoryQuery, UserProfile,
};
use std::sync::Arc;
use uuid::Uuid;

/// Service for user directory operations
#[async_trait]
pub trait UserDirectoryService: Send + Sync {
    /// List company user directory (active members)
    async fn list_company_users(
        &self,
        company_id: Uuid,
        query: UserDirectoryQuery,
    ) -> ServiceResult<CompanyUserDirectoryResponse>;

    /// List admin user directory (instance-wide, requires admin)
    async fn list_admin_users(
        &self,
        query: UserDirectoryQuery,
    ) -> ServiceResult<AdminUserDirectoryResponse>;
}

/// Placeholder implementation of UserDirectoryService
pub struct UserDirectoryServiceImpl {}

impl UserDirectoryServiceImpl {
    pub fn new() -> Self {
        Self {}
    }

    fn mock_company_users(&self, company_id: Uuid, limit: usize, offset: usize) -> Vec<CompanyUserDirectoryEntry> {
        // Mock 10 users for pagination demonstration
        let total_users = 10;
        let start = offset.min(total_users);
        let end = (offset + limit).min(total_users);

        (start..end)
            .map(|i| {
                let user_id = Uuid::new_v4();
                CompanyUserDirectoryEntry {
                    principal_id: user_id,
                    status: "active".to_string(),
                    user: Some(UserProfile {
                        id: user_id,
                        email: Some(format!("user{}@company-{}.example.com", i, &company_id.to_string()[..8])),
                        name: Some(format!("User {}", i)),
                        image: None,
                    }),
                }
            })
            .collect()
    }

    fn mock_admin_users(&self, limit: usize, offset: usize) -> Vec<AdminUserDirectoryEntry> {
        // Mock 15 instance-wide users
        let total_users = 15;
        let start = offset.min(total_users);
        let end = (offset + limit).min(total_users);

        (start..end)
            .map(|i| {
                let user_id = Uuid::new_v4();
                AdminUserDirectoryEntry {
                    id: user_id,
                    email: Some(format!("admin-user{}@instance.example.com", i)),
                    name: Some(format!("Admin User {}", i)),
                    image: None,
                    is_instance_admin: i < 3, // First 3 are admins
                    active_company_membership_count: (i % 5 + 1) as i32,
                }
            })
            .collect()
    }
}

#[async_trait]
impl UserDirectoryService for UserDirectoryServiceImpl {
    async fn list_company_users(
        &self,
        company_id: Uuid,
        query: UserDirectoryQuery,
    ) -> ServiceResult<CompanyUserDirectoryResponse> {
        // TODO: Implement real database query with search filtering
        // TODO: Add permission check - user must be company member

        let users = self.mock_company_users(company_id, query.limit, query.offset);
        let total = 10; // Mock total count

        Ok(CompanyUserDirectoryResponse {
            users,
            total,
            limit: query.limit,
            offset: query.offset,
        })
    }

    async fn list_admin_users(
        &self,
        query: UserDirectoryQuery,
    ) -> ServiceResult<AdminUserDirectoryResponse> {
        // TODO: Implement real database query with search filtering
        // TODO: Add permission check - assertIsInstanceAdmin

        let users = self.mock_admin_users(query.limit, query.offset);
        let total = 15; // Mock total count

        Ok(AdminUserDirectoryResponse {
            users,
            total,
            limit: query.limit,
            offset: query.offset,
        })
    }
}

/// Factory function to create UserDirectoryService
pub fn create_user_directory_service() -> Arc<dyn UserDirectoryService> {
    Arc::new(UserDirectoryServiceImpl::new())
}
