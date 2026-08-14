//! Summary Slot 后台任务链（迁移自 Paperclip `services/summary-slots.ts` +
//! `services/summary-slot-finalization.ts`）。
//!
//! generate 创建 hidden issue（assignee = Summarizer 内置 agent），description 内嵌
//! scope snapshot + JSON payload；agent 完成后经 `PUT /summary-slots/...` 写回
//! （写回时校验 Summarizer + generationIssueId + runId 匹配）。issue 终态触发
//! finalization 将 slot 置为 failed。

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::status_card_worker::{SUMMARIZER_BUILT_IN_KEY, TERMINAL_ISSUE_STATUSES};

/// 生成任务创建结果。
#[derive(Debug, Clone)]
pub struct SummarySlotGeneration {
    pub slot_id: Uuid,
    pub generating_issue_id: Uuid,
    pub already_generating: bool,
}

/// Summary Slot 后台 worker。
#[derive(Clone)]
pub struct SummarySlotWorker {
    pool: PgPool,
}

impl SummarySlotWorker {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 解析 Summarizer 内置 agent（对应 Paperclip builtIns.get + SUMMARIZER_BUILT_IN_KEY）。
    pub async fn resolve_summarizer_agent_id(&self, company_id: Uuid) -> Result<Uuid, String> {
        let built_in: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM agents WHERE company_id = $1 AND metadata->>'builtInKey' = $2 \
             LIMIT 1",
        )
        .bind(company_id)
        .bind(SUMMARIZER_BUILT_IN_KEY)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("resolve built-in summarizer: {e}"))?;
        built_in.ok_or_else(|| "Summarizer built-in agent is not configured".to_string())
    }

    /// 查找既有 slot（对应 findSlotRow）。
    async fn find_slot(
        &self,
        company_id: Uuid,
        scope_kind: &str,
        scope_id: Option<Uuid>,
        slot_key: &str,
    ) -> Result<Option<Uuid>, String> {
        let row = sqlx::query_scalar::<_, Option<Uuid>>(
            "SELECT id FROM summary_slots \
             WHERE company_id = $1 AND scope_kind = $2 AND slot_key = $3 \
               AND ($4::uuid IS NULL AND scope_id IS NULL OR scope_id = $4) \
             LIMIT 1",
        )
        .bind(company_id)
        .bind(scope_kind)
        .bind(slot_key)
        .bind(scope_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("find slot: {e}"))?;
        Ok(row.flatten())
    }

    /// 幂等 upsert slot（对应 upsertSlot）。
    async fn upsert_slot(
        &self,
        company_id: Uuid,
        scope_kind: &str,
        scope_id: Option<Uuid>,
        slot_key: &str,
        status: &str,
        generating_issue_id: Option<Uuid>,
    ) -> Result<Uuid, String> {
        let id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO summary_slots (company_id, scope_kind, scope_id, slot_key, status, generating_issue_id) \
             VALUES ($1,$2,$3,$4,$5,$6) \
             ON CONFLICT (company_id, scope_kind, scope_id, slot_key) DO UPDATE \
               SET status = EXCLUDED.status, \
                   generating_issue_id = COALESCE(EXCLUDED.generating_issue_id, summary_slots.generating_issue_id), \
                   failure_reason = NULL, updated_at = NOW() \
             RETURNING id",
        )
        .bind(company_id)
        .bind(scope_kind)
        .bind(scope_id)
        .bind(slot_key)
        .bind(status)
        .bind(generating_issue_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| format!("upsert slot: {e}"))?;
        Ok(id)
    }

    /// 构建 scope snapshot（对齐 Paperclip buildScopeSnapshot 的轻量版：按状态分组 issue）。
    async fn build_scope_snapshot(
        &self,
        company_id: Uuid,
        scope_kind: &str,
        scope_id: Option<Uuid>,
    ) -> Result<String, String> {
        let (where_sql, project_id) = match scope_kind {
            "project" => ("AND project_id = $2".to_string(), scope_id),
            "project_workspace" => ("AND project_workspace_id = $2".to_string(), scope_id),
            _ => ("".to_string(), None),
        };
        // scope 需要 scope_id 但缺失时，返回空快照（避免 SQL 参数占位符悬空）。
        if !where_sql.is_empty() && project_id.is_none() {
            return Ok("## Prebuilt scope snapshot\n\nSnapshot generated at "
                .to_string()
                + &Utc::now().to_rfc3339()
                + "\n\n(scope target not resolved — empty snapshot)");
        }
        let sql = format!(
            "SELECT identifier, title, status::text AS status, priority::text AS priority, updated_at \
             FROM issues WHERE company_id = $1 AND hidden_at IS NULL {} \
             ORDER BY updated_at DESC LIMIT 12",
            where_sql
        );
        let mut query = sqlx::query(&sql).bind(company_id);
        if let Some(pid) = project_id {
            query = query.bind(pid);
        }
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| format!("build scope snapshot: {e}"))?;
        let mut groups: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for row in rows {
            let status: String = row.get("status");
            let identifier: Option<String> = row.get("identifier");
            let title: String = row.get("title");
            let priority: String = row.get("priority");
            let updated_at: DateTime<Utc> = row.get("updated_at");
            let line = format!(
                "- {} — {} ({}; updated {})",
                identifier.as_deref().unwrap_or("Unnumbered issue"),
                title,
                priority,
                updated_at.to_rfc3339()
            );
            groups.entry(status).or_default().push(line);
        }
        let mut out = vec![
            "## Prebuilt scope snapshot".to_string(),
            "".to_string(),
            format!("Snapshot generated at {}.", Utc::now().to_rfc3339()),
            "Use this bounded, company-scoped snapshot as the issue source of truth for this run. Do not call issue-list endpoints.".to_string(),
            "".to_string(),
        ];
        for (status, lines) in groups {
            out.push(format!("### {}", status));
            if lines.is_empty() {
                out.push("- None.".to_string());
            } else {
                out.extend(lines);
            }
            out.push("".to_string());
        }
        Ok(out.join("\n"))
    }

    /// 生成（对应 summarySlotService.generate：创建 hidden issue + 占位 slot）。
    pub async fn generate(
        &self,
        company_id: Uuid,
        scope_kind: &str,
        scope_id: Option<Uuid>,
        slot_key: &str,
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<Uuid>,
    ) -> Result<SummarySlotGeneration, String> {
        // 幂等：已有 generating 且 issue 活跃 -> 返回 in-flight。
        if let Some(slot_id) = self.find_slot(company_id, scope_kind, scope_id, slot_key).await? {
            let row = sqlx::query(
                "SELECT generating_issue_id AS gid, status FROM summary_slots WHERE id = $1",
            )
            .bind(slot_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| format!("load slot: {e}"))?
            .ok_or_else(|| "Summary slot not found".to_string())?;
            let gid: Option<Uuid> = row.get("gid");
            let slot_status: String = row.get("status");
            if slot_status == "generating" {
                if let Some(gid) = gid {
                    let issue_status: Option<String> =
                        sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
                            .bind(gid)
                            .fetch_optional(&self.pool)
                            .await
                            .map_err(|e| format!("load generation issue: {e}"))?
                            .flatten();
                    if let Some(status) = issue_status {
                        if !TERMINAL_ISSUE_STATUSES.contains(&status.as_str()) {
                            return Ok(SummarySlotGeneration {
                                slot_id,
                                generating_issue_id: gid,
                                already_generating: true,
                            });
                        }
                    }
                }
            }
        }

        let summarizer = self.resolve_summarizer_agent_id(company_id).await?;
        let scope_snapshot = self
            .build_scope_snapshot(company_id, scope_kind, scope_id)
            .await?;
        let now = Utc::now();
        let title = format!(
            "Summarize {} on {}",
            scope_kind,
            now.format("%Y-%m-%d %H:%M UTC")
        );
        let description = format!(
            "Generate the {} summary for this scope.\n\n\
             - Read current slot: `GET /api/companies/{}/summary-slots/{}/{}`\n\
             - Write revision: `PUT /api/companies/{}/summary-slots/{}/{}`\n\n\
             Write one short, colloquial Markdown summary that opens with the 1–3 specific, concrete, actionable items the reader should do right now to unblock this work.\n\n\
             ```json\n{}\n```\n\n{}",
            scope_kind,
            company_id,
            scope_kind,
            slot_key,
            company_id,
            scope_kind,
            slot_key,
            serde_json::to_string_pretty(&json!({
                "scopeKind": scope_kind,
                "scopeId": scope_id,
                "slotKey": slot_key,
                "generationIssueId": null,
            }))
            .unwrap_or_default(),
            scope_snapshot,
        );
        let issue_id = Uuid::new_v4();
        // origin_fingerprint 必须唯一（issues 表有 (company_id, origin_fingerprint) 唯一约束）。
        let origin_fingerprint = format!("summary_slot_generation:{}", issue_id);
        sqlx::query(
            "INSERT INTO issues \
             (id, company_id, title, description, status, priority, assignee_agent_id, \
              created_by_agent_id, created_by_user_id, hidden_at, origin_kind, origin_fingerprint) \
             VALUES ($1,$2,$3,$4,'todo','medium',$5,$6,$7,NOW(),'summary_slot_generation',$8)",
        )
        .bind(issue_id)
        .bind(company_id)
        .bind(&title)
        .bind(&description)
        .bind(summarizer)
        .bind(created_by_agent_id)
        .bind(created_by_user_id)
        .bind(&origin_fingerprint)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("create generation issue: {e}"))?;
        // 回填 generationIssueId。
        let description = format!(
            "Generate the {} summary for this scope.\n\n\
             ```json\n{}\n```\n\n{}",
            scope_kind,
            serde_json::to_string_pretty(&json!({
                "scopeKind": scope_kind,
                "scopeId": scope_id,
                "slotKey": slot_key,
                "generationIssueId": issue_id.to_string(),
            }))
            .unwrap_or_default(),
            scope_snapshot,
        );
        let _ = sqlx::query("UPDATE issues SET description = $2 WHERE id = $1")
            .bind(issue_id)
            .bind(&description)
            .execute(&self.pool)
            .await;
        let slot_id = self
            .upsert_slot(
                company_id,
                scope_kind,
                scope_id,
                slot_key,
                "generating",
                Some(issue_id),
            )
            .await?;
        Ok(SummarySlotGeneration {
            slot_id,
            generating_issue_id: issue_id,
            already_generating: false,
        })
    }

    /// finalization：issue 终态时 slot -> failed（对应 finalizeSummarySlotsForTerminalIssue）。
    pub async fn finalize_terminal_issues(&self) -> Result<usize, String> {
        let terminal = sqlx::query(
            "SELECT i.id AS issue_id, i.company_id, i.identifier, i.title, i.status::text AS status \
             FROM issues i \
             WHERE i.status = ANY($1::issue_status[]) \
               AND EXISTS (SELECT 1 FROM summary_slots s \
                           WHERE s.generating_issue_id = i.id AND s.status = 'generating')",
        )
        .bind(&TERMINAL_ISSUE_STATUSES[..])
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("query terminal generations: {e}"))?;
        let mut finalized = 0usize;
        for row in terminal {
            let issue_id: Uuid = row.get("issue_id");
            let company_id: Uuid = row.get("company_id");
            let identifier: Option<String> = row.get("identifier");
            let title: String = row.get("title");
            let status: String = row.get("status");
            let label = identifier
                .map(|i| format!("{}: {}", i, title))
                .unwrap_or_else(|| title.clone());
            let failure_reason = if status == "cancelled" {
                format!(
                    "Summary generation task {} was cancelled before writing a summary.",
                    label
                )
            } else {
                format!(
                    "Summary generation task {} finished without writing a summary.",
                    label
                )
            };
            let updated = sqlx::query(
                "UPDATE summary_slots SET status = 'failed', failure_reason = $2, updated_at = NOW() \
                 WHERE company_id = $3 AND generating_issue_id = $1 AND status = 'generating'",
            )
            .bind(issue_id)
            .bind(&failure_reason)
            .bind(company_id)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("finalize slot: {e}"))?;
            finalized += updated.rows_affected() as usize;
        }
        Ok(finalized)
    }

    /// 写回校验（对应 assertSummarizerWriter 的轻量版：agent 必须为 Summarizer 且
    /// generationIssueId 匹配当前 slot 占位）。
    pub async fn assert_summarizer_writer(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
        generation_issue_id: Option<Uuid>,
        scope_kind: &str,
        scope_id: Option<Uuid>,
        slot_key: &str,
    ) -> Result<(), String> {
        let marker: Option<String> = sqlx::query_scalar(
            "SELECT metadata->>'builtInKey' FROM agents WHERE id = $1 AND company_id = $2",
        )
        .bind(agent_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("load writer agent: {e}"))?
        .flatten();
        if marker.as_deref() != Some(SUMMARIZER_BUILT_IN_KEY) {
            return Err("Only the Summarizer built-in agent may write summaries".to_string());
        }
        let Some(gid) = generation_issue_id else {
            return Err("Summary writes must identify the active generation task".to_string());
        };
        let slot_gid: Option<Uuid> = sqlx::query_scalar(
            "SELECT generating_issue_id FROM summary_slots \
             WHERE company_id = $1 AND scope_kind = $2 AND slot_key = $3 \
               AND ($4::uuid IS NULL AND scope_id IS NULL OR scope_id = $4) \
             LIMIT 1",
        )
        .bind(company_id)
        .bind(scope_kind)
        .bind(slot_key)
        .bind(scope_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("load slot generation: {e}"))?
        .flatten();
        if slot_gid != Some(gid) {
            return Err("Summary write does not match the active generation task".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_statuses_match_paperclip() {
        assert!(TERMINAL_ISSUE_STATUSES.contains(&"done"));
        assert!(TERMINAL_ISSUE_STATUSES.contains(&"cancelled"));
        assert!(!TERMINAL_ISSUE_STATUSES.contains(&"blocked"));
    }

    #[test]
    fn built_in_key_is_summarizer() {
        assert_eq!(SUMMARIZER_BUILT_IN_KEY, "summarizer");
    }
}
