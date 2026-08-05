#!/usr/bin/env node

// Negative-contract smoke test for the migrated Paperclip MCP gateway.
// Usage:
// BASE_URL=http://127.0.0.1:3102 TOKEN=ptg_... ISSUE_ID=... \
//   AGENT_ID=... node scripts/paperclip-mcp-security-smoke.mjs

const baseUrl = process.env.BASE_URL ?? "http://127.0.0.1:3102";
const token = process.env.TOKEN;
const issueId = process.env.ISSUE_ID;
const agentId = process.env.AGENT_ID;
const companyId = process.env.COMPANY_ID;
const runId = process.env.RUN_ID;
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

async function rest(path, method = "GET", body) {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  let value;
  try { value = JSON.parse(text); } catch { value = { raw: text }; }
  return { status: response.status, value };
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

let policy = "skipped";
let crossCompany = "skipped";
if (process.env.CROSS_COMPANY_ISSUE_ID) {
  const foreignIssue = await post({
    jsonrpc: "2.0", id: sequence++, method: "tools/call",
    params: { name: "paperclipGetIssue", arguments: { issueId: process.env.CROSS_COMPANY_ISSUE_ID } },
  }, sessionId);
  if (foreignIssue.status < 400 || !foreignIssue.value.error) throw new Error("cross-company issue was readable");
  const foreignList = await post({
    jsonrpc: "2.0", id: sequence++, method: "tools/call",
    params: { name: "paperclipListIssues", arguments: { companyId: process.env.CROSS_COMPANY_ID } },
  }, sessionId);
  if (foreignList.status >= 400 || foreignList.value.error) throw new Error(`companyId filter request failed unexpectedly: ${JSON.stringify(foreignList)}`);
  const listedIssues = foreignList.value.result?.structuredContent ?? (() => {
    const text = foreignList.value.result?.content?.find((item) => item.type === "text")?.text;
    try { return text === undefined ? undefined : JSON.parse(text); } catch { return text; }
  })();
  if (Array.isArray(listedIssues) && listedIssues.some((item) => item.id === process.env.CROSS_COMPANY_ISSUE_ID)) {
    throw new Error("cross-company issue appeared in session-scoped list");
  }
  crossCompany = "passed";
}
let crossRun = "skipped";
if (process.env.OTHER_RUN_ID) {
  if (!companyId) throw new Error("COMPANY_ID is required for cross-run test");
  const otherRunSession = await rest(`/api/tool-gateway/sessions`, "POST", {
    companyId, agentId, runId: process.env.OTHER_RUN_ID,
  });
  if (otherRunSession.status !== 400) throw new Error(`other agent/run session was accepted: ${JSON.stringify(otherRunSession)}`);
  crossRun = "passed";
}
if (process.env.TEST_POLICY === "1") {
  if (!companyId || !runId) throw new Error("COMPANY_ID and RUN_ID are required for policy tests");
  const deny = await rest(`/api/companies/${companyId}/tools/policies`, "POST", {
    name: `mcp-security-deny-${Date.now()}`,
    policyType: "deny",
    priority: 10000,
    selectors: { toolName: "paperclipAddComment" },
  });
  if (deny.status !== 201) throw new Error(`deny policy creation failed: ${JSON.stringify(deny)}`);
  const deniedCall = await post({
    jsonrpc: "2.0", id: sequence++, method: "tools/call",
    params: { name: "paperclipAddComment", arguments: { issueId, body: "must be denied" } },
  }, sessionId);
  if (deniedCall.status !== 403 || !deniedCall.value.error) throw new Error("deny policy did not block the tool");
  await rest(`/api/companies/${companyId}/tools/policies/${deny.value.id}`, "DELETE");

  const approval = await rest(`/api/companies/${companyId}/tools/policies`, "POST", {
    name: `mcp-security-approval-${Date.now()}`,
    policyType: "require_approval",
    priority: 10000,
    selectors: { toolName: "paperclipAddComment" },
  });
  if (approval.status !== 201) throw new Error(`approval policy creation failed: ${JSON.stringify(approval)}`);
  const approvalCall = await post({
    jsonrpc: "2.0", id: sequence++, method: "tools/call",
    params: { name: "paperclipAddComment", arguments: { issueId, body: "must await approval" } },
  }, sessionId);
  const approvalResult = approvalCall.value?.result?.structuredContent;
  const actionRequestId = approvalResult?.actionRequestId;
  if (approvalCall.status !== 200 || approvalResult?.decision !== "require_approval" || !actionRequestId) {
    throw new Error(`approval policy did not create action request: ${JSON.stringify(approvalCall)}`);
  }
  const declined = await rest(`/api/tool-gateway/action-requests/${actionRequestId}/decline`, "POST", { companyId });
  if (declined.status !== 200 || declined.value.status !== "declined") throw new Error(`approval decline failed: ${JSON.stringify(declined)}`);
  const decisions = await rest(`/api/companies/${companyId}/tools/runs/${runId}/decisions`);
  const declinedDecision = decisions.value?.decisions?.find((item) => item.invocation?.id === approvalResult.invocationId);
  if (declinedDecision?.invocation?.status !== "denied" || declinedDecision?.invocation?.errorCode !== "approval_declined") {
    throw new Error(`declined audit state is wrong: ${JSON.stringify(declinedDecision)}`);
  }
  await rest(`/api/companies/${companyId}/tools/policies/${approval.value.id}`, "DELETE");
  policy = "passed";
}

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

console.log(JSON.stringify({ passed: true, crossAgentCheckout: "passed", unsafeApiPath: "passed", mismatchedSession: "passed", invalidToken: "passed", expiredToken: expired, crossCompany, crossRun, policy }));
