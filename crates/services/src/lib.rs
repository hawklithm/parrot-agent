pub mod access;
pub mod activity_log;
pub mod adapter_config_normalizer;
pub mod adapter_registry;
pub mod builtin_adapter_types;
pub mod adapter_plugin_store;
pub mod adapter_registry_state;
pub mod server_adapter;
pub mod adapters;
pub mod agent_service;
pub mod config_revision_service;
pub mod config_revision_service_impl;
pub mod adapter_executor;
pub mod environment_runtime_service;
pub mod errors;
pub mod saga;
pub mod secret_provider;
pub mod secret_service;
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
pub mod attachment_service;
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
    EnvironmentHealthProber, ConsistencyCheckJob
};
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
pub use server_adapter::{AdapterRegistry, ServerAdapterModule};
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
