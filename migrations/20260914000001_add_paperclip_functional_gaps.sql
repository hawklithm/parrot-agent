-- Paperclip parity: durable runtime, plugin, transfer, annotation, and
-- pipeline-support records that are consumed by the application layer.
--
-- The existing Parrot tables remain the compatibility surface for the older
-- API.  These tables deliberately use Paperclip's names and column semantics
-- so new services can share the same durable contract without renaming the
-- existing data in place.

CREATE TABLE IF NOT EXISTS agent_task_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    adapter_type TEXT NOT NULL,
    task_key TEXT NOT NULL,
    session_params_json JSONB,
    session_display_id TEXT,
    last_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE UNIQUE INDEX IF NOT EXISTS agent_task_sessions_company_agent_adapter_task_uq
    ON agent_task_sessions(company_id, agent_id, adapter_type, task_key);
CREATE INDEX IF NOT EXISTS agent_task_sessions_company_agent_updated_idx
    ON agent_task_sessions(company_id, agent_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS agent_task_sessions_company_task_updated_idx
    ON agent_task_sessions(company_id, task_key, updated_at DESC);

CREATE TABLE IF NOT EXISTS heartbeat_run_events (
    id BIGSERIAL PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES heartbeat_runs(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    stream TEXT,
    level TEXT,
    color TEXT,
    message TEXT,
    payload JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE UNIQUE INDEX IF NOT EXISTS heartbeat_run_events_run_seq_uq
    ON heartbeat_run_events(run_id, seq);
CREATE INDEX IF NOT EXISTS heartbeat_run_events_run_seq_idx
    ON heartbeat_run_events(run_id, seq);
CREATE INDEX IF NOT EXISTS heartbeat_run_events_company_run_idx
    ON heartbeat_run_events(company_id, run_id);
CREATE INDEX IF NOT EXISTS heartbeat_run_events_company_created_idx
    ON heartbeat_run_events(company_id, created_at DESC);

CREATE TABLE IF NOT EXISTS plugin_config (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    config_json JSONB NOT NULL DEFAULT '{}',
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(plugin_id, company_id)
);

CREATE TABLE IF NOT EXISTS plugin_state (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    scope_kind TEXT NOT NULL,
    scope_id TEXT,
    namespace TEXT NOT NULL DEFAULT 'default',
    state_key TEXT NOT NULL,
    value_json JSONB NOT NULL DEFAULT 'null',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE UNIQUE INDEX IF NOT EXISTS plugin_state_unique_entry_uq
    ON plugin_state(plugin_id, scope_kind, scope_id, namespace, state_key)
    NULLS NOT DISTINCT;
CREATE INDEX IF NOT EXISTS plugin_state_plugin_scope_idx
    ON plugin_state(plugin_id, scope_kind, scope_id, namespace);

CREATE TABLE IF NOT EXISTS plugin_entities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    company_id UUID REFERENCES companies(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    scope_kind TEXT NOT NULL,
    scope_id TEXT,
    external_id TEXT,
    title TEXT,
    status TEXT,
    data JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
-- PostgreSQL 16 supports NULLS NOT DISTINCT, which is the Paperclip
-- uniqueness contract: instance-scoped entities (NULL company_id) and
-- entities without an external id must still be replaceable rather than
-- accumulating duplicate rows.
CREATE UNIQUE INDEX IF NOT EXISTS plugin_entities_external_uq
    ON plugin_entities(company_id, plugin_id, entity_type, external_id)
    NULLS NOT DISTINCT;
CREATE INDEX IF NOT EXISTS plugin_entities_plugin_company_type_idx
    ON plugin_entities(plugin_id, company_id, entity_type);
CREATE INDEX IF NOT EXISTS plugin_entities_scope_idx
    ON plugin_entities(scope_kind, scope_id);

CREATE TABLE IF NOT EXISTS plugin_database_namespaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    plugin_key TEXT NOT NULL,
    namespace_name TEXT NOT NULL,
    namespace_mode TEXT NOT NULL DEFAULT 'schema',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(plugin_id, namespace_name)
);
CREATE INDEX IF NOT EXISTS plugin_database_namespaces_status_idx
    ON plugin_database_namespaces(status);

CREATE TABLE IF NOT EXISTS plugin_migrations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    plugin_key TEXT NOT NULL,
    namespace_name TEXT NOT NULL,
    migration_key TEXT NOT NULL,
    checksum TEXT NOT NULL,
    plugin_version TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_at TIMESTAMPTZ,
    error_message TEXT,
    UNIQUE(plugin_id, namespace_name, migration_key)
);
CREATE INDEX IF NOT EXISTS plugin_migrations_plugin_status_idx
    ON plugin_migrations(plugin_id, status);

CREATE TABLE IF NOT EXISTS plugin_webhook_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    company_id UUID REFERENCES companies(id) ON DELETE CASCADE,
    webhook_key TEXT NOT NULL,
    external_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    duration_ms INTEGER,
    error TEXT,
    payload JSONB NOT NULL,
    headers JSONB NOT NULL DEFAULT '{}',
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS plugin_webhook_deliveries_plugin_created_idx
    ON plugin_webhook_deliveries(plugin_id, created_at DESC);
CREATE INDEX IF NOT EXISTS plugin_webhook_deliveries_company_created_idx
    ON plugin_webhook_deliveries(company_id, created_at DESC);

CREATE TABLE IF NOT EXISTS plugin_company_settings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    settings_json JSONB NOT NULL DEFAULT '{}',
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, plugin_id)
);
CREATE INDEX IF NOT EXISTS plugin_company_settings_company_plugin_idx
    ON plugin_company_settings(company_id, plugin_id);

CREATE TABLE IF NOT EXISTS tool_access_audit_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    gateway_id UUID REFERENCES tool_mcp_gateways(id) ON DELETE SET NULL,
    gateway_token_id UUID REFERENCES tool_mcp_gateway_tokens(id) ON DELETE SET NULL,
    gateway_public_id TEXT,
    client_name TEXT,
    correlation_id TEXT,
    connection_id UUID REFERENCES tool_connections(id) ON DELETE SET NULL,
    catalog_entry_id UUID REFERENCES tool_catalog_entries(id) ON DELETE SET NULL,
    actor_type TEXT NOT NULL DEFAULT 'system',
    actor_id TEXT,
    action TEXT NOT NULL,
    outcome TEXT NOT NULL,
    reason_code TEXT,
    details JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS tool_access_audit_events_company_created_idx
    ON tool_access_audit_events(company_id, created_at DESC);
CREATE INDEX IF NOT EXISTS tool_access_audit_events_connection_idx
    ON tool_access_audit_events(connection_id, created_at DESC);
CREATE INDEX IF NOT EXISTS tool_access_audit_events_company_gateway_idx
    ON tool_access_audit_events(company_id, gateway_id, created_at DESC);

CREATE TABLE IF NOT EXISTS tool_runtime_metric_counters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    metric TEXT NOT NULL,
    bucket_start_at TIMESTAMPTZ NOT NULL,
    count INTEGER NOT NULL DEFAULT 0 CHECK (count >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, metric, bucket_start_at)
);
CREATE INDEX IF NOT EXISTS tool_runtime_metric_counters_company_metric_bucket_idx
    ON tool_runtime_metric_counters(company_id, metric, bucket_start_at DESC);

