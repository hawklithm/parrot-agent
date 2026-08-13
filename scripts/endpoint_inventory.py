#!/usr/bin/env python3
"""Paperclip vs Parrot endpoint inventory generator (P2.1).

Extracts declared HTTP endpoints from both codebases, normalizes paths,
and emits an alignment report (implemented / missing / parrot-only) as
markdown. Both codebases mount their route groups under ``/api``.

Methodology & limitations:
- Paperclip: scans `server/src/routes/*.ts` for `router|app.METHOD("path"`.
- Parrot: scans `crates/api/src/routes/*.rs` for `.route("path", ...)` and
  the handler methods listed in the same call.
- Path normalization maps `:companyId` / `:company_id` / `${var}` -> `:param`,
  strips trailing slashes.
- Paperclip sub-mounts: auth.ts routes are served under `/api/auth`
  (`app.use("/api/auth", authRoutes)`), all other route files under `/api`.
  Parrot routes are nested under `/api` (`app_state.rs: .nest("/api", ...)`).
- Status is structural only (implemented = same method + normalized path
  exists in Parrot). `partial` / `by-design` require human review; obvious
  Parrot-only endpoints are listed separately.
"""

import os
import re
import sys
from collections import OrderedDict

ROOT = "/Users/adazhao/workspace"
PAPERCLIP_SRC = os.path.join(ROOT, "paperclip/server/src")
PARROT_ROUTES = os.path.join(ROOT, "parrot/parrot-agent/crates/api/src/routes")
OUT = os.path.join(ROOT, "parrot/parrot-agent/ENDPOINT_INVENTORY.md")

PAPERCLIP_SUBMOUNTS = {"auth.ts": "/api/auth"}


def normalize(path: str) -> str:
    p = re.sub(r"\$\{[^}]*\}", ":param", path)
    p = re.sub(r":[A-Za-z_][A-Za-z0-9_]*", ":param", p)
    p = p.rstrip("/") or "/"
    return p


def paperclip_endpoints() -> OrderedDict:
    eps = OrderedDict()  # (method, norm_path) -> [(decl_path, file:line), ...]
    routes_dir = os.path.join(PAPERCLIP_SRC, "routes")
    for fn in sorted(os.listdir(routes_dir)):
        if not fn.endswith(".ts"):
            continue
        with open(os.path.join(routes_dir, fn), encoding="utf-8", errors="replace") as f:
            for lineno, line in enumerate(f, 1):
                for m in re.finditer(
                    r"(?:router|app)\.(get|post|patch|put|delete|all|options)\(\s*[\"`']([^\"`']+)[\"`']",
                    line,
                ):
                    method = m.group(1).upper()
                    decl = m.group(2).strip()
                    if not decl.startswith("/"):
                        continue
                    mount = PAPERCLIP_SUBMOUNTS.get(fn, "/api")
                    full = decl if decl.startswith(mount) else mount.rstrip("/") + decl
                    eps.setdefault((method, normalize(full)), []).append(
                        (decl, f"routes/{fn}:{lineno}")
                    )
    return eps


def parrot_endpoints() -> OrderedDict:
    eps = OrderedDict()  # (method, norm_path) -> [(decl_path, file), ...]
    for fn in sorted(os.listdir(PARROT_ROUTES)):
        if not fn.endswith(".rs"):
            continue
        text = open(os.path.join(PARROT_ROUTES, fn), encoding="utf-8", errors="replace").read()
        idx = 0
        while True:
            i = text.find(".route(", idx)
            if i == -1:
                break
            # balanced-paren scan to capture the full .route(...) call
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
                    full = "/api" + (decl if decl.startswith("/") else "/" + decl)
                    eps.setdefault((method.upper(), normalize(full)), []).append(
                        (decl, f"{fn}")
                    )
            idx = j + 1
    return eps


def main() -> int:
    pc = paperclip_endpoints()
    pr = parrot_endpoints()

    pc_keys = set(pc)
    pr_keys = set(pr)
    implemented = sorted(pc_keys & pr_keys)
    missing = sorted(pc_keys - pr_keys)
    parrot_only = sorted(pr_keys - pc_keys)

    lines = []
    lines.append("# Paperclip / Parrot Endpoint Inventory")
    lines.append("")
    lines.append("自动生成：`scripts/endpoint_inventory.py`（P2.1 脚本化检查）。")
    lines.append("状态为**结构性匹配**（方法 + 规范化路径）；`partial`/`by-design` 需人工复核。")
    lines.append("")
    lines.append(f"- Paperclip endpoints: **{len(pc_keys)}**")
    lines.append(f"- Parrot endpoints: **{len(pr_keys)}**")
    lines.append(f"- Implemented (match): **{len(implemented)}**")
    lines.append(f"- Missing in Parrot: **{len(missing)}**")
    lines.append(f"- Parrot-only (extension): **{len(parrot_only)}**")
    lines.append("")

    lines.append("## 1. Implemented（Paperclip 在 Parrot 中已存在）")
    lines.append("")
    lines.append("| Paperclip Endpoint | Parrot Source |")
    lines.append("|---|---|")
    for key in implemented:
        method, norm = key
        pc_ref = pc[key][0]
        pr_ref = pr[key][0]
        lines.append(f"| `{method} {norm}` | `{pr_ref[1]}` |")
    lines.append("")

    lines.append("## 2. Missing in Parrot（需人工判定 partial / by-design）")
    lines.append("")
    lines.append("| Paperclip Endpoint | Declared | Source |")
    lines.append("|---|---|---|")
    for key in missing:
        method, norm = key
        decl, src = pc[key][0]
        lines.append(f"| `{method} {norm}` | `{decl}` | `{src}` |")
    lines.append("")

    lines.append("## 3. Parrot-only（Parrot 扩展端点）")
    lines.append("")
    lines.append("| Parrot Endpoint | Source |")
    lines.append("|---|---|")
    for key in parrot_only:
        method, norm = key
        decl, src = pr[key][0]
        lines.append(f"| `{method} {norm}` | `{src}` |")
    lines.append("")

    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"wrote {OUT}: implemented={len(implemented)} missing={len(missing)} parrot_only={len(parrot_only)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
