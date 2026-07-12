# Models Crate Glob Re-export 冲突分析与修复方案

## 一、问题概述

`crates/models/src/lib.rs` 使用了 44 行 `pub use xxx::*;` 全局 glob re-export。
多个模块定义了同名类型，导致 `models::TypeName` 解析到**错误的类型**，
在 `services` 和 `repositories` crate 中引发大量编译错误（当前约 179 个）。

Rust 编译器对 glob re-export 的规则：**后导出的覆盖先导出的**。
`lib.rs` 第 57 行 `pub use environment::*;` 先于第 62 行 `pub use execution_environment::*;`
先于第 75 行 `pub use realtime_environment::*;`，因此 `realtime_environment` 的类型会"胜出"，
但实际代码需要的是 `execution_environment` 的类型。

---

## 二、冲突类型全览（49 个冲突类型名，9 组冲突）

### 组 1：environment / execution_environment / realtime_environment（14 个冲突类型）

这是最严重的冲突组，三个模块定义了大量同名类型。

| 类型名 | environment.rs | execution_environment.rs | realtime_environment.rs | 规范模块 | 说明 |
|--------|:-:|:-:|:-:|---------|------|
| `EnvironmentDriver` | enum (Local/Ssh/Sandbox/Plugin) | enum (同左) | enum (同左) | `execution_environment` | 三个定义相同，但 `Hash`/`Copy` 只加在了 `environment.rs` |
| `EnvironmentStatus` | enum | enum | enum | `execution_environment` | |
| `LeaseStatus` | enum | — | enum | `environment` | execution_environment 用 `EnvironmentLeaseStatus` |
| `LocalEnvironmentConfig` | struct | — | struct (空) | `environment` | realtime 版本是空结构体 |
| `SshEnvironmentConfig` | struct | — | struct | `environment` | |
| `SandboxEnvironmentConfig` | struct | — | struct | `environment` | |
| `Environment` | struct | — | struct | `environment` | execution_environment 用 `ExecutionEnvironment` |
| `EnvironmentLease` | struct | — | struct | `environment` | execution_environment 用 `RuntimeLease` |
| `ExecutionWorkspace` | struct | struct (有方法) | struct | `execution_environment` | 规范版有 `is_ready()`/`is_running()` 方法 |
| `CreateEnvironmentInput` | struct | struct | — | `execution_environment` | repositories 使用此版 |
| `UpdateEnvironmentInput` | struct | struct | — | `execution_environment` | repositories 使用此版 |
| `WorkspaceMode` | — | enum (Ephemeral/Persistent) | enum (同左) | `execution_environment` | |
| `WorkspaceStatus` | — | enum (Provisioning/Ready/Running/Teardown/Error/Archived) | enum (不同变体) | `execution_environment` | |
| `EnvironmentLeasePolicy` | — | enum | — | `execution_environment` | 唯一定义 |
| `EnvironmentLeaseStatus` | — | enum | — | `execution_environment` | 唯一定义 |
| `EnvironmentLeaseCleanupStatus` | — | enum | — | `execution_environment` | 唯一定义 |
| `EnvironmentProbeResult` | — | struct | — | `execution_environment` | 唯一定义 |
| `EnvironmentDeleteBlastRadius` | — | struct | — | `execution_environment` | 唯一定义 |

**冲突影响**：`environment_service.rs`、`environment_driver/` 目录下所有文件、`workspace_service.rs`、`lease_service.rs`、`issue_checkout_service.rs` 等。

### 组 2：agent / approval（1 个冲突类型）

| 类型名 | agent.rs | approval.rs | 规范模块 | 说明 |
|--------|----------|-------------|---------|------|
| `Approval` | struct (agent 审批) | struct (approval 工作流) | `approval` | 两个完全不同的业务概念 |

**冲突影响**：`approval_service.rs` 全文 19 处 `Approval` 引用歧义。

### 组 3：agent / state_machine（1 个冲突类型）

| 类型名 | agent.rs | state_machine.rs | 规范模块 | 说明 |
|--------|----------|-------------------|---------|------|
| `AgentStateMachine` | struct | struct (有 impl) | `state_machine` | state_machine 版有完整状态转换逻辑 |

### 组 4：auth / company（2 个冲突类型）

| 类型名 | auth.rs | company.rs | 规范模块 | 说明 |
|--------|---------|------------|---------|------|
| `MembershipRole` | enum | enum | `auth` | |
| `PrincipalType` | enum | enum | `auth` | |

**冲突影响**：`authorization_service_complete.rs` 的 `MembershipRole` 无法作为 HashMap key（`Hash` trait 加在了错误版本上）。

### 组 5：case / pipeline（2 个冲突类型）

| 类型名 | case.rs | pipeline.rs | 规范模块 | 说明 |
|--------|---------|-------------|---------|------|
| `CaseEvent` | struct (case 事件) | struct (pipeline 事件) | 各自模块 | 两个不同的业务概念，需共存 |
| `CreateCaseInput` | struct | struct | 各自模块 | 同上 |

**冲突影响**：`case_service.rs`、`pipeline_service.rs`。