-- Every governed tool call is both an execution record and a governance
-- record.  Keep the old, detailed `tool_call_events` stream for run replay,
-- while mirroring its security-relevant fields into Paperclip's audit ledger.
-- A database trigger covers all current and future call paths (built-ins,
-- MCP, plugins, approvals) without relying on individual handlers to remember
-- a second write.
CREATE OR REPLACE FUNCTION parrot_mirror_tool_call_event()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    INSERT INTO tool_access_audit_events (
        company_id, connection_id, catalog_entry_id, actor_type, actor_id,
        action, outcome, reason_code, correlation_id, details
    ) VALUES (
        NEW.company_id,
        NEW.connection_id,
        NEW.catalog_entry_id,
        COALESCE(NEW.actor_type, 'system'),
        NEW.actor_id,
        NEW.event_type,
        NEW.outcome,
        NEW.reason_code,
        COALESCE(NEW.invocation_id::text, NEW.action_request_id::text),
        COALESCE(NEW.metadata, '{}'::jsonb)
            || jsonb_build_object(
                'agentId', NEW.agent_id,
                'runId', NEW.run_id,
                'toolName', NEW.tool_name,
                'decision', NEW.decision,
                'argumentsSummary', NEW.arguments_summary,
                'requestSummary', NEW.request_summary,
                'resultSummary', NEW.result_summary,
                'errorCode', NEW.error_code,
                'errorMessage', NEW.error_message
            )
    );

    INSERT INTO tool_runtime_metric_counters
        (company_id, metric, bucket_start_at, count, created_at, updated_at)
    VALUES (
        NEW.company_id,
        'tool_call.' || NEW.event_type,
        date_trunc('minute', COALESCE(NEW.created_at, NOW())),
        1,
        COALESCE(NEW.created_at, NOW()),
        COALESCE(NEW.created_at, NOW())
    )
    ON CONFLICT (company_id, metric, bucket_start_at)
    DO UPDATE SET count = tool_runtime_metric_counters.count + 1,
                  updated_at = EXCLUDED.updated_at;
    RETURN NEW;
