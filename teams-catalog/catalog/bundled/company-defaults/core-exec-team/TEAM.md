---
name: Core Exec Team
description: 新公司默认的最小可运转组织：CEO 负责优先级与派活，CTO 负责技术执行，QA 负责验收。
schema: agentcompanies/v1
slug: core-exec-team
category: company-defaults
key: parrot/bundled/company-defaults/core-exec-team
manager: agents/ceo/AGENTS.md
includes:
  - agents/cto/AGENTS.md
  - agents/qa/AGENTS.md
defaultInstall: true
recommendedForCompanyTypes:
  - startup
  - software
  - generalist
tags:
  - default
  - executive
  - engineering
  - qa
requiredSkills:
  - task-planning
  - issue-triage
  - qa-acceptance
---

# Core Exec Team

安装后会在公司内创建三个 Agent：CEO、CTO、QA，并按 `reportsTo` 建立汇报关系。
若安装时提供了 `targetManagerAgentId`，根节点（CEO）会挂到该 Agent 之下。

## 内容

- `CEO` — 战略、优先级、派活。
- `CTO` — 技术执行与工程质量，汇报给 CEO。
- `QA` — 验收与证据留存，汇报给 CTO。

## 安装语义

- 重名 Agent 按 `collisionStrategy`（`skip` / `rename` / `fail`）处理，默认 `skip`。
- 整个安装在单个数据库事务内完成，任一步失败整体回滚。
- 同一 company 重复安装默认幂等返回，`force=true` 才会重装。
