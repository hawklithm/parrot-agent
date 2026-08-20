pub mod access;
pub mod activity_log;
pub mod adapter_config_normalizer;
pub mod adapter_registry;
pub mod builtin_adapter_types;
pub mod adapter_plugin_store;
pub mod adapter_registry_state;
pub mod npm_manager;
pub mod adapter_package_loader;
pub mod adapter_install_lock;
pub mod adapter_install_transaction;
pub mod server_adapter;
pub mod adapters;
pub mod plugin_worker_manager;
pub use plugin_worker_manager::PluginWorkerManager;
pub mod plugin_runtime_sandbox;
pub mod plugin_job_scheduler;
pub use plugin_job_scheduler::{PluginJobScheduler, PluginJobSchedulerOptions};
pub mod plugin_job_coordinator;
pub mod plugin_tool_registry;
pub mod plugin_capability_validator;
pub mod execution_timeout;
pub mod resource_monitor;
pub mod plugin_managed_resources;
pub mod tool_gateway;
pub mod tool_access_policy;
pub mod tool_access_control;
pub mod tool_runtime_supervisor;
pub mod agent_permissions_service;
pub mod agent_invokability_service;
pub mod agent_assignability_service;
pub mod agent_action_audit_service;
pub mod agent_secret_bindings_service;
pub mod agent_start_lock_service;
pub mod workspace_runtime_service;

pub mod workspace_realization_service;
pub mod session_workspace_cwd;
pub mod attention_service;
pub mod dashboard_service;
pub mod feedback_service;
pub mod recovery_observability_service;
pub mod decision_service;
pub mod decision_queue_service;
pub mod decision_retention_service;
pub mod decision_signing_service;
pub mod decision_training_service;
pub mod decision_wakeup_service;
pub mod cross_issue_influence_limit_service;
pub mod project_workspace_runtime_config_service;
pub mod tool_runtime_metrics_service;
pub mod database_backup_health_service;
pub mod document_service;
pub mod document_annotation_service;
pub mod issue_relation_service;
pub mod status_card_service;
pub mod status_card_finalization_service;
pub mod webhook_service;
pub mod summary_slot_service;
pub mod mcp_http_service;
pub mod remote_http_endpoint_guard;
pub mod tool_access_audit_service;
pub mod tool_profile_binding_precedence;
pub mod rate_limit_service;
pub mod audit_log_service;
pub mod metric_collector_service;
pub mod successful_run_handoff_state;
pub mod issue_blast_radius;
pub mod smoke_lab;
pub mod catalog_provenance;
pub mod portable_path;
pub mod managed_resource_drift;
pub mod runtime_skill_selections;
pub mod default_agent_instructions;
pub mod environment_execution_target;
pub mod issue_dependency_service;
pub mod issue_liveness_service;
pub mod run_continuations_service;
pub mod run_liveness_service;
pub mod issue_goal_fallback_service;
pub mod issue_rewake_throttle_service;
pub mod run_scratch_service;
pub mod tool_oauth_service;
pub mod agent_instructions_service;
pub mod plugin_ipc_protocol;
pub mod execution_allowlist_service;
pub mod execution_workspace_policy_service;
pub mod principal_access_compatibility_service;
pub mod source_trust_service;
pub mod routable_blocked_service;
pub mod agent_service;
pub mod plugin_resource_limiter;
pub mod config_revision_service;
pub mod trust_preset_resolver_service;
pub mod low_trust_runtime_containment_service;
pub mod change_consent_gate_service;
pub mod config_revision_service_impl;
pub mod adapter_executor;
pub mod environment_runtime_service;
pub mod routine_coordinator_service;
pub mod company_artifacts_service;
pub mod company_member_roles_service;
pub mod environment_probe_service;
pub mod workspace_instance_cleanup_service;
pub mod invite_grants_service;
pub mod invite_rate_limit_service;
pub mod pipeline_case_outputs_service;
pub mod github_external_object_provider_service;
pub mod workspace_operation_log_store_service;
pub mod workspace_runtime_read_model_service;
pub mod live_events_service;
pub mod git_credentials_service;
pub mod plan_review_context_service;
pub mod stalled_review_decisions_service;
pub mod company_search_service;
pub mod productivity_review_service;
pub mod github_fetch_service;
pub mod github_pull_request_merge_service;
pub mod sidebar_badges_service;
pub mod hot_restart_service;
pub mod managed_config_service;
pub mod built_in_agent_metadata;
pub mod built_in_agents;
pub mod email_service;
pub mod plugin_dev_watcher;
pub mod plugin_log_retention_service;
pub mod plugin_install_guard_service;
pub mod errors;
pub mod saga;
pub mod secret_provider;
pub mod secret_service;
pub mod environment_custom_image_runtime_service;
pub mod environment_custom_images_service;
pub mod environment_run_orchestrator_service;
pub mod sse_service;
pub mod websocket_service;
pub use sse_service::{InMemorySseService, SseService};
pub mod access_service;
pub mod asset_service;
pub mod approval_execution;
pub mod agent_hire_hook;
pub use approval_execution::{
    ApprovalExecutionResult, ApprovalExecutor, DefaultApprovalExecutor, HireAgentPayload,
};
pub use agent_hire_hook::{
    AdapterHireHook, HireApprovedPayload, HireHookResult, NotifyHireApprovedInput,
    notify_hire_approved,
};
pub mod auth;
pub mod authorization_service;
pub mod built_in_agent_service;
pub mod built_in_agent_service_impl;
pub mod case_service;
pub mod environment_driver;
pub mod file_resource_service;
pub mod issue_comment_service;
pub mod issue_plan_decomposition_service;
pub mod issue_thread_interaction_service;
pub mod issue_workspace_validation;
pub mod issue_service;
pub mod issue_tree_control_service;
pub mod lease_service;
pub mod mock_environment_services;
pub mod org_chart_service;
pub mod org_chart_service_impl;
pub mod routine_service;
pub mod secret_provider_service;
pub mod user_secret_service;
pub mod workspace_operation_service;
pub mod workspace_service;
pub use routine_service::{RoutineService, RoutineServiceImpl};
pub mod event_listeners;
pub mod goal_service;
pub mod plugin_tool_dispatcher_enhanced;
pub mod jwt_service;
pub mod routine_service_impl;
pub mod session_service;
pub use goal_service::{
    CreateGoalInput, DefaultGoalService, GoalHierarchy, GoalService, UpdateGoalInput,
};
pub mod pipeline_service;
pub use pipeline_service::{
    AdvanceCaseInput, BulkReviewResult, CaseReviewDecision, CaseReviewInput, CreateCaseInput,
    DefaultPipelineService, HealthWarning, PipelineService,
};

