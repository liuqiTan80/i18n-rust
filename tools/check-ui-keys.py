#!/usr/bin/env python3
"""ui.toml 键完备性检查：以 zh 为基准，校验各语言包界面消息键齐全。

检查项（任一失败退出码非零）：
1. 每个语言包 ["界面消息"] 必须包含 zh 的全部键（缺键会回退键名/中文，界面出现原始键名）
2. 界面消息键不得误放进 ["解释"] 段（LSP hover 专用段；引擎 parse_ui 只读界面消息段）
3. 共有键的 {} 占位符数量必须与 zh 一致（翻译时丢失/多写占位符会导致参数错位）

用法：python3 tools/check-ui-keys.py [--lang-pack-dir crates/engine/lang-packs]
"""

import argparse
import os
import re
import sys
import tomllib

PLACEHOLDER = re.compile(r"\{\}")


def load_sections(path: str) -> dict:
    with open(path, "rb") as f:
        return tomllib.load(f)


def main() -> int:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    parser = argparse.ArgumentParser(description="ui.toml 界面消息键完备性检查")
    parser.add_argument(
        "--lang-pack-dir",
        default=os.path.join(repo_root, "crates", "engine", "lang-packs"),
        help="语言包根目录（默认 crates/engine/lang-packs）",
    )
    args = parser.parse_args()

    root = args.lang_pack_dir
    langs = sorted(d for d in os.listdir(root) if os.path.isdir(os.path.join(root, d)))
    zh = load_sections(os.path.join(root, "zh", "ui.toml"))
    zh_ui = zh.get("界面消息", {})

    errors = 0
    for lang in langs:
        path = os.path.join(root, lang, "ui.toml")
        if not os.path.exists(path):
            print(f"❌ {lang}: 缺少 ui.toml")
            errors += 1
            continue
        data = load_sections(path)
        ui = data.get("界面消息", {})
        expl = data.get("解释", {})

        missing = sorted(set(zh_ui) - set(ui))
        if missing:
            print(f"❌ {lang}: 界面消息缺 {len(missing)} 键：{', '.join(missing[:8])}"
                  + (" …" if len(missing) > 8 else ""))
            errors += 1

        misplaced = sorted(set(expl) & set(zh_ui))
        if misplaced:
            print(f"❌ {lang}: {len(misplaced)} 个界面消息键误放在 [\"解释\"] 段："
                  f"{', '.join(misplaced[:8])}" + (" …" if len(misplaced) > 8 else ""))
            errors += 1

        bad_ph = []
        for key, zh_val in zh_ui.items():
            val = ui.get(key)
            if isinstance(val, str) and isinstance(zh_val, str):
                if len(PLACEHOLDER.findall(val)) != len(PLACEHOLDER.findall(zh_val)):
                    bad_ph.append(key)
        if bad_ph:
            print(f"❌ {lang}: {len(bad_ph)} 个共有键的 {{}} 占位符数量与 zh 不一致："
                  f"{', '.join(sorted(bad_ph)[:8])}" + (" …" if len(bad_ph) > 8 else ""))
            errors += 1

    if errors:
        print(f"\nui.toml 键检查失败：{errors} 项问题（基准：zh，共 {len(zh_ui)} 键）")
        return 1
    print(f"✅ ui.toml 键检查通过：{len(langs)} 个语言包均含 zh 全量 {len(zh_ui)} 键，占位符一致")
    return 0


if __name__ == "__main__":
    sys.exit(main())
