#!/usr/bin/env python3
"""Generate ENDPOINT_MIGRATION_BACKLOG.md from the inventory classification.

Per user decision: by-design-candidate (25) + missing (108) = 133 endpoints
are ALL to be migrated. Emits a backlog with Paperclip source + suggested
Parrot module + matching Paperclip audit event (from the action vocabulary)
+ priority bucket, for incremental implementation.
"""

import os
import re
import sys

ROOT = os.environ.get("PARROT_WORKSPACE", r"D:\workspace")
PAPERCLIP_SRC = os.path.join(ROOT, "paperclip/server/src")
PARROT_ROUTES = os.path.join(ROOT, "parrot/parrot-agent/crates/api/src/routes")
OUT = os.path.join(ROOT, "parrot/parrot-agent/ENDPOINT_MIGRATION_BACKLOG.md")

# paperclip action vocabulary (from activity-log extraction)
PC_ACTIONS = set()
PC_ACTIONS_PATH = os.environ.get("PAPERCLIP_ACTIONS_FILE", "")
if PC_ACTIONS_PATH and os.path.exists(PC_ACTIONS_PATH):
    PC_ACTIONS = set(open(PC_ACTIONS_PATH, encoding="utf-8").read().split())

BY_DESIGN_CANDIDATE_SEGMENTS = {
    "cloud", "smoke-lab", "cli-auth", "sidebar-preferences", "status-cards",
    "summary-slots", "tool-gateway", "tool-applications", "import",
    "board-claim", "_plugins", "recovery-observability",
}
BY_DESIGN_CANDIDATE_SUFFIXES = (".csv", ".svg")


def normalize(path):
    p = re.sub(r"\$\{[^}]*\}", ":param", path)
    p = re.sub(r"\*[A-Za-z0-9_]+", ":param", p)
    p = re.sub(r":[A-Za-z_][A-Za-z0-9_]*", ":param", p)
    return p.rstrip("/") or "/"


def paperclip_mount_map():
    func_to_file = {}
    routes_dir = os.path.join(PAPERCLIP_SRC, "routes")
    for fn in os.listdir(routes_dir):
        if not fn.endswith(".ts"):
            continue
        text = open(os.path.join(routes_dir, fn), encoding="utf-8", errors="replace").read()
        for m in re.finditer(r"export function (\w+Routes)\b", text):
            func_to_file[m.group(1)] = fn
    mounts = {}
    app_text = open(os.path.join(PAPERCLIP_SRC, "app.ts"), encoding="utf-8", errors="replace").read()
    for m in re.finditer(r"(?:api|app)\.use\(\s*\"([^\"]*)\",\s*(\w+Routes)\b", app_text):
        if m.group(2) in func_to_file:
            mounts[func_to_file[m.group(2)]] = m.group(1)
    return mounts


def paperclip_endpoints(mounts):
    eps = {}
    routes_dir = os.path.join(PAPERCLIP_SRC, "routes")
    for fn in sorted(os.listdir(routes_dir)):
        if not fn.endswith(".ts"):
            continue
        mount = mounts.get(fn, "/api")
        prefix = mount if mount.startswith("/api") else ("/api" + mount if mount.startswith("/") else "/api")
        with open(os.path.join(routes_dir, fn), encoding="utf-8", errors="replace") as f:
            for lineno, line in enumerate(f, 1):
                for m in re.finditer(
                    r"(?:router|app|api)\.(get|post|patch|put|delete|all|options)\(\s*[\"`']([^\"`']+)[\"`']",
                    line,
                ):
                    method, decl = m.group(1).upper(), m.group(2).strip()
                    if not decl.startswith("/"):
                        continue
                    full = decl if decl.startswith(prefix) else prefix.rstrip("/") + decl
                    eps.setdefault((method, normalize(full)), []).append(
                        (decl, f"routes/{fn}:{lineno}")
                    )
    return eps


