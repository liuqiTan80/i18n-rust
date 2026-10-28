#!/usr/bin/env python3
"""VS Code 扩展界面文案一致性检查（不依赖 npm/node_modules）。

扩展运行时文案不走 Rust 的 `ui.f()`，而是由 `scripts/gen-ui-strings.mjs` 从
`crates/engine/lang-packs/<lang>/ui.toml` 的 `ext_*` 键生成
`src/generated/ui-strings.ts`，再由 `extension.ts` 用 `界面文案['键']` 取用。
链路有三个可断点，且断在运行时才暴露（拿到 `undefined`，弹窗显示空白）：

1. 代码消费的键 ∉ 生成器白名单 `消费的键` → 生成表里根本没这个键
2. 生成表落后于 ui.toml（改了文案没跑 `npm run gen:ui`）→ 界面继续显示旧文案
3. ui.toml 某语言缺 `ext_*` 键 → 该语言用户拿到 undefined（非中文界面最惨）

检查项（任一失败退出码非零）：
- 代码消费的键集合 == 生成器白名单 == 生成表键集合（11 语言块键集合还需完全一致）
- 生成表每条文案与对应 ui.toml 当前值逐字相等（生成物陈旧即报错）
- 每个 `ext_*` 键在 11 语言的 `["界面消息"]` 段都存在，且 `{}` 占位符数量与 zh 一致

用法：python3 tools/check-extension-ui-keys.py
"""

import argparse
import json
import os
import re
import sys
import tomllib

LANGS = ["ar", "de", "en", "es", "fr", "hi", "ja", "ko", "pt", "ru", "zh"]
PLACEHOLDER = re.compile(r"\{\}")
# extension.ts 等源码里的取用形式：界面文案['ext_xxx']
CONSUMED = re.compile(r"界面文案\['([a-z0-9_]+)'\]")
# 生成表里的条目形式：        ext_xxx: "...",
GENERATED_ENTRY = re.compile(r"^        ([a-z0-9_]+): (\".*\"),$", re.M)
# 生成器白名单数组（消费的键 = [...]，元素为单引号字符串）
WHITELIST_BLOCK = re.compile(r"const 消费的键 = \[(.*?)\];", re.S)
WHITELIST_ENTRY = re.compile(r"'([a-z0-9_]+)'")


def ui_messages(path: str) -> dict:
    """读取 ui.toml 的 ["界面消息"] 段（缺失返回空字典，由调用方报错）。"""
    with open(path, "rb") as f:
        return tomllib.load(f).get("界面消息", {})


def consumed_keys(src_dir: str) -> set:
    """扫描扩展源码（排除生成物）收集 `界面文案['键']` 消费到的键。"""
    keys = set()
    for dirpath, _dirnames, filenames in os.walk(src_dir):
        if "generated" in os.path.relpath(dirpath, src_dir).split(os.sep):
            continue
        for name in filenames:
            if not name.endswith(".ts"):
                continue
            with open(os.path.join(dirpath, name), encoding="utf-8") as f:
                keys.update(CONSUMED.findall(f.read()))
    return keys


def whitelist_keys(script: str) -> list:
    """读取生成器里的 `消费的键` 数组（保序，用于校验生成表条目顺序确定性）。"""
    with open(script, encoding="utf-8") as f:
        block = WHITELIST_BLOCK.search(f.read())
    if not block:
        raise SystemExit(f"未能在 {script} 中找到 `const 消费的键 = [...]`")
    return WHITELIST_ENTRY.findall(block.group(1))


def generated_table(path: str) -> dict:
    """解析生成表为 {语言: {键: 文案}}。"""
    with open(path, encoding="utf-8") as f:
        text = f.read()
    table, lang = {}, None
    for line in text.splitlines():
        head = re.match(r"^    ([a-z]{2}): \{$", line)
        if head:
            lang = head.group(1)
            table[lang] = {}
            continue
        if lang is not None:
            for key, literal in GENERATED_ENTRY.findall(line):
                # 生成器用 JSON.stringify 写值，JSON 解析即可逐字还原
                table[lang][key] = json.loads(literal)
            if line == "    },":
                lang = None
    return table


