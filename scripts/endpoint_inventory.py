#!/usr/bin/env python3
"""Paperclip vs Parrot endpoint inventory generator (P2.1).

Extracts declared HTTP endpoints from both codebases, normalizes paths,
and emits an alignment report as markdown with per-endpoint status:
implemented / partial / by-design-candidate / missing.

Mount-aware extraction:
- Paperclip: `app.ts` mounts each route file under an explicit prefix
  (`api.use("/companies", companyRoutes(...))`), else defaults to `/api`.
  `auth.ts` is mounted at `/api/auth`. Direct `api.METHOD` / `app.METHOD`
  registrations are also captured.
- Parrot: `app_state.rs` nests the whole route group under `/api`
  (`.nest("/api", api_routes)`); route files declare paths relative to that.

Status classification (structural, documented heuristics):
- implemented: same METHOD + normalized path exists in Parrot.
- partial: same normalized path exists in Parrot with a different METHOD.
- by-design-candidate: path hits a curated list of platform/UI-specific
  Paperclip-only domains (cloud-managed, smoke-lab, CLI auth, sidebar,
  status-cards, summary-slots, tool-gateway, import, board-claim, csv/org.svg
  exports, _plugins ui-static...). These are *candidates*; product owner
  confirms the final by-design list.
- missing: no structural match.

Methodology notes are embedded in the report.
"""

import os
import re
import sys
from collections import OrderedDict

ROOT = os.environ.get("PARROT_WORKSPACE", r"D:\workspace")
PAPERCLIP_SRC = os.path.join(ROOT, "paperclip/server/src")
PARROT_ROUTES = os.path.join(ROOT, "parrot/parrot-agent/crates/api/src/routes")
OUT = os.path.join(ROOT, "parrot/parrot-agent/ENDPOINT_INVENTORY.md")

# Paperclip-only platform/UI domains that are candidates for by-design exclusion.
BY_DESIGN_CANDIDATE_SEGMENTS = {
    "cloud", "smoke-lab", "cli-auth", "sidebar-preferences", "status-cards",
    "summary-slots", "tool-gateway", "tool-applications", "import",
    "board-claim", "_plugins", "recovery-observability",
}
BY_DESIGN_CANDIDATE_SUFFIXES = (".csv", ".svg")


def normalize(path: str) -> str:
    p = re.sub(r"\$\{[^}]*\}", ":param", path)
    p = re.sub(r"\*[A-Za-z0-9_]+", ":param", p)  # express wildcards
    p = re.sub(r":[A-Za-z_][A-Za-z0-9_]*", ":param", p)
    p = p.rstrip("/") or "/"
    return p


def paperclip_mount_map() -> dict:
    """file basename -> mount prefix ('' means mounted at /api)."""
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
    # api.use("/prefix", xxxRoutes(...))  and  app.use("/api/...", xxxRoutes(...))
    for m in re.finditer(r"(?:api|app)\.use\(\s*\"([^\"]*)\",\s*(\w+Routes)\b", app_text):
        prefix, func = m.group(1), m.group(2)
        if func in func_to_file:
            mounts[func_to_file[func]] = prefix
    return mounts


def paperclip_endpoints(mounts: dict) -> OrderedDict:
    eps = OrderedDict()  # (method, norm_path) -> [(decl_path, file:line), ...]
    routes_dir = os.path.join(PAPERCLIP_SRC, "routes")
    for fn in sorted(os.listdir(routes_dir)):
        if not fn.endswith(".ts"):
            continue
        mount = mounts.get(fn, "/api")
        # app.ts mounts are relative to the `api` router which itself is at /api;
        # auth.ts's app.use("/api/auth", ...) already includes /api.
        if mount.startswith("/api"):
            prefix = mount
        else:
            prefix = ("/api" + mount) if mount.startswith("/") else "/api"
        with open(os.path.join(routes_dir, fn), encoding="utf-8", errors="replace") as f:
            for lineno, line in enumerate(f, 1):
                for m in re.finditer(
                    r"(?:router|app|api)\.(get|post|patch|put|delete|all|options)\(\s*[\"`']([^\"`']+)[\"`']",
                    line,
                ):
                    method = m.group(1).upper()
                    decl = m.group(2).strip()
                    if not decl.startswith("/"):
                        continue
                    if decl.startswith(prefix):
                        full = decl
                    else:
                        full = prefix.rstrip("/") + decl
                    eps.setdefault((method, normalize(full)), []).append(
                        (decl, f"routes/{fn}:{lineno}")
                    )

    # Direct api./app. registrations in app.ts / index.ts
    for src_name in ("app.ts", "routes/index.ts"):
        src_path = os.path.join(PAPERCLIP_SRC, src_name)
        if not os.path.exists(src_path):
            continue
        with open(src_path, encoding="utf-8", errors="replace") as f:
            for lineno, line in enumerate(f, 1):
                for m in re.finditer(
                    r"\b(api|app)\.(get|post|patch|put|delete|all)\(\s*[\"`']([^\"`']+)[\"`']",
                    line,
                ):
                    base, method, decl = m.group(1), m.group(2).upper(), m.group(3).strip()
                    if not decl.startswith("/"):
                        continue
                    full = ("/api" if base == "api" else "") + decl
                    eps.setdefault((method, normalize(full)), []).append(
                        (decl, f"{src_name}:{lineno}")
                    )
    return eps


