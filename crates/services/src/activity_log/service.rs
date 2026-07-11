use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::errors::ServiceResult;
use super::types::{Activity, ActivityAction, ActivityFeed, ActivityLevel, ActivityMetadata, ActivityQuery, ActivityStats, AggregationPeriod, ResourceType};

#[async_trait]
pub trait ActivityLogService: Send + Sync {
    async fn log_activity(
        &self,
        company_id: Uuid,
        actor_type: String,
        actor_id: Uuid,
        action: ActivityAction,
        resource_type: ResourceType,
        resource_id: Uuid,
        metadata: ActivityMetadata,
        level: ActivityLevel,
        tags: Vec<String>,
    ) -> ServiceResult<Activity>;

    async fn query_activities(&self, query: ActivityQuery) -> ServiceResult<Vec<Activity>>;

    async fn get_activity_feed(&self, query: ActivityQuery) -> ServiceResult<ActivityFeed>;

    async fn aggregate_by_period(
        &self,
        company_id: Uuid,
        period: AggregationPeriod,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> ServiceResult<Vec<ActivityStats>>;

    async fn get_hot_resources(
        &self,
        company_id: Uuid,
        resource_type: ResourceType,
        limit: i64,
    ) -> ServiceResult<Vec<(Uuid, i64)>>;

    async fn export_json(
        &self,
        query: ActivityQuery,
    ) -> ServiceResult<String>;

    async fn export_csv(
        &self,
        query: ActivityQuery,
    ) -> ServiceResult<String>;

    async fn archive_old_logs(
        &self,
        company_id: Uuid,
        before: DateTime<Utc>,
    ) -> ServiceResult<i64>;

    async fn scrub_sensitive_data(&self, metadata: ActivityMetadata) -> ActivityMetadata;
}

pub struct ActivityLogServiceImpl<R> {
    repository: R,
}

impl<R> ActivityLogServiceImpl<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl<R> ActivityLogService for ActivityLogServiceImpl<R>
where
    R: ActivityLogRepository + Send + Sync,
{
    async fn log_activity(
        &self,
        company_id: Uuid,
        actor_type: String,
        actor_id: Uuid,
        action: ActivityAction,
        resource_type: ResourceType,
        resource_id: Uuid,
        metadata: ActivityMetadata,
        level: ActivityLevel,
        tags: Vec<String>,
    ) -> ServiceResult<Activity> {
        let scrubbed_metadata = self.scrub_sensitive_data(metadata).await;

        let activity = Activity {
            id: Uuid::new_v4(),
            company_id,
            actor_type,
            actor_id,
            action,
            resource_type,
            resource_id,
            metadata: scrubbed_metadata,
            level,
            tags,
            created_at: Utc::now(),
        };

        self.repository.insert(activity.clone()).await?;
        Ok(activity)
    }

    async fn query_activities(&self, query: ActivityQuery) -> ServiceResult<Vec<Activity>> {
        self.repository.query(query).await
    }

    async fn get_activity_feed(&self, query: ActivityQuery) -> ServiceResult<ActivityFeed> {
        let total_count = self.repository.count(&query).await?;
        let activities = self.repository.query(query.clone()).await?;
        let has_more = query.limit.map_or(false, |limit| {
            query.offset.unwrap_or(0) + limit < total_count
        });

        Ok(ActivityFeed {
            activities,
            total_count,
            has_more,
        })
    }

    async fn aggregate_by_period(
        &self,
        company_id: Uuid,
        period: AggregationPeriod,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> ServiceResult<Vec<ActivityStats>> {
        self.repository.aggregate(company_id, period, start_time, end_time).await
    }

    async fn get_hot_resources(
        &self,
        company_id: Uuid,
        resource_type: ResourceType,
        limit: i64,
    ) -> ServiceResult<Vec<(Uuid, i64)>> {
        self.repository.hot_resources(company_id, resource_type, limit).await
    }

    async fn export_json(&self, query: ActivityQuery) -> ServiceResult<String> {
        let activities = self.repository.query(query).await?;
        serde_json::to_string_pretty(&activities)
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn export_csv(&self, query: ActivityQuery) -> ServiceResult<String> {
        let activities = self.repository.query(query).await?;
        let mut wtr = csv::Writer::from_writer(vec![]);

        for activity in activities {
            wtr.serialize(activity)
                .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?;
        }

        let data = wtr.into_inner()
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?;
        String::from_utf8(data)
            .map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))
    }

    async fn archive_old_logs(&self, company_id: Uuid, before: DateTime<Utc>) -> ServiceResult<i64> {
        self.repository.archive(company_id, before).await
    }

    async fn scrub_sensitive_data(&self, mut metadata: ActivityMetadata) -> ActivityMetadata {
        if let Some(obj) = metadata.changes.as_mut().and_then(|v| v.as_object_mut()) {
            obj.remove("api_key");
            obj.remove("token");
            obj.remove("password");
            obj.remove("secret");
        }

        if let Some(obj) = metadata.context.as_object_mut() {
            obj.remove("api_key");
            obj.remove("token");
            obj.remove("password");
            obj.remove("secret");
        }

        metadata
    }
}

#[async_trait]
pub trait ActivityLogRepository: Send + Sync {
    async fn insert(&self, activity: Activity) -> ServiceResult<()>;
    async fn query(&self, query: ActivityQuery) -> ServiceResult<Vec<Activity>>;
    async fn count(&self, query: &ActivityQuery) -> ServiceResult<i64>;
    async fn aggregate(
        &self,
        company_id: Uuid,
        period: AggregationPeriod,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> ServiceResult<Vec<ActivityStats>>;
    async fn hot_resources(
        &self,
        company_id: Uuid,
        resource_type: ResourceType,
        limit: i64,
    ) -> ServiceResult<Vec<(Uuid, i64)>>;
    async fn archive(&self, company_id: Uuid, before: DateTime<Utc>) -> ServiceResult<i64>;
}