pub mod approval_service;
pub use approval_service::{ApprovalService, DefaultApprovalService};
pub mod activity_log_service;
pub mod asset_storage;
pub mod attachment_service;
pub mod attachment_types;
pub mod codex_local_isolation;
pub mod environment_diagnostics_service;
pub mod environment_service;
pub mod issue_checkout_service;
pub mod issue_comment_service_impl;
pub mod issue_service_complete;
pub mod routine_trigger_service;
pub mod secret_provider_config_service;
pub mod secret_remote_import_service;
pub mod skill_registry_service;
pub mod skill_registry_service_impl;
pub mod work_product_service;

pub use environment_diagnostics_service::{
    EnvironmentDiagnosticsService, MockEnvironmentDiagnosticsService,
};
pub use secret_provider_config_service::{
    DefaultSecretProviderConfigServiceImpl, MockSecretProviderConfigService,
    SecretProviderConfigService,
};
pub use secret_remote_import_service::{MockSecretRemoteImportService, SecretRemoteImportService};
pub use skill_registry_service::{MockSkillRegistryService, SkillRegistryService};
pub use skill_registry_service_impl::DefaultSkillRegistryServiceImpl;
pub mod skill_policy_service;
pub use skill_policy_service::{
    DefaultSkillPolicyService, DenialType, PolicyDecision, SkillPolicyError, SkillPolicyService,
    SkillPolicyResult,
};
pub mod teams_catalog_service;
pub use teams_catalog_service::{
    CatalogTeam, CatalogTeamAgent, DefaultTeamsCatalogService, InstallActor, TeamsCatalogError,
    TeamsCatalogResult, TeamsCatalogService,
};

