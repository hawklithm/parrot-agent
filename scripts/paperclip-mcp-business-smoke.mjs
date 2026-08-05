#!/usr/bin/env node

// End-to-end Paperclip builtin-tool smoke test. It deliberately uses only the
// MCP gateway, so it tests the same contract used by Claude/Codex.
// Usage: BASE_URL=http://127.0.0.1:3102 TOKEN=ptg_... node scripts/paperclip-mcp-business-smoke.mjs

const baseUrl = process.env.BASE_URL ?? "http://127.0.0.1:3102";
const token = process.env.TOKEN;
if (!token) throw new Error("TOKEN is required");

const headers = {
  authorization: `Bearer ${token}`,
  accept: "application/json",
  "content-type": "application/json",
};

async function post(body, sessionId) {
  const requestHeaders = { ...headers };
  if (sessionId) requestHeaders["mcp-session-id"] = sessionId;
  const response = await fetch(`${baseUrl}/api/tool-gateway/mcp`, {
    method: "POST",
    headers: requestHeaders,
    body: JSON.stringify(body),
  });
  const text = await response.text();
  let value;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error(`MCP returned non-JSON HTTP ${response.status}`);
  }
  if (!response.ok) throw new Error(`MCP HTTP ${response.status}: ${JSON.stringify(value)}`);
  return { value, sessionId: response.headers.get("mcp-session-id") ?? sessionId };
}

let sequence = 1;
async function call(name, arguments_, sessionId) {
  const result = await post({
    jsonrpc: "2.0",
    id: sequence++,
    method: "tools/call",
    params: { name, arguments: arguments_ },
  }, sessionId);
  if (result.value.error) throw new Error(`${name}: ${JSON.stringify(result.value.error)}`);
  if (result.value.result?.isError) throw new Error(`${name}: tool returned isError`);
  const resultValue = result.value.result;
  if (resultValue?.structuredContent !== undefined) return resultValue.structuredContent;
  const text = resultValue?.content?.find((item) => item.type === "text")?.text;
  try { return text === undefined ? undefined : JSON.parse(text); } catch { return text; }
}

const initialized = await post({
  jsonrpc: "2.0",
  id: sequence++,
  method: "initialize",
  params: {
    protocolVersion: "2025-03-26",
    capabilities: {},
    clientInfo: { name: "paperclip-business-smoke", version: "1" },
  },
});
const sessionId = initialized.sessionId;
if (!sessionId) throw new Error("initialize did not return Mcp-Session-Id");

const listed = await post({
  jsonrpc: "2.0",
  id: sequence++,
  method: "tools/list",
  params: {},
}, sessionId);
const tools = listed.value.result?.tools ?? [];
const builtinTools = tools.filter((tool) => tool.source === "paperclip_builtin");
if (builtinTools.length !== 41) throw new Error(`expected 41 Paperclip tools, got ${builtinTools.length}`);

const created = await call("paperclipCreateIssue", {
  title: "MCP business contract smoke",
  description: "Created entirely through the migrated Paperclip MCP gateway.",
  status: "todo",
  priority: "medium",
}, sessionId);
const issueId = created?.id;
if (!issueId) throw new Error(`create issue returned no id: ${JSON.stringify(created)}`);

const fetched = await call("paperclipGetIssue", { issueId }, sessionId);
if (fetched?.id !== issueId) throw new Error("get issue did not return the created issue");

const updated = await call("paperclipUpdateIssue", {
  issueId,
  title: "MCP business contract smoke updated",
  status: "todo",
}, sessionId);
if (updated?.id !== issueId) throw new Error("update issue did not return the created issue");

const comment = await call("paperclipAddComment", {
  issueId,
  body: "MCP business smoke comment",
  presentation: { kind: "message", tone: "info" },
  metadata: {
    version: 1,
    sections: [{ rows: [{ type: "text", text: "written through MCP" }] }],
  },
}, sessionId);
const commentId = comment?.comment?.id ?? comment?.id;
if (!commentId) throw new Error(`add comment returned no id: ${JSON.stringify(comment)}`);

const comments = await call("paperclipListComments", { issueId }, sessionId);
if (!Array.isArray(comments) || !comments.some((item) => item.id === commentId)) {
  throw new Error("list comments did not include the created comment");
}
const fetchedComment = await call("paperclipGetComment", { issueId, commentId }, sessionId);
if ((fetchedComment?.comment?.id ?? fetchedComment?.id) !== commentId) throw new Error("get comment mismatch");

