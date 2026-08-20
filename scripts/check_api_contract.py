#!/usr/bin/env python3
"""Failing structural Paperclip -> Parrot HTTP contract check.

This deliberately checks only method/path coverage. Request/response schemas,
authorization and UI reachability remain separate parity gates documented in
the generated inventory.
"""

import argparse
import os
import sys

import endpoint_inventory as inventory


def main() -> int:
    parser = argparse.ArgumentParser(description="Check Paperclip/Parrot HTTP method-path coverage")
    parser.add_argument("--allow-auth-wildcard", action="store_true", help="allow Paperclip's auth catch-all route pending independent auth review")
    args = parser.parse_args()

    mounts = inventory.paperclip_mount_map()
    paperclip = inventory.paperclip_endpoints(mounts)
    parrot = inventory.parrot_endpoints()
    parrot_by_path = {}
    for method, path in parrot:
        parrot_by_path.setdefault(path, set()).add(method)

    missing = []
    partial = []
    for method, path in sorted(paperclip):
        if (method, path) in parrot:
            continue
        if method == "ALL" and path.startswith("/api/auth/") and args.allow_auth_wildcard:
            continue
        if path in parrot_by_path:
            partial.append((method, path, sorted(parrot_by_path[path])))
        else:
            missing.append((method, path))

    print(f"paperclip={len(paperclip)} parrot={len(parrot)} missing={len(missing)} partial={len(partial)}")
    for method, path in missing:
        print(f"MISSING {method} {path}")
    for method, path, methods in partial:
        print(f"PARTIAL {method} {path} (parrot methods: {','.join(methods)})")
    return 1 if missing or partial else 0


if __name__ == "__main__":
    sys.exit(main())
