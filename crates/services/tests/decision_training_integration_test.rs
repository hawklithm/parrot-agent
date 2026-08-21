use chrono::Utc;
use services::decision_training_service::{
    DecisionTrainingService, DecisionTrainingSourceKind, ListInput, PersistSnapshotInput,
    PgDecisionTrainingService, UpdateInput,
};
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::test]
async fn decision_training_service_round_trip_is_durable_and_scoped() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping decision training integration test: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.expect("connect database");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    let company_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let source_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let prefix = format!("DT{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Decision Training Integration Company")
        .bind(&prefix)
        .execute(&pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO issues (id, company_id, title, identifier) VALUES ($1, $2, $3, $4)")
        .bind(issue_id)
        .bind(company_id)
        .bind("Training integration issue")
        .bind(format!("{prefix}-1"))
        .execute(&pool)
        .await
        .expect("insert issue");

    let service = PgDecisionTrainingService::new(pool.clone());
    let example_id = service
        .persist_snapshot(PersistSnapshotInput {
            company_id,
            source_kind: DecisionTrainingSourceKind::IssueApproval,
            source_id,
            issue_id,
            cutoff_at: Utc::now(),
            notes: "initial note".to_string(),
            tags: vec!["review".to_string()],
            quality_score: Some(0.5),
            decision_outcome: Some("approved".to_string()),
            retention_policy: "scrub_deleted_comments_v1".to_string(),
            snapshot: serde_json::json!({
                "version": 1,
                "capturedAt": Utc::now().to_rfc3339(),
                "issue": {"id": issue_id, "title": "Training integration issue", "status": "backlog"},
                "comments": [],
                "runs": [],
                "decision": {"kind": "approval", "payload": {}, "outcome": "approved"}
            }),
            created_by_user_id: user_id.to_string(),
        })
        .await
        .expect("persist snapshot");
    assert_eq!(example_id, service.get_example(example_id).await.unwrap().unwrap().id);

    let listed = service
        .list_examples(ListInput {
            company_id,
            source_kind: Some(DecisionTrainingSourceKind::IssueApproval),
            project_id: None,
            author_id: Some(user_id.to_string()),
            query: Some("integration".to_string()),
            limit: Some(10),
            offset: Some(0),
        })
        .await
        .expect("list examples");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, example_id);

    let updated = service
        .update_example(
            example_id,
            UpdateInput {
                notes: Some("updated note".to_string()),
                tags: Some(vec!["approved".to_string(), "approved".to_string()]),
                quality_score: Some(1.0),
                updated_by_user_id: user_id,
            },
        )
        .await
        .expect("update example");
    assert_eq!(updated.notes.as_deref(), Some("updated note"));
    assert_eq!(updated.tags, vec!["approved"]);
    assert_eq!(updated.quality_score, Some(1.0));
    assert_eq!(updated.notes_history.len(), 1);

    assert!(service.delete_example(example_id).await.expect("delete example"));
    assert!(service.get_example(example_id).await.unwrap().is_none());

    sqlx::query("DELETE FROM issues WHERE id = $1")
        .bind(issue_id)
        .execute(&pool)
        .await
        .expect("cleanup issue");
    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .expect("cleanup company");
}
