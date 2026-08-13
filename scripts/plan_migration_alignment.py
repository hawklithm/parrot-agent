#!/usr/bin/env python3
"""Cross-reference Parrot migrations vs Paperclip schema baseline.

Produces MIGRATION_ALIGNMENT_PLAN.md: for every Parrot table that has a
company_id FK, compare the current Parrot ON DELETE policy against the
Paperclip design (from PAPERCLIP_SCHEMA_BASELINE extraction). Actions:
- ADD_CASCADE   : Paperclip=company_id FK cascade, Parrot lacks it → needs migration
- MATCH_RESTRICT: both restrict / no cascade → no change
- REVIEW        : no Paperclip counterpart (Parrot-only) or Paperclip has no
                  direct company_id FK on the same table name → human review
"""

import os
import re
import sys

MIGRATIONS = "/Users/adazhao/workspace/parrot/parrot-agent/migrations"
OUT = "/Users/adazhao/workspace/parrot/parrot-agent/MIGRATION_ALIGNMENT_PLAN.md"

# Paperclip: table -> onDelete for company_id FK (from baseline extraction; hardcoded re-extract here)
PAPERCLIP_SCHEMA_DIR = "/Users/adazhao/workspace/paperclip/packages/db/src/schema"
REF_RE = re.compile(r'pgTable\(\s*"(\w+)"[\s\S]*?references\(\(\) => companies\.id(?:,\s*\{ onDelete: "(\w+)" \})?\)')

def paperclip_company_policy():
    policy = {}
    for fn in os.listdir(PAPERCLIP_SCHEMA_DIR):
        if not fn.endswith(".ts"):
            continue
        text = open(os.path.join(PAPERCLIP_SCHEMA_DIR, fn), encoding="utf-8", errors="replace").read()
        m = re.search(r'pgTable\(\s*"(\w+)"', text)
        if not m:
            continue
        table = m.group(1)
        # find references to companies.id within this file
        for rm in re.finditer(r'references\(\(\) => companies\.id(?:,\s*\{ onDelete: "(\w+)" \})?\)', text):
            policy.setdefault(table, rm.group(1) or "restrict")
    return policy


def split_statements(text):
    return [s.strip() for s in re.split(r";\s*(?:\n|$)", text) if s.strip()]


def parse_parrot_tables():
    """table -> {has_company_id, fk_company_cascade}"""
    out = {}
    for fn in sorted(os.listdir(MIGRATIONS)):
        if not fn.endswith(".sql"):
            continue
        sql = open(os.path.join(MIGRATIONS, fn), encoding="utf-8", errors="replace").read()
        for st in split_statements(sql):
            m = re.search(r"CREATE TABLE (?:IF NOT EXISTS )?(\w+)", st, re.I)
            if not m:
                continue
            tname = m.group(1).lower()
            has_company_id = bool(re.search(r"\bcompany_id\b", st, re.I))
            fk_cascade = bool(re.search(
                r"\bcompany_id\b[^,;]*?REFERENCES\s+companies[^,;]*ON DELETE CASCADE",
                st, re.I,
            )) or bool(re.search(
                r"FOREIGN KEY\s*\(\s*company_id\s*\)[^)]*REFERENCES\s+companies[^)]*ON DELETE CASCADE",
                st, re.I,
            ))
            out[tname] = {"has_company_id": has_company_id, "fk_cascade": fk_cascade, "file": fn}
    return out


def singular(name):
    if name.endswith("ies"):
        return name[:-3] + "y"
    if name.endswith("ses") or name.endswith("sses"):
        return name
    if name.endswith("s") and not name.endswith("ss"):
        return name[:-1]
    return name


