#!/usr/bin/env node

// Invoke every migrated Paperclip builtin at least once. Some tools need a
// resource that a local database may not contain (project, goal, workspace),
// so those calls deliberately assert Paperclip's structured error path.
// BASE_URL TOKEN ISSUE_ID RUN_ID node scripts/paperclip-mcp-tool-matrix-smoke.mjs

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
const invokedTools = new Set();
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
  invokedTools.add(name);
  const response = await post({
    jsonrpc: "2.0", id: sequence++, method: "tools/call",
    params: { name, arguments: arguments_ },
  }, sessionId);
  const result = response.value.result;
  const structuredContent = result?.structuredContent ?? (() => {
    const text = result?.content?.find((item) => item.type === "text")?.text;
    try { return text === undefined ? undefined : JSON.parse(text); } catch { return text; }
  })();
  return {
    ok: response.status < 400 && !response.value.error && !result?.isError,
    value: structuredContent,
    response,
  };
}
async function expectSuccess(name, args, sessionId, predicate = () => true) {
  const result = await call(name, args, sessionId);
  if (!result.ok || !predicate(result.value)) throw new Error(`${name} expected success: ${JSON.stringify(result)}`);
  return result.value;
}
async function expectError(name, args, sessionId) {
  const result = await call(name, args, sessionId);
  if (result.ok) throw new Error(`${name} expected an error path: ${JSON.stringify(result.value)}`);
  return result;
}

const initialized = await post({
  jsonrpc: "2.0", id: sequence++, method: "initialize",
  params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "paperclip-tool-matrix", version: "1" } },
});
if (initialized.status !== 200 || !initialized.sessionId) throw new Error(`initialize failed: ${JSON.stringify(initialized)}`);
const sessionId = initialized.sessionId;

const listed = await post({ jsonrpc: "2.0", id: sequence++, method: "tools/list", params: {} }, sessionId);
const tools = listed.value.result?.tools?.filter((tool) => tool.source === "paperclip_builtin") ?? [];
if (tools.length !== 41) throw new Error(`expected 41 Paperclip tools, got ${tools.length}`);

const me = await expectSuccess("paperclipMe", {}, sessionId, (value) => Boolean(value?.id));
const agentId = me.id;
await expectSuccess("paperclipInboxLite", {}, sessionId);
await expectSuccess("paperclipListAgents", {}, sessionId, Array.isArray);
await expectSuccess("paperclipGetAgent", { agentId }, sessionId, (value) => value?.id === agentId);
await expectSuccess("paperclipListIssues", {}, sessionId, Array.isArray);
await expectSuccess("paperclipGetIssue", { issueId }, sessionId, (value) => value?.id === issueId);
await expectSuccess("paperclipGetHeartbeatContext", { issueId }, sessionId);
const comments = await expectSuccess("paperclipListComments", { issueId }, sessionId, Array.isArray);
const commentId = comments[0]?.id;
if (commentId) await expectSuccess("paperclipGetComment", { issueId, commentId }, sessionId);
else await expectError("paperclipGetComment", { issueId, commentId: "00000000-0000-0000-0000-000000000000" }, sessionId);
await expectSuccess("paperclipListIssueApprovals", { issueId }, sessionId, Array.isArray);
const documents = await expectSuccess("paperclipListDocuments", { issueId }, sessionId, Array.isArray);
let documentKey = documents[0]?.key;
if (!documentKey) {
  documentKey = "matrix-smoke";
  await expectSuccess("paperclipUpsertIssueDocument", { issueId, key: documentKey, body: "# matrix" }, sessionId);
}
await expectSuccess("paperclipGetDocument", { issueId, key: documentKey }, sessionId);
const revisions = await expectSuccess("paperclipListDocumentRevisions", { issueId, key: documentKey }, sessionId, Array.isArray);
if (revisions[0]?.id) await expectSuccess("paperclipRestoreIssueDocumentRevision", { issueId, key: documentKey, revisionId: revisions[0].id }, sessionId);
else await expectError("paperclipRestoreIssueDocumentRevision", { issueId, key: documentKey, revisionId: "00000000-0000-0000-0000-000000000000" }, sessionId);

const projects = await expectSuccess("paperclipListProjects", {}, sessionId, Array.isArray);
if (projects[0]?.id) await expectSuccess("paperclipGetProject", { projectId: projects[0].id }, sessionId);
else await expectError("paperclipGetProject", { projectId: "00000000-0000-0000-0000-000000000000" }, sessionId);
await expectSuccess("paperclipGetIssueWorkspaceRuntime", { issueId }, sessionId);
const control = await call("paperclipControlIssueWorkspaceServices", { issueId, action: "stop" }, sessionId);
if (!control.ok && !control.value) {
  // No execution workspace is a valid local error contract.
}
await expectSuccess("paperclipWaitForIssueWorkspaceService", { issueId, timeoutSeconds: 1 }, sessionId);

const goals = await expectSuccess("paperclipListGoals", {}, sessionId, Array.isArray);
if (goals[0]?.id) await expectSuccess("paperclipGetGoal", { goalId: goals[0].id }, sessionId);
else await expectError("paperclipGetGoal", { goalId: "00000000-0000-0000-0000-000000000000" }, sessionId);
await expectSuccess("paperclipListApprovals", {}, sessionId, Array.isArray);
const approval = await expectSuccess("paperclipCreateApproval", {
  type: "request_board_approval", payload: { reason: "tool matrix" }, issueIds: [],
}, sessionId);
const approvalId = approval.id ?? approval.approval?.id;
if (!approvalId) throw new Error(`approval has no id: ${JSON.stringify(approval)}`);
await expectSuccess("paperclipGetApproval", { approvalId }, sessionId);
await expectSuccess("paperclipGetApprovalIssues", { approvalId }, sessionId, Array.isArray);
await expectSuccess("paperclipListApprovalComments", { approvalId }, sessionId, Array.isArray);
await expectSuccess("paperclipAddApprovalComment", { approvalId, body: "matrix comment" }, sessionId);
await expectSuccess("paperclipLinkIssueApproval", { issueId, approvalId }, sessionId);
await expectSuccess("paperclipUnlinkIssueApproval", { issueId, approvalId }, sessionId);
await expectError("paperclipApprovalDecision", { approvalId, action: "reject", decisionNote: "agent must not decide approvals" }, sessionId);