END;
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger WHERE tgname = 'parrot_tool_call_event_audit_trigger'
    ) THEN
        CREATE TRIGGER parrot_tool_call_event_audit_trigger
            AFTER INSERT ON tool_call_events
            FOR EACH ROW EXECUTE FUNCTION parrot_mirror_tool_call_event();
    END IF;
END
$$;

-- Backfill the ledger for installations that already had tool-call history
-- before this migration. The invocation/action request is the stable
-- correlation key for a historical event; duplicate re-runs are harmless.
INSERT INTO tool_access_audit_events (
    company_id, connection_id, catalog_entry_id, actor_type, actor_id,
    action, outcome, reason_code, correlation_id, details, created_at
)
SELECT e.company_id, e.connection_id, e.catalog_entry_id,
       COALESCE(e.actor_type, 'system'), e.actor_id, e.event_type, e.outcome,
       e.reason_code, COALESCE(e.invocation_id::text, e.action_request_id::text),
       COALESCE(e.metadata, '{}'::jsonb)
         || jsonb_build_object('agentId', e.agent_id, 'runId', e.run_id,
                               'toolName', e.tool_name, 'decision', e.decision,
                               'argumentsSummary', e.arguments_summary,
                               'resultSummary', e.result_summary,
                               'errorCode', e.error_code,
                               'errorMessage', e.error_message),
       e.created_at
  FROM tool_call_events e
 WHERE NOT EXISTS (
       SELECT 1 FROM tool_access_audit_events a
        WHERE a.company_id = e.company_id
          AND a.action = e.event_type
          AND a.created_at = e.created_at
           AND a.correlation_id = COALESCE(e.invocation_id::text, e.action_request_id::text)
   );

INSERT INTO tool_runtime_metric_counters
    (company_id, metric, bucket_start_at, count, created_at, updated_at)
SELECT company_id,
       'tool_call.' || event_type,
       date_trunc('minute', created_at),
       COUNT(*)::integer,
       MIN(created_at),
       MAX(created_at)
  FROM tool_call_events
 GROUP BY company_id, event_type, date_trunc('minute', created_at)
ON CONFLICT (company_id, metric, bucket_start_at)
DO UPDATE SET count = tool_runtime_metric_counters.count + EXCLUDED.count,
              updated_at = EXCLUDED.updated_at;