pub use agent_service::{
    AgentService, CreateAgentInput, DefaultAgentService, NormalizedAgentRow, ServiceError,
    UpdateAgentInput,
};
pub use built_in_agent_service::{
    BuiltInAgentBundleDefinition, BuiltInAgentDefinition, BuiltInAgentKey,
    BuiltInAgentMetadataRegistry, BuiltInAgentStatus,
};
pub use built_in_agent_service_impl::{
    BuiltInAgentError, BuiltInAgentResult, BuiltInAgentService, DefaultBuiltInAgentService,
    ProvisionInput, ReconcileResult,
};
pub use cross_issue_influence_limit_service::{
    CrossIssueInfluenceKind, CrossIssueInfluenceLimitService,
    DefaultCrossIssueInfluenceLimitService, InfluenceLimitError,
    ObserveCrossIssueInfluenceInput,
};
pub use case_service::CaseService;
pub use case_service::MockCaseService;
pub use config_revision_service::{
    ConfigChange, ConfigDiff, ConfigRevisionError, ConfigRevisionResult, ConfigRevisionService,
    ConfigSnapshot,
};
pub use config_revision_service_impl::ConfigRevisionServiceImpl;
pub use environment_runtime_service::{
    DefaultEnvironmentRuntimeService, EnvironmentLease, EnvironmentRuntimeError,
    EnvironmentRuntimeService, ExecutionTargetResult, LeaseStatus, WorkspaceRealizationResult,
};
pub use errors::ServiceResult;
pub use issue_comment_service::{
    CommentServiceError, CommentServiceResult, IssueCommentService, IssueCommentServiceImpl,
};
pub use issue_service::IssueService;
pub use issue_service_complete::{DefaultIssueService, LegacyIssueService};
pub use issue_tree_control_service::{
    IssueTreeControlService, IssueTreeControlServiceImpl, TreeControlServiceError,
    TreeControlServiceResult,
};
pub use issue_thread_interaction_service::{
    IssueThreadInteractionService, InteractionCreator, InteractionResolver,
};
pub use mock_environment_services::{
    MockEnvironmentLeaseService, MockEnvironmentService, MockExecutionWorkspaceService,
};
pub use models::OrgNode;
pub use org_chart_service::{get_role_label, OrgChartError, OrgChartService, ROLE_LABELS};
pub use org_chart_service_impl::DefaultOrgChartService;
pub use secret_service::{
    DefaultSecretService, EnvBinding, ResolvedAdapterConfig, RuntimeSecretManifestEntry,
    SecretReference, SecretResolutionOutcome, SecretService, SecretServiceError,
};
pub mod custom_image_setup_service;
pub mod invite_resource_service;
pub mod invite_service;
pub mod openclaw_service;
pub mod skills_service;
pub mod user_directory_service;
pub mod user_secret_definition_service;
pub use invite_resource_service::*;
pub mod routine_annotation_service;
pub use custom_image_setup_service::CustomImageSetupService;
pub use invite_service::{InviteService, InviteServiceImpl};
pub use openclaw_service::OpenClawService;
pub use org_chart_service::*;
pub use routine_annotation_service::*;
pub use user_directory_service::UserDirectoryService;
pub use user_secret_definition_service::UserSecretDefinitionService;
pub mod issue_repository;
pub use issue_repository::*;
pub use issue_service::*;
pub mod issue_service_mock;
pub use case_service::*;
pub use issue_service_mock::*;

// Shared repository double used by service unit tests.  Several older tests
// referenced this from the crate root, but the mock was removed when the
// repository trait gained company-scoped method parameters.
#[cfg(test)]
pub struct MockIssueRepository;

#[cfg(test)]
impl MockIssueRepository {
    pub fn new() -> Self { Self }
}

