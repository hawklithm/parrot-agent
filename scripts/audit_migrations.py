#!/usr/bin/env python3
"""Audit Parrot migrations for:
1. Duplicate CREATE TABLE statements (same table name in multiple migrations)
2. Duplicate CREATE INDEX / UNIQUE INDEX (same index name)
3. Migration number conflicts or gaps
4. Missing IF NOT EXISTS / IF EXISTS on DDL that could fail on re-run
5. Dangerous operations (DROP TABLE, DROP COLUMN, ALTER COLUMN TYPE) without safety checks
"""

import os
import re
import sys
import hashlib
from collections import defaultdict

MIGRATIONS_DIR = os.path.join(
    os.environ.get("CARGO_MANIFEST_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
    "migrations",
)

CREATE_TABLE_RE = re.compile(r'CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)', re.IGNORECASE)
CREATE_INDEX_RE = re.compile(r'CREATE\s+(UNIQUE\s+)?INDEX\s+(?:CONCURRENTLY\s+)?(?:IF\s+NOT\s+EXISTS\s+)?(\w+(?:_\w+)+)\s+ON\b', re.IGNORECASE)
DROP_TABLE_RE = re.compile(r'DROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?(\w+)', re.IGNORECASE)
DROP_COLUMN_RE = re.compile(r'DROP\s+COLUMN\s+(?:IF\s+EXISTS\s+)?(\w+)', re.IGNORECASE)
ALTER_COLUMN_RE = re.compile(r'ALTER\s+COLUMN\s+(\w+)\s+TYPE\s+\w+', re.IGNORECASE)

def audit():
    if not os.path.isdir(MIGRATIONS_DIR):
        print(f"Migrations directory not found: {MIGRATIONS_DIR}")
        return 1

    files = sorted(os.listdir(MIGRATIONS_DIR))
    sql_files = [f for f in files if f.endswith('.sql')]
    
    print(f"Auditing {len(sql_files)} migration files in {MIGRATIONS_DIR}")
    print()

    tables_created = {}   # table_name -> first migration
    tables_dropped = set()
    indexes_created = {}  # index_name -> first migration
    # Track dangerous ops
    dangerous_ops = []
    
    # Checksums for drift detection
    checksums = {}
    
    errors = 0
    warnings = 0

    for fn in sql_files:
        path = os.path.join(MIGRATIONS_DIR, fn)
        with open(path, encoding='utf-8') as f:
            content = f.read()
        
        # Checksum for drift detection
        checksums[fn] = hashlib.sha256(content.encode()).hexdigest()[:16]
        
        # Check for wrapping transaction
        has_begin = 'BEGIN;' in content.upper() or 'BEGIN TRANSACTION;' in content.upper()
        has_commit = 'COMMIT;' in content.upper()
        if has_begin and not has_commit:
            print(f"  WARN: {fn} has BEGIN but no COMMIT (possible orphan transaction)")
            warnings += 1
        
        # --- CREATE TABLE ---
        for m in CREATE_TABLE_RE.finditer(content):
            tbl = m.group(1).lower()
            ifnot = 'IF NOT EXISTS' in m.group(0).upper()
            if tbl in tables_created:
                first = tables_created[tbl]
                if ifnot:
                    print(f"  DUPLICATE TABLE: {tbl} created in {fn}, first in {first} (has IF NOT EXISTS, safe)")
                    warnings += 1
                else:
                    print(f"  !! DUPLICATE TABLE: {tbl} created in {fn} WITHOUT IF NOT EXISTS, first in {first}")
                    errors += 1
            else:
                tables_created[tbl] = fn
        
        # --- CREATE INDEX ---
        for m in CREATE_INDEX_RE.finditer(content):
            idx = m.group(2).lower()
            ifnot = 'IF NOT EXISTS' in m.group(0).upper()
            if idx in indexes_created:
                first = indexes_created[idx]
                if ifnot:
                    print(f"  DUPLICATE INDEX: {idx} in {fn}, first in {first} (safe with IF NOT EXISTS)")
                    warnings += 1
                else:
                    print(f"  !! DUPLICATE INDEX: {idx} in {fn} WITHOUT IF NOT EXISTS, first in {first}")
                    errors += 1
            else:
                indexes_created[idx] = fn
        
        # --- Dangerous operations ---
        for m in DROP_TABLE_RE.finditer(content):
            tbl = m.group(1).lower()
            has_if = 'IF EXISTS' in m.group(0).upper()
            if not has_if:
                dangerous_ops.append((fn, f"DROP TABLE {tbl} (no IF EXISTS)"))
        
        for m in ALTER_COLUMN_RE.finditer(content):
            col = m.group(1).lower()
            dangerous_ops.append((fn, f"ALTER COLUMN TYPE {col} (may fail if data incompatible)"))
        
        for m in DROP_COLUMN_RE.finditer(content):
            col = m.group(1).lower()
            has_if = 'IF EXISTS' in m.group(0).upper()
            if not has_if:
                dangerous_ops.append((fn, f"DROP COLUMN {col} (no IF EXISTS)"))

    # --- Migration numbering ---
    print()
    print("=== Migration Numbering ===")
    numbered = [f for f in sql_files if re.match(r'^\d+_', f)]
    for fn in numbered:
        num = int(re.match(r'(\d+)', fn).group(1))
        if num == 0:
            continue  # 00_init_schema_unified is special
        
    # Check for gaps
    nums = sorted([int(re.match(r'(\d+)', f).group(1)) for f in numbered if re.match(r'^\d+_', f) and int(re.match(r'(\d+)', f).group(1)) > 0])
    for i, n in enumerate(nums):
        if i > 0 and n != nums[i-1] + 1 and nums[i-1] != 0:
            print(f"  GAP: {n} after {nums[i-1]}")
            warnings += 1
    
    # Check for files with same number prefix
    num_map = defaultdict(list)
    for fn in sql_files:
        m = re.match(r'^(\d+)', fn)
        if m:
            num_map[m.group(1)].append(fn)
    for num, names in sorted(num_map.items()):
        if len(names) > 1:
            print(f"  CONFLICT: number '{num}' used by: {', '.join(names)}")
            errors += 1

    # --- Dangerous ops summary ---
    print()
    print("=== Dangerous Operations ===")
    if dangerous_ops:
        for fn, desc in dangerous_ops:
            print(f"  !! {fn}: {desc}")
            warnings += 1
    else:
        print("  None found")

    # --- Summary ---
    print()
    print(f"=== Summary ===")
    print(f"  Tables created: {len(tables_created)} unique")
    print(f"  Indexes created: {len(indexes_created)} unique")
    print(f"  Errors: {errors}")
    print(f"  Warnings: {warnings}")
    print(f"  Checksums (first 10):")
    for fn in list(checksums.keys())[:10]:
        print(f"    {checksums[fn]}  {fn}")
    if len(checksums) > 10:
        print(f"    ... and {len(checksums)-10} more")
    
    return 0 if errors == 0 else 1


if __name__ == "__main__":
    sys.exit(audit())
