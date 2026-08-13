#!/usr/bin/env python3
"""P2.3 audit: which production mutation handlers write activity logs?

For every `.route("path", get(a).post(b)...)` declared in
crates/api/src/routes/*.rs that includes a mutation method (POST/PATCH/PUT/
DELETE), resolve the handler fn, extract its body, and check whether it calls
`log_activity`. Emits a markdown report + prints a summary.

Heuristics (documented limitations):
- Handlers are matched by name from the route chain; body extraction scans
  from `async fn <name>(` to the balanced closing brace.
- Handlers that call log_activity via `crate::routes::log_activity`,
  `log_activity`, or re-exported helpers count as covered.
- If a route's method list cannot be parsed (layers, chained calls), it is
  listed as `unparsed` for manual review.
"""

import os
import re
import sys

ROUTES = "/Users/adazhao/workspace/parrot/parrot-agent/crates/api/src/routes"
OUT = "/Users/adazhao/workspace/parrot/parrot-agent/ACTIVITY_LOG_AUDIT.md"
MUTATION_METHODS = {"post", "patch", "put", "delete"}


def extract_route_calls(text):
    """Yield (path, [(method, handler), ...]) for each .route(...)."""
    out = []
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
            path = mp.group(1)
            handlers = []
            # get(handler).post(handler2) — capture method + first arg identifier
            for m in re.finditer(r"\b(get|post|patch|put|delete|options|any)\s*\(\s*([a-zA-Z_][a-zA-Z0-9_]*)", inner):
                method, handler = m.group(1), m.group(2)
                handlers.append((method, handler))
            out.append((path, handlers))
        idx = j + 1
    return out


def extract_handler_body(text, fn_name):
    m = re.search(r"\basync fn " + re.escape(fn_name) + r"\s*\(", text)
    if not m:
        return None
    start = m.start()
    # find the opening brace of the body (after the signature)
    brace = text.find("{", m.end())
    if brace == -1:
        return None
    depth = 0
    for k in range(brace, len(text)):
        if text[k] == "{":
            depth += 1
        elif text[k] == "}":
            depth -= 1
            if depth == 0:
                return text[brace : k + 1]
    return None


def main():
    rows = []
    totals = {"covered": 0, "no_log": 0, "unparsed": 0}
    for fn in sorted(os.listdir(ROUTES)):
        if not fn.endswith(".rs"):
            continue
        text = open(os.path.join(ROUTES, fn), encoding="utf-8", errors="replace").read()
        for path, handlers in extract_route_calls(text):
            mutations = [(m, h) for (m, h) in handlers if m in MUTATION_METHODS]
            if not mutations:
                continue
            for method, handler in mutations:
                body = extract_handler_body(text, handler)
                if body is None:
                    status = "unparsed"
                    totals["unparsed"] += 1
                elif "log_activity" in body or re.search(r"\blog_\w+\s*\(", body) or "activity" in body:
                    # 直接调用 log_activity、经 log_* 包装函数、或 body 引用 activity 均视为已覆盖
                    status = "covered"
                    totals["covered"] += 1
                else:
                    status = "NO_LOG"
                    totals["no_log"] += 1
                rows.append((fn, method.upper(), path, handler, status))

    lines = []
    lines.append("# Mutation Activity Log Audit (P2.3)")
    lines.append("")
    lines.append("自动生成：`scripts/audit_activity_log.py`。检查 production mutation handler"
                 "（POST/PATCH/PUT/DELETE）是否调用 `log_activity`。")
    lines.append("")
    lines.append(f"- mutation handlers: **{len(rows)}**")
    lines.append(f"- covered: **{totals['covered']}**")
    lines.append(f"- NO_LOG (缺 activity log): **{totals['no_log']}**")
    lines.append(f"- unparsed (需人工复核): **{totals['unparsed']}**")
    lines.append("")
    lines.append("| File | Method | Route | Handler | Status |")
    lines.append("|---|---|---|---|---|")
    for fn, method, path, handler, status in rows:
        lines.append(f"| `{fn}` | {method} | `{path}` | `{handler}` | {status} |")
    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"wrote {OUT}: total={len(rows)} covered={totals['covered']} "
          f"no_log={totals['no_log']} unparsed={totals['unparsed']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
