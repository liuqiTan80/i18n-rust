#!/usr/bin/env python3
"""ui.toml 键索引生成器：为语言包贡献者生成「界面消息键 → 消费位置与占位符实参」字典。

动机：一个语言包有 600+ 界面消息键，键名是内部缩写（mg_ / mc_ / arg_ …），
新增第 12 种语言的贡献者只看到母语键与中文原文，既不知道这条文案出现在哪条
命令的哪个时刻（错误？提示？帮助文本？语气无从判断），也不知道每个 `{}` 占位
填的是什么。这些信息全在代码里，人工维护字典必然漂移，因此改为自动生成。

产出 `docs/ui-keys.en.md`：按键名前缀分组，每键列出消费点（文件:行）、
占位符个数、从调用处提取的实参表达式、以及 zh 原文。

两种模式：
- `--write`：生成/更新索引文件
- `--check`（默认）：门禁。① 索引与 ui.toml/代码现状一致（防陈旧）；
  ② 无人消费的键必须为空——新增界面消息键却没有任何调用点即失败，
  防止死键混进语言包让后续所有语言白译一遍。

动态取键（静态扫描看不见、但确实在用）以前缀白名单显式登记，见 DYNAMIC_PREFIXES。

用法：python3 tools/ui-keys-index.py [--write] [--quiet]
"""

import argparse
import os
import re
import sys
import tomllib

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ZH_UI = os.path.join(REPO_ROOT, "crates", "engine", "lang-packs", "zh", "ui.toml")
DOC_PATH = os.path.join(REPO_ROOT, "docs", "ui-keys.en.md")

# 扫描范围：Rust 侧三 crate 的 src（含 engine 的 语言.rs）+ 扩展源码与生成器 +
# 扩展 contributes 的 %键% 引用（package.json）
SCAN_DIRS = [
    os.path.join(REPO_ROOT, "crates", "cli", "src"),
    os.path.join(REPO_ROOT, "crates", "lsp", "src"),
    os.path.join(REPO_ROOT, "crates", "engine", "src"),
    os.path.join(REPO_ROOT, "tools", "vscode-extension", "src"),
]
# 扩展的两个生成器（scripts/gen-ui-strings.mjs 的「消费的键」白名单、gen-nls.mjs）
# 只登记键名、并不渲染文案，故不入扫描范围：否则一个只在白名单里、代码从不使用的键
# 会被当成「有消费点」而躲过死键检查。
SCAN_SUFFIXES = (".rs", ".ts", ".mjs")
PACKAGE_JSON = os.path.join(REPO_ROOT, "tools", "vscode-extension", "package.json")

# 动态取键前缀：键名在运行时拼出，静态扫描必然误判为死键。每条附出处。
DYNAMIC_PREFIXES = {
    # crate::语言::t(&format!("unicode_name_{:04X}", ch as u32))——同形字符诊断的字符名
    "unicode_name_": "crates/engine/src/unicode_confusion.rs (format! by codepoint)",
    # render_issue(): `code|arg1|arg2` 结构化问题条目按 code 取文案
    "mc_danger_": "crates/cli/src/mapping_check.rs render_issue()",
}

# 前缀 → 章节标题（贡献者按命令排查用）
GROUP_LABELS = [
    ("ext_", "VS Code extension (contributes labels, commands, settings)"),
    ("cmd_", "CLI: subcommand descriptions (clap help)"),
    ("arg_", "CLI: argument descriptions (clap help)"),
    ("cli_", "CLI: general output"),
    ("mg_", "CLI: `mapping` generation internals"),
    ("mc_", "CLI: `mapping check` findings"),
    ("mapping_", "CLI: `mapping` user-facing flow"),
    ("lang_", "CLI: `lang` subcommands"),
    ("lc_", "CLI: language-pack download/verification"),
    ("install_", "CLI: `install` subcommands"),
    ("doctor_", "CLI: `doctor` output"),
    ("lsp_", "Language server (LSP) messages"),
    ("diag_", "CLI/engine: diagnostic rendering"),
    ("unicode_name_", "Engine: Unicode character names (dynamic, by codepoint)"),
    ("guide_", "CLI: guided first-run tutorial"),
    ("crate_", "CLI: crate scaffolding (`rzc new`)"),
    ("load_", "Engine: language-pack loading"),
    ("err_", "Engine: lexer/loader errors"),
    ("run_", "CLI: `rzc run` output"),
    ("build_", "CLI: build pipeline output"),
    ("check_", "CLI: `rzc check` output"),
    ("transpile_", "Engine/CLI: transpile messages"),
]

PLACEHOLDER = re.compile(r"\{\}")


def load_ui_messages(path: str) -> dict:
    with open(path, "rb") as f:
        return tomllib.load(f).get("界面消息", {})


