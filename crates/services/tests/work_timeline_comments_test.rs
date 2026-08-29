//! Regression coverage for work-timeline issue-comment queries.
//!
//! Issue comments store their author in the Parrot `actor_type`/`actor_id`
//! columns. Keep this test against the live schema so a legacy canonical
//! `author_*` query cannot compile successfully and fail only at runtime.

use chrono::{Duration, Utc};
use services::work_timeline_service::{DefaultWorkTimelineService, WorkTimelineService};
use sqlx::PgPool;
use uuid::Uuid;

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

async fn seed_company_and_issue(pool: &PgPool) -> (Uuid, Uuid) {
    let company_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let prefix = format!("WT{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Work timeline comments")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, created_at, updated_at)
         VALUES ($1, $2, 'Timeline issue', 'todo', NOW(), NOW())",
    )
    .bind(issue_id)
    .bind(company_id)
    .execute(pool)
    .await
    .expect("insert issue");

    (company_id, issue_id)
}

async fn cleanup(pool: &PgPool, company_id: Uuid) {
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(pool)
        .await;
}

#[sqlx::test]
async fn work_timeline_reads_actor_columns_and_excludes_tombstones(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, issue_id) = seed_company_and_issue(&pool).await;
    let user_id = Uuid::new_v4();
    let deleted_comment_id = Uuid::new_v4();
    let active_comment_id = Uuid::new_v4();

    for (comment_id, body) in [
        (deleted_comment_id, "deleted timeline comment"),
        (active_comment_id, "active timeline comment"),
    ] {
        sqlx::query(
            "INSERT INTO issue_comments
             (id, company_id, issue_id, body, actor_type, actor_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, 'user'::comment_actor_type, $5, NOW(), NOW())",
        )
        .bind(comment_id)
        .bind(company_id)
        .bind(issue_id)
        .bind(body)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("insert comment");
    }
    sqlx::query(
        "UPDATE issue_comments
         SET body = '', metadata = NULL, deleted_at = NOW(), deleted_by_type = 'user', deleted_by_user_id = $2
         WHERE id = $1",
    )
    .bind(deleted_comment_id)
    .bind(user_id.to_string())
    .execute(&pool)
    .await
    .expect("tombstone comment");

    let service = DefaultWorkTimelineService { pool: pool.clone() };
    let from = Utc::now() - Duration::minutes(5);
    let to = Utc::now() + Duration::minutes(5);
    let events = service
        .load_issue_comments(company_id, &[issue_id], from, to)
        .await
        .expect("load timeline comments");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].actor_id, format!("user:{user_id}"));
    assert_eq!(events[0].issue_id, issue_id.to_string());

    let visible_issue_ids = service
        .apply_user_lens(company_id, user_id, vec![issue_id], from, to)
        .await
        .expect("apply user lens");
    assert_eq!(visible_issue_ids, vec![issue_id]);

    sqlx::query("UPDATE issue_comments SET deleted_at = NOW() WHERE id = $1")
        .bind(active_comment_id)
        .execute(&pool)
        .await
        .expect("tombstone active comment");
    let no_comment_events = service
        .load_issue_comments(company_id, &[issue_id], from, to)
        .await
        .expect("reload timeline comments");
    assert!(no_comment_events.is_empty());
    let no_visible_issue_ids = service
        .apply_user_lens(company_id, user_id, vec![issue_id], from, to)
        .await
        .expect("reapply user lens");
    assert!(no_visible_issue_ids.is_empty());

    cleanup(&pool, company_id).await;
}