CREATE TABLE IF NOT EXISTS company_transfer_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID REFERENCES companies(id) ON DELETE SET NULL,
    direction TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    actor_key TEXT NOT NULL,
    container_ref JSONB NOT NULL,
    idempotency_key TEXT NOT NULL,
    manifest_sha256 TEXT,
    manifest JSONB,
    chunk_count INTEGER NOT NULL DEFAULT 0,
    blob_count INTEGER NOT NULL DEFAULT 0,
    completed_parts JSONB NOT NULL DEFAULT '[]',
    error TEXT,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS company_transfer_runs_company_idx
    ON company_transfer_runs(company_id, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS company_transfer_runs_idempotency_direction_uq
    ON company_transfer_runs(idempotency_key, direction);
CREATE INDEX IF NOT EXISTS company_transfer_runs_actor_status_idx
    ON company_transfer_runs(actor_key, status);

CREATE TABLE IF NOT EXISTS document_annotation_anchor_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    thread_id UUID NOT NULL REFERENCES document_annotation_threads(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    from_revision_id UUID REFERENCES document_revisions(id) ON DELETE SET NULL,
    from_revision_number INTEGER,
    to_revision_id UUID REFERENCES document_revisions(id) ON DELETE SET NULL,
    to_revision_number INTEGER NOT NULL,
    previous_anchor JSONB NOT NULL,
    next_anchor JSONB,
    anchor_state TEXT NOT NULL,
    anchor_confidence TEXT NOT NULL,
    failure_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS document_annotation_anchor_snapshots_company_thread_idx
    ON document_annotation_anchor_snapshots(company_id, thread_id, created_at DESC);
CREATE INDEX IF NOT EXISTS document_annotation_anchor_snapshots_company_document_revision_idx
    ON document_annotation_anchor_snapshots(company_id, document_id, to_revision_number);

CREATE TABLE IF NOT EXISTS adapter_auth_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    adapter_type TEXT NOT NULL,
    started_by_user_id TEXT NOT NULL,
    provider_lease_id TEXT,
    status TEXT NOT NULL DEFAULT 'starting',
    expires_at TIMESTAMPTZ,
    promotion_expires_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    failure_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS adapter_auth_sessions_company_status_idx
    ON adapter_auth_sessions(company_id, status);
CREATE UNIQUE INDEX IF NOT EXISTS adapter_auth_sessions_company_adapter_active_uq
    ON adapter_auth_sessions(company_id, adapter_type)
    WHERE status IN ('starting', 'waiting_for_user', 'promoting');
CREATE INDEX IF NOT EXISTS adapter_auth_sessions_environment_idx
    ON adapter_auth_sessions(environment_id);
CREATE INDEX IF NOT EXISTS adapter_auth_sessions_expires_idx
    ON adapter_auth_sessions(expires_at);
CREATE INDEX IF NOT EXISTS adapter_auth_sessions_provider_lease_idx
    ON adapter_auth_sessions(provider_lease_id);

CREATE TABLE IF NOT EXISTS execution_workspace_runtime_leases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    execution_workspace_id UUID NOT NULL UNIQUE REFERENCES execution_workspaces(id) ON DELETE CASCADE,
    owner_key TEXT NOT NULL,
    owner_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    owner_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    owner_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    last_action TEXT NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    renewed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS execution_workspace_runtime_leases_company_workspace_idx
    ON execution_workspace_runtime_leases(company_id, execution_workspace_id);
CREATE INDEX IF NOT EXISTS execution_workspace_runtime_leases_company_owner_idx
    ON execution_workspace_runtime_leases(company_id, owner_key);
CREATE INDEX IF NOT EXISTS execution_workspace_runtime_leases_expires_at_idx
    ON execution_workspace_runtime_leases(expires_at);

CREATE TABLE IF NOT EXISTS document_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    starred_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, user_id, document_id)
);
CREATE INDEX IF NOT EXISTS document_memberships_company_user_starred_idx
    ON document_memberships(company_id, user_id, starred_at);

CREATE TABLE IF NOT EXISTS issue_reference_mentions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    source_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    target_issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_record_id UUID,
    document_key TEXT,
    matched_text TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS issue_reference_mentions_company_source_issue_idx
    ON issue_reference_mentions(company_id, source_issue_id);
CREATE INDEX IF NOT EXISTS issue_reference_mentions_company_target_issue_idx
    ON issue_reference_mentions(company_id, target_issue_id);
CREATE INDEX IF NOT EXISTS issue_reference_mentions_company_issue_pair_idx
    ON issue_reference_mentions(company_id, source_issue_id, target_issue_id);
CREATE UNIQUE INDEX IF NOT EXISTS issue_reference_mentions_source_record_uq
    ON issue_reference_mentions(company_id, source_issue_id, target_issue_id, source_kind, source_record_id)
    WHERE source_record_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS issue_reference_mentions_null_record_uq
    ON issue_reference_mentions(company_id, source_issue_id, target_issue_id, source_kind)
    WHERE source_record_id IS NULL;

CREATE TABLE IF NOT EXISTS issue_execution_decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    stage_id UUID NOT NULL,
    stage_type TEXT NOT NULL,
    actor_agent_id UUID REFERENCES agents(id),
    actor_user_id TEXT,
    outcome TEXT NOT NULL,
    body TEXT NOT NULL,
    created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS issue_execution_decisions_company_issue_idx
    ON issue_execution_decisions(company_id, issue_id);
CREATE INDEX IF NOT EXISTS issue_execution_decisions_stage_idx
    ON issue_execution_decisions(issue_id, stage_id, created_at);

CREATE TABLE IF NOT EXISTS company_onboarding_seeds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    revision TEXT NOT NULL,
    mission TEXT,
    agent_name TEXT,
    agent_role TEXT,
    first_task_title TEXT,
    first_task_details TEXT,
    goal_id UUID REFERENCES goals(id) ON DELETE SET NULL,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id)
);

