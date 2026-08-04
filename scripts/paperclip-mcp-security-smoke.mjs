#!/usr/bin/env node

// Negative-contract smoke test for the migrated Paperclip MCP gateway.
// Usage:
// BASE_URL=http://127.0.0.1:3102 TOKEN=ptg_... ISSUE_ID=... \
//   AGENT_ID=... node scripts/paperclip-mcp-security-smoke.mjs

const baseUrl = process.env.BASE_URL ?? "http://127.0.0.1:3102";
const token = process.env.TOKEN;
const issueId = process.env.ISSUE_ID;
const agentId = process.env.AGENT_ID;
const otherAgentId = process.env.OTHER_AGENT_ID ?? "ffb971ab-7bb0-4b14-8112-bbb038cefb2e";
if (!token || !issueId || !agentId) throw new Error("TOKEN, ISSUE_ID and AGENT_ID are required");

const headers = {
  authorization: `Bearer ${token}`,
  accept: "application/json",
  "content-type": "application/json",
};

async function post(body, sessionId, bearer = token) {
  const requestHeaders = { ...headers, authorization: `Bearer ${bearer}` };
  if (sessionId) requestHeaders["mcp-session-id"] = sessionId;
  const response = await fetch(`${baseUrl}/api/tool-gateway/mcp`, {
    method: "POST", headers: requestHeaders, body: JSON.stringify(body),
  });
  const text = await response.text();
  let value;
  try { value = JSON.parse(text); } catch { value = { raw: text }; }
  return { status: response.status, value, sessionId: response.headers.get("mcp-session-id") ?? sessionId };
}

let sequence = 1;
const initialize = await post({
  jsonrpc: "2.0", id: sequence++, method: "initialize",
  params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "security-smoke", version: "1" } },
});
if (initialize.status !== 200 || !initialize.sessionId) throw new Error(`initialize failed: ${JSON.stringify(initialize)}`);
const sessionId = initialize.sessionId;

const wrongAgent = await post({
  jsonrpc: "2.0", id: sequence++, method: "tools/call",
  params: { name: "paperclipCheckoutIssue", arguments: { issueId, agentId: otherAgentId } },
}, sessionId);
if (wrongAgent.status < 400 || !wrongAgent.value.error) throw new Error("cross-agent checkout was not rejected");

const invalidPath = await post({
  jsonrpc: "2.0", id: sequence++, method: "tools/call",
  params: { name: "paperclipApiRequest", arguments: { method: "GET", path: "http://evil.invalid/api" } },
}, sessionId);
if (invalidPath.status < 400 || !invalidPath.value.error) throw new Error("unsafe API path was not rejected");

const wrongSession = await post({
  jsonrpc: "2.0", id: sequence++, method: "tools/list", params: {},
}, "00000000-0000-0000-0000-000000000000");
if (wrongSession.status < 400 || !wrongSession.value.error) throw new Error("mismatched MCP session id was not rejected");

const invalidToken = await post({
  jsonrpc: "2.0", id: 99, method: "initialize",
  params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "security-smoke", version: "1" } },
}, undefined, `${token}-invalid`);
if (invalidToken.status !== 401 || !invalidToken.value.error) throw new Error("invalid token was not rejected");

let expired = "skipped";
if (process.env.TEST_EXPIRY === "1") {
  const created = await fetch(`${baseUrl}/api/tool-gateway/sessions`, {
    method: "POST", headers,
    body: JSON.stringify({ companyId: process.env.COMPANY_ID, agentId, runId: process.env.RUN_ID, ttlMs: 60000 }),
  });
  const session = await created.json();
  if (!created.ok || !session.token) throw new Error(`short-lived session creation failed: ${JSON.stringify(session)}`);
  await new Promise((resolve) => setTimeout(resolve, 61000));
  const expiredResponse = await post({
    jsonrpc: "2.0", id: 100, method: "initialize",
    params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "security-smoke", version: "1" } },
  }, undefined, session.token);
  if (expiredResponse.status !== 401) throw new Error(`expired token was accepted: ${JSON.stringify(expiredResponse)}`);
  expired = "passed";
}

console.log(JSON.stringify({ passed: true, crossAgentCheckout: "passed", unsafeApiPath: "passed", mismatchedSession: "passed", invalidToken: "passed", expiredToken: expired }));