const document = await call("paperclipUpsertIssueDocument", {
  issueId,
  key: "mcp-smoke",
  title: "MCP smoke document",
  format: "markdown",
  body: "# MCP smoke\n\nCreated through Paperclip MCP.",
  changeSummary: "business contract smoke",
}, sessionId);
if (!document) throw new Error("upsert document returned no value");
const documents = await call("paperclipListDocuments", { issueId }, sessionId);
if (!Array.isArray(documents) || !documents.some((item) => item.key === "mcp-smoke")) {
  throw new Error("list documents did not include the created document");
}
const fetchedDocument = await call("paperclipGetDocument", { issueId, key: "mcp-smoke" }, sessionId);
if (!fetchedDocument) throw new Error("get document returned no value");
const revisions = await call("paperclipListDocumentRevisions", { issueId, key: "mcp-smoke" }, sessionId);
if (!Array.isArray(revisions) || revisions.length < 1) throw new Error("document revision was not created");
const revisionId = revisions[0]?.id ?? revisions[0]?.revisionId;
if (!revisionId) throw new Error("document revision has no id");
await call("paperclipRestoreIssueDocumentRevision", { issueId, key: "mcp-smoke", revisionId }, sessionId);

const suggest = await call("paperclipSuggestTasks", {
  issueId,
  continuationPolicy: "none",
  payload: { version: 1, tasks: [{ clientKey: "smoke-task", title: "MCP suggested task" }] },
}, sessionId);
if (!suggest) throw new Error("suggest tasks returned no value");
const questions = await call("paperclipAskUserQuestions", {
  issueId,
  continuationPolicy: "none",
  payload: { version: 1, questions: [{ id: "smoke-choice", prompt: "Choose one", selectionMode: "single", options: [{ id: "yes", label: "Yes" }] }] },
}, sessionId);
if (!questions) throw new Error("ask user questions returned no value");
const confirmation = await call("paperclipRequestConfirmation", {
  issueId,
  continuationPolicy: "none",
  payload: { version: 1, prompt: "Confirm MCP smoke" },
}, sessionId);
if (!confirmation) throw new Error("request confirmation returned no value");
const checkbox = await call("paperclipRequestCheckboxConfirmation", {
  issueId,
  continuationPolicy: "none",
  payload: { version: 1, prompt: "Select MCP smoke options", options: [{ id: "one", label: "One" }] },
}, sessionId);
if (!checkbox) throw new Error("request checkbox confirmation returned no value");

const approval = await call("paperclipCreateApproval", {
  type: "hire_agent",
  payload: { reason: "MCP business contract smoke" },
  issueIds: [issueId],
}, sessionId);
const approvalId = approval?.id ?? approval?.approval?.id;
if (!approvalId) throw new Error(`create approval returned no id: ${JSON.stringify(approval)}`);
const listedApprovals = await call("paperclipListApprovals", {}, sessionId);
if (!Array.isArray(listedApprovals) || !listedApprovals.some((item) => item.id === approvalId)) throw new Error("approval was not listed");
const fetchedApproval = await call("paperclipGetApproval", { approvalId }, sessionId);
if ((fetchedApproval?.id ?? fetchedApproval?.approval?.id) !== approvalId) throw new Error("get approval mismatch");
const approvalIssues = await call("paperclipGetApprovalIssues", { approvalId }, sessionId);
if (!Array.isArray(approvalIssues) || !approvalIssues.some((item) => (item.issueId ?? item.id) === issueId)) throw new Error("approval issue link missing");
await call("paperclipAddApprovalComment", { approvalId, body: "Approval comment through MCP" }, sessionId);
const approvalComments = await call("paperclipListApprovalComments", { approvalId }, sessionId);
if (!Array.isArray(approvalComments) || approvalComments.length < 1) throw new Error("approval comment was not listed");

const context = await call("paperclipGetHeartbeatContext", { issueId }, sessionId);
if (!context) throw new Error("heartbeat context returned no value");
const runtime = await call("paperclipGetIssueWorkspaceRuntime", { issueId }, sessionId);
if (!runtime) throw new Error("workspace runtime returned no value");

const escaped = await call("paperclipApiRequest", {
  method: "GET",
  path: `/issues/${issueId}`,
}, sessionId);
if (escaped?.id !== issueId) throw new Error("paperclipApiRequest returned the wrong issue");

console.log(JSON.stringify({
  passed: true,
  builtinToolCount: builtinTools.length,
  issueId,
  commentId,
  documentKey: "mcp-smoke",
  revisionCount: revisions.length,
  approvalId,
}));
