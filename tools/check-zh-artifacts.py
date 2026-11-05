#!/usr/bin/env python3
"""zh 分支门禁：方言源（.zh）纪律检查——产物同步 + 一词多译审计。

zh 分支用 .zh 方言以中文为第一语言重构后端：每个 `crates/*/src/**/*.zh`
都有一个提交在旁边的转译产物（同目录同名 `.rs`，`rzc transpile` 生成后经
`rustfmt` 规范化——产物手改、方言源改后忘记重转译、或转译后漏跑 fmt，两侧
都会漂移）。本门禁逐文件按同一口径重生成并比对字节。

另有**一词多译审计**（zh 分支编码约定：排除一切一词多译）：等长剥离注释与
字面量后对代码区逐 token 比对语言包转译面表，凡「中文词 → 多个英文值」的
歧义词出现在代码区即判红，除非在 tools/zh-ambiguity-exemptions.json 的
语法位豁免表登记了理由（位置决定取值、实测确定）。

还有**绑定位劫持审计**：keywords.toml 的词条在词法层替换、无位置保护，绑定名
与参数名一样会被改写（实测 `让 无 = x` 转成 `let None = x`），因此 let/参数/
字段/闭包参数/for 变量/模式内变量这些位置禁止使用关键字表词形（详见
check_binding_hijack 的说明）。

用法：python3 tools/check-zh-artifacts.py [--rzc target/debug/rzc] [--fix]
  --fix  跳过比对与歧义审计，直接重新生成全部产物（就地覆盖 .rs）
"""

import argparse
import collections
import json
import os
import re
import subprocess
import sys
import unicodedata

# 语言包中不进入转译替换的节（描述/危险提示表），一词多译审计时排除
NON_TRANSLATED_SECTIONS = {"解释", "危险等级", "危险备注"}


def strip_literals(text: str) -> str:
    """等长剥离注释与字符串/字符字面量内容（换行保留，行号可比）

    覆盖 // 、/*（可嵌套）、"…"（转义）、r"…"/r#"…"#、b"…"、'x'、b'x'；
    生命周期标注 'a 不是字面量，按原文保留。
    """
    out = []
    i, n = 0, len(text)
    depth = 0
    while i < n:
        c = text[i]
        if depth:
            if text.startswith("*/", i):
                depth -= 1
                out.append("  ")
                i += 2
                continue
            out.append("\n" if c == "\n" else " ")
            i += 1
            continue
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(" " * (j - i))
            i = j
            continue
        if text.startswith("/*", i):
            depth += 1
            out.append("  ")
            i += 2
            continue
        # 字符串字面量（含 r/b 前缀变体）
        m = re.match(r'(?:b?r#*)?"|(?:b?)"', text[i:])
        if m:
            tok = m.group(0)
            if "r" in tok:  # raw：以 # 计数找闭合
                hashes = tok[tok.index("r") + 1 : -1]
                closer = '"' + hashes
                j = text.find(closer, i + len(tok))
                end = n if j < 0 else j + len(closer)
            else:
                j = i + len(tok)
                while j < n:
                    if text[j] == "\\":
                        j += 2
                        continue
                    if text[j] == '"':
                        j += 1
                        break
                    j += 1
                end = min(j, n)
            out.append(tok[0] + " " * (end - i - 1))
            i = end
            continue
        # 字符字面量（'x' / '\n' / '\u{…}' / 前置非标识符位，排除生命周期）
        m = re.match(r"'(?:\\.|[^\\'])'", text[i:])
        if m and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            out.append("'" + " " * (len(m.group(0)) - 1))
            i += len(m.group(0))
            continue
        out.append(c)
        i += 1
    return "".join(out)


def normalize(text: str) -> str:
    """NFKC 归一化（等长时启用；不等长退回原文，宁多报不误漏）"""
    norm = unicodedata.normalize("NFKC", text)
    return norm if len(norm) == len(text) else text


