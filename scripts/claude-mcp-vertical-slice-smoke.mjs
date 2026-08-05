#!/usr/bin/env node

// Run a real Claude Code process against the migrated Paperclip MCP gateway.
// The gateway token is read from the environment and is never printed.
//
// Usage:
// BASE_URL=http://127.0.0.1:3100 TOKEN=ptg_... ISSUE_ID=<issue> \
//   node scripts/claude-mcp-vertical-slice-smoke.mjs

import { spawn } from "node:child_process";

const baseUrl = process.env.BASE_URL ?? "http://127.0.0.1:3100";
const token = process.env.TOKEN;
const issueId = process.env.ISSUE_ID;
const model = process.env.CLAUDE_MODEL ?? "deepseek-v4-flash";
const claudeBin = process.env.CLAUDE_BIN ?? "claude";
const systemPrompt = process.env.CLAUDE_SYSTEM_PROMPT;

if (!token) throw new Error("TOKEN is required");
if (!issueId) throw new Error("ISSUE_ID is required");

const mcpConfig = JSON.stringify({
  mcpServers: {
    paperclip: {
      type: "http",
      url: `${baseUrl.replace(/\/$/, "")}/api/tool-gateway/mcp`,
      headers: { Authorization: `Bearer ${token}` },
    },
  },
});

const prompt = [
  `Use the Paperclip MCP tools for issue ${issueId}.`,
  `First call paperclipGetIssue with issueId ${issueId}.`,
  `Then call paperclipAddComment with issueId ${issueId} and body "Claude MCP vertical slice verification".`,
  `Finally call paperclipCreateIssue with title "Claude MCP child verification" and parentId ${issueId}.`,
  "Do not call any other tools. Report the returned IDs after the calls.",
].join("\n");

const args = [
    "--mcp-config", mcpConfig,
    "--model", model,
    "--print", "-",
    "--output-format", "stream-json",
    "--verbose",
    "--dangerously-skip-permissions",
    "--max-turns", "10",
  ];
if (systemPrompt) args.push("--system-prompt", systemPrompt);

const child = spawn(
  claudeBin,
  args,
  { stdio: ["pipe", "pipe", "pipe"] },
);

child.stdin.end(prompt);

let stdout = "";
let stderr = "";
child.stdout.on("data", (chunk) => { stdout += chunk.toString(); });
child.stderr.on("data", (chunk) => { stderr += chunk.toString(); });

const exitCode = await new Promise((resolve, reject) => {
  child.once("error", reject);
  child.once("close", resolve);
});

const toolCalls = new Set();
let lastText = "";
for (const line of stdout.split(/\r?\n/)) {
  if (!line.trim()) continue;
  let event;
  try {
    event = JSON.parse(line);
  } catch {
    continue;
  }
  const visit = (value) => {
    if (!value || typeof value !== "object") return;
    if (value.type === "tool_use" && typeof value.name === "string") toolCalls.add(value.name);
    if (value.type === "text" && typeof value.text === "string") lastText = value.text;
    if (Array.isArray(value)) value.forEach(visit);
    else Object.values(value).forEach(visit);
  };
  visit(event);
}

const required = ["paperclipGetIssue", "paperclipAddComment", "paperclipCreateIssue"];
const missing = required.filter((name) => !toolCalls.has(name));
if (exitCode !== 0 || missing.length > 0) {
  console.error(JSON.stringify({
    passed: false,
    exitCode,
    model,
    toolCalls: [...toolCalls],
    missing,
    lastText,
    stderr: stderr.trim().slice(-2000),
  }));
  process.exitCode = 1;
} else {
  console.log(JSON.stringify({ passed: true, model, toolCalls: [...toolCalls] }));
}