def collect_sources() -> list:
    files = []
    for d in SCAN_DIRS:
        if not os.path.isdir(d):
            continue
        for dirpath, _dirnames, filenames in os.walk(d):
            for name in sorted(filenames):
                if name.endswith(SCAN_SUFFIXES):
                    files.append(os.path.join(dirpath, name))
    return files


def rel(path: str) -> str:
    return os.path.relpath(path, REPO_ROOT).replace(os.sep, "/")


def split_top_level(text: str) -> list:
    """按顶层逗号切分实参列表：括号与引号内的逗号不算分隔。"""
    parts, depth, quote, buf = [], 0, None, []
    i = 0
    while i < len(text):
        ch = text[i]
        if quote:
            buf.append(ch)
            if ch == "\\" and i + 1 < len(text):
                buf.append(text[i + 1])
                i += 2
                continue
            if ch == quote:
                quote = None
        elif ch in "\"'":
            quote = ch
            buf.append(ch)
        elif ch in "([{":
            depth += 1
            buf.append(ch)
        elif ch in ")]}":
            depth -= 1
            buf.append(ch)
        elif ch == "," and depth == 0:
            parts.append("".join(buf).strip())
            buf = []
        else:
            buf.append(ch)
        i += 1
    if buf:
        parts.append("".join(buf).strip())
    return [p for p in parts if p]


ARG_PREFIX = re.compile(r"\s*,\s*&\[")


def extract_args(after: str) -> list:
    """提取 `ui.f("key", &[a, b])` 的实参切片内容（`after` 为键名右侧的源码）。

    必须紧跟 `, &[` 才算实参：只在整个窗口里找第一个 `&[` 会误抓到相邻代码的
    切片（曾把 `ApiEntry]`、`&json_path.display()…` 当成实参显示）。
    """
    m = ARG_PREFIX.match(after[:400])
    if not m:
        return []
    window = after[m.end() - 1 : 400]
    depth, end = 0, None
    for i, ch in enumerate(window):
        if ch in "[({":
            depth += 1
        elif ch in ")]}":
            depth -= 1
            if depth == 0:
                end = i
                break
    if end is None:
        return []
    inner = re.sub(r"//[^\n]*", "", window[1:end])
    inner = " ".join(inner.split())
    return [arg.strip() for arg in split_top_level(inner)]


def build_index(messages: dict) -> dict:
    """键 → 消费点清单。每个消费点记录文件、行号与实参表达式。"""
    hits = {key: [] for key in messages}
    for path in collect_sources():
        text = read_text(path)
        lines = text.splitlines()
        # 行起始偏移，便于把匹配位置换算成行号
        offsets, pos = [], 0
        for line in lines:
            offsets.append(pos)
            pos += len(line) + 1
        for key in messages:
            for token in (f'"{key}"', f"'{key}'", f'["{key}"]'):
                for m in re.finditer(re.escape(token), text):
                    p = m.start()
                    idx = next(i for i in range(len(offsets) - 1, -1, -1) if offsets[i] <= p)
                    site = {
                        "file": rel(path),
                        "line": idx + 1,
                        "args": extract_args(text[m.end() :]),
                    }
                    # `"key"` 与 `["key"]`（扩展的 界面文案['key'] 形态）会命中同一行，
                    # 按 文件+行 去重，避免索引里出现重复消费点
                    if not any(s["file"] == site["file"] and s["line"] == site["line"] for s in hits[key]):
                        hits[key].append(site)
    if os.path.exists(PACKAGE_JSON):
        pkg = read_text(PACKAGE_JSON)
        for key in messages:
            if f"%{key}%" in pkg:
                hits[key].append({"file": rel(PACKAGE_JSON), "line": 0, "args": []})
    return hits


def read_text(path: str) -> str:
    with open(path, encoding="utf-8") as f:
        return f.read()


def is_dynamic(key: str) -> bool:
    return any(key.startswith(p) for p in DYNAMIC_PREFIXES)


def one_line(text: str, limit: int = 120) -> str:
    flat = " ".join(str(text).split())
    flat = flat.replace("|", "\\|")
    return flat if len(flat) <= limit else flat[: limit - 1] + "…"


def group_of(key: str) -> str:
    for prefix, label in GROUP_LABELS:
        if key.startswith(prefix):
            return label
    return "Other UI messages"