def build_ambiguous_map(repo_root: str) -> dict:
    """从 zh 语言包的转译面表收集「中文词 → 多个英文值」的一词多译集"""
    base = os.path.join(repo_root, "crates", "engine", "lang-packs", "zh")
    files = [
        os.path.join(base, "keywords.toml"),
        os.path.join(base, "stdlib.toml"),
        os.path.join(base, "module_paths.toml"),
    ]
    crates_dir = os.path.join(base, "crates")
    if os.path.isdir(crates_dir):
        for name in sorted(os.listdir(crates_dir)):
            if name.endswith(".toml"):
                files.append(os.path.join(crates_dir, name))
    nat2eng = collections.defaultdict(set)
    entry_re = re.compile(r'^"([^"]+)"\s*=\s*"([^"]*)"')
    section_re = re.compile(r'^\[\"?([^\]\"]*)\"?\]\s*$')
    for path in files:
        if not os.path.isfile(path):
            continue
        section = None
        with open(path, encoding="utf-8") as f:
            for line in f:
                s = line.strip()
                ms = section_re.match(s)
                if ms:
                    section = ms.group(1)
                    continue
                if section in NON_TRANSLATED_SECTIONS:
                    continue
                m = entry_re.match(s)
                if m:
                    nat2eng[m.group(1)].add(m.group(2))
    return {k: sorted(v) for k, v in nat2eng.items() if len(v) > 1}


def load_exemptions(repo_root: str) -> dict:
    """语法位豁免表：位置决定取值、实测确定的核心词允许使用"""
    path = os.path.join(repo_root, "tools", "zh-ambiguity-exemptions.json")
    if not os.path.isfile(path):
        return {}
    with open(path, encoding="utf-8") as f:
        return json.load(f).get("语法位允许", {})


def load_decl_exemptions(repo_root: str) -> dict:
    """声明位有意替换表：平台规定死的名字（trait 方法名等）只能用词形写"""
    path = os.path.join(repo_root, "tools", "zh-decl-exemptions.json")
    if not os.path.isfile(path):
        return {}
    with open(path, encoding="utf-8") as f:
        return json.load(f).get("声明位允许", {})


def check_ambiguity(repo_root: str, sources: list) -> int:
    """扫描每个 .zh 源的代码区 token，命中一词多译且不在豁免表 → 判红"""
    ambiguous = build_ambiguous_map(repo_root)
    exempt = load_exemptions(repo_root)
    banned = {normalize(k): (k, v) for k, v in ambiguous.items() if k not in exempt}
    token_re = re.compile(r"(r#)?(\w+)")
    violations = 0
    for zh in sources:
        with open(zh, encoding="utf-8") as f:
            cleaned = normalize(strip_literals(f.read()))
        for m in token_re.finditer(cleaned):
            if m.group(1):
                continue  # r# 原样标识符：刻意保留母语词本身，不参与替换
            hit = banned.get(m.group(2))
            if not hit:
                continue
            word, values = hit
            line = cleaned.count("\n", 0, m.start()) + 1
            print(f"FAIL 一词多译：{zh}:{line} 使用歧义词 `{word}` => {values}")
            violations += 1
    if not violations and sources:
        print(f"一词多译审计：{len(ambiguous)} 个歧义词在册，{len(exempt)} 个语法位豁免，代码区零命中")
    return violations


def build_keyword_table(repo_root: str):
    """读 zh 的 keywords.toml，返回（中文词形 → 英文值集，英文值 → 中文词形集）"""
    path = os.path.join(
        repo_root, "crates", "engine", "lang-packs", "zh", "keywords.toml"
    )
    nat2eng = collections.defaultdict(set)
    eng2nat = collections.defaultdict(set)
    if not os.path.isfile(path):
        return {}, {}
    entry_re = re.compile(r'^"([^"]+)"\s*=\s*"([^"]*)"')
    section_re = re.compile(r'^\[\"?([^\]"]*)\"?\]\s*$')
    section = None
    with open(path, encoding="utf-8") as f:
        for line in f:
            s = line.strip()
            ms = section_re.match(s)
            if ms:
                section = ms.group(1)
                continue
            if section in NON_TRANSLATED_SECTIONS:
                continue
            m = entry_re.match(s)
            if m:
                nat2eng[m.group(1)].add(m.group(2))
                eng2nat[m.group(2)].add(m.group(1))
    return dict(nat2eng), dict(eng2nat)


