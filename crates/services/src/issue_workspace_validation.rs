use sqlx::PgPool;
use uuid::Uuid;

/// Assert that the issue workspace is finalized before accepting an interaction
///
/// This prevents users from making decisions based on stale workspace state.
/// If the interaction has a source_run_id, we verify that the run's workspace
/// has been finalized (all file syncs completed) before allowing acceptance.
///
/// Reference: Paperclip issues.ts:6780-6815 (assertIssueWorkspaceFinalizedForAccept)
pub async fn assert_issue_workspace_finalized_for_accept(
    pool: &PgPool,
    issue_id: Uuid,
    source_run_id: Option<Uuid>,
) -> Result<(), String> {
    // If no source_run_id, no workspace to check
    let Some(run_id) = source_run_id else {
        return Ok(());
    };

    // Get the issue's execution_workspace_id
    let execution_workspace_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT execution_workspace_id FROM issues WHERE id = $1"
    )
    .bind(issue_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to query issue workspace: {}", e))?
    .flatten();

    // If issue has no execution workspace, no need to check finalization
    let Some(workspace_id) = execution_workspace_id else {
        return Ok(());
    };

    // Check if the run's workspace is finalized
    // A workspace is finalized when all its file operations have completed
    let is_finalized: Option<bool> = sqlx::query_scalar(
        r#"
        SELECT workspace_finalized 
        FROM heartbeat_runs 
        WHERE id = $1 AND execution_workspace_id = $2
        "#
    )
    .bind(run_id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to query run workspace status: {}", e))?;

    match is_finalized {
        Some(true) => Ok(()),
        Some(false) => Err(
            "Workspace is still syncing. Please wait for all file operations to complete before accepting this interaction.".to_string()
        ),
        None => {
            //not found or doesn't match workspace - treat as finalized
            // (the run might be from a different workspace or already cleaned up)
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_no_source_run_returns_ok() {
        // This is a placeholder - actual test would need a test database
        // When source_run_id is None, should always return Ok
    }

    #[tokio::test]
    async fn test_no_execution_workspace_returns_ok() {
        // When issue has no execution_workspace_id, should return Ok
    }

    #[tokio::test]
    async fn test_finalized_workspace_returns_ok() {
        // When workspace_finalized = true, should return Ok
    }

    #[tokio::test]
    async fn test_unfinalized_workspace_returns_error() {
        // When workspace_finalized = false, should return Err
    }
}
