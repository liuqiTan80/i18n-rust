#!/usr/bin/env python3
"""mapping check 告警基线：告警数只减不增。

运行 rzc mapping check（全部内置语言 + 跨语言检查）并打印完整输出，
统计其中的 ⚠️ 告警行数，与 tools/mapping-baseline.json 的基线比对：
- 超过基线：列出告警行，退出码非零（新增告警必须修复，或连同理由上调基线）；
- 等于基线：通过；
- 低于基线：提示下调基线（本次仍通过）。

mapping check 本体失败（error 级，如重复键 / 关键字避让 / 跨文件冲突）时
原样透传其退出码。

背景：当前基线 2 条均为已明示的结构性差异（zh 独有表与 en 恒等包的同义词
折叠致跨语言条目数不一致；en 无「消息翻译」节、回退英文原文）——
见 docs/translation-status.md「第三方库映射（crates/ 覆盖差异）」。

用法：python3 tools/check-mapping-warnings.py [--rzc target/debug/rzc]
"""

import argparse
import json
import os
import subprocess
import sys


def main() -> int:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    ap = argparse.ArgumentParser(description="mapping check 告警数基线（只减不增）")
    ap.add_argument("--rzc", default=None,
                    help="rzc 可执行文件（默认仓库内 target/debug/rzc）")
    ap.add_argument("--baseline",
                    default=os.path.join(repo_root, "tools", "mapping-baseline.json"),
                    help="告警基线文件（默认 tools/mapping-baseline.json）")
    args = ap.parse_args()

    rzc = args.rzc or os.path.join(repo_root, "target", "debug", "rzc")
    if not os.path.exists(rzc):
        print(f"❌ 未找到 {rzc}，请先 cargo build --bin rzc")
        return 1

    proc = subprocess.run([rzc, "mapping", "check"],
                          capture_output=True, text=True,
                          encoding="utf-8", errors="replace")
    output = proc.stdout + proc.stderr
    print(output.rstrip())
    if proc.returncode != 0:
        print(f"\n❌ mapping check 本体失败（退出码 {proc.returncode}）")
        return proc.returncode

    warnings = [line.rstrip() for line in output.splitlines() if "⚠️" in line]
    with open(args.baseline, encoding="utf-8") as f:
        baseline = json.load(f)
    limit = baseline["max_warnings"]

    print()
    if len(warnings) > limit:
        print(f"❌ 告警数 {len(warnings)} 超过基线 {limit}：")
        for w in warnings:
            print(f"  {w}")
        print(f"   新增告警必须修复，或连同理由上调 "
              f"{os.path.relpath(args.baseline, repo_root)} 后提交")
        return 1
    if len(warnings) < limit:
        print(f"📉 告警数 {len(warnings)} 低于基线 {limit}——"
              f"请把 max_warnings 下调为 {len(warnings)} 后提交本文件")
        return 0
    print(f"✅ 告警数 {len(warnings)} 与基线一致（只减不增）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
