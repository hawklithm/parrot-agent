#!/usr/bin/env python3
"""Extract Paperclip drizzle schema baseline (P2.4 alignment reference).

For every schema/*.ts table, extract:
- db column names that reference another table (FK) + onDelete policy
- table-level index definitions
Focus: company_id FK policy (cascade / set null / restrict) and indexes,
used as the "Paperclip design" baseline for aligning Parrot migrations.
"""

import os
import re
import sys

ROOT = os.environ.get("PARROT_WORKSPACE", r"D:\workspace")
SCHEMA_DIR = os.path.join(ROOT, "paperclip", "packages", "db", "src", "schema")
OUT = os.path.join(ROOT, "parrot", "parrot-agent", "PAPERCLIP_SCHEMA_BASELINE.md")

REF_RE = re.compile(r'references\(\(\) => (\w+)\.id(?:,\s*\{ onDelete: "(\w+)" \})?\)')
IDX_RE = re.compile(r'(?:uniqueIndex|index)\("([^"]+)"\)\s*\.on\(\s*([^)]*?)\s*\)', re.S)
COL_NAME_RE = re.compile(r'(\w+)\(\s*"([a-z_]+)"')


def column_db_name(text, ref_pos):
    """Find the db column name preceding a .references() call."""
    head = text[:ref_pos]
    m = list(COL_NAME_RE.finditer(head))
    return m[-1].group(2) if m else "?"


def main():
    rows = []  # (table, column, target, ondelete)
    indexes = []  # (table, index_name, columns)
    files = sorted(os.listdir(SCHEMA_DIR))
    for fn in files:
        if not fn.endswith(".ts"):
            continue
        text = open(os.path.join(SCHEMA_DIR, fn), encoding="utf-8", errors="replace").read()
        m = re.search(r'pgTable\(\s*"(\w+)"', text)
        if not m:
            continue
        table = m.group(1)
        for rm in REF_RE.finditer(text):
            target = rm.group(1)
            ondelete = rm.group(2) or "restrict"
            col = column_db_name(text, rm.start())
            rows.append((table, col, target, ondelete))
        for im in IDX_RE.finditer(text):
            cols = ", ".join(c.strip().split(".")[-1] for c in im.group(2).split(","))
            indexes.append((table, im.group(1), cols))

    company_fk = sorted(
        (t, c, d) for (t, c, target, d) in rows if c == "company_id" and target == "companies"
    )
    no_company_id = sorted({t for (t, c, d) in company_fk} ^ {t for (t, _, _, _) in rows} - {t for (t, c, d) in company_fk})
    # tables without any company_id FK reference
    all_tables = sorted({t for (t, _, _, _) in rows})
    with_company = {t for (t, c, d) in company_fk}
    without_company = sorted(set(all_tables) - with_company)

    lines = []
    lines.append("# Paperclip Schema Baseline (drizzle)")
    lines.append("")
    lines.append("自动提取：`scripts/extract_paperclip_schema.py`。作为 Parrot migration 对齐的基准。")
    lines.append("")
    lines.append(f"- schema files: **{len(files)}**；FK references: **{len(rows)}**；index defs: **{len(indexes)}**")
    lines.append("")

    lines.append("## 1. company_id → companies FK 的 onDelete 策略")
    lines.append("")
    lines.append("| table | onDelete |")
    lines.append("|---|---|")
    for t, c, d in company_fk:
        lines.append(f"| `{t}` | `{d}` |")
    lines.append("")

    lines.append("## 2. 无 company_id 列（或未直连 companies FK）的表")
    lines.append("")
    lines.append("| table |")
    lines.append("|---|")
    for t in without_company:
        lines.append(f"| `{t}` |")
    lines.append("")

    lines.append("## 3. company_id 相关索引")
    lines.append("")
    lines.append("| table | index | columns |")
    lines.append("|---|---|---|")
    for t, name, cols in indexes:
        if "company" in cols.lower() or "company" in name.lower():
            lines.append(f"| `{t}` | `{name}` | `{cols}` |")
    lines.append("")

    lines.append("## 4. 全部索引（参考）")
    lines.append("")
    lines.append("| table | index | columns |")
    lines.append("|---|---|---|")
    for t, name, cols in indexes:
        lines.append(f"| `{t}` | `{name}` | `{cols}` |")
    lines.append("")

    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"wrote {OUT}: tables={len(all_tables)} company_fk={len(company_fk)} "
          f"no_company={len(without_company)} indexes={len(indexes)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