def parrot_endpoints() -> OrderedDict:
    eps = OrderedDict()
    for fn in sorted(os.listdir(PARROT_ROUTES)):
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
                c = text[k]
                if c == "(":
                    depth += 1
                elif c == ")":
                    depth -= 1
                    if depth == 0:
                        j = k
                        break
            inner = text[i : j + 1]
            mp = re.search(r'\.route\(\s*"([^"]*)"', inner)
            if mp:
                decl = mp.group(1)
                methods = re.findall(r"\b(get|post|patch|put|delete|options)\s*\(", inner)
                for method in set(methods):
                    full = decl if decl.startswith("/api") else ("/api" + (decl if decl.startswith("/") else "/" + decl))
                    eps.setdefault((method.upper(), normalize(full)), []).append(
                        (decl, fn)
                    )
            idx = j + 1
    # The public health route is mounted directly in app_state.rs rather than
    # in crates/api/src/routes/*.rs; include direct application routes so the
    # inventory does not report a mounted endpoint as missing.
    app_state = os.path.join(os.path.dirname(PARROT_ROUTES), "app_state.rs")
    if os.path.exists(app_state):
        text = open(app_state, encoding="utf-8", errors="replace").read()
        for m in re.finditer(r'\.route\(\s*"([^"]+)"\s*,\s*([^\n]+)', text):
            decl, handlers = m.group(1), m.group(2)
            full = "/api" + (decl if decl.startswith("/") else "/" + decl)
            for method in re.findall(r'\b(get|post|patch|put|delete|options)\s*\(', handlers):
                eps.setdefault((method.upper(), normalize(full)), []).append(
                    (decl, "app_state.rs")
                )
    return eps


def classify(method: str, norm: str, pr_keys, pr_by_path):
    if (method, norm) in pr_keys:
        return "implemented", ""
    # partial: same path, different method
    others = pr_by_path.get(norm, set())
    if any(m != method for m in others):
        return "partial", f"path exists with method(s) {sorted(others - {method})[:3]}"
    # by-design candidate
    segs = norm.strip("/").split("/")
    if len(segs) >= 2 and segs[1] in BY_DESIGN_CANDIDATE_SEGMENTS:
        return "by-design-candidate", f"Paperclip-only domain '/{segs[1]}'"
    if norm.endswith(BY_DESIGN_CANDIDATE_SUFFIXES):
        return "by-design-candidate", f"export artifact suffix"
    return "missing", ""


def main() -> int:
    mounts = paperclip_mount_map()
    pc = paperclip_endpoints(mounts)
    pr = parrot_endpoints()

    pc_keys = set(pc)
    pr_keys = set(pr)
    pr_by_path = {}
    for (m, p) in pr_keys:
        pr_by_path.setdefault(p, set()).add(m)

    implemented = []
    partial = []
    by_design = []
    missing = []
    for key in sorted(pc_keys):
        method, norm = key
        status, note = classify(method, norm, pr_keys, pr_by_path)
        target = {"implemented": implemented, "partial": partial,
                  "by-design-candidate": by_design, "missing": missing}[status]
        target.append((key, note))

    parrot_only = sorted(pr_keys - pc_keys)

    lines = []
    lines.append("# Paperclip / Parrot Endpoint Inventory")
    lines.append("")
    lines.append("自动生成：`scripts/endpoint_inventory.py`（P2.1 脚本化检查）。")
    lines.append("")
    lines.append("## 方法")
    lines.append("")
    lines.append("- **Mount-aware**：Paperclip 按 `app.ts` 的 `api.use(\"/prefix\", xxxRoutes)` 挂载前缀解析完整路径"
                 "（`auth.ts` → `/api/auth`，其余默认 `/api`）；Parrot 按 `app_state.rs` 的 `/api` nest 解析。")
    lines.append("- **Status**：`implemented`（方法+规范化路径匹配）；`partial`（路径匹配但方法不同）；"
                 "`by-design-candidate`（命中 Paperclip 平台/UI 专属域，需产品确认）；`missing`（无结构匹配）。")
    lines.append("- **Limitation**：状态为结构性判定；`partial` 语义、`by-design-candidate` 最终清单需人工复核。")
    lines.append("")
    lines.append(f"- Paperclip endpoints: **{len(pc_keys)}**")
    lines.append(f"- Parrot endpoints: **{len(pr_keys)}**")
    lines.append(f"- implemented: **{len(implemented)}**")
    lines.append(f"- partial: **{len(partial)}**")
    lines.append(f"- by-design-candidate: **{len(by_design)}**")
    lines.append(f"- missing: **{len(missing)}**")
    lines.append(f"- Parrot-only (extension): **{len(parrot_only)}**")
    lines.append("")

    def table(title, rows, show_note=True):
        lines.append(f"## {title}")
        lines.append("")
        lines.append("| Method+Path | Source | Note |" if show_note else "| Method+Path | Source |")
        lines.append("|---|---|---|" if show_note else "|---|---|")
        for key, note in rows:
            method, norm = key
            decl, src = pc[key][0]
            if show_note:
                lines.append(f"| `{method} {norm}` | `{src}` | {note} |")
            else:
                lines.append(f"| `{method} {norm}` | `{src}` |")
        lines.append("")

    table("1. Implemented", implemented, show_note=False)
    table("2. Partial（路径存在、方法不同）", partial)
    table("3. By-design candidates（平台/UI 专属，需产品确认）", by_design)
    table("4. Missing", missing)
    lines.append("## 5. Parrot-only（Parrot 扩展端点）")
    lines.append("")
    lines.append("| Method+Path | Source |")
    lines.append("|---|---|")
    for key in parrot_only:
        method, norm = key
        decl, src = pr[key][0]
        lines.append(f"| `{method} {norm}` | `{src}` |")
    lines.append("")

    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"wrote {OUT}: impl={len(implemented)} partial={len(partial)} "
          f"by_design={len(by_design)} missing={len(missing)} parrot_only={len(parrot_only)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
