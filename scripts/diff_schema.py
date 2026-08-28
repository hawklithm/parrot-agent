#!/usr/bin/env python3
"""Compare Paperclip drizzle schema against Parrot migration SQL to find:
1. Tables in Paperclip not created in any Parrot migration
2. Tables in Parrot not in Paperclip (Parrot-specific)
3. Column-level differences for shared tables (basic heuristic)
"""

import os
import re
import sys
from collections import defaultdict

ROOT = os.environ.get("PARROT_WORKSPACE", "/mnt/d/workspace")

# Paths
PAPERCLIP_SCHEMA_DIR = os.path.join(ROOT, "paperclip", "packages", "db", "src", "schema")
PARROT_MIGRATIONS_DIR = os.path.join(ROOT, "parrot", "parrot-agent", "migrations")
OUT = os.path.join(ROOT, "parrot", "parrot-agent", "PAPERCLIP_SCHEMA_DIFF.md")

# Regexes
COL_DEF_RE = re.compile(r'(\w+)\s*[:(]\s*"([a-z_]+)"\s*[),]?\s*(?://.*)?$', re.MULTILINE)
TABLE_NAME_RE = re.compile(r'export\s+(?:const\s+\w+\s*=\s*)?(?:pgTable|defineTable)\s*\(\s*["\'](\w+)["\']', re.MULTILINE)

# Parrot migration patterns
PARROT_CREATE_TABLE_RE = re.compile(r'CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)', re.IGNORECASE)
PARROT_ALTER_ADD_COL_RE = re.compile(r'ALTER\s+TABLE\s+(\w+)\s+ADD\s+(?:COLUMN\s+)?(?:IF\s+NOT\s+EXISTS\s+)?(\w+)', re.IGNORECASE)
PARROT_TABLE_COMMENT_RE = re.compile(r'COMMENT\s+ON\s+TABLE\s+(\w+)', re.IGNORECASE)

def extract_paperclip_tables():
    """Extract table names from Paperclip drizzle schema files."""
    tables = set()
    if not os.path.isdir(PAPERCLIP_SCHEMA_DIR):
        print(f"Paperclip schema dir not found: {PAPERCLIP_SCHEMA_DIR}", file=sys.stderr)
        return tables, {}
    
    table_files = {}
    for fn in sorted(os.listdir(PAPERCLIP_SCHEMA_DIR)):
        if not fn.endswith('.ts'):
            continue
        path = os.path.join(PAPERCLIP_SCHEMA_DIR, fn)
        with open(path, encoding='utf-8') as f:
            content = f.read()
        for m in TABLE_NAME_RE.finditer(content):
            tbl = m.group(1).lower()
            tables.add(tbl)
            table_files[tbl] = fn
    return tables, table_files

def extract_parrot_tables():
    """Extract table names from Parrot migration SQL files."""
    tables = set()
    if not os.path.isdir(PARROT_MIGRATIONS_DIR):
        print(f"Parrot migrations dir not found: {PARROT_MIGRATIONS_DIR}", file=sys.stderr)
        return tables, {}
    
    migration_files = {}
    for fn in sorted(os.listdir(PARROT_MIGRATIONS_DIR)):
        if not fn.endswith('.sql'):
            continue
        path = os.path.join(PARROT_MIGRATIONS_DIR, fn)
        with open(path, encoding='utf-8') as f:
            content = f.read()
        for m in PARROT_CREATE_TABLE_RE.finditer(content):
            tbl = m.group(1).lower()
            if tbl not in tables:
                tables.add(tbl)
                migration_files[tbl] = fn
        # Also catch tables mentioned in COMMENT ON TABLE
        for m in PARROT_TABLE_COMMENT_RE.finditer(content):
            tbl = m.group(1).lower()
            if tbl not in tables:
                tables.add(tbl)
                migration_files[tbl] = fn
    return tables, migration_files

def main():
    pc_tables, pc_files = extract_paperclip_tables()
    pr_tables, pr_files = extract_parrot_tables()
    
    missing_in_parrot = sorted(pc_tables - pr_tables)
    extra_in_parrot = sorted(pr_tables - pc_tables)
    shared = sorted(pc_tables & pr_tables)
    
    lines = []
    lines.append("# Paperclip ↔ Parrot Schema Diff")
    lines.append("")
    lines.append(f"自动生成：`scripts/diff_schema.py`")
    lines.append(f"Paperclip schema dir: `{PAPERCLIP_SCHEMA_DIR}`")
    lines.append(f"Parrot migrations dir: `{PARROT_MIGRATIONS_DIR}`")
    lines.append("")
    lines.append(f"## 统计")
    lines.append("")
    lines.append(f"| 来源 | 表数量 |")
    lines.append(f"|---|---|")
    lines.append(f"| Paperclip 声明 | {len(pc_tables)} |")
    lines.append(f"| Parrot 创建 | {len(pr_tables)} |")
    lines.append(f"| 共有 | {len(shared)} |")
    lines.append(f"| Paperclip 独有（缺失） | {len(missing_in_parrot)} |")
    lines.append(f"| Parrot 独有（扩展） | {len(extra_in_parrot)} |")
    lines.append("")
    
    if missing_in_parrot:
        lines.append(f"## Paperclip 独有表（Parrot 缺失）")
        lines.append("")
        lines.append("| 表名 | Paperclip 文件 | 备注 |")
        lines.append("|---|---|---|")
        for tbl in missing_in_parrot:
            f = pc_files.get(tbl, '?')
            # Check if table is intentionally skipped
            note = ""
            if tbl.startswith('_') or tbl.endswith('_history'):
                note = "可能为内部/历史表"
            lines.append(f"| `{tbl}` | `{f}` | {note} |")
        lines.append("")
    else:
        lines.append("## Paperclip 独有表（Parrot 缺失）")
        lines.append("")
        lines.append("所有 Paperclip 表都已在 Parrot 迁移中找到对应 CREATE TABLE。")
        lines.append("")
    
    if extra_in_parrot:
        lines.append(f"## Parrot 独有表（Parrot 扩展，Paperclip 无对应）")
        lines.append("")
        lines.append("| 表名 | 首次迁移 |")
        lines.append("|---|---|")
        for tbl in extra_in_parrot:
            f = pr_files.get(tbl, '?')
            lines.append(f"| `{tbl}` | `{f}` |")
        lines.append("")
    
    lines.append(f"## 共有表")
    lines.append("")
    lines.append(f"共 {len(shared)} 张表在两者中都有定义。")
    lines.append("")
    
    lines.append(f"## 说明")
    lines.append("")
    lines.append("- 本对比只检查表级别存在性。列级、索引级和约束级差异需要逐表详细分析。")
    lines.append("- Parrot 的 `00_init_schema_unified.sql` 是统一基线，包含大部分核心表。")
    lines.append("- Paperclip 使用 Drizzle ORM schema 定义；Parrot 使用纯 SQL 迁移。")
    lines.append("- Parrot 独有表可能是 Paperclip 上线后的新功能，或是 Parrot 自定义扩展。")
    lines.append("- Paperclip 独有且 Parrot 缺失的表需要手动判断是否属于当前迁移范围。")
    lines.append("")
    
    with open(OUT, 'w', encoding='utf-8') as f:
        f.write('\n'.join(lines))
    
    print(f"Written: {OUT}")
    print(f"  Paperclip tables: {len(pc_tables)}")
    print(f"  Parrot tables:    {len(pr_tables)}")
    print(f"  Shared:           {len(shared)}")
    print(f"  Missing in Parrot: {len(missing_in_parrot)}")
    print(f"  Extra in Parrot:   {len(extra_in_parrot)}")
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