#[cfg(test)]
#[async_trait::async_trait]
impl repositories::IssueRepository for MockIssueRepository {
    async fn get_by_id(&self, _id: uuid::Uuid) -> Result<Option<models::Issue>, repositories::RepositoryError> { unimplemented!() }
    async fn list_by_company(&self, _company_id: uuid::Uuid, _filter: &models::IssueQueryFilter, _pagination: &models::Pagination) -> Result<Vec<models::Issue>, repositories::RepositoryError> { Ok(Vec::new()) }
    async fn count_by_company(&self, _company_id: uuid::Uuid, _filter: &models::IssueQueryFilter) -> Result<i64, repositories::RepositoryError> { Ok(0) }
    async fn create(&self, _input: models::CreateIssueInput) -> Result<models::Issue, repositories::RepositoryError> { unimplemented!() }
    async fn update(&self, _id: uuid::Uuid, _input: models::UpdateIssueInput) -> Result<models::Issue, repositories::RepositoryError> { unimplemented!() }
    async fn delete(&self, _id: uuid::Uuid) -> Result<(), repositories::RepositoryError> { Ok(()) }
    async fn search(&self, _company_id: uuid::Uuid, _query: &str, _pagination: &models::Pagination) -> Result<Vec<models::Issue>, repositories::RepositoryError> { Ok(Vec::new()) }
    async fn get_by_identifier(&self, _identifier: &str) -> Result<Option<models::Issue>, repositories::RepositoryError> { Ok(None) }
    async fn list_by_parent(&self, _parent_id: uuid::Uuid, _pagination: &models::Pagination) -> Result<Vec<models::Issue>, repositories::RepositoryError> { Ok(Vec::new()) }
    async fn get_by_ids(&self, _ids: Vec<uuid::Uuid>) -> Result<Vec<models::Issue>, repositories::RepositoryError> { Ok(Vec::new()) }
    async fn list_ancestors(&self, _issue_id: uuid::Uuid) -> Result<Vec<models::Issue>, repositories::RepositoryError> { Ok(Vec::new()) }
}
pub mod comment_service;
pub use comment_service::*;
pub mod tree_control_service;
pub use attachment_service::AttachmentService;
pub use attachment_service::*;
pub use environment_driver::*;
pub use environment_service::*;
pub use lease_service::*;
pub use tree_control_service::*;
pub use user_secret_service::{UserSecretService, UserSecretServiceImpl};
pub use work_product_service::WorkProductService;
pub use work_product_service::*;
pub mod company_service;
pub use company_service::*;
pub mod resource_membership_service;
pub use resource_membership_service::ResourceMembershipService;
pub mod project_service;
pub use project_service::ProjectService;
pub mod authorization_service_complete;
pub mod invite_service_complete;
pub use invite_service_complete::*;
pub mod event_bus_service;
pub use event_bus_service::InMemoryEventBus;
pub mod saga_orchestrator;
pub use saga_orchestrator::*;
pub mod consistency_service;
pub use consistency_service::*;
pub mod agent_access_service;
pub mod recovery_action_service;
pub use recovery_action_service::*;
pub mod monitor_scheduler;
pub use monitor_scheduler::*;
pub mod plan_decomposition_service;
pub use plan_decomposition_service::*;
pub mod issue_diagnostics_service;
pub use issue_diagnostics_service::*;
pub mod heartbeat_service;
pub mod issue_assignment_wakeup;
pub use issue_assignment_wakeup::*;
pub mod low_trust_service;
pub use heartbeat_service::*;
pub mod task_watchdog;
pub use task_watchdog::{
    classify_subtree, ClassifierInput, ClassifierState, DefaultWatchdogService, StoppedLeaf,
    WatchdogService,
};
pub mod label_service;
pub mod term_service;
pub use term_service::*;
pub mod instance_settings_service;
pub use instance_settings_service::{
    AutoRecoveryPreview, AutoRecoveryResult, DatabaseBackupResult, DefaultInstanceSettingsService,
    ExperimentalSettings, GeneralSettings, InstanceSettings, InstanceSettingsService,
};
pub mod cost_service;
pub use cost_service::{
    BudgetEnforcementScope, BudgetIncidentDto, BudgetIncidentResolveInput, BudgetOverview,
    BudgetPolicy, BudgetPolicySummary, BudgetService, CostEventDto, CostService, CostSummaryDto,
    CostSummaryWithBudget, CreateCostEventInput, CreateFinanceEventInput, DefaultBudgetService,
    DefaultCostService, DefaultFinanceService, FinanceEventDto, FinanceService, FinanceSummaryDto,
    FinanceSummaryRowDto, QuotaWindow, UpsertPolicyInput, WindowSpend, WindowSpendEntry,
};
pub mod text_utils;
pub use text_utils::*;
pub mod retry;
pub use retry::*;
pub mod routine_execution_service;
pub use routine_execution_service::{RoutineExecutionService, DispatchRoutineRunInput, RoutineRun, RoutineRunSource};
pub mod routine_variable_service;
pub use routine_variable_service::{
    RoutineVariableValue, get_builtin_routine_variable_values, 
    resolve_routine_variable_values, ResolveVariableInput,
    assert_routine_variable_definitions, sanitize_routine_variable_inputs,
};
pub mod routine_template;
pub use routine_template::{
    extract_routine_variable_names,
    interpolate_routine_template,
    sync_routine_variables_with_template,
    is_valid_routine_variable_name,
    is_routine_date_variable_name,
    unescape_routine_variable_name,
};
pub mod job_scheduler;
pub use job_scheduler::{
    JobScheduler, ScheduledJob, JobSchedule, JobStatus, JobExecutionRecord,
    RoutineCronTrigger, MonitorCheckJob, LeaseExpiryScanner,
    EnvironmentHealthProber, StuckRunDetector, ConsistencyCheckJob,
    StatusCardSchedulerJob, SummarySlotFinalizerJob,
    HeartbeatRecoveryJob,
    monitor_backoff_seconds, is_env_stale, is_run_stuck, ENV_IDLE_TIMEOUT,
};
pub mod status_card_worker;
pub use status_card_worker::{
    StatusCardWorker, StatusCardDeltaChange, StatusCardFingerprint, FingerprintEntry,
    build_status_card_fingerprint, diff_status_card_fingerprint,
    extract_issue_mentions, filter_status_card_changes, next_status_card_evaluation_at,
    status_card_changes_hash, status_card_fingerprint_hash,
    choose_status_card_update_kind, ChooseUpdateKindInput, evaluate_status_card_policy,
    EvaluatePolicyInput, is_within_status_card_active_hours, GenerationEnqueue,
    SUMMARIZER_BUILT_IN_KEY, TERMINAL_ISSUE_STATUSES, STALLED_GENERATION_STATUSES,
    STATUS_CARD_MAX_MENTIONED_ISSUES,
};
pub mod summary_slot_worker;
pub use summary_slot_worker::{SummarySlotWorker, SummarySlotGeneration};
pub mod config;
pub use config::*;
pub mod adapter_plugin;
pub mod plugin_service;
pub mod plugin_loader;
pub mod plugin_lifecycle;
pub mod plugin_tool_dispatcher;
pub mod plugin_config_validator;
pub mod cloud_upstream_service;
pub mod work_timeline_service;
pub use cloud_upstream_service::{CloudUpstreamService, DefaultCloudUpstreamService};
pub use work_timeline_service::{DefaultWorkTimelineService, WorkTimelineQuery, WorkTimelineService};
pub use plugin_service::{DefaultPluginService, PluginService, PluginServiceError};
pub mod company_portability_service;
pub use company_portability_service::{
    ExportService, ImportService, InboxService, DefaultCompanyPortabilityService,
};
pub use adapter_plugin::{
    resolve_model_profile_application, AdapterInstallRequest, AdapterModelProfileDefinition,
    AdapterPluginError, AdapterPluginLoader, AdapterPluginRecord, AdapterPluginResult,
    AdapterPluginStore, AdapterSkillEntry, AppliedModelProfileConfigSource,
    DefaultAdapterPluginLoader, ModelProfileApplication, ModelProfileKey,
    ModelProfileRequestSource,
};
pub use adapter_registry::create_default_adapter_registry;
pub use server_adapter::{AdapterRegistry, ServerAdapterModule, create_default_server_adapter_registry};
pub use adapter_registry_state::AdapterRegistryState;
pub use adapter_executor::*;
pub mod issue_execution_lock_service;
pub use adapters::*;
pub use issue_execution_lock_service::*;
pub use label_service::*;
pub use low_trust_service::*;
// 注意: activity_log_service 和 access 都定义了 ResourceType，
// 使用通配符导入会导致 ambiguous_glob_reexports 警告。
// 改为精确导入以消除歧义。
pub use access::abac::{
    AccessDecision, Action, Actor, AgentActor, AgentPermissions, AuthorizationPolicy, TrustPreset,
    UserActor,
};
pub use access::access_service::{
    AccessError, AccessService, DefaultAccessService, ResourceContext, ResourceType,
};
