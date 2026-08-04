#!/usr/bin/env node

// Success-path checkout/release contract test.
// BASE_URL TOKEN ISSUE_ID RUN_ID node scripts/paperclip-mcp-checkout-release-smoke.mjs

const baseUrl = process.env.BASE_URL ?? "http://127.0.0.1:3102";
const token = process.env.TOKEN;
const issueId = process.env.ISSUE_ID;
const runId = process.env.RUN_ID;
if (!token || !issueId || !runId) throw new Error("TOKEN, ISSUE_ID and RUN_ID are required");

const headers = {
  authorization: `Bearer ${token}`,
  accept: "application/json",
  "content-type": "application/json",
};
let sequence = 1;
async function post(body, sessionId) {
  const requestHeaders = { ...headers };
  if (sessionId) requestHeaders["mcp-session-id"] = sessionId;
  const response = await fetch(`${baseUrl}/api/tool-gateway/mcp`, {
    method: "POST", headers: requestHeaders, body: JSON.stringify(body),
  });
  const value = await response.json();
  return { status: response.status, value, sessionId: response.headers.get("mcp-session-id") ?? sessionId };
}
async function call(name, arguments_, sessionId) {
  const response = await post({
    jsonrpc: "2.0", id: sequence++, method: "tools/call",
    params: { name, arguments: arguments_ },
  }, sessionId);
  if (response.status >= 400 || response.value.error) throw new Error(`${name} failed: ${JSON.stringify(response)}`);
  return response.value.result?.structuredContent;
}
async function restIssue() {
  const response = await fetch(`${baseUrl}/api/issues/${issueId}`, { headers: { accept: "application/json" } });
  return response.json();
}

const initialized = await post({
  jsonrpc: "2.0", id: sequence++, method: "initialize",
  params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "checkout-release-smoke", version: "1" } },
});
if (initialized.status !== 200 || !initialized.sessionId) throw new Error(`initialize failed: ${JSON.stringify(initialized)}`);
const sessionId = initialized.sessionId;

const checkout = await call("paperclipCheckoutIssue", { issueId, expectedStatuses: ["todo"] }, sessionId);
if (!checkout) throw new Error("checkout returned no value");
const checkedOut = await restIssue();
if (checkedOut.checkoutRunId !== runId || checkedOut.executionRunId !== runId || checkedOut.status !== "in_progress") {
  throw new Error(`checkout execution fields are wrong: ${JSON.stringify(checkedOut)}`);
}

const release = await call("paperclipReleaseIssue", {
  issueId, targetStatus: "done", result: "checkout/release smoke completed",
}, sessionId);
if (!release) throw new Error("release returned no value");
const released = await restIssue();
if (released.status !== "done" || released.checkoutRunId !== null || released.executionRunId !== null) {
  throw new Error(`release execution fields are wrong: ${JSON.stringify(released)}`);
}

console.log(JSON.stringify({ passed: true, issueId, runId, checkoutStatus: checkedOut.status, releaseStatus: released.status }));
