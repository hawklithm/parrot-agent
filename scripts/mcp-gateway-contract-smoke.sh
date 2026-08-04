#!/bin/sh
set -eu

# Black-box MCP contract smoke test.
# Usage: BASE_URL=http://127.0.0.1:3102 TOKEN=ptg_... ./scripts/mcp-gateway-contract-smoke.sh

base_url=${BASE_URL:-http://127.0.0.1:3102}
token=${TOKEN:?TOKEN is required}

post() {
  curl --noproxy '*' -sS -X POST "$base_url/api/tool-gateway/mcp" "$@"
}

status=$(curl --noproxy '*' -sS -o /dev/null -w '%{http_code}' \
  -X POST "$base_url/api/tool-gateway/mcp" \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}')
[ "$status" = 401 ] || { echo "expected unauthenticated initialize to return 401" >&2; exit 1; }

response_file=$(mktemp)
trap 'rm -f "$response_file"' EXIT
post -i \
  -H "authorization: Bearer $token" \
  -H 'accept: application/json' \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"contract-smoke","version":"1"}}}' >"$response_file"
session_id=$(sed -n 's/^[Mm][Cc][Pp]-[Ss]ession-[Ii][Dd]:[[:space:]]*\([^[:space:]]*\).*/\1/p' "$response_file" | tr -d '\r' | head -1)
[ -n "$session_id" ] || { echo 'initialize did not return a session id' >&2; exit 1; }

list=$(post \
  -H "authorization: Bearer $token" \
  -H "mcp-session-id: $session_id" \
  -H 'accept: application/json' \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}')
printf '%s' "$list" | grep -q 'paperclipGetIssue' || { echo 'tools/list did not expose Paperclip tools' >&2; exit 1; }

invalid=$(post \
  -H "authorization: Bearer $token" \
  -H "mcp-session-id: $session_id" \
  -H 'accept: application/json' \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"paperclipGetIssue","arguments":{}}}')
printf '%s' "$invalid" | grep -q 'invalid_tool_arguments' || { echo 'invalid tool arguments were not rejected' >&2; exit 1; }

unknown=$(post \
  -H "authorization: Bearer $token" \
  -H "mcp-session-id: $session_id" \
  -H 'accept: application/json' \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":4,"method":"not/a/method","params":{}}')
printf '%s' "$unknown" | grep -qi 'method not found' || { echo 'unknown method was not rejected' >&2; exit 1; }

notification_status=$(curl --noproxy '*' -sS -o /dev/null -w '%{http_code}' \
  -X POST "$base_url/api/tool-gateway/mcp" \
  -H "authorization: Bearer $token" \
  -H "mcp-session-id: $session_id" \
  -H 'accept: application/json' \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}')
[ "$notification_status" = 202 ] || { echo 'notification did not return 202' >&2; exit 1; }

sse_headers=$(curl --noproxy '*' -sS -D - -o /dev/null --max-time 2 \
  -H "authorization: Bearer $token" \
  -H "mcp-session-id: $session_id" \
  -H 'accept: text/event-stream' \
  "$base_url/api/tool-gateway/mcp" || true)
printf '%s' "$sse_headers" | grep -qi 'content-type: text/event-stream' || { echo 'SSE GET did not negotiate text/event-stream' >&2; exit 1; }

close_status=$(curl --noproxy '*' -sS -o /dev/null -w '%{http_code}' \
  -X DELETE "$base_url/api/tool-gateway/mcp" \
  -H "authorization: Bearer $token" \
  -H "mcp-session-id: $session_id")
[ "$close_status" = 200 ] || { echo "DELETE did not close the MCP session, got $close_status" >&2; exit 1; }

revoked_status=$(curl --noproxy '*' -sS -o /dev/null -w '%{http_code}' \
  -X POST "$base_url/api/tool-gateway/mcp" \
  -H "authorization: Bearer $token" \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":5,"method":"initialize","params":{}}')
[ "$revoked_status" = 401 ] || { echo "revoked token remained usable, got $revoked_status" >&2; exit 1; }

echo 'MCP gateway contract smoke test passed'
