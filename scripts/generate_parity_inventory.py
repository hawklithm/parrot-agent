#!/usr/bin/env python3
"""Generate the remaining M0 structural parity inventories.

The output is deliberately an inventory, not a completion claim: source-file
presence is useful for routing the next audit, while behavior is verified by
the migration plan's later contract and integration gates.
"""
import os
import re

ROOT = os.environ.get("PARROT_WORKSPACE", r"D:\workspace")
PAPERCLIP = os.path.join(ROOT, "paperclip")
PARROT = os.path.join(ROOT, "parrot")
OUT = os.path.join(PARROT, "parrot-agent", "PARITY_COMPONENT_INVENTORY.md")


def files(root, suffixes=()):
    result = []
    for base, _, names in os.walk(root):
        for name in names:
            if not suffixes or name.endswith(suffixes):
                result.append(os.path.relpath(os.path.join(base, name), root).replace("\\", "/"))
    return sorted(result)


def matching(items, patterns):
    return [item for item in items if any(re.search(pattern, item, re.I) for pattern in patterns)]


def section(title, pc_items, parrot_items, note):
    lines = [f"## {title}", "", note, "", f"- Paperclip: **{len(pc_items)}**", f"- Parrot: **{len(parrot_items)}**", "", "| Paperclip evidence | Parrot evidence |", "|---|---|"]
    for left, right in zip(pc_items, parrot_items):
        lines.append(f"| `{left}` | `{right}` |")
    longer = pc_items[len(parrot_items):]
    for item in longer:
        lines.append(f"| `{item}` | *(no structural counterpart in this slice)* |")
    for item in parrot_items[len(pc_items):]:
        lines.append(f"| *(Parrot extension)* | `{item}` |")
    return lines + [""]


pc_cli = [item for item in files(os.path.join(PAPERCLIP, "cli"), (".ts", ".tsx")) if "__tests__" not in item and not item.endswith(".test.ts")]
parrot_cli = files(os.path.join(PARROT, "parrot-agent", "crates", "cli"), (".rs", ".toml"))
pc_workers = matching(files(os.path.join(PAPERCLIP, "server", "src"), (".ts",)), [r"job", r"worker", r"scheduler", r"queue", r"cron", r"heartbeat"])
parrot_workers = matching(files(os.path.join(PARROT, "parrot-agent", "crates", "services", "src"), (".rs",)), [r"job", r"worker", r"scheduler", r"routine", r"heartbeat", r"recovery"])
pc_providers = matching(files(os.path.join(PAPERCLIP, "packages"), (".ts", ".tsx")), [r"provider", r"adapter", r"sandbox", r"storage", r"secret"])
parrot_providers = matching(files(os.path.join(PARROT, "parrot-agent", "crates"), (".rs",)), [r"provider", r"adapter", r"sandbox", r"storage", r"secret"])
pc_ui = matching(files(os.path.join(PAPERCLIP, "ui", "src"), (".ts", ".tsx")), [r"route", r"page", r"sidebar", r"settings", r"issue", r"agent"])
parrot_ui = matching(files(os.path.join(PARROT, "parrot-web-ui", "src"), (".ts", ".tsx")), [r"route", r"page", r"sidebar", r"settings", r"issue", r"agent"])

lines = ["# Paperclip / Parrot Component Inventory", "", "自动生成：`scripts/generate_parity_inventory.py`。", "", "该清单用于 M0 的结构差异定位；不能替代 API 行为、权限、Schema、E2E 或视觉验收。", ""]
lines += section("CLI", pc_cli, parrot_cli, "CLI 归属已固定为 `parrot-agent/crates/cli`；后续需逐命令核对参数、认证、退出码和输出。")
lines += section("UI", pc_ui, parrot_ui, "按源文件名和目录做初筛；前端页面可达性、权限、状态和交互需在 UI 阶段逐页验收。")
lines += section("Worker / Scheduler", pc_workers, parrot_workers, "按 Worker、Job、Scheduler、Cron、Heartbeat 等关键词做初筛；需继续核对触发周期、幂等、恢复和并发策略。")
lines += section("Provider / Adapter / Sandbox / Storage / Secret", pc_providers, parrot_providers, "按 provider/adapter/sandbox/storage/secret 关键词做初筛；需继续核对运行时能力矩阵和安全边界。")

with open(OUT, "w", encoding="utf-8") as handle:
    handle.write("\n".join(lines))
print(f"wrote {OUT}")