def check_binding_hijack(repo_root: str, sources: list) -> int:
    """绑定位用了 keywords.toml 词形 → 判红

    关键字表在词法层替换，**不区分位置**，因此连绑定名、参数名都会被改写。实测
    （`rzc transpile`）：
      让 无 = x      -> let None = x      （语法错误）
      让 文本 = "t"   -> let str = "t"     （同上）
      让 列 = 1       -> let column = 1    （静默改名，最贵的一类）
      成功(自身)      -> Ok(Self)          （模式内变量被顶掉）
    stdlib / 别名表的词条只在方法位、类型位替换，绑定位透传（`让 路径 = `、
    `让 组件 = ` 实测存活），所以本检查的禁名集只取 keywords.toml。

    语法前缀词形从表里反查（按英文原名 let/for/mut/Some/Ok…），语言包改名时
    检查跟着走，不会悄悄失效。
    """
    nat2eng, eng2nat = build_keyword_table(repo_root)
    if not nat2eng:
        return 0
    decl_exempt = load_decl_exemptions(repo_root)

    def alt(english: str, fallback: str) -> str:
        words = sorted(eng2nat.get(english, []), key=len, reverse=True)
        return "|".join(re.escape(w) for w in words) if words else fallback

    mut = alt("mut", "可变")
    patterns = [
        ("let 绑定名", r"(?:^|[\s;{}()])(?:%s)\s+(?:%s\s+)?(\w+)(?!\s*\()"
         % (alt("let", "让"), mut)),
        ("for 循环变量", r"(?:^|[\s;{}()])(?:%s)\s+(\w+)\s+在" % alt("for", "对于")),
        ("闭包参数", r"\(\s*(?:%s\s+)?\|([^|]*)\|" % alt("move", "移动")),
        ("参数/字段名", r"(\w+)\s*:(?!:)"),
        # 括号里跟的若是 `名::` 路径、`名(` 调用、`名!` 宏或 `名 {` 结构体字面量，
        # 那都是表达式不是绑定，跳过（实测 `成功(自身::载入文件(…))` 里的 `自身` 是
        # 路径前缀、`有值(向量![..])` 转译 `Some(vec![..])`、`成功(自身 {…})` 转译
        # `Ok(Self {…})`——均为有意替换的正确产物；裸 `名` 仍照判，真实绑定不放过）
        ("模式内绑定", r"(?:%s|%s|%s)\(\s*(?:%s\s+)?(\w+)(?!\s*::)(?!\s*\()(?!\s*!)(?!\s*\{)"
         % (alt("Some", "有值"), alt("Ok", "成功"), alt("Err", "错误"), mut)),
        # 声明名位：函数/结构体/常量等名字同样被词法层顶掉（实测
        # `函数 格式化(…)` → `fn format(…)`，静默改名后仍能编译）
        ("声明名", r"(?:^|[\s;{}()])(?:%s)\s+(\w+)" % "|".join(
            alt(en, zh) for en, zh in [
                ("fn", "函数"), ("const", "常量"), ("static", "静态"),
                ("struct", "结构体"), ("enum", "枚举"), ("trait", "特征"),
                ("type", "类型"), ("impl", "实现"), ("mod", "模块"),
            ]
        )),
    ]
    patterns = [(kind, re.compile(rx)) for kind, rx in patterns]
    skip = re.compile(r"^(?:%s|%s|%s)$" % (mut, alt("move", "移动"), alt("ref", "引用绑定")))

    violations = 0
    exempt_hits = 0
    for zh in sources:
        with open(zh, encoding="utf-8") as f:
            cleaned = normalize(strip_literals(f.read()))
        for kind, rx in patterns:
            for m in rx.finditer(cleaned):
                for raw in m.group(1).split(","):
                    name = raw.strip()
                    if not name or skip.match(name) or name.startswith("_"):
                        continue
                    values = nat2eng.get(name)
                    if not values:
                        continue
                    # 声明名位豁免：平台把名字钉死（trait 方法名 `fmt` 等），
                    # 方言侧只能用词形，替换出规定名正是目的
                    if kind == "声明名" and name in decl_exempt:
                        exempt_hits += 1
                        continue
                    line = cleaned.count("\n", 0, m.start()) + 1
                    print(f"FAIL 绑定位劫持：{zh}:{line} {kind} `{name}` => {sorted(values)}")
                    print("     关键字表词条在任意位置都会被替换，此处会静默改名或"
                          "产出非法 Rust；请改用语言包无词条的中文名（加字成词，如 "
                          "`无` → `空值`、`文本` → `文本串`）")
                    violations += 1
    if not violations and sources:
        extra = f"，声明位有意替换 {exempt_hits} 处（均在册）" if exempt_hits else ""
        print(f"绑定位审计：{len(nat2eng)} 个关键字词条在册，{len(sources)} 个方言源的绑定位零命中{extra}")
    return violations