CREATE TABLE IF NOT EXISTS feedback_exports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    feedback_vote_id UUID NOT NULL REFERENCES feedback_votes(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    author_user_id TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    vote TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'local_only',
    destination TEXT,
    export_id TEXT,
    consent_version TEXT,
    schema_version TEXT NOT NULL DEFAULT 'paperclip-feedback-envelope-v2',
    bundle_version TEXT NOT NULL DEFAULT 'paperclip-feedback-bundle-v2',
    payload_version TEXT NOT NULL DEFAULT 'paperclip-feedback-v1',
    payload_digest TEXT,
    payload_snapshot JSONB,
    target_summary JSONB NOT NULL,
    redaction_summary JSONB,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_attempted_at TIMESTAMPTZ,
    exported_at TIMESTAMPTZ,
    failure_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(feedback_vote_id)
);
CREATE INDEX IF NOT EXISTS feedback_exports_company_created_idx
    ON feedback_exports(company_id, created_at DESC);
CREATE INDEX IF NOT EXISTS feedback_exports_company_status_idx
    ON feedback_exports(company_id, status, created_at DESC);
CREATE INDEX IF NOT EXISTS feedback_exports_company_issue_idx
    ON feedback_exports(company_id, issue_id, created_at DESC);
CREATE INDEX IF NOT EXISTS feedback_exports_company_project_idx
    ON feedback_exports(company_id, project_id, created_at DESC);
CREATE INDEX IF NOT EXISTS feedback_exports_company_author_idx
    ON feedback_exports(company_id, author_user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS pipeline_case_blockers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    case_id UUID NOT NULL REFERENCES pipeline_cases(id) ON DELETE CASCADE,
    blocked_by_case_id UUID NOT NULL REFERENCES pipeline_cases(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(case_id, blocked_by_case_id),
    CHECK(case_id <> blocked_by_case_id)
);
CREATE INDEX IF NOT EXISTS pipeline_case_blockers_company_case_idx
    ON pipeline_case_blockers(company_id, case_id);
CREATE INDEX IF NOT EXISTS pipeline_case_blockers_blocked_by_idx
    ON pipeline_case_blockers(blocked_by_case_id);

CREATE TABLE IF NOT EXISTS pipeline_case_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    case_id UUID NOT NULL REFERENCES pipeline_cases(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, case_id, key),
    UNIQUE(company_id, case_id, document_id)
);
CREATE INDEX IF NOT EXISTS pipeline_case_documents_document_idx
    ON pipeline_case_documents(document_id);

CREATE TABLE IF NOT EXISTS pipeline_case_issue_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    case_id UUID NOT NULL REFERENCES pipeline_cases(id) ON DELETE CASCADE,
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'linked',
    created_by_run_id UUID REFERENCES heartbeat_runs(id) ON DELETE SET NULL,
    automation_attempt_id UUID,
    retired_at TIMESTAMPTZ,
    retired_by_attempt_id UUID,
    retired_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(case_id, issue_id)
);
CREATE INDEX IF NOT EXISTS pipeline_case_issue_links_company_case_idx
    ON pipeline_case_issue_links(company_id, case_id);
CREATE INDEX IF NOT EXISTS pipeline_case_issue_links_issue_idx
    ON pipeline_case_issue_links(issue_id);
CREATE INDEX IF NOT EXISTS pipeline_case_issue_links_role_idx
    ON pipeline_case_issue_links(case_id, role);

CREATE TABLE IF NOT EXISTS pipeline_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    pipeline_id UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, pipeline_id, key),
    UNIQUE(company_id, pipeline_id, document_id)
);
CREATE INDEX IF NOT EXISTS pipeline_documents_document_idx
    ON pipeline_documents(document_id);

CREATE TABLE IF NOT EXISTS pipeline_automation_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    case_id UUID REFERENCES pipeline_cases(id) ON DELETE CASCADE,
    automation_id TEXT NOT NULL,
    triggering_event_id UUID,
    routine_id UUID REFERENCES routines(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    execution_issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    retry_of_execution_id UUID REFERENCES pipeline_automation_executions(id) ON DELETE SET NULL,
    generation INTEGER NOT NULL DEFAULT 1,
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(case_id, automation_id, triggering_event_id),
    CHECK(status IN ('queued', 'running', 'succeeded', 'failed'))
);
CREATE INDEX IF NOT EXISTS pipeline_automation_executions_company_case_idx
    ON pipeline_automation_executions(company_id, case_id, created_at DESC);
CREATE INDEX IF NOT EXISTS pipeline_automation_executions_status_idx
    ON pipeline_automation_executions(company_id, status, created_at DESC);
