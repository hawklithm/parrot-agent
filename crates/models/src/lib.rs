pub mod activity_log;
pub mod adapter;
pub mod agent;
pub mod agent_api_key;
pub mod approval;
pub mod assets;
pub mod auth;
pub mod budget;
pub mod case;
pub mod company;
pub mod cost_event;
pub mod custom_image_setup;
pub mod environment;
pub mod environment_diagnostics;
pub mod error;
pub mod event_bus;
pub mod events;
pub mod execution_environment;
pub mod finance_event;
pub mod goal;
pub mod invite;
pub mod invite_resource;
pub mod issue;
pub mod issue_auxiliary;
pub mod issue_comment;
pub mod issue_tree_control;
pub mod label;
pub mod openclaw;
pub mod org_chart;
pub mod pipeline;
pub mod plugin;
pub mod project;
pub mod realtime_environment;
pub mod routine;
pub mod routine_annotation;
pub mod saga;
pub mod secret_provider;
pub mod secret_provider_config;
pub mod secret_remote_import;
pub mod secrets;
pub mod skill;
pub mod sse;
pub mod state_machine;
pub mod task_watchdog;
pub mod user_directory;
pub mod user_secret;
pub mod user_secret_definition;
pub mod websocket;

// ===== 显式导出：消除 glob re-export 歧义 =====
// 规则：显式导出优先于 glob 导出，放在 glob 之前即可消除歧义
// 统一使用 execution_environment 版本（repositories 使用此版本）
pub use approval::Approval;
pub use auth::{MembershipRole, PrincipalType};
pub use case::{CaseEvent, CreateCaseInput};
pub use environment::{
    Environment, EnvironmentLease, ExecutionWorkspaceMode, ExecutionWorkspaceStatus,
    ExecutionWorkspaceStrategyType, LeaseStatus, LocalEnvironmentConfig, SandboxEnvironmentConfig,
    SshEnvironmentConfig,
};
pub use environment_diagnostics::EnvironmentDeleteBlastRadius;
pub use event_bus::{
    AgentEvent, ApprovalEvent, EnvironmentEvent, Event, EventBus, EventHandler, GoalEvent,
    IssueEvent, RoutineEvent,
};
pub use execution_environment::{
    CreateEnvironmentInput, CreateExecutionWorkspaceInput, CreateRuntimeLeaseInput,
    EnvironmentCapabilities, EnvironmentDriver, EnvironmentLeaseCleanupStatus,
    EnvironmentLeasePolicy, EnvironmentLeaseStatus, EnvironmentProbeResult, EnvironmentStatus,
    ExecutionEnvironment, ExecutionWorkspace, RuntimeLease, UpdateEnvironmentInput,
    UpdateExecutionWorkspaceInput, UpdateRuntimeLeaseInput, WorkspaceMode, WorkspaceStatus,
    WorkspaceStrategyType,
};
pub use secret_provider::{ProviderHealthStatus, SecretProviderConfig};
pub use secrets::{SecretBinding, UserSecret, UserSecretDefinition};
pub use state_machine::AgentStateMachine;

// ===== glob re-export（非冲突类型继续使用） =====
pub use activity_log::*;
pub use adapter::*;
pub use agent::*;
pub use agent_api_key::*;
pub use approval::*;
pub use assets::*;
pub use auth::*;
pub use case::*;
pub use company::*;
pub use cost_event::*;
pub use custom_image_setup::*;
pub use environment_diagnostics::*;
pub use error::*;
pub use event_bus::*;
pub use events::*;
pub use goal::*;
pub use invite::*;
pub use invite_resource::*;
pub use issue::*;
pub use issue_auxiliary::*;
pub use issue_comment::*;
pub use issue_tree_control::*;
pub use label::*;
pub use openclaw::*;
pub use org_chart::*;
pub use pipeline::*;
pub use plugin::*;
pub use project::*;
pub use realtime_environment::*;
pub use routine::*;
pub use routine_annotation::*;
pub use saga::*;
pub use secret_provider::*;
pub use secret_provider_config::*;
pub use secret_remote_import::*;
pub use secrets::*;
pub use skill::*;
pub use sse::*;
pub use state_machine::*;
pub use user_directory::*;
pub use user_secret::*;
pub use user_secret_definition::*;
pub use websocket::*;
