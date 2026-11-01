#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""术语表 ↔ 语言包一致性检查

校验 tutorials/总术语表.md 中「对应关键字/宏/方法/类型/模块/特征 `X`」的
引用是否都能在 zh 语言包（crates/engine/lang-packs/zh）的映射表中找到对应
方言词——术语表是教程的索引资产，语言包是编译期事实来源，两者漂移会让
学习者按术语表写代码却无法转译。

规则（宽松但有效）：
- 关键字/类型：keywords.toml 全部节的键（扁平化，与引擎 flatten_sections 一致）；
- 宏：keywords.toml [宏] 节键（宏节键不带感叹号，术语表的 `打印行!` 去 `!` 匹配）；
- 方法/特征：stdlib.toml [标识符] 键（方法名 `克隆()` 去括号匹配）；
- 模块：module_paths.toml + stdlib.toml 的 [模块路径] 键；
- 符号 `*`、命令 `rzc check`、写法 `#[派生(...)]`、无类别的引用跳过（非语言包引用）。

用法：python3 tools/verify-glossary.py
退出码非零表示存在缺失项（CI 门禁）。
"""

import re
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
GLOSSARY = REPO_ROOT / "tutorials" / "总术语表.md"
LANG_PACK = REPO_ROOT / "crates" / "engine" / "lang-packs" / "zh"

# 术语表引用类别 → 在语言包中的查找位置
# 跳过：符号 / 命令 / 写法 / 无类别（非语言包引用）
SKIP_KINDS = {"符号", "命令", "写法", None}

REF_RE = re.compile(r"对应(关键字|宏|方法|类型|模块|特征)? `([^`]+)`")


def load_toml_tables(path: Path) -> dict:
    """解析 TOML 为 {节名: {键: 值}}；解析失败返回空字典（脚本报错由调用方体现）"""
    try:
        with open(path, "rb") as f:
            data = tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError) as e:
        print(f"❌ 语言包文件解析失败 {path}: {e}")
        sys.exit(1)
    tables = {}
    for section, table in data.items():
        if isinstance(table, dict):
            tables[section] = {str(k): str(v) for k, v in table.items()}
    return tables


def build_lookup() -> dict:
    """构造 zh 语言包的全部查找表：{类别: 键集合}"""
    keywords = load_toml_tables(LANG_PACK / "keywords.toml")
    stdlib = load_toml_tables(LANG_PACK / "stdlib.toml")
    module_paths = load_toml_tables(LANG_PACK / "module_paths.toml")

    # 关键字/类型：全部节键扁平化（含 声明/控制流/类型/宏/派生特征/标准库成员）
    keyword_keys = set()
    for keys in keywords.values():
        keyword_keys.update(keys)
    # 宏：仅 [宏] 节键
    macro_keys = set(keywords.get("宏", {}).keys())
    # 标识符（方法/特征/类型名）：stdlib [标识符]
    ident_keys = set(stdlib.get("标识符", {}).keys())
    # 模块路径：module_paths + stdlib 的 [模块路径]
    module_keys = set(module_paths.get("模块路径", {}).keys())
    module_keys.update(stdlib.get("模块路径", {}).keys())

    return {
        # 关键字：keywords 全部节键 ∪ stdlib 标识符键（属性参数如 #[测试] 的
        # 「测试」存放在标识符表，术语表将其归为关键字引用）
        "关键字": keyword_keys | ident_keys,
        "类型": keyword_keys | ident_keys,
        "宏": macro_keys,
        "方法": ident_keys,
        "特征": ident_keys,
        "模块": module_keys,
    }


def normalize(term: str, kind: str) -> str:
    """术语表写法 → 语言包键名：宏去 `!`、方法去 `()`"""
    if kind == "宏" and term.endswith("!"):
        return term[:-1]
    if kind == "方法":
        return term.rstrip("()")
    return term


def check_glossary() -> int:
    if not GLOSSARY.exists():
        print(f"❌ 未找到术语表：{GLOSSARY}")
        return 1
    lookup = build_lookup()

    refs = []  # (行号, 类别, 术语)
    for lineno, line in enumerate(GLOSSARY.read_text(encoding="utf-8").splitlines(), 1):
        for m in REF_RE.finditer(line):
            kind = m.group(1)
            if kind in SKIP_KINDS:
                continue
            refs.append((lineno, kind, m.group(2)))

    missing = []
    for lineno, kind, term in refs:
        key = normalize(term, kind)
        if key in lookup[kind]:
            continue
        missing.append(f"第 {lineno} 行：对应{kind} `{term}`（语言包无「{key}」）")

    if not refs:
        # 检查对象为空不是「术语表很干净」，而是门禁瞎了：REF_RE 认的是
        # 「对应(类别) `词`」这一种写法，而术语表是散文式文档，把措辞统一改写
        # （翻译成别的语言、或改成表格列）都是合法编辑动作。命中 0 条时 missing
        # 也恒为 0，旧版照样报「✅ 术语表一致性检查通过：0 处「对应…」引用全部
        # 可在 zh 语言包找到」（实测：只在内存里把副本里的「对应」二字换掉，
        # 44 处命中→ 0 处，返回码仍为 0）。
        print(f"❌ 术语表里没解析到任何「对应… `词`」引用：{GLOSSARY}\n"
              "   引用写法改了要同步改本脚本的 REF_RE；空对象上的「全部可找到」不是通过。",
              file=sys.stderr)
        return 1

    if missing:
        print(f"❌ 术语表与 zh 语言包不一致（{len(missing)} 处）：")
        for item in missing:
            print(f"  - {item}")
        print("修正方向：术语表改用语言包已有词，或在语言包补齐对应键。")
        return 1

    print(f"✅ 术语表一致性检查通过：{len(refs)} 处「对应…」引用全部可在 zh 语言包找到")
    return 0


# Windows 控制台/管道默认 GBK：输出里的 ✅/❌ 无法编码会触发
# UnicodeEncodeError，把「检查通过」变成崩溃；保留终端本地编码、
# 仅在不支持时替换字符（中文可读，emoji 退化为 ?）
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(errors="replace")


if __name__ == "__main__":
    sys.exit(check_glossary())
