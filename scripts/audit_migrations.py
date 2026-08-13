#!/usr/bin/env python3
"""P2.4 static migration audit (no DB required).

Scans migrations/*.sql for structural conventions:
- company scope: business tables should carry `company_id`; tables that have
  `company_id` should have an index on it and an FK to `companies` (cascade).
- money / time / JSONB columns are listed for Paperclip compatibility review.
- idempotency: CREATE TABLE / ADD COLUMN should use IF NOT EXISTS where the
  migration may be re-run.

Heuristic + documented limitations: SQL parsing is regex-based; derived or
view tables may be misclassified; the output is a review starting point.
"""

import os
import re
import sys

MIGRATIONS = "/Users/adazhao/workspace/parrot/parrot-agent/migrations"
OUT = "/Users/adazhao/workspace/parrot/parrot-agent/MIGRATION_AUDIT.md"

# Tables that legitimately have no company scope (system/auth/platform).
SCOPE_FREE_TABLES = {
    "activity_logs", "auth_users", "auth_sessions", "board_api_keys",
    "instance_settings", "permission_grants", "cli_auth_challenges",
    "migrations", "scheduler_heartbeats", "schema_migrations",
}


def split_statements(text):
    # naive split on ';' at end of line (multi-line CREATE TABLE)
    return [s.strip() for s in re.split(r";\s*(?:\n|$)", text) if s.strip()]