### 组 6：event_bus / events（9+ 个冲突类型）

| 类型名 | event_bus.rs | events.rs | 规范模块 | 说明 |
|--------|-------------|-----------|---------|------|
| `Event` | trait (有 `as_any()`) | trait (无 `as_any()`) | `event_bus` | event_bus 版新增了 `as_any()` |
| `EventHandler` | trait | trait | `event_bus` | |
| `EventBus` | trait | trait | `event_bus` | |
| `IssueEvent` | struct | struct | `event_bus` | |
| `ApprovalEvent` | struct | struct | `event_bus` | |
| `RoutineEvent` | struct | struct | `event_bus` | |
| `AgentEvent` | struct | struct | `event_bus` | |
| `EnvironmentEvent` | struct | struct | `event_bus` | |
| `GoalEvent` | struct | struct | `event_bus` | |

**冲突影响**：`event_bus_service.rs`、`event_listeners.rs` 等所有事件相关代码。

### 组 7：secrets / user_secret / user_secret_definition（3 个冲突类型）

| 类型名 | secrets.rs | user_secret.rs | user_secret_definition.rs | 规范模块 | 说明 |
|--------|-----------|----------------|--------------------------|---------|------|
| `UserSecretDefinition` | struct (14 字段) | struct (5 字段) | struct (10 字段) | `secrets` | secrets 版字段最全 |
| `UserSecret` | struct | struct | — | `secrets` | |
| `SecretBinding` | struct | struct | struct | `secrets` | |

**冲突影响**：`user_secret_service.rs`、`user_secret_definition_service.rs`、`secret_provider_service.rs`。

### 组 8：issue_document / issue_auxiliary / issue（5 个冲突类型）

| 类型名 | issue_document.rs | issue_auxiliary.rs | issue.rs | 规范模块 | 说明 |
|--------|-------------------|-------------------|----------|---------|------|
| `LockDocumentInput` | struct | — | struct | `issue_document` | |
| `Attachment` | struct | struct | — | `issue_document` | |
| `CreateWorkProductInput` | struct | struct | — | `issue_document` | |
| `UpdateWorkProductInput` | struct | struct | — | `issue_document` | |
| `AnnotationThreadStatus` | enum | — | — | `issue_document` | 唯一定义（无冲突） |

### 组 9：其他零散冲突

| 类型名 | 模块 A | 模块 B | 规范模块 |
|--------|--------|--------|---------|
| `ProviderHealthStatus` | `secret_provider.rs` | `secret_provider_config.rs` | `secret_provider_config` |
| `SecretProviderConfig` | `secret_provider.rs` | `secret_provider_config.rs` | `secret_provider_config` |
| `ResourceType` | `auth.rs` | `invite_resource.rs` | `auth` |
| `TrustPreset` | `auth.rs` | `invite_resource.rs` | `auth` |
| `AccessDecision` | `auth.rs` | `invite_resource.rs` | `auth` |
| `Actor` / `AgentActor` | `auth.rs` | `invite_resource.rs` | `auth` |
| `AgentPermissions` | `auth.rs` | `invite_resource.rs` | `auth` |

---

## 三、解决方案

### 方案选择：修改 `lib.rs` 显式导出 + 保留 glob 作为兜底

**核心思路**：
1. 在 `lib.rs` 中，对冲突类型使用**显式命名导出**（`pub use module::Type;`）覆盖 glob 的歧义
2. Rust 规则：**显式导出优先于 glob 导出**，因此显式导出可以消除歧义
3. 不需要修改任何 services/repositories 文件 — 它们继续用 `models::TypeName` 即可
4. 非冲突类型继续用 glob 导出，无需逐一列举

**为什么选这个方案**：
- 改动量最小（只改 `lib.rs` 一个文件）
- 不影响下游 crate 的 import 方式
- 显式导出在 glob 之前生效，编译器不会报 ambiguous 警告

### 具体实施

#### 第 1 步：修改 `crates/models/src/lib.rs`

在 glob re-export **之前**插入显式导出行：

