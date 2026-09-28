#!/usr/bin/env python3
"""语言包一致性与完整性检查（诊断翻译层的静态安全网）。

为什么需要它：`errors.toml` 此前没有任何门禁脚本读取——唯一的守夜人是引擎里
「附录C 错误码有教学提示」那一项测试，只覆盖 21 个错误码。批量补键时踩过的两个
坑都属静态可查却无人查：含撇号的键误用 TOML 单引号字面量串（整包静默解析失败）、
以及占位符编号与键的捕获个数不匹配（填空永远落空）。

词条形态与引擎 `ErrorTranslationManager::query_by_message` 严格对齐：
  精确键   无 `?`、不以 `~` 开头        —— 整句相等才命中
  前缀键   普通字符串，消息以它开头      —— 残段（尾）填 {q0}/{q1}
  后缀键   以 `~` 开头，消息以去掉 `~` 的它结尾 —— 残段（头）填 {q0}/{q1}
  通配段键 含 `?`，第 n 个捕获填 {q0}…{q(n-1)}  —— 编号必须 < ? 的个数

检查项（错误退出码非零；--strict 时告警也计入失败）：
1. 全部语言包的全部 TOML（含 crates/ 子目录）可被 tomllib 回读解析
2. 错误码表：各语言与 zh 的错误码集合完全一致
3. 消息表：各语言（en 除外，其无「消息翻译」节属预期）键集合完全一致
4. 占位符编号与键形态匹配（见上表），杜绝「填不进去」的死占位符
5. 非 en 包的消息模板不得与英文原文逐字符相同（补了等于没补）
6. 兜底键过短告警：前缀键短于 12 字符时极易吞掉整句开头
7. lang_info.toml：字段集合、「版本」、「扩展名」与目录名一致
8. 附录C 错误码在全部语言包均有「教学提示」

用法：python3 tools/check-lang-packs.py [--lang-pack-dir DIR] [--strict] [--quiet]
"""

import argparse
import os
import re
import sys
import tomllib

# 与 tutorials/附录C：常见错误信息字典.md 的 `### E0xxx` 标题同步维护
# （引擎 crates/engine/src/lib.rs 的同名测试用同一份清单，两处须一起改）
APPENDIX_C_CODES = [
    "E0004", "E0063", "E0072", "E0106", "E0204", "E0261", "E0277", "E0308", "E0382",
    "E0405", "E0425", "E0432", "E0502", "E0507", "E0508", "E0531", "E0573", "E0596",
    "E0597", "E0599", "E0603",
]

MESSAGE_SECTION = "消息翻译"
WILDCARD = "?"
SUFFIX_MARKER = "~"
SHORT_PREFIX_LIMIT = 12
CODE_RE = re.compile(r"^E\d{4}$")
# 引擎 fill_dynamic_placeholders 只处理 q0..q2
PLACEHOLDER_MAX_INDEX = 2
TEMPLATE_FIELD = "消息模板"


def load(path: str) -> dict:
    with open(path, "rb") as f:
        return tomllib.load(f)


def key_shape(key: str) -> str:
    """与引擎分支顺序一致：后缀键 → 通配段键 → 前缀键（精确键按前缀同形处理）。"""
    if key.startswith(SUFFIX_MARKER):
        return "suffix"
    if WILDCARD in key:
        return "segment"
    return "prefix"


def template_refs(template: str):
    return sorted({int(m) for m in re.findall(r"\{q(\d+)\}", template)})