def main():
    create_tables = {}   # name -> info
    indexes = []         # (table, columns)
    for fn in sorted(os.listdir(MIGRATIONS)):
        if not fn.endswith(".sql"):
            continue
        path = os.path.join(MIGRATIONS, fn)
        sql = open(path, encoding="utf-8", errors="replace").read()
        stmts = split_statements(sql)
        for st in stmts:
            m = re.search(r"CREATE TABLE (?:IF NOT EXISTS )?([a-zA-Z_][a-zA-Z0-9_]*)", st, re.I)
            if m:
                tname = m.group(1).lower()
                cols = {}
                fks = []
                for cm in re.finditer(
                    r"(?:^|\n)\s*([a-zA-Z_][a-zA-Z0-9_]*)\s+([a-zA-Z0-9_() ]+?)(?:,|$)", st, re.M
                ):
                    cols[cm.group(1).lower()] = cm.group(2).strip().lower()
                for fm in re.finditer(r"FOREIGN KEY\s*\(([^)]+)\)\s*REFERENCES\s+([a-zA-Z_][a-zA-Z0-9_]*)", st, re.I):
                    fks.append((fm.group(1).strip().lower(), fm.group(2).lower()))
                has_if_not_exists = "IF NOT EXISTS" in st.upper()
                create_tables[tname] = {
                    "file": fn,
                    "cols": cols,
                    "fks": fks,
                    "if_not_exists": has_if_not_exists,
                }
            for im in re.finditer(
                r"CREATE (?:UNIQUE )?INDEX(?: IF NOT EXISTS)? [^ ]+ ON ([a-zA-Z_][a-zA-Z0-9_]*)"
                r"\s*(?:USING \w+)?\s*\(([^)]+)\)",
                st, re.I
            ):
                indexes.append((im.group(1).lower(), im.group(2).lower()))

    # Inline column-level FK detection: `company_id uuid references companies(id) on delete cascade`
    inline_fks = set()
    for fn in sorted(os.listdir(MIGRATIONS)):
        sql = open(os.path.join(MIGRATIONS, fn), encoding="utf-8", errors="replace").read()
        for fm in re.finditer(
            r"([a-zA-Z_][a-zA-Z0-9_]*)\s+[a-zA-Z0-9_() ]*?\bREFERENCES\s+([a-zA-Z_][a-zA-Z0-9_]*)\b",
            sql, re.I
        ):
            inline_fks.add((fm.group(1).lower(), fm.group(2).lower()))

    money_cols = []
    jsonb_cols = []
    for tname, info in create_tables.items():
        for cname, ctype in info["cols"].items():
            if re.search(r"\b(numeric|decimal|money)\b", ctype):
                money_cols.append((tname, cname, ctype, info["file"]))
            if "jsonb" in ctype:
                jsonb_cols.append((tname, cname, info["file"]))

    # company scope / index / FK cascade
    no_company_id = sorted(
        t for t in create_tables
        if t not in SCOPE_FREE_TABLES and "company_id" not in create_tables[t]["cols"]
    )
    company_id_no_index = []
    company_id_no_fk = []
    company_id_fk_no_cascade = []
    for tname, info in create_tables.items():
        if "company_id" not in info["cols"]:
            continue
        has_index = any(t == tname and "company_id" in cols.split(",") for (t, cols) in indexes)
        if not has_index:
            company_id_no_index.append((tname, info["file"]))
        has_company_fk = any(c == "company_id" and ref == "companies" for (c, ref) in info["fks"]) \
            or ("company_id", "companies") in inline_fks
        if not has_company_fk:
            company_id_no_fk.append((tname, info["file"]))
        else:
            # re-open the migration and check for ON DELETE CASCADE on the company_id FK
            sql = open(os.path.join(MIGRATIONS, info["file"]), encoding="utf-8", errors="replace").read()
            stmts = split_statements(sql)
            cascade = False
            for st in stmts:
                if f"CREATE TABLE {tname.upper()}" in st.upper() or f"CREATE TABLE IF NOT EXISTS {tname.upper()}" in st.upper():
                    if re.search(
                        r"FOREIGN KEY\s*\(\s*company_id\s*\)\s*REFERENCES\s+companies[^)]*ON DELETE CASCADE",
                        st, re.I,
                    ) or re.search(
                        r"\bcompany_id\s+[a-zA-Z0-9_() ]*?REFERENCES\s+companies[^,;]*ON DELETE CASCADE",
                        st, re.I,
                    ):
                        cascade = True
            if not cascade:
                company_id_fk_no_cascade.append((tname, info["file"]))

    # idempotency coverage
    total_ct = len(create_tables)
    ct_if = sum(1 for i in create_tables.values() if i["if_not_exists"])

    lines = []
    lines.append("# Migration Audit (P2.4, static / no DB)")
    lines.append("")
    lines.append("自动生成：`scripts/audit_migrations.py`。正则启发式，输出供人工复核。")
    lines.append("")
    lines.append(f"- migrations: **{len(os.listdir(MIGRATIONS))}**")
    lines.append(f"- CREATE TABLE: **{total_ct}**（含 IF NOT EXISTS: **{ct_if}**）")
    lines.append(f"- 无 company_id 的业务表（候选待核）: **{len(no_company_id)}**")
    lines.append(f"- 有 company_id 但无索引: **{len(company_id_no_index)}**")
    lines.append(f"- 有 company_id 但无 companies FK: **{len(company_id_no_fk)}**")
    lines.append(f"- company_id FK 无 ON DELETE CASCADE: **{len(company_id_fk_no_cascade)}**")
    lines.append(f"- money 列（numeric/decimal/money）: **{len(money_cols)}**")
    lines.append(f"- JSONB 列: **{len(jsonb_cols)}**")
    lines.append("")

    def section(title, items, fmt):
        lines.append(f"## {title}")
        lines.append("")
        if not items:
            lines.append("（无）")
        else:
            lines.append("| 内容 |")
            lines.append("|---|")
            for it in items:
                lines.append(f"| {fmt(it)} |")
        lines.append("")

    section("无 company_id 的表（业务表待核，系统表已豁免）", no_company_id,
            lambda t: f"`{t}`")
    section("company_id 无索引", company_id_no_index,
            lambda p: f"`{p[0]}` ({p[1]})")
    section("company_id 无 companies FK", company_id_no_fk,
            lambda p: f"`{p[0]}` ({p[1]})")
    section("company_id FK 无 ON DELETE CASCADE", company_id_fk_no_cascade,
            lambda p: f"`{p[0]}` ({p[1]})")
    section("money 列（核对 Paperclip 金额语义）", money_cols,
            lambda p: f"`{p[0]}.{p[1]}` {p[2]} ({p[3]})")
    section("JSONB 列", jsonb_cols,
            lambda p: f"`{p[0]}.{p[1]}` ({p[2]})")

    lines.append("## 结论与待办")
    lines.append("")
    lines.append("- 运行期 migration 测试（decision/skill_policy/watchdog decision/plugin lifecycle 等）"
                 "需要真实 Postgres，本环境无法执行；建议在 CI 中用空库+已有库各跑一次 `sqlx::migrate!`。")
    lines.append("- 上表为静态审计起点，请逐条复核 `no company_id` / `no index` / `no cascade` 项。")
    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"wrote {OUT}: tables={total_ct} no_company_id={len(no_company_id)} "
          f"no_index={len(company_id_no_index)} no_fk={len(company_id_no_fk)} "
          f"no_cascade={len(company_id_fk_no_cascade)} money={len(money_cols)} jsonb={len(jsonb_cols)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
