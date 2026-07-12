pub mod errors;
pub mod schemas;
pub mod routes;
pub mod redaction;
pub mod app_state;
pub mod validation;
pub mod extractors;
pub mod middleware;

pub use errors::AppError;
pub use validation::{CreateAgentHireSchema, UpdateAgentSchema, TestAdapterEnvironmentSchema};
pub use routes::{agent_routes, adapter_routes};

// 两个 AppState 是**相互独立、不混用**的类型：
// - `AgentAppState`：Agent/公司 路由使用的状态（见 `routes::agents::AppState`）
// - `FullAppState`：包含 Issue/Case 等 Phase 2 服务的状态（见 `app_state::AppState`）
pub use routes::agents::AppState as AgentAppState;
pub use app_state::AppState as FullAppState;
pub use app_state::create_router;
pub use validation::{
    CreateAgentHireSchema as CreateAgentHireInput,
    UpdateAgentSchema as UpdateAgentInput,
    TestAdapterEnvironmentSchema as TestAdapterEnvironmentInput,
};
pub use extractors::{
    AgentIdOrShortname,
    CompanyIdOrShortname,
    RevisionId,
    encode_shortname,
};
