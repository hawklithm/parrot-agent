# Paperclip Rust 后端实现任务拆解

> 基于已生成的后端架构分析文档，拆解为 Rust 版本实现任务。每个任务不超过 3 个改动点，使用 checkbox 格式便于标记完成状态。
> **已补充缺口**: 参见 `coverage-gaps.md` 了解本次更新添加的 43 个缺失功能

---

## 📁 文档目录

| 模块 | 文件 | 任务数 | 阶段数 | 覆盖率 |
|------|------|--------|--------|--------|
| Agent 管理 | [`agent-management-tasks.md`](agent-management-tasks.md) | 59 | 3 | ✅ 100% (已补全) |
| Issue/Case 管理 | [`issue-case-management-tasks.md`](issue-case-management-tasks.md) | 52 | 3 | ✅ 100% (已补全) |
| 认证授权 | [`auth-authorization-tasks.md`](auth-authorization-tasks.md) | 59 | 3 | ✅ 100% |
| Pipeline/Adapter | [`pipeline-adapter-tasks.md`](pipeline-adapter-tasks.md) | 40+ | 3 | ✅ 100% |
| Company/Org 组织 | [`company-org-tasks.md`](company-org-tasks.md) | 53 | 6 | ✅ 100% |
| Routine/Goal 自动化 | [`routine-goal-tasks.md`](routine-goal-tasks.md) | 81 | 3 | ✅ 100% (已补全) |
| 实时通信与执行环境 | [`realtime-environment-tasks.md`](realtime-environment-tasks.md) | 91 | 3 | ✅ 100% (已补全) |
| **跨模块集成** | [`cross-module-integration-tasks.md`](cross-module-integration-tasks.md) | 90 | 3 | 🆕 新增 |
| **缺口分析** | [`coverage-gaps.md`](coverage-gaps.md) | 析报告 |
| **逻辑链路分析** | [`logic-chain-gap-analysis.md`](logic-chain-gap-analysis.md) | - | - | 📊 深度分析 |

---

## 📊 统计摘要

- **模块总数**: 8 (新增跨模块集成层)
- **任务总数**: 525+ (从 350 增加到 525)
- **阶段划分**: 3-6 阶段/模块
- **任务粒度**: 每任务 ≤3 个改动点
- **本次补充**: 
  - 43 个端点缺失 (已补充)
  - 37 个逻辑链路缺失 (已补充)
  - 90 个跨模块集成任务 (已补充)

---

## 🆕 本次更新 (2026/07/11)

### 第一轮：补充端点缺失 (43项)

1. **实时通信与执行环境模块** (+26 项)
   - User Secret Definitions 系统（11 个端点）
   - Secret Provider Configuration 系统（9 个端点）
   - 环境诊断端点（probe, acquire, blast-radius）
   - Secret Remote Import（2 个端点）
   - Custom Image Session 管理（2 个端点）

2. **认证授权模块** (+11 项)
   - Skills 系统（3 个端点）
   - Invite 子资源（5 个端点）
   - OpenClaw 端点
   - 用户目录查询
   - 实例管理员用户列表

3. **Routine/Goal 模块** (+4 项)
   - Routine 文档注释系统（4 个端点）

4. **Company/Org 模块** (+2 项)
   - 组织架构图生成（SVG）
   - 公司统计详细逻辑

### 第二轮：补充逻辑链路缺失 (37项)

**P0 - 关键数据完整性** (4项)
- Issue Checkout → Environment Lease 原子性
- Approval批准 → Issue解除阻塞事务保证
- Agent执行触发机制定义
- 工作空间创建触发器

**P1 - 后台调度器** (9项)
- Monitor定时检查调度器
- 租约过期/僵尸租约清理器
- 环境健康探测调度器
- 工作空间空闲回收器
- Routine Cron触发器
- 成本聚合/事件清理/Secret轮换器

**P2 - 跨模块集成合约** (15项)
- SessionManagement/EnvironmentRuntime/SkillService
- CostEventService/ApprovalService
- ActivityLog/Budget/Heartbeat统一接口
- 等服务接口定义

**P3 - 状态机完整性** (9项)
- Issue/Agent/Case/BuiltInAgent状态机
- RoutineRun/Goal/Approval/Environment/Lease状态机

### 第三轮：新增跨模块集成层 (90项)

**核心基础设施**:
1. 统一活动日志服务 (~15任务)
2. 事件总线（EventBus）(~15任务)
3. Saga编排器 (~20任务)
4. 状态漂移检测作业 (~15任务)
5. 全局错误恢复策略 (~15任务)
6. 后台调度器统一管理 (~10任务)

详见:
- [`coverage-gaps.md`](coverage-gaps.md) - 端点缺失分析
- [`logic-chain-gap-analysis.md`](logic-chain-gap-analysis.md) - 逻辑链路深度分析

---

## 🏗️ 实现依赖顺序

```
阶段一（基础架构）
├── 数据模型层（枚举 + 结构体）
├── 适配器模式层（trait + registry）
└── 权限与访问控制层（trait + 基础断言）
            │
            ▼
阶段二（核心业务）
├── Agent CRUD 服务层
├── Issue/Case CRUD 服务层
├── Company/Project Service
├── Routine/Goal 服务层
└── 环境与工作空间服务
            │
            ▼
阶段三（高级特性）
├── 配置版本控制
├── 内置 Agent 资源协调
├── 实时通信（SSE/WebSocket）
└── 预算与成本统计
            │
            ▼
阶段四（可选：高级组织特性）
├── Company Skills 全生命周期
├── Activity Log
├── 导入导出
└── Cloud Upstreams PKCE OAuth
            │
            ▼
阶段五（API 路由层）
└── 全部 HTTP 端点实现
            │
            ▼
阶段六（集成与测试）
├── 认证中间件
├── 路由组装
└── 集成测试
```

---

## 🔧 Rust 技术选型建议

| 领域 | 推荐选型 |
|------|----------|
| Web 框架 | `axum` 0.7+ |
| 数据库 | `sqlx` 0.7+ (async PostgreSQL) |
| ORM | `sea-orm` 或 `sqlx` |
| 请求验证 | `garde` 或 `validator` |
| JWT | `jsonwebtoken` |
| 加密 | `sha2`, `hmac`, `subtle` |
| 错误处理 | `thiserror` + `anyhow` |
| 序列化 | `serde` + `serde_json` |
| 异步运行时 | `tokio` |
| 测试 | `tokio::test` + `testcontainers` |

---

## 📋 任务格式示例

```markdown
### 阶段一：基础架构

- [x] **任务名称**
  - 改动点1
  - 改动点2
  - 改动点3

### 阶段二：核心功能
...
```

---

*Generated on 2026/07/11*
*基于 architecture/backend/ 下的 7 个架构分析文档*