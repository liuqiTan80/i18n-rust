#!/usr/bin/env python3
"""mapping check 告警基线：告警数只减不增，另加逐语言覆盖面对账。

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

    exe = ".exe" if os.name == "nt" else ""
    rzc = args.rzc or os.path.join(repo_root, "target", "debug", "rzc" + exe)
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

    # 覆盖面对账：告警「只减不增」单独看是不能分辨「真的变好了」和「根本没检查」的。
    # 旧版只数 ⚠️ 行：把 `--rzc` 换成一个什么都不输出、直接 exit 0 的脚本，得到
    # 「📉 告警数 0 低于基线 2——请把 max_warnings 下调为 0」并 EXIT=0（还会教人
    # 把基线调成 0，从此把静默坐实为「健在」）。因此要求每个语言都能单独跑出来：
    # 内置语言清单与 lang-packs 目录不一致、或整段校验逻辑提前退出，都在这里响。每
    # 次约 90 ms，11 个语言共约 1 s。告警数仍只从上面那一次聚合输出统计。
    packs_dir = os.path.join(repo_root, "crates", "engine", "lang-packs")
    expected = sorted(d for d in os.listdir(packs_dir)
                      if os.path.isdir(os.path.join(packs_dir, d))) \
        if os.path.isdir(packs_dir) else []
    if not expected:
        print(f"❌ 语言包目录里没有语言子目录：{packs_dir}\n"
              "   无覆盖面可比对，不能把「告警数不超基线」当成通过。", file=sys.stderr)
        return 1
    for lang in expected:
        one = subprocess.run([rzc, "mapping", "check", lang],
                             capture_output=True, text=True,
                             encoding="utf-8", errors="replace")
        body = (one.stdout + one.stderr).strip()
        if one.returncode != 0 or not body:
            print(f"❌ 覆盖面对账失败：`mapping check {lang}` 退出码 "
                  f"{one.returncode}、输出 {len(body)} 字符\n"
                  f"   语言包在位共 {len(expected)} 个：{expected}\n"
                  "   单语言跑不出来时，聚合检查的「0 告警」只能理解为没检查，不是好消息。",
                  file=sys.stderr)
            return 1

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
              f"请把 max_warnings 下调为 {len(warnings)} 后提交本文件"
              f"（下调前先确认 mapping check 仍覆盖全部 {len(expected)} 个语言）")
        return 0
    print(f"✅ 告警数 {len(warnings)} 与基线一致（只减不增）")
    return 0


# Windows 控制台/管道默认 GBK：输出里的 ✅/❌ 无法编码会触发
# UnicodeEncodeError，把「检查通过」变成崩溃；保留终端本地编码、
# 仅在不支持时替换字符（中文可读，emoji 退化为 ?）
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(errors="replace")


if __name__ == "__main__":
    sys.exit(main())