def infer_event(method, path):
    """Guess the Paperclip-style audit event for a mutation endpoint."""
    segs = path.strip("/").split("/")
    domain = "issue" if "issues" in segs else \
             "agent" if "agents" in segs else \
             "company" if "companies" in segs else \
             (segs[1] if len(segs) > 1 else "resource")
    verb = {"POST": "created", "PATCH": "updated", "PUT": "updated", "DELETE": "deleted"}.get(method, "updated")
    return f"{domain}.{verb}"


def main():
    mounts = paperclip_mount_map()
    pc = paperclip_endpoints(mounts)
    pc_keys = set(pc)

    # Parrot endpoints (existing): set of (method, norm_path)
    pr_keys = set()
    for fn in os.listdir(PARROT_ROUTES):
        if not fn.endswith(".rs"):
            continue
        text = open(os.path.join(PARROT_ROUTES, fn), encoding="utf-8", errors="replace").read()
        idx = 0
        while True:
            i = text.find(".route(", idx)
            if i == -1:
                break
            depth = 0
            j = i
            for k in range(i, len(text)):
                if text[k] == "(":
                    depth += 1
                elif text[k] == ")":
                    depth -= 1
                    if depth == 0:
                        j = k
                        break
            inner = text[i : j + 1]
            mp = re.search(r'\.route\(\s*"([^"]*)"', inner)
            if mp:
                decl = mp.group(1)
                full = decl if decl.startswith("/api") else ("/api" + (decl if decl.startswith("/") else "/" + decl))
                for method in re.findall(r"\b(get|post|patch|put|delete|options)\s*\(", inner):
                    pr_keys.add((method.upper(), normalize(full)))
            idx = j + 1

    # Keep direct application mounts (currently /health in app_state.rs) in
    # sync with endpoint_inventory.py.
    app_state = os.path.join(os.path.dirname(PARROT_ROUTES), "app_state.rs")
    if os.path.exists(app_state):
        text = open(app_state, encoding="utf-8", errors="replace").read()
        for m in re.finditer(r'\.route\(\s*"([^"]+)"\s*,\s*([^\n]+)', text):
            decl, handlers = m.group(1), m.group(2)
            full = "/api" + (decl if decl.startswith("/") else "/" + decl)
            for method in re.findall(r'\b(get|post|patch|put|delete|options)\s*\(', handlers):
                pr_keys.add((method.upper(), normalize(full)))

    rows = []
    for key in sorted(pc_keys):
        method, norm = key
        if (method, norm) in pr_keys:
            continue  # implemented
        decl, src = pc[key][0]
        segs = norm.strip("/").split("/")
        if len(segs) >= 2 and segs[1] in BY_DESIGN_CANDIDATE_SEGMENTS or norm.endswith(BY_DESIGN_CANDIDATE_SUFFIXES):
            status = "by-design-candidate"
        else:
            status = "missing"
        # priority bucket
        domain = segs[1] if len(segs) > 1 else "?"
        event = infer_event(method, norm)
        rows.append((method, norm, decl, src, status, domain, event))

    lines = []
    lines.append("# Endpoint Migration Backlog (Paperclip → Parrot, 全量)")
    lines.append("")
    lines.append("自动生成：`scripts/gen_endpoint_backlog.py`。按用户决策：**by-design-candidate + missing 全部迁移**。")
    lines.append("")
    lines.append(f"- 待迁移端点: **{len(rows)}**（by-design-candidate: {sum(1 for r in rows if r[4]=='by-design-candidate')}，"
                 f"missing: {sum(1 for r in rows if r[4]=='missing')}）")
    lines.append("- 实现每个端点时同步：handler + service + schema（如需）+ 权限 + activity log 事件（见 audit event 列）。")
    lines.append("")
    lines.append("| # | Method | Path | Paperclip source | 状态 | 域 | 建议 audit event |")
    lines.append("|---|---|---|---|---|---|---|")
    for i, (method, norm, decl, src, status, domain, event) in enumerate(rows, 1):
        lines.append(f"| {i} | {method} | `{norm}` | `{src}` | {status} | {domain} | `{event}` |")
    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"wrote {OUT}: total={len(rows)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