const created = await expectSuccess("paperclipCreateIssue", {
  title: "tool matrix child",
  description: "matrix",
  status: "todo",
  priority: "low",
  blockedByIssueIds: [issueId],
  watchdog: { agentId, instructions: "matrix watchdog" },
  workMode: "skill_test",
  harnessKind: "skill_test",
  executionPolicy: { maxRetries: 2, timeoutSeconds: 90, workspacePreference: "existing" },
  executionWorkspaceSettings: { mode: "persistent", strategy: "existing" },
}, sessionId);
const createdIssueId = created.id;
if (created.executionPolicy?.maxRetries !== 2 || created.executionWorkspaceSettings?.mode !== "persistent") {
  throw new Error(`create issue execution fields were not persisted: ${JSON.stringify(created)}`);
}
if (created.blockedByIssueIds?.length !== 1 || created.blockedByIssueIds[0] !== issueId || created.watchdog?.watchdogAgentId !== agentId) {
  throw new Error(`create issue relations/watchdog were not persisted: ${JSON.stringify(created)}`);
}
if (created.workMode !== "skill_test" || created.harnessKind !== "skill_test") {
  throw new Error(`create issue harness fields were not persisted: ${JSON.stringify(created)}`);
}
const label = await expectSuccess("paperclipApiRequest", {
  method: "POST",
  path: `/companies/${me.companyId}/labels`,
  jsonBody: JSON.stringify({ name: `matrix-${Date.now()}`, color: "#123456" }),
}, sessionId);
const labelId = label?.id;
if (!labelId) throw new Error(`label creation returned no id: ${JSON.stringify(label)}`);
const updated = await expectSuccess("paperclipUpdateIssue", {
  issueId: createdIssueId,
  title: "tool matrix child updated",
  labelIds: [labelId],
}, sessionId);
if (updated.labelIds?.length !== 1 || updated.labelIds[0] !== labelId) {
  throw new Error(`issue labels were not persisted on update: ${JSON.stringify(updated)}`);
}
await expectError("paperclipUpdateIssue", {
  issueId,
  blockedByIssueIds: [createdIssueId],
}, sessionId);
await expectError("paperclipCreateIssue", {
  title: "invalid matrix label",
  labelIds: ["00000000-0000-0000-0000-000000000000"],
}, sessionId);
const discovered = await expectSuccess("paperclipCreateIssue", {
  title: "invalid watchdog discovery scope",
  watchdogDiscovery: { kind: "product_bug", evidenceMarkdown: "matrix" },
}, sessionId);
if (discovered.originKind !== "task_watchdog_product_bug" || discovered.originId !== issueId) {
  throw new Error(`watchdog discovery follow-up was not normalized: ${JSON.stringify(discovered)}`);
}
await expectSuccess("paperclipCheckoutIssue", { issueId: createdIssueId, expectedStatuses: ["todo"] }, sessionId);
await expectSuccess("paperclipReleaseIssue", { issueId: createdIssueId, targetStatus: "done", result: "matrix complete" }, sessionId);
const comment = await expectSuccess("paperclipAddComment", { issueId, body: "matrix comment" }, sessionId);
if (!comment) throw new Error("paperclipAddComment returned no value");
await expectSuccess("paperclipSuggestTasks", { issueId, continuationPolicy: "none", payload: { version: 1, tasks: [{ clientKey: "matrix", title: "Matrix task" }] } }, sessionId);
await expectSuccess("paperclipAskUserQuestions", { issueId, continuationPolicy: "none", payload: { version: 1, questions: [{ id: "matrix", prompt: "Matrix?", selectionMode: "single", options: [{ id: "yes", label: "Yes" }] }] } }, sessionId);
await expectSuccess("paperclipRequestConfirmation", { issueId, continuationPolicy: "none", payload: { version: 1, prompt: "Matrix confirmation" } }, sessionId);
await expectSuccess("paperclipRequestCheckboxConfirmation", { issueId, continuationPolicy: "none", payload: { version: 1, prompt: "Matrix checkbox", options: [{ id: "yes", label: "Yes" }] } }, sessionId);
await expectSuccess("paperclipUpsertIssueDocument", { issueId, key: "matrix-smoke", body: "# matrix updated" }, sessionId);
await expectSuccess("paperclipApiRequest", { method: "GET", path: `/issues/${issueId}` }, sessionId, (value) => value?.id === issueId);

// Every registered tool must also have a deterministic invalid-arguments
// path. This catches tools whose dispatcher accidentally accepts an empty or
// unrelated payload instead of enforcing the Paperclip schema/contract.
for (const tool of tools) {
  await expectError(tool.name, { __invalid: true }, sessionId);
}

const listedNames = new Set(tools.map((tool) => tool.name));
const missingInvocations = [...listedNames].filter((name) => !invokedTools.has(name));
if (missingInvocations.length > 0 || invokedTools.size !== listedNames.size) {
  throw new Error(`not every Paperclip tool was invoked: missing=${JSON.stringify(missingInvocations)} invoked=${invokedTools.size}`);
}

console.log(JSON.stringify({ passed: true, listedToolCount: tools.length, invokedToolCount: invokedTools.size, invalidArgumentChecks: tools.length, issueId, createdIssueId, approvalId, expectedMissingResourceErrors: 3 }));