def render(hits: dict, messages: dict) -> str:
    groups = {}
    for key in messages:
        groups.setdefault(group_of(key), []).append(key)

    used = sorted(k for k, v in hits.items() if v)
    dynamic = sorted(k for k in messages if is_dynamic(k) and not hits.get(k))
    unused = sorted(k for k, v in hits.items() if not v and not is_dynamic(k))

    out = [
        "# `ui.toml` message key index",
        "",
        "<!-- 由 tools/ui-keys-index.py 生成，请勿手工编辑：先改生成器再 `python3 tools/ui-keys-index.py --write`。 -->",
        "",
        "Every entry under `[\"界面消息\"]` in a language pack is listed below with the",
        "place in the code that renders it, how many `{}` placeholders it carries, and the",
        "actual argument expressions passed at those call sites. Use it to judge the tone",
        "of a message (error, hint, help text) and what each placeholder stands for",
        "before you translate it.",
        "",
        f"Key count: **{len(messages)}** · statically referenced: {len(used)} ·",
        f"resolved at runtime by prefix: {len(dynamic)} · unused: {len(unused)}",
        "",
        "| Prefix | Section |",
        "|---|---|",
    ]
    for prefix, label in GROUP_LABELS:
        n = sum(1 for k in messages if k.startswith(prefix))
        if n:
            out.append(f"| `{prefix}*` | {label} ({n}) |")
    out += [
        "",
        "Placeholder arguments are shown as they appear in source, e.g. `&[&created.to_string(),",
        "&path.display().to_string()]` → `count / path-ish expression`. A key with no `{}` needs",
        "no substitution at all.",
        "",
    ]
    for _prefix, label in GROUP_LABELS:
        keys = sorted(groups.get(label, []))
        if not keys:
            continue
        out += [
            f"## {label}",
            "",
            "| Key | `{}` | Rendered at | Arguments | zh source text |",
            "|---|---|---|---|---|",
        ]
        for key in keys:
            n = len(PLACEHOLDER.findall(messages[key]))
            sites = hits.get(key, [])
            if is_dynamic(key):
                where = "dynamic"
                args = DYNAMIC_PREFIXES[next(p for p in DYNAMIC_PREFIXES if key.startswith(p))]
            else:
                where = (
                    "<br>".join(
                        f"`{s['file']}`" if not s["line"] else f"`{s['file']}:{s['line']}`"
                        for s in sites[:3]
                    )
                    or "—"
                )
                arg_set = []
                for s in sites:
                    for a in s["args"]:
                        if a and a not in arg_set:
                            arg_set.append(a)
                args = " / ".join(one_line(a, 34) for a in arg_set[:4]) or "—"
            out.append(
                f"| `{key}` | {n} | {where or '`package.json` (%key%)'} | {one_line(args, 90)} | {one_line(messages[key])} |"
            )
        out.append("")

    out += [
        "## Keys with no static call site",
        "",
        "These are resolved at runtime (prefix registered in the generator), so a plain",
        "text search will not find them — they still must be translated.",
        "",
        "| Key | Why static search misses it |",
        "|---|---|",
    ]
    for key in dynamic:
        prefix = next(p for p in DYNAMIC_PREFIXES if key.startswith(p))
        out.append(f"| `{key}` | {DYNAMIC_PREFIXES[prefix]} |")
    if not dynamic:
        out.append("| _none_ | |")
    out.append("")

    out += [
        "## Unused keys",
        "",
        "No code path renders these. `tools/ui-keys-index.py --check` fails if the set",
        "grows, so a newly added key has to be wired up in the same change.",
        "",
        "```text",
    ]
    out += unused or ["(none)"]
    out += ["```", ""]
    return "\n".join(out)


def main() -> int:
    if hasattr(sys.stdout, "reconfigure"):  # Windows GBK 控制台兜底
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description="生成/校验 ui.toml 界面消息键索引")
    parser.add_argument("--write", action="store_true", help="写入 docs/ui-keys.en.md")
    parser.add_argument("--quiet", action="store_true", help="只在失败时输出")
    args = parser.parse_args()

    messages = load_ui_messages(ZH_UI)
    if not messages:
        print("❌ 未能读取 zh/ui.toml 的 [\"界面消息\"] 段")
        return 1
    hits = build_index(messages)
    doc = render(hits, messages)

    if args.write:
        with open(DOC_PATH, "w", encoding="utf-8") as f:
            f.write(doc)
        if not args.quiet:
            print(f"✅ 已生成 {rel(DOC_PATH)}（{len(messages)} 键）")
        return 0

    expected = read_text(DOC_PATH) if os.path.exists(DOC_PATH) else None
    problems = []
    if expected is None:
        problems.append(f"索引缺失：{rel(DOC_PATH)}（跑 python3 tools/ui-keys-index.py --write）")
    elif expected != doc:
        problems.append(
            f"索引与 ui.toml/代码现状不一致（{rel(DOC_PATH)} 陈旧）：跑 python3 tools/ui-keys-index.py --write 后提交"
        )
    unused = sorted(
        k for k, v in hits.items() if not v and not is_dynamic(k)
    )
    if unused:
        problems.append(
            "以下界面消息键没有任何调用点（死键会让后续所有语言白译一遍）：" + ", ".join(unused)
        )
    if problems:
        for p in problems:
            print("❌ " + p)
        return 1
    if not args.quiet:
        used = sum(1 for v in hits.values() if v)
        dyn = sum(1 for k in messages if is_dynamic(k) and not hits.get(k))
        print(
            f"✅ ui.toml 键索引门禁通过：{len(messages)} 键（静态引用 {used}，动态前缀 {dyn}，死键 0），索引与代码一致"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
