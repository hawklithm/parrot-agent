# Paperclip MCP Gateway 运行手册

## 本地启动

```sh
export DATABASE_URL=postgres://postgres:admin123@localhost:5432/parrot_agent_dev
export PORT=3102
export PAPERCLIP_API_URL=http://127.0.0.1:3102/api
cargo run -p parrot-server
```

启动日志必须出现 `running migrations` 和 `listening on http://0.0.0.0:3102`。服务会对旧数据库执行幂等迁移，不需要删库重建。

## MCP smoke test

脚本需要一个仍处于 `queued` 或 `running` 的 heartbeat run。服务内部 adapter 会自动创建 token；手工测试也可以在 run 活跃期间创建 session：

```sh
curl -X POST http://127.0.0.1:3102/api/tool-gateway/sessions \
  -H 'content-type: application/json' \
  --data '{"companyId":"<company>","agentId":"<agent>","runId":"<run>"}'
```

只把返回的 token 临时放入当前 shell，然后执行：

```sh
BASE_URL=http://127.0.0.1:3102 TOKEN='<ptg_token>' \
  sh scripts/mcp-gateway-contract-smoke.sh
```

脚本覆盖无 token、initialize、tools/list、invalid params、未知 method、notification、SSE GET、DELETE 撤销和撤销后拒绝。token 不要写入日志、Issue、shell history 或提交。

## Claude/Codex adapter

- `claude_local` 使用 stdin prompt，并注入 `/api/tool-gateway/mcp` 与 `PAPERCLIP_TOOL_GATEWAY_TOKEN`。
- `codex_local` 使用 `codex exec`，通过 `-c mcp_servers.paperclip.url=...` 和 `-c mcp_servers.paperclip.bearer_token_env_var=...` 注入同一网关。
- 真实 CLI 的模型名必须是本机账号支持的模型；不支持的模型会在 MCP 之前直接失败。
- 日志中的 shell command 会保留完整参数结构，但 Bearer token 已替换为 `[PAPERCLIP_TOOL_GATEWAY_TOKEN]`。

## 常见故障

- `Company not found`：先确认 onboarding 的 `GET /api/companies` 已返回公司，再创建 Agent/Issue；agent-hire 不会替代公司初始化。
- `Invalid API key` 或 `password_hash` 缺失：确认旧库已经执行 auth 兼容迁移，并重启服务。
- `Tool gateway session is expired or revoked`：run 已结束或 session 被 DELETE/adapter cleanup 撤销，需要创建新的活动 run。
- Codex 报不支持模型：使用 `codex doctor` 查看默认模型，并在 Agent `adapterConfig.model` 中配置该模型。
- MCP 工具调用失败：先查看 `heartbeat_runs.stdout_excerpt`、`tool_invocations` 和 `tool_call_events`；不要把 token 或完整敏感 prompt 贴到普通日志。

## 验证命令

```sh
cargo check -p parrot-server
cargo test -p api --lib
git diff --check
```

`cargo test --workspace --no-fail-fast` 还可能暴露仓库中与 MCP 无关的既存 adapter/consistency 测试失败，应单独记录，不能用删表或跳过迁移掩盖。