def find_dialect_sources(repo_root: str) -> list:
    """收集 crates/*/src 下全部 .zh 方言源（按路径排序，输出确定）"""
    found = []
    crates_dir = os.path.join(repo_root, "crates")
    for crate in sorted(os.listdir(crates_dir)):
        src = os.path.join(crates_dir, crate, "src")
        for dirpath, _dirnames, filenames in os.walk(src):
            for name in sorted(filenames):
                if name.endswith(".zh"):
                    found.append(os.path.join(dirpath, name))
    return found


def transpile_and_format(rzc: str, zh: str):
    """转译 + rustfmt 规范化，返回期望产物文本；失败返回 None

    rzc 转译输出不做格式规范化，而提交侧要求 cargo fmt 幂等（否则
    `make fmt-check` 与产物内容互相打架），因此期望值 = rustfmt(rzc transpile)。
    """
    result = subprocess.run(
        [rzc, "transpile", zh], capture_output=True, text=True, encoding="utf-8"
    )
    if result.returncode != 0:
        print(f"FAIL 转译失败：{zh}\n{result.stderr}", file=sys.stderr)
        return None
    # 走 stdin 而非临时文件：rustfmt 读文件参数时会解析 `pub mod X;` 子模块，父
    # 模块（diagnostic/mod 等）的兄弟 .rs 不在同一临时目录，会 rc=1 失败；stdin 只
    # 格式化给定文本、不做模块解析，且与产物生成管线逐字节一致（实测 stdin 输出 ==
    # 已提交的 mod.rs）。口径同本工具 FAIL 提示的 `rzc transpile X | rustfmt > X.rs`。
    fmt = subprocess.run(
        ["rustfmt", "--edition", "2024"],
        input=result.stdout,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if fmt.returncode != 0:
        print(f"FAIL rustfmt 失败：{zh}\n{fmt.stderr}", file=sys.stderr)
        return None
    return fmt.stdout


def main() -> int:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    ap = argparse.ArgumentParser(description="zh 分支方言源纪律检查（产物同步 + 一词多译审计）")
    ap.add_argument("--rzc", default=None, help="rzc 二进制路径（默认 target/debug/rzc）")
    ap.add_argument("--fix", action="store_true", help="重新生成全部产物而非比对（跳过歧义审计）")
    args = ap.parse_args()

    rzc = args.rzc or os.path.join(repo_root, "target", "debug", "rzc")
    if not os.path.isfile(rzc):
        print(f"找不到 rzc：{rzc}（先 cargo build -p rzc，或用 --rzc 指定）")
        return 2

    sources = find_dialect_sources(repo_root)
    if not sources:
        print("未找到任何 .zh 方言源，无需检查")
        return 0

    drift = 0
    for zh in sources:
        rs = zh[:-3] + ".rs"
        expected = transpile_and_format(rzc, zh)
        if expected is None:
            drift += 1
            continue
        if args.fix:
            with open(rs, "w", encoding="utf-8", newline="") as f:
                f.write(expected)
            continue
        if not os.path.isfile(rs):
            print(f"FAIL 缺少产物：{rs}（rzc transpile {zh} | rustfmt --edition 2024 > {rs}）")
            drift += 1
            continue
        with open(rs, encoding="utf-8", newline="") as f:
            committed = f.read()
        if committed != expected:
            print(f"FAIL 产物漂移：{rs} 与 {zh} 的转译结果不一致")
            print(f"     再生成：rzc transpile {zh} | rustfmt --edition 2024 > {rs}")
            drift += 1

    ambiguity = 0 if args.fix else check_ambiguity(repo_root, sources)
    bindings = 0 if args.fix else check_binding_hijack(repo_root, sources)

    if args.fix:
        print(f"已重新生成 {len(sources)} 个产物")
        return 0
    if drift or ambiguity or bindings:
        if drift:
            print(f"共 {drift} 处产物漂移（改动 .zh 后必须重新转译产物再提交）")
        if ambiguity:
            print(f"共 {ambiguity} 处一词多译命中（改写代码绕开，或附实测证据进豁免表）")
        if bindings:
            print(f"共 {bindings} 处绑定位撞关键字表词条（改名绕开，关键字表无位置保护）")
        return 1
    print(f"OK：{len(sources)} 个方言源的产物全部同步，代码区无一词多译、绑定位干净")
    return 0


if __name__ == "__main__":
    sys.exit(main())