```rust
// ===== 显式导出：消除 glob re-export 歧义 =====
// 规则：显式导出优先于 glob 导出，放在 glob 之前即可消除歧义

// --- 组 1: environment 三模块冲突 ---
// execution_environment 为规范模块（repositories 使用此版本）
pub use execution_environment::{
    EnvironmentDriver, EnvironmentStatus,
    ExecutionEnvironment,
    EnvironmentLeaseStatus, EnvironmentLeaseCleanupStatus, EnvironmentLeasePolicy,
    RuntimeLease, CreateRuntimeLeaseInput, UpdateRuntimeLeaseInput,
    EnvironmentProbeResult, EnvironmentCapabilities, EnvironmentDeleteBlastRadius,
    CreateEnvironmentInput, UpdateEnvironmentInput,
    WorkspaceMode, WorkspaceStrategyType, WorkspaceStatus,
    ExecutionWorkspace, CreateExecutionWorkspaceInput, UpdateExecutionWorkspaceInput,
};
// environment 为规范模块（LeaseStatus, *Config, Environment, EnvironmentLease 仅此处有）
pub use environment::{
    LeaseStatus,
    LocalEnvironmentConfig, SshEnvironmentConfig, SandboxEnvironmentConfig,
    Environment, EnvironmentLease,
    ExecutionWorkspaceMode, ExecutionWorkspaceStrategyType, ExecutionWorkspaceStatus,
};

// --- 组 2: agent / approval 冲突 ---
pub use approval::Approval;

// --- 组 3: agent / state_machine 冲突 ---
pub use state_machine::AgentStateMachine;

// --- 组 4: auth / company 冲突 ---
pub use auth::{MembershipRole, PrincipalType};

// --- 组 5: case / pipeline 冲突 ---
// 两者都需要，但 glob 会导致歧义。显式导出 case 版本作为默认，
// pipeline 版本需通过 models::pipeline::CaseEvent 访问
pub use case::{CaseEvent, CreateCaseInput};

// --- 组 6: event_bus / events 冲突 ---
// event_bus 为规范模块（有 as_any() 方法）
pub use event_bus::{
    Event, EventHandler, EventBus,
    IssueEvent, ApprovalEvent, RoutineEvent, AgentEvent, EnvironmentEvent, GoalEvent,
};

// --- 组 7: secrets / user_secret / user_secret_definition 冲突 ---
// secrets 为规范模块（字段最全）
pub use secrets::{UserSecretDefinition, UserSecret, SecretBinding};

// --- 组 8: issue_document / issue_auxiliary / issue 冲突 ---
pub use issue_document::{
    LockDocumentInput, Attachment, CreateWorkProductInput, UpdateWorkProductInput,
};

// --- 组 9: 其他零散冲突 ---
pub use secret_provider_config::{ProviderHealthStatus, SecretProviderConfig};
pub use auth::{ResourceType, TrustPreset, AccessDecision, Actor, AgentActor, AgentPermissions};

// ===== glob re-export（非冲突类型继续使用） =====
pub use activity_log::*;
pub use adapter::*;
// ... 其余 glob 不变 ...
```

#### 第 2 步：处理 `Hash` / `Copy` trait 位置问题

当前 `EnvironmentDriver` 的 `Hash` + `Copy` 加在了 `environment.rs` 版本上，
但显式导出后 `models::EnvironmentDriver` 指向 `execution_environment.rs` 版本。

**修复**：在 `execution_environment.rs` 的 `EnvironmentDriver` 上也添加 `Hash, Copy`：

```rust
// crates/models/src/execution_environment.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum EnvironmentDriver {
    Local,
    Ssh,
    Sandbox,
    Plugin,
}
```

同时移除 `environment.rs` 版本上多余的 `Hash`/`Copy`（如果该版本不再被使用）。

#### 第 3 步：处理 case / pipeline 的 CaseEvent 共存问题

`case::CaseEvent` 和 `pipeline::CaseEvent` 是两个不同的业务概念，都需要存在。
显式导出 `case::CaseEvent` 作为默认 `models::CaseEvent` 后：

- `case_service.rs` 继续用 `models::CaseEvent`（解析到 `case::CaseEvent`）✅
- `pipeline_service.rs` 需要改用 `models::pipeline::CaseEvent`（已在前一步修复）✅

#### 第 4 步：验证和后续修复

修改 `lib.rs` 后，重新运行 `cargo check -p models` 确认无 ambiguous 警告。
然后运行 `cargo check --workspace` 查看剩余错误。
预计 glob 冲突修复后，约 **60-80 个错误** 会自动消除（所有因类型解析错误导致的 E0308）。

剩余错误将是真正的模型字段不匹配和方法签名变更，需要逐文件修复。

---

## 四、风险评估

| 风险 | 等级 | 说明 |
|------|------|------|
| 显式导出改变了某些类型的解析结果 | **中** | 某些代码可能依赖了"错误"的类型版本（如 `realtime_environment::WorkspaceStatus`），切换后可能引入新的类型不匹配 |
| `Hash`/`Copy` trait 迁移 | **低** | 只需确保规范版本有正确的 derive |
| case/pipeline 共存 | **低** | 只需 `pipeline_service.rs` 使用全限定路径 |
| 遗漏冲突类型 | **低** | 编译器会报告剩余的 ambiguous 警告，可逐步补充 |

### 回滚方案

如果显式导出引入过多新错误，可以：
1. 只对 **组 1**（environment 三模块冲突）做显式导出，这是影响最大的部分
2. 其他组保持 glob，在 services 中逐个文件改用全限定路径导入

---

## 五、实施清单

- [ ] 1. 修改 `crates/models/src/lib.rs`：在 glob 之前插入显式导出
- [ ] 2. 在 `execution_environment.rs` 的 `EnvironmentDriver` 上添加 `Hash, Copy` derive
- [ ] 3. 运行 `cargo check -p models` 确认无 ambiguous 警告
- [ ] 4. 运行 `cargo check -p repositories` 确认 repositories 编译通过
- [ ] 5. 运行 `cargo check -p services` 查看剩余错误
- [ ] 6. 逐文件修复 services 中剩余的类型不匹配错误
- [ ] 7. 运行 `cargo check --workspace` 确认全量编译通过
