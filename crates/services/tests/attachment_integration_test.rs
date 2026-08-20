use std::sync::Arc;

use models::issue_auxiliary::UploadAttachmentInput;
use services::asset_storage::LocalStorageService;
use services::attachment_service::{AttachmentService, LocalAttachmentService};
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::test]
async fn attachment_upload_read_list_delete_roundtrip() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping attachment integration test: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.expect("connect database");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    let company_id = Uuid::new_v4();
    let parent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Attachment Integration Company")
        .bind(format!("AT{}", &company_id.simple().to_string()[..6]))
        .execute(&pool)
        .await
        .expect("insert company");

    let storage_dir = std::env::temp_dir().join(format!("parrot-attachment-{}", Uuid::new_v4()));
    let storage = Arc::new(LocalStorageService::new(storage_dir.clone()));
    let service = LocalAttachmentService::with_storage(pool.clone(), storage);

    let attachment = service
        .upload_attachment(
            "issue",
            parent_id,
            company_id,
            UploadAttachmentInput {
                filename: "roundtrip.txt".to_string(),
                content_type: "text/plain".to_string(),
                size: 11,
                content: b"hello world".to_vec(),
            },
        )
        .await
        .expect("upload attachment");
    assert_eq!(attachment.filename, "roundtrip.txt");

    let listed = service
        .list_attachments("issue", parent_id, company_id)
        .await
        .expect("list attachments");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, attachment.id);

    let content = service
        .get_attachment_content(attachment.id, company_id)
        .await
        .expect("read attachment");
    assert_eq!(content, b"hello world");
    assert!(service
        .get_attachment_content(attachment.id, Uuid::new_v4())
        .await
        .is_err());

    service
        .delete_attachment(attachment.id, company_id)
        .await
        .expect("delete attachment");
    assert!(service
        .get_attachment_content(attachment.id, company_id)
        .await
        .is_err());

    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .expect("cleanup company");
    let _ = tokio::fs::remove_dir_all(storage_dir).await;
}