def main() -> int:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    parser = argparse.ArgumentParser(description="扩展界面文案一致性检查")
    parser.add_argument(
        "--lang-pack-dir",
        default=os.path.join(repo_root, "crates", "engine", "lang-packs"),
        help="语言包根目录（默认 crates/engine/lang-packs）",
    )
    parser.add_argument(
        "--ext-dir",
        default=os.path.join(repo_root, "tools", "vscode-extension"),
        help="扩展目录（默认 tools/vscode-extension）",
    )
    args = parser.parse_args()

    src_dir = os.path.join(args.ext_dir, "src")
    script = os.path.join(args.ext_dir, "scripts", "gen-ui-strings.mjs")
    table_path = os.path.join(src_dir, "generated", "ui-strings.ts")
    for path in (src_dir, script, table_path, args.lang_pack_dir):
        if not os.path.exists(path):
            print(f"❌ 路径不存在：{path}")
            return 1

    errors = 0
    code_keys = consumed_keys(src_dir)
    white = whitelist_keys(script)
    table = generated_table(table_path)

    # 1. 代码消费 == 生成器白名单
    whitelist_ok = code_keys == set(white)
    if not whitelist_ok:
        print(f"❌ 代码消费的键与生成器白名单不一致：仅代码 {sorted(code_keys - set(white))}，"
              f"仅白名单 {sorted(set(white) - code_keys)}")
        print("   → 新增/删除文案后需同步 scripts/gen-ui-strings.mjs 的「消费的键」")
        errors += 1
    if len(white) != len(set(white)):
        print(f"❌ 生成器白名单存在重复键：{white}")
        errors += 1
        whitelist_ok = False

    # 2. 生成表：11 语言齐全、键集合一致、条目顺序与白名单一致
    if sorted(table) != sorted(LANGS):
        print(f"❌ 生成表语言块异常：期望 {sorted(LANGS)}，实际 {sorted(table)}")
        errors += 1
    for lang in LANGS:
        keys = table.get(lang, {})
        if set(keys) != code_keys:
            print(f"❌ 生成表 {lang} 键集合与代码消费不一致："
                  f"缺失 {sorted(code_keys - set(keys))}，多余 {sorted(set(keys) - code_keys)}")
            print("   → 执行 npm run gen:ui（或 node scripts/gen-ui-strings.mjs）刷新生成表")
            errors += 1
        elif whitelist_ok and list(keys) != white:
            # 仅在根因已定位（白名单一致）时才比顺序，否则不重复报 11 遍
            print(f"❌ 生成表 {lang} 条目顺序与「消费的键」不一致（生成物应可复现）")
            errors += 1

    # 3. ui.toml 侧：键齐全 + 文案逐字相等 + 占位符数量与 zh 对齐
    zh_msgs = ui_messages(os.path.join(args.lang_pack_dir, "zh", "ui.toml"))
    zh_ph = {k: len(PLACEHOLDER.findall(v)) for k, v in zh_msgs.items()}
    for lang in LANGS:
        msgs = ui_messages(os.path.join(args.lang_pack_dir, lang, "ui.toml"))
        missing = sorted(code_keys - set(msgs))
        if missing:
            print(f"❌ {lang}/ui.toml 的 [\"界面消息\"] 缺少扩展消费的键：{missing}")
            errors += 1
        for key in sorted(code_keys & set(msgs)):
            if lang in table and table[lang].get(key) is not None and table[lang][key] != msgs[key]:
                print(f"❌ 生成表落后于 ui.toml：{lang}/{key}")
                print(f"   生成物：{table[lang][key]!r}")
                print(f"   语言包：{msgs[key]!r}")
                print("   → 执行 npm run gen:ui 刷新 src/generated/ui-strings.ts")
                errors += 1
                break
            if lang != "zh" and PLACEHOLDER.findall(msgs[key]) and len(PLACEHOLDER.findall(msgs[key])) != zh_ph[key]:
                print(f"❌ {lang}/{key} 的 {{}} 占位符数量 {len(PLACEHOLDER.findall(msgs[key]))}"
                      f" 与 zh 的 {zh_ph[key]} 不一致")
                errors += 1

    if errors:
        print(f"\n扩展界面文案检查失败：{errors} 项问题（基准：代码消费 {len(code_keys)} 键 × {len(LANGS)} 语言）")
        return 1
    print(f"✅ 扩展界面文案检查通过：代码/生成器/生成表三方一致"
          f"（{len(code_keys)} 键 × {len(LANGS)} 语言，生成物与 ui.toml 逐字相等）")
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