def main():
    pc_policy = paperclip_company_policy()
    pt_tables = parse_parrot_tables()

    rows = []
    for tname, info in sorted(pt_tables.items()):
        if not info["has_company_id"]:
            continue
        candidates = {tname, singular(tname), tname + "s", singular(tname) + "s"}
        pc = next((pc_policy[k] for k in candidates if k in pc_policy), None)
        if pc is None:
            action = "REVIEW"
            note = f"Paperclip 无同名表（Parrot-only），按 Paperclip 语义人工确认"
        elif pc == "cascade":
            action = "ADD_CASCADE" if not info["fk_cascade"] else "OK"
            note = f"Paperclip={pc}"
        else:
            action = "MATCH_RESTRICT" if not info["fk_cascade"] else "CHANGE_TO_RESTRICT"
            note = f"Paperclip={pc}"
        rows.append((tname, info["file"], info["fk_cascade"], pc, action, note))

    lines = []
    lines.append("# Migration Alignment Plan (Parrot ↔ Paperclip)")
    lines.append("")
    lines.append("自动生成：`scripts/plan_migration_alignment.py`。")
    lines.append("规则：**Paperclip 的 company_id FK onDelete 怎么设计，Parrot 就怎么设计**。")
    lines.append("")
    lines.append(f"- Parrot 含 company_id 的表: **{len(rows)}**")
    counts = {}
    for _, _, _, _, action, _ in rows:
        counts[action] = counts.get(action, 0) + 1
    for k, v in sorted(counts.items()):
        lines.append(f"- {k}: **{v}**")
    lines.append("")
    lines.append("| Parrot table | migration | Parrot 当前 cascade | Paperclip 设计 | 动作 | 说明 |")
    lines.append("|---|---|---|---|---|---|")
    for tname, file, cur, pc, action, note in rows:
        lines.append(
            f"| `{tname}` | `{file}` | {'yes' if cur else 'no'} | {pc or 'n/a'} | {action} | {note} |"
        )
    lines.append("")
    lines.append("## 动作说明")
    lines.append("")
    lines.append("- **ADD_CASCADE**：Paperclip 为 cascade 而 Parrot 缺 → 需新增 migration（DROP+ADD constraint）。")
    lines.append("- **MATCH_RESTRICT**：两边都是 restrict/无 cascade → 无需改动。")
    lines.append("- **CHANGE_TO_RESTRICT**：Parrot 是 cascade 而 Paperclip 是 restrict → 需移除 cascade（按 Paperclip 对齐）。")
    lines.append("- **REVIEW**：Parrot-only 或 Paperclip 无直接 company_id FK → 人工确认。")
    lines.append("")

    # ---- #4: Parrot tables lacking company_id but Paperclip counterpart has it ----
    pc_with = set()
    for fn in os.listdir(PAPERCLIP_SCHEMA_DIR):
        if not fn.endswith(".ts"):
            continue
        text = open(os.path.join(PAPERCLIP_SCHEMA_DIR, fn), encoding="utf-8", errors="replace").read()
        m = re.search(r'pgTable\(\s*"(\w+)"', text)
        if m and re.search(r'references\(\(\) => companies\.id', text):
            pc_with.add(m.group(1))
    exempt = {
        "companies", "auth_users", "auth_sessions", "board_api_keys",
        "instance_settings", "permission_grants", "cli_auth_challenges",
        "migrations", "schema_migrations", "scheduler_heartbeats", "instance_user_roles",
    }
    lines.append("## 5. #4：Parrot 缺 company_id 而 Paperclip 对应表有 company_id（需补列+回填）")
    lines.append("")
    lines.append("| Parrot table | migration | Paperclip 对应 |")
    lines.append("|---|---|---|")
    for tname, info in sorted(pt_tables.items()):
        if info["has_company_id"] or tname in exempt:
            continue
        candidates = {tname, singular(tname), tname + "s", singular(tname) + "s"}
        hit = next((c for c in candidates if c in pc_with), None)
        if hit:
            lines.append(f"| `{tname}` | `{info['file']}` | `{hit}` |")
    lines.append("")
    lines.append("> 加 `company_id uuid NOT NULL REFERENCES companies(id) ON DELETE NO ACTION` 需先回填"
                 "（按各表父链路推导 company_id），本迁移不自动生成，逐表人工设计后补 migration。")
    lines.append("")

    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    # ---- generate the alignment migration SQL (#3 FK policy + #5 indexes) ----
    sql_out = os.path.join(MIGRATIONS, "20260813000001_align_company_fk_to_paperclip.sql")
    sql = ["-- 对齐 company_id FK onDelete 策略到 Paperclip 设计（自动生成）",
           "-- 规则：Paperclip 的 company_id FK onDelete 怎么设计，Parrot 就怎么设计。",
           "-- #3：FK 策略（ADD_CASCADE / CHANGE_TO_RESTRICT）；#5：company_id 索引（幂等）。",
           "-- #4（补 company_id 列）需回填设计，未纳入本迁移，见 MIGRATION_ALIGNMENT_PLAN.md。",
           ""]
    for tname, file, cur, pc, action, note in rows:
        if action not in ("ADD_CASCADE", "CHANGE_TO_RESTRICT"):
            continue
        policy = "ON DELETE CASCADE" if pc == "cascade" else "ON DELETE NO ACTION"
        sql.append(f"-- {tname} ({file}): Paperclip={pc}, Parrot 当前 {'cascade' if cur else 'no-action'}")
        sql.append(f"DO $$")
        sql.append(f"DECLARE fk_name text;")
        sql.append(f"BEGIN")
        sql.append(f"  SELECT con.conname INTO fk_name")
        sql.append(f"  FROM pg_constraint con")
        sql.append(f"  JOIN pg_class rel ON rel.oid = con.conrelid")
        sql.append(f"  WHERE rel.relname = '{tname}'")
        sql.append(f"    AND con.contype = 'f'")
        sql.append(f"    AND con.confrelid = 'companies'::regclass")
        sql.append(f"    AND EXISTS (SELECT 1 FROM unnest(con.conkey) k")
        sql.append(f"                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k")
        sql.append(f"                WHERE a.attname = 'company_id')")
        sql.append(f"  LIMIT 1;")
        sql.append(f"  IF fk_name IS NOT NULL THEN")
        sql.append(f"    EXECUTE format('ALTER TABLE {tname} DROP CONSTRAINT %I', fk_name);")
        sql.append(f"  END IF;")
        sql.append(f"END $$;")
        sql.append(f"ALTER TABLE {tname}")
        sql.append(f"  ADD CONSTRAINT {tname}_company_id_fkey")
        sql.append(f"  FOREIGN KEY (company_id) REFERENCES companies(id) {policy};")
        sql.append("")

    # #5: company_id indexes for no-index tables whose Paperclip counterpart has a company index
    pc_company_index_tables = set()
    for fn in os.listdir(PAPERCLIP_SCHEMA_DIR):
        if not fn.endswith(".ts"):
            continue
        text = open(os.path.join(PAPERCLIP_SCHEMA_DIR, fn), encoding="utf-8", errors="replace").read()
        m = re.search(r'pgTable\(\s*"(\w+)"', text)
        if not m:
            continue
        if re.search(r'[a-z]*index\("[^"]*"\)\s*\.on\([^)]*company', text, re.S):
            pc_company_index_tables.add(m.group(1))
    added_indexes = []
    for tname, info in sorted(pt_tables.items()):
        if not info["has_company_id"]:
            continue
        candidates = {tname, singular(tname), tname + "s", singular(tname) + "s"}
        if not (candidates & pc_company_index_tables):
            continue
        # check Parrot already has an index covering company_id
        has_idx = False
        mfile = os.path.join(MIGRATIONS, info["file"])
        mtext = open(mfile, encoding="utf-8", errors="replace").read()
        for im in re.finditer(r"CREATE (?:UNIQUE )?INDEX(?: IF NOT EXISTS)? \w+ ON \w+\s*(?:USING \w+)?\s*\(([^)]+)\)", mtext, re.I):
            if "company_id" in im.group(1):
                has_idx = True
                break
        if not has_idx:
            sql.append(f"-- {tname}: Paperclip 有 company_id 索引，Parrot 缺失（#5）")
            sql.append(f"CREATE INDEX IF NOT EXISTS idx_{tname}_company_id ON {tname}(company_id);")
            sql.append("")
            added_indexes.append(tname)

    with open(sql_out, "w", encoding="utf-8") as f:
        f.write("\n".join(sql))
    print(f"wrote {OUT}: rows={len(rows)} " + " ".join(f"{k}={v}" for k, v in sorted(counts.items())))
    print(f"wrote {sql_out}: fk_changes={sum(1 for r in rows if r[4] in ('ADD_CASCADE','CHANGE_TO_RESTRICT'))} "
          f"new_indexes={len(added_indexes)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
