# Endpoint Migration Backlog (Paperclip → Parrot, 全量)

自动生成：`scripts/gen_endpoint_backlog.py`。按用户决策：**by-design-candidate + missing 全部迁移**。

- 待迁移端点: **0**（by-design-candidate: 0，missing: 0）
- 实现每个端点时同步：handler + service + schema（如需）+ 权限 + activity log 事件（见 audit event 列）。

| # | Method | Path | Paperclip source | 状态 | 域 | 建议 audit event |
|---|---|---|---|---|---|---|