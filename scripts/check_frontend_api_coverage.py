#!/usr/bin/env python3
"""Check frontend API modules against backend routes to find coverage gaps.

Usage:
  PARROT_WORKSPACE=/mnt/d/workspace python3 scripts/check_frontend_api_coverage.py
"""

import os
import re
import sys
from collections import defaultdict

ROOT = os.environ.get("PARROT_WORKSPACE", "/mnt/d/workspace")
FRONTEND_API_DIR = os.path.join(ROOT, "parrot", "parrot-web-ui", "src", "api")
BACKEND_ROUTES_DIR = os.path.join(ROOT, "parrot", "parrot-agent", "crates", "api", "src", "routes")

def extract_frontend_api_calls():
    patterns = defaultdict(list)
    if not os.path.isdir(FRONTEND_API_DIR):
        print(f"Frontend API dir not found: {FRONTEND_API_DIR}", file=sys.stderr)
        return patterns
    api_path_re = re.compile(r"""['"`](/api/[\w/:{}]+)['"`]""")
    for fn in sorted(os.listdir(FRONTEND_API_DIR)):
        if not (fn.endswith('.ts') or fn.endswith('.tsx')):
            continue
        path = os.path.join(FRONTEND_API_DIR, fn)
        with open(path, encoding='utf-8') as f:
            content = f.read()
        for m in api_path_re.finditer(content):
            patterns[m.group(1)].append(fn)
    return patterns

def extract_backend_routes():
    routes = set()
    if not os.path.isdir(BACKEND_ROUTES_DIR):
        print(f"Backend routes dir not found: {BACKEND_ROUTES_DIR}", file=sys.stderr)
        return routes
    route_re = re.compile(r"""\.route\(['"]([\w/:{}-]+)['"]""")
    for fn in sorted(os.listdir(BACKEND_ROUTES_DIR)):
        if not fn.endswith('.rs'):
            continue
        path = os.path.join(BACKEND_ROUTES_DIR, fn)
        with open(path, encoding='utf-8') as f:
            content = f.read()
        for m in route_re.finditer(content):
            routes.add(m.group(1))
    return routes

def normalize_path(path):
    p = path
    if p.startswith('/api/'):
        p = p[4:]
    p = re.sub(r':(\w+)', r'{\1}', p)
    return p

def main():
    frontend = extract_frontend_api_calls()
    backend = extract_backend_routes()
    
    fe_norm = set()
    for path in frontend:
        fe_norm.add(normalize_path(path))
    
    be_norm = set()
    for path in backend:
        be_norm.add(normalize_path(path))
    
    be_only = sorted(be_norm - fe_norm)
    fe_only = sorted(fe_norm - be_norm)
    common = sorted(be_norm & fe_norm)
    
    print(f"Backend routes: {len(be_norm)}")
    print(f"Frontend API paths: {len(fe_norm)}")
    print(f"Common: {len(common)}")
    print(f"Backend-only (gap): {len(be_only)}")
    print(f"Frontend-only (dead): {len(fe_only)}")
    print()
    
    if be_only:
        print(f"=== Backend-only ({len(be_only)}) ===")
        for r in be_only:
            print(f"  {r}")
    print()
    
    if fe_only:
        print(f"=== Frontend-only ({len(fe_only)}) ===")
        for r in fe_only:
            files = frontend.get(r, frontend.get('/api' + r, []))
            if files:
                print(f"  {r} (in {', '.join(set(files))})")
            else:
                print(f"  {r}")
    
    return 0 if len(be_only) < 100 else 1

if __name__ == "__main__":
    sys.exit(main())