def main() -> int:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    parser = argparse.ArgumentParser(description="语言包一致性与完整性检查")
    parser.add_argument(
        "--lang-pack-dir",
        default=os.path.join(repo_root, "crates", "engine", "lang-packs"),
        help="语言包根目录（默认 crates/engine/lang-packs）",
    )
    parser.add_argument("--strict", action="store_true", help="告警也视为失败")
    parser.add_argument("--quiet", action="store_true", help="只在失败时输出")
    args = parser.parse_args()

    root = args.lang_pack_dir
    langs = sorted(d for d in os.listdir(root) if os.path.isdir(os.path.join(root, d)))
    errors, warnings = [], []
    warn = warnings.append
    fail = errors.append

    # ---- 1. 全部 TOML 可解析（含 crates/ 子目录） ----
    # parsed 键为 (语言, 以语言目录为基准的相对路径)，如 ("zh", "crates/网络.toml")
    parsed, toml_files = {}, 0
    for lang in langs:
        base = os.path.join(root, lang)
        for dirpath, _dirs, files in os.walk(base):
            for name in sorted(files):
                if not name.endswith(".toml"):
                    continue
                path = os.path.join(dirpath, name)
                rel = os.path.relpath(path, base)
                toml_files += 1
                try:
                    parsed[(lang, rel)] = load(path)
                except Exception as exc:  # 语法坏会整包被丢弃，属最高危
                    fail(f"{lang}/{rel} 无法解析（该文件会被整体丢弃）：{exc}")

    tables = {lang: parsed.get((lang, "errors.toml"), {}) for lang in langs}
    msg_tables = {lang: (tables.get(lang) or {}).get(MESSAGE_SECTION, {}) for lang in langs}

    # ---- 2. 错误码集合齐平（基准 zh） ----
    zh_codes = {k for k in tables.get("zh", {}) if CODE_RE.match(k)}
    for lang in langs:
        codes = {k for k in tables.get(lang, {}) if CODE_RE.match(k)}
        if codes != zh_codes:
            miss, extra = sorted(zh_codes - codes), sorted(codes - zh_codes)
            fail(f"{lang}/errors.toml 错误码集合与 zh 不一致（{len(codes)} vs {len(zh_codes)}）"
                 + (f" 缺：{', '.join(miss[:6])}" if miss else "")
                 + (f" 多：{', '.join(extra[:6])}" if extra else ""))

    # ---- 3. 消息表键集合齐平（en 无此节，属预期，以 zh 为基准比对其余 9 语言） ----
    zh_msgs = set(msg_tables.get("zh", {}))
    for lang in langs:
        if lang == "en":
            if msg_tables.get("en"):
                warn("en/errors.toml 出现「消息翻译」节（rustc 原文即英文，通常不需要）")
            continue
        keys = set(msg_tables.get(lang, {}))
        if keys != zh_msgs:
            miss, extra = sorted(zh_msgs - keys), sorted(keys - zh_msgs)
            fail(f"{lang}/errors.toml 消息表与 zh 不齐平（{len(keys)} vs {len(zh_msgs)}）"
                 + (f" 缺 {len(miss)} 条：{miss[:2]}" if miss else "")
                 + (f" 多 {len(extra)} 条：{extra[:2]}" if extra else ""))

    # ---- 4/5/6. 词条形态、占位符编号、是否真译、兜底键长度 ----
    for lang in langs:
        for key, entry in msg_tables.get(lang, {}).items():
            shape = key_shape(key)
            if not isinstance(entry, dict):
                fail(f"{lang}: 键「{key[:60]}」的值不是表（应为 {{消息模板 = …}}）")
                continue
            tpl = entry.get(TEMPLATE_FIELD)
            if not isinstance(tpl, str) or not tpl:
                fail(f"{lang}: 键「{key[:60]}」缺少非空「{TEMPLATE_FIELD}」")
                continue
            refs = template_refs(tpl)
            for n in refs:
                if n > PLACEHOLDER_MAX_INDEX:
                    fail(f"{lang}: 模板引用 q{n}，但引擎只填到 q{PLACEHOLDER_MAX_INDEX}"
                         f"（键「{key[:50]}」）")
            if shape == "segment":
                cap = key.count(WILDCARD)
                for n in refs:
                    if n >= cap:
                        fail(f"{lang}: 通配段键只有 {cap} 个 {WILDCARD}，模板却引用 q{n}"
                             f"（该占位符永远填不进）：{key[:50]}")
            elif refs:
                for n in refs:
                    if n > 1:
                        kind = "后缀" if shape == "suffix" else "前缀"
                        fail(f"{lang}: {kind}键只能填 q0/q1（残段启发式），"
                             f"模板引用了 q{n}：{key[:50]}")
            if lang != "en" and tpl.strip() == key.strip():
                fail(f"{lang}: 模板与英文原文相同（等于没翻译）：{key[:60]}")
            if shape == "prefix" and len(key) < SHORT_PREFIX_LIMIT \
                    and not key.endswith((" ", "`")):
                # 以反引号结尾（如 ``function ` ``）是「捕获反引号内的名」的合法前缀键，
                # 以空格结尾是词边界；两者都不易吞句，不告警
                warn(f"{lang}: 前缀键仅 {len(key)} 字符且不以空格/反引号结尾，可能吞掉更长句子的"
                     f"开头：{key[:40]!r}")

    # ---- 7. lang_info 一致 ----
    base = None
    for lang in langs:
        data = parsed.get((lang, "lang_info.toml"))
        if data is None:
            fail(f"{lang} 缺少 lang_info.toml（引擎无法识别包身份与版本）")
            continue
        info = data.get("语言包", {})
        if not info:
            fail(f"{lang}/lang_info.toml 缺少 [语言包] 段")
            continue
        if base is None:
            base = (lang, set(info), info.get("版本"))
            continue
        b_lang, b_keys, b_ver = base
        if set(info) != b_keys:
            fail(f"{lang}/lang_info.toml 字段集合与 {b_lang} 不一致："
                 f"{sorted(set(info) ^ b_keys)}")
        if info.get("版本") != b_ver:
            fail(f"{lang}/lang_info.toml 版本 {info.get('版本')} 与 {b_lang} 的 {b_ver} 不一致"
                 "（版本漂移会让用户环境副本与内置表的覆盖关系不可预期）")
        if info.get("扩展名") != lang:
            fail(f"{lang}/lang_info.toml「扩展名」={info.get('扩展名')} 与目录名 {lang} 不符")

    # ---- 8. 附录C 教学提示 ----
    for lang in langs:
        table = tables.get(lang)
        if not table:
            continue
        for code in APPENDIX_C_CODES:
            entry = table.get(code)
            if not isinstance(entry, dict) or not entry.get("教学提示"):
                fail(f"{lang}/errors.toml 附录C 错误码 [{code}] 缺少「教学提示」")

    # ---- 汇总 ----
    seg_keys = sorted(k for k in zh_msgs if WILDCARD in k)
    if not (errors or warnings):
        print(f"✅ 语言包检查：{len(langs)} 语言 / {toml_files} 个 TOML / "
              f"{len(zh_codes)} 错误码 / {len(zh_msgs)} 条消息翻译键（含 {len(seg_keys)} 条通配段键）"
              "齐平且可解析")
    else:
        for msg in warnings:
            print(f"⚠️  {msg}")
        for msg in errors:
            print(f"❌ {msg}")
    if errors or (args.strict and warnings):
        if not errors:
            for msg in warnings:
                print(f"⚠️  {msg}")
        print(f"\n语言包检查失败：{len(errors)} 项错误、{len(warnings)} 项告警"
              f"{'（--strict 下告警计入失败）' if args.strict and warnings else ''}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
