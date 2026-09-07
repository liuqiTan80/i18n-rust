#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""教程代码块可编译性验证工具（CI 硬门禁）

从 tutorials/*.md 提取 ```rust 围栏代码块，逐个生成临时项目并用 rzc check
验证方言可编译性。分类与断言规则：

- 完整程序（含 `函数 主函数`）：原样验证，断言编译通过；
- 片段（无主函数）：包裹成完整程序后验证（顶层声明外置、语句进主函数），
  断言编译通过；
- 错误示例（含 ❌ / 报错 / 编译失败 / 错误[E\\d{4}] 标记，或块内首行注释
  `// 预期错误: EXXXX`）：无主函数时同样包裹（使错误在完整程序语境下复现），
  断言编译失败；若带 `// 预期错误: EXXXX[, EYYYY]` 标记，额外断言实际
  错误码包含预期码；
- 输出示例（块内 `// 预期输出: 内容` 标记）：断言编译通过且 rzc run 的
  stdout 与预期一致（多行用 `// 预期输出:` + 后续注释行；每行剥尾部空白、
  忽略首尾空行后精确比较）；
- 省略块（含 ... / …… / 省略 且无主函数）：跳过。

Result 的 `错误(...)` 构造、`错误(原因)` 模式匹配、`xxx错误` 类型名等
合法用法不会误判为错误示例（错误标记判定不含裸 `错误` 词）。

可选 --serialize 模式：按章节把非错误示例块按出现顺序拼接成一个大程序
验证（覆盖跨块续写的上下文依赖），顶层声明去重（同名保留第一个），
语句合并进主函数。注意：该模式是尽力而为的实验性验证，教程大量使用
同名函数/特征/枚举分块渐进教学与错误示例块（其变量声明被跳过），与
合并验证本质冲突，个别章节失败属固有噪音，不作为 CI 门禁。

退出码：非预期失败为 1（CI 门禁用）；--json 输出机器可读报告。

用法：
  python3 tools/verify-tutorials.py [--serialize] [--json 报告.json] \
      [--rzc 路径] [--dir 教程目录] [--parallel N]
"""
import argparse
import glob
import json
import os
import re
import subprocess
import sys
import tempfile
import textwrap
from concurrent.futures import ThreadPoolExecutor

# ---------- 常量 ----------
OMIT_MARKS = ("...", "……", "省略")
# 错误示例标记：裸 `错误` 词（Result::Err 构造/错误类型名）不判定
ERR_MARKS = ("❌", "报错", "编译失败")
# 错误码提取：兼容各语言诊断前缀（中文 错误 / 日文 エラー / 英文 error / 俄文 Ошибка）
ERR_CODE_RE = re.compile(r"(?:错误|エラー|error|[Оо]шибка)\[?\s*(E\d{4})\]?")
# 预期标记行：代码块内注释，支持三类：
#   `// 预期错误: E0384[, E0308]`   断言编译失败，且实际错误码包含预期码
#   `// 预期错误: any`              断言编译失败（任意错误）
#   `// 预期行为: 通过`             断言编译通过（风格演示/设计警告类）
#   `// 预期行为: 运行失败`         断言编译通过但运行非零退出（越界/切片 panic 类）
#   `// 预期输出: 3`                断言编译通过且 stdout 与预期一致（单行）
#   `// 预期输出:` + 后续注释行    断言多行输出（须放在代码块末尾）
EXPECT_ERR_RE = re.compile(r"^\s*//\s*预期错误[:：]\s*(E\d{4}(?:\s*[,，]\s*E\d{4})*|any)\s*$")
EXPECT_BEHAVIOR_RE = re.compile(r"^\s*//\s*预期行为[:：]\s*(通过|运行失败)\s*$")
EXPECT_OUT_RE = re.compile(r"^\s*//\s*预期输出[:：]\s*(.*?)\s*$")
# 多行预期输出：`// 预期输出:` 之后的注释行（`// 内容` → 内容）
EXPECT_OUT_LINE_RE = re.compile(r"^\s*//\s?(.*)$")

# 片段包裹：顶层声明词头（其后允许空格/泛型参数/括号/!；
# 异步/不安全 为前缀修饰，后跟 函数/结构体/块）
DECL_WORDS = ("fn ", "struct ", "impl ", "enum ", "use ", "mod ", "pub ", "const ", "static ", "type ",
              "使用", "结构体", "实现", "特征", "枚举", "常量",
              "类型", "函数", "外部", "宏规则", "宏", "模块", "异步", "不安全",
              # 日本語 (ja)
              "関数 ", "構造体 ", "実装 ", "列挙型 ", "使用 ", "定数 ", "型 ",
              "モジュール ", "公開 ", "外部 ", "トレイト ", "非同期 ", "安全でない ")

CARGO_TMPL = """[package]
name = "verify"
version = "0.1.0"
edition = "2021"

[dependencies]
"""


# ---------- 提取 ----------
def extract_blocks(md_path):
    """返回 [(文件基名, 起始行, 节标题, 块内容)]，起始行为 1-based 围栏内首行。
    节标题为最近的 `#`/`##`/`###` 标题行文本（串联模式按节分组）。"""
    with open(md_path, encoding="utf-8") as f:
        lines = f.readlines()
    blocks = []
    in_block = False
    start = 0
    section = ""
    for i, ln in enumerate(lines):
        s = ln.strip()
        if not in_block:
            m = re.match(r"^#{1,3}\s+(.+)$", s)
            if m:
                section = m.group(1).strip()
            if s == "```rust":
                in_block = True
                start = i + 1
        else:
            if s.startswith("```"):
                blocks.append((os.path.basename(md_path), start, section,
                               "".join(lines[start:i])))
                in_block = False
    if in_block:
        blocks.append((os.path.basename(md_path), start, section, "".join(lines[start:])))
    return blocks


# ---------- 分类 ----------

# 各语言的主函数签名片段（用于"完整程序"识别与包裹头）。
MAIN_FUNCS = {
    "zh": "函数 主函数",
    "ja": "関数 主関数",
    "ru": "функция главная",
}
# 未登记方言的语言（en 等）按标准 Rust 处理。
STD_MAIN = "fn main"


def is_complete_program(content, lang="zh"):
    """完整程序识别：语言对应的方言主函数，或标准 Rust（fn main）。"""
    if lang in MAIN_FUNCS:
        return MAIN_FUNCS[lang] in content
    return STD_MAIN in content


NATIVE_KEYWORDS = {
    "zh": ("函数", "让 ", "打印行", "如果", "匹配", "循环", "对于"),
    "ja": ("関数", "宣言 ", "表示行", "もし", "マッチ", "ループ", "各"),
    "ru": ("функция", "пусть ", "печатай_строку", "если", "матч", "цикл", "для "),
}


def has_native_keywords(content, lang):
    """片段是否使用对应语言方言关键词（决定包裹主函数用哪种头）。"""
    if lang in NATIVE_KEYWORDS:
        return any(k in content for k in NATIVE_KEYWORDS[lang])
    # 标准 Rust：用 fn/let/struct... 等英文关键词探测
    return any(k in content for k in ("fn ", "let ", "struct ", "impl ", "match ", "println!"))


def main_header(lang):
    """各语言方言的主函数头。"""
    if lang in MAIN_FUNCS:
        return MAIN_FUNCS[lang] + "() {"
    return STD_MAIN + "() {"


def classify(content, lang="zh"):
    if any(k in content for k in OMIT_MARKS) and not is_complete_program(content, lang):
        return "省略"
    if any(m in content for m in ERR_MARKS) or ERR_CODE_RE.search(content):
        return "错误示例"
    if is_complete_program(content, lang):
        return "完整程序"
    return "片段"


def classify_task(expected, behavior, expected_output, content, lang="zh"):
    """按标记优先级定任务类型：预期错误/行为标记 → 错误示例；
    预期输出标记 → 输出示例；否则按内容分类。"""
    if expected is not None or behavior:
        return "错误示例"
    if expected_output is not None:
        return "输出示例"
    return classify(content, lang)


def parse_marks(content):
    """解析块内 `// 预期错误:` / `// 预期行为:` / `// 预期输出:` 标记。

    返回 (预期错误码集合或 None, 行为标记字符串或 None, 预期输出字符串或 None,
    剥离标记后的内容)。
    behavior 取值：None（默认按编译失败断言）/ "通过" / "运行失败"。
    expected 为 None 时（无预期错误标记或 any）只断言编译失败。
    expected_output 为 None 时不做运行断言；非 None 时断言编译通过且
    stdout 与预期一致（多行形式：`// 预期输出:` 后连续注释行皆为输出内容）。"""
    lines = content.splitlines(keepends=True)
    expected = None
    behavior = None
    expected_output = None
    rest = []
    i, n = 0, len(lines)
    while i < n:
        ln = lines[i]
        m = EXPECT_ERR_RE.match(ln)
        if m:
            text = m.group(1)
            expected = set() if text == "any" else set(re.split(r"[,，]", text))
            i += 1
            continue
        m = EXPECT_BEHAVIOR_RE.match(ln)
        if m:
            behavior = m.group(1)
            i += 1
            continue
        m = EXPECT_OUT_RE.match(ln)
        if m:
            inline = m.group(1)
            out_lines = [inline] if inline else []
            i += 1
            if not inline:
                # 多行形式：收集后续注释行直至非注释行
                while i < n:
                    cm = EXPECT_OUT_LINE_RE.match(lines[i])
                    if cm:
                        out_lines.append(cm.group(1))
                        i += 1
                    else:
                        break
            expected_output = "\n".join(out_lines)
            continue
        rest.append(ln)
        i += 1
    return expected, behavior, expected_output, "".join(rest)


# ---------- 片段包裹 ----------
def is_decl_head(st):
    for w in DECL_WORDS:
        if st.startswith(w):
            rest = st[len(w):]
            if not rest or rest[0] in " <(!":
                return True
    return False


def ends_with(line, ch):
    """行剥掉行尾注释后（`//` 之后忽略）是否以 ch 结尾。

    取最后一个 ch 的位置判断——字符串字面量里的 `//`（如
    `"http://x";`）不影响结果；行尾注释（如 `使用 a; // 注释`）
    不破坏 `;`/`}` 收口判定。"""
    s = line.rstrip()
    pos = s.rfind(ch)
    if pos < 0:
        return False
    tail = s[pos + 1:].lstrip()
    return not tail or tail.startswith("//")


def collect_decl(lines, i):
    """从 i 行开始收集完整顶层声明，返回结束下标（不含）。

    - `#[...]` 属性行并入下一个声明；
    - 块类声明（函数/结构体/枚举/实现/特征/宏规则/外部）允许签名跨行
      （如 `哪里` 子句换行），找到 `{` 后按花括号配对收口；
    - 非块类单行声明（使用/常量/类型/宏 简式）以 `;` 收口。
    收口判定忽略行尾注释（`使用 a::b; // 注释` 仍按 `;` 收口）。
    不按行尾 `;` 收块声明——函数体最后语句也可能以 `;` 结尾。"""
    j = i
    while j < len(lines) and lines[j].strip().startswith("#["):
        j += 1
    if j >= len(lines):
        return j
    st = lines[j].strip()
    is_block = bool(re.match(
        r"^(?:(?:异步|不安全)\s+)?(函数|结构体|枚举|实现|特征|外部|宏规则|宏|模块)\b", st))
    if not is_block:
        j += 1
        while j < len(lines):
            if ends_with(lines[j - 1], ";"):
                break
            j += 1
        return j
    depth = 0
    started = False
    while j < len(lines):
        if not started:
            if "{" not in lines[j]:
                # 函数原型/单行无体声明（trait 内常见）
                if ends_with(lines[j], ";"):
                    return j + 1
                j += 1
                continue
            started = True
        depth += lines[j].count("{") - lines[j].count("}")
        j += 1
        if depth <= 0 and ends_with(lines[j - 1], "}"):
            break
    return j


def wrap_snippet(content, lang="zh"):
    """片段包裹：按花括号配对识别完整顶层声明（含 结构体/枚举/函数/宏规则 块），
    声明放外面，其余语句进主函数"""
    lines = content.splitlines(keepends=True)
    top, body = [], []
    i, n = 0, len(lines)
    while i < n:
        st = lines[i].strip()
        if not st:
            (top if top and not body else body).append(lines[i])
            i += 1
            continue
        if st.startswith("//"):
            # 注释行独立进顶层，不启动配对收集（避免吞掉后续语句）
            top.append(lines[i])
            i += 1
            continue
        if is_decl_head(st) or st.startswith("#["):
            j = collect_decl(lines, i)
            top.extend(lines[i:j])
            i = j
        else:
            body.append(lines[i])
            i += 1
    if not body:
        body = ["    // （无语句）\n"]
    header = "\n" + main_header(lang) + "\n" if has_native_keywords(content, lang) else "\n" + main_header(lang) + "\n"
    return "".join(top) + header + "".join(body) + "}\n"


# ---------- 项目生成 ----------
def make_project(work_dir, src_text, deps="", lang="zh"):
    """在 work_dir 下生成 Cargo.toml + src/主函数.zh，返回 src 路径"""
    os.makedirs(os.path.join(work_dir, "src"), exist_ok=True)
    with open(os.path.join(work_dir, "Cargo.toml"), "w", encoding="utf-8") as f:
        f.write(CARGO_TMPL.replace("[dependencies]\n", "[dependencies]\n" + deps))
    ext = {"zh": "主函数.zh"}.get(lang, f"main.{lang}")
    src_path = os.path.join(work_dir, "src", ext)
    with open(src_path, "w", encoding="utf-8") as f:
        f.write(src_text)
    return src_path


def block_deps(fname, index):
    """按块上下文补充外部依赖（fname 为教程文件名，index 为全库块序号）"""
    deps = ""
    if "第二十四章" in fname:
        deps += 'futures = "0.3"\n'
    if index == 96:  # 25.11 彩蛋（需 rustc_lexer）
        deps += 'rustc_lexer = "0.1"\n'
    return deps


# ---------- 验证 ----------
def normalize_output(text):
    """规范化输出：每行剥尾部空白，去首尾空行（中间空行保留）。"""
    lines = [ln.rstrip() for ln in text.splitlines()]
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    return "\n".join(lines)


def output_matches(want, actual):
    """预期行按序匹配实际输出行（宽容编译警告等附加输出）。

    实际输出可能夹杂编译警告（如 警告[dead_code]）与帮助信息，
    只要预期各行按原顺序出现即为匹配。"""
    lines = want.splitlines()
    i = 0
    for ln in actual.splitlines():
        if i < len(lines) and ln == lines[i]:
            i += 1
    return i == len(lines)


def check_one(rzc, src_path, cwd, expected, behavior, expected_output=None, timeout=120):
    """rzc check（需要运行时验证时加 rzc run）。

    返回 (ok, returncode, full_output)。ok 语义：
    - behavior 为 "通过"：编译通过
    - behavior 为 "运行失败"：编译通过且运行非零退出
    - expected_output 非 None：编译通过且 stdout 规范化后与预期一致
    - 其他（错误示例默认）：编译失败"""
    try:
        r = subprocess.run([rzc, "check", src_path], capture_output=True, text=True,
                           timeout=timeout, cwd=cwd)
        out = (r.stdout + r.stderr).strip()
        compiled_ok = r.returncode == 0
        if expected_output is not None:
            if not compiled_ok:
                return False, r.returncode, out
            rr = subprocess.run([rzc, "run", src_path], capture_output=True, text=True,
                                timeout=timeout, cwd=cwd)
            if rr.returncode != 0:
                return False, rr.returncode, (rr.stdout + rr.stderr).strip()
            actual = normalize_output(rr.stdout)
            want = normalize_output(expected_output)
            if not output_matches(want, actual):
                return False, rr.returncode, ("输出不匹配\n"
                                              f"  预期: {want!r}\n"
                                              f"  实际: {actual!r}")
            return True, rr.returncode, rr.stdout
        if behavior == "运行失败":
            if not compiled_ok:
                return False, r.returncode, out
            rr = subprocess.run([rzc, "run", src_path], capture_output=True, text=True,
                                timeout=timeout, cwd=cwd)
            out = (rr.stdout + rr.stderr).strip()
            return rr.returncode != 0, rr.returncode, out
        if behavior == "通过":
            return compiled_ok, r.returncode, out
        # 错误示例默认：断言编译失败（预期错误码由调用方核对）
        return compiled_ok, r.returncode, out
    except subprocess.TimeoutExpired:
        return False, -1, "超时"
    except Exception as e:
        return False, -2, str(e)


def actual_error_codes(output):
    return set(ERR_CODE_RE.findall(output))


# ---------- 单块任务 ----------
def build_task(work, index, fname, start, content, expected, behavior, expected_output, serialize, lang="zh"):
    """构造单个块的验证：完整程序/错误示例（无主函数则包裹）/片段包裹。

    带 `// 预期错误:` 或 `// 预期行为:` 标记的块按错误示例断言
    （教学意图明确的故意报错/风格演示），不因缺少 ❌ 而走片段断言；
    带 `// 预期输出:` 标记的块归为"输出示例"，断言编译通过 + 输出匹配。"""
    kind = classify_task(expected, behavior, expected_output, content, lang)
    if serialize:
        # 串联模式：由调用方拼接，这里不单独验证
        return None
    if kind == "省略":
        return None
    deps = block_deps(fname, index)
    if kind == "错误示例":
        if is_complete_program(content, lang):
            src = content
        else:
            src = wrap_snippet(textwrap.dedent(content), lang)
    elif kind == "完整程序":
        src = content
    elif kind == "输出示例":
        # 与完整程序同规则：有主函数原样，无主函数（片段）包裹
        src = content if is_complete_program(content, lang) else wrap_snippet(textwrap.dedent(content), lang)
    else:
        src = wrap_snippet(textwrap.dedent(content), lang)
    d = os.path.join(work, f"b{index:03d}")
    os.makedirs(d, exist_ok=True)
    src_path = make_project(d, src, deps, lang)
    return (index, fname, start, kind, expected, behavior, expected_output, src_path, d)


# ---------- 串联（章节级） ----------
def extract_main_body(content, lang="zh"):
    """括号配对提取 `函数 主函数() { ... }` 或对应语言的等价主函数，返回 (函数体, 其余部分)。
    主函数签名行整体从其余部分移除（残留签名会让后续 collect_decl 配对错乱）。
    注释行里的主函数签名不匹配（先跳过以 `//` 开头的行）。
    找不到时返回 (None, content)。"""
    main_re = re.compile(r"函数\s+主函数\s*\(\s*\)" if lang == "zh"
                         else (r"関数\s+主関数\s*\(\s*\)" if lang == "ja"
                               else r"fn\s+main\s*\(\s*\)"))
    for m in main_re.finditer(content):
        line_start = content.rfind("\n", 0, m.start()) + 1
        if content[line_start:m.start()].strip().startswith("//"):
            continue
        try:
            i = content.index("{", m.start())
        except ValueError:
            continue
        depth = 0
        for j in range(i, len(content)):
            if content[j] == "{":
                depth += 1
            elif content[j] == "}":
                depth -= 1
                if depth == 0:
                    return content[i + 1:j], content[:line_start] + content[j + 1:]
    return None, content


def decl_key(decl):
    """提取定义名用于去重（先剥掉 `#[...]` 属性前缀）：
    - 函数/结构体/枚举/特征/模块（含 `异步 函数`）同名保留第一个（E0428）；
    - `实现 特征 对于 类型`（特征可带泛型参数）按 (特征, 类型) 去重（E0119）；
    - inherent impl（`实现 类型`）多块合法，不去重（KEEP）；
    - 使用/常量/类型/宏 按完整文本去重（使用 剥掉行内注释后比对，
      避免同一路径因注释差异重复 use → E0252）。"""
    body = re.sub(r"^(?:\s*#\[[^\]]*\]\s*)+\s*", "", decl)
    m = re.match(r"^使用\s+(.+?)\s*;", body, re.S)
    if m:
        return ("使用", m.group(1).strip())
    m = re.match(r"^(?:(?:异步|不安全)\s+)?(函数|结构体|枚举|特征|模块)\s+([A-Za-z_一-鿿][\w一-鿿]*)", body)
    if m:
        return (m.group(1), m.group(2))
    # 特征名支持 `::` 路径（如 标准库::错误模块::错误特征）与泛型参数
    m = re.match(r"^实现\s+([A-Za-z_一-鿿][\w一-鿿]*(?:::[A-Za-z_一-鿿][\w一-鿿]*)*(?:<[^>]*>)?)\s+对于\s+"
                 r"([A-Za-z_一-鿿][\w一-鿿]*(?:::[A-Za-z_一-鿿][\w一-鿿]*)*(?:<[^>]*>)?)", body)
    if m:
        return ("impl", m.group(1), m.group(2))
    if body.startswith("实现"):
        return ("KEEP",)
    return None


def impl_method_names(decl):
    """提取 impl 块内 `函数 名字` 方法名集合（同名方法跨 impl 块重复 → E0592）。"""
    return set(re.findall(r"^\s*函数\s+([A-Za-z_一-鿿][\w一-鿿]*)", decl, flags=re.M))


def append_decl(decl, raw_lines, top_lines, seen_decl, seen_name, seen_methods):
    """顶层定义去重后加入 top_lines。

    - 方法片段（`函数 名字(&自我)` 等需 impl 上下文）跳过：拼接无意义；
    - 同名 函数/结构体/枚举/特征 定义保留第一个（教学分块不同写法）；
    - `实现 特征 对于 类型` 按 (特征, 类型) 去重；inherent impl 多块合法；
    - impl 内同名方法跨块重复时跳过（E0592）；其余按完整声明文本去重。
    返回 True 表示已加入或按规则跳过，调用方继续；False 未使用（保留签名对称）。"""
    if re.match(r"^(?:异步\s+)?函数\s+\w+\s*\([^)]*\b自我\b", decl):
        return False
    key = decl_key(decl)
    if key == ("KEEP",):
        methods = impl_method_names(decl)
        if methods & seen_methods:
            return False
        seen_methods |= methods
        top_lines.extend(raw_lines)
        return True
    if key is not None:
        if key in seen_name:
            return False
        seen_name.add(key)
        if key[0] == "impl":
            methods = impl_method_names(decl)
            if methods & seen_methods:
                return False
            seen_methods |= methods
    elif decl in seen_decl:
        return False
    else:
        seen_decl.add(decl)
    top_lines.extend(raw_lines)
    return True


def build_serialized(work, chapter_blocks, base_name, allowlist, deps="", lang="zh"):
    """把一章的非错误示例块按顺序拼接：顶层声明去重，语句合并进主函数。

    跳过带教学标记的块（故意报错/风格演示）与白名单中 category 为
    故意报错/环境依赖/练习答案 的块；上下文依赖白名单块保留（串联的意义正是补全它们）。
    声明与语句均保持原序（变量同名遮蔽合法，方法链续行不会被拆散）；
    完整程序块的主函数体用 `{ }` 作用域隔离（块内结构体/函数定义不跨块冲突）；
    函数/结构体/枚举/特征 同名保留第一个，`实现 特征 对于 类型` 按 (特征, 类型)
    去重，inherent impl 多块合法；impl 内同名方法跨块去重；方法片段（&自我）跳过。
    返回 (项目 src 路径, 项目目录)，失败返回 (None, None)。"""
    seen_decl = set()
    seen_name = set()
    seen_methods = set()
    top_lines, body_lines, main_lines = [], [], []
    for index, fname, start, content, expected, behavior, expected_output in chapter_blocks:
        if expected is not None or behavior or expected_output is not None:
            continue
        item = allowlist.get((fname, start))
        if item and item.get("category") in ("故意报错", "环境依赖", "练习答案"):
            continue
        if classify(content, lang) in ("错误示例", "省略"):
            continue
        content = textwrap.dedent(content)
        if (MAIN_FUNCS.get(lang, STD_MAIN)) in content:
            # 完整程序块：主函数体提取为语句（作用域隔离，避免块内 item 重复定义），
            # 其余顶层声明并入（按定义名去重）
            body, rest = extract_main_body(content, lang)
            if body is not None:
                main_lines.append("{\n" + body + "}\n")
            rest_lines = (rest or "").splitlines(keepends=True)
            k, n2 = 0, len(rest_lines)
            while k < n2:
                st = rest_lines[k].strip()
                if not st or st.startswith("//") or (MAIN_FUNCS.get(lang, STD_MAIN) in st):
                    k += 1
                    continue
                if is_decl_head(st) or st.startswith("#["):
                    j2 = collect_decl(rest_lines, k)
                    decl = "".join(rest_lines[k:j2]).strip()
                    append_decl(decl, rest_lines[k:j2], top_lines, seen_decl, seen_name, seen_methods)
                    k = j2
                else:
                    body_lines.append(rest_lines[k])
                    k += 1
            continue
        # 片段：声明与语句分流（完整声明按花括号配对收集）
        lines = content.splitlines(keepends=True)
        i, n = 0, len(lines)
        while i < n:
            st = lines[i].strip()
            if not st or st.startswith("//"):
                i += 1
                continue
            if is_decl_head(st) or st.startswith("#["):
                j = collect_decl(lines, i)
                decl = "".join(lines[i:j]).strip()
                append_decl(decl, lines[i:j], top_lines, seen_decl, seen_name, seen_methods)
                i = j
            else:
                body_lines.append(lines[i])
                i += 1
    src = "".join(top_lines) + "\n" + main_header(lang) + "\n" \
        + "".join(main_lines) + "".join(body_lines) + "}\n"
    d = os.path.join(work, f"serial_{base_name}")
    os.makedirs(d, exist_ok=True)
    src_path = make_project(d, src, deps, lang)
    return src_path, d


# ---------- 主流程 ----------
def main():
    ap = argparse.ArgumentParser(description="教程代码块可编译性验证（CI 硬门禁）")
    ap.add_argument("--lang", default="zh", help="教程语言（决定临时项目扩展名与包裹头）")
    ap.add_argument("--dir", default=None, help="教程目录（默认仓库根 tutorials/）")
    ap.add_argument("--rzc", default=None, help="rzc 可执行文件（默认仓库内 target/debug/rzc）")
    ap.add_argument("--json", default=None, help="输出机器可读报告到文件")
    ap.add_argument("--parallel", type=int, default=10, help="并行度（默认 10）")
    ap.add_argument("--serialize", action="store_true", help="附加章节串联验证")
    ap.add_argument("--allowlist", default=None,
                    help="预期失败白名单 JSON（条目 {file, line, category, note}）",)
    args = ap.parse_args()

    allowlist = {}
    if args.allowlist and os.path.isfile(args.allowlist):
        with open(args.allowlist, encoding="utf-8") as f:
            for item in json.load(f):
                allowlist[(item["file"], item["line"])] = item
    elif args.allowlist:
        print(f"⚠️ 白名单不存在：{args.allowlist}（先 --gen-allowlist 生成）", file=sys.stderr)

    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    tut_dir = args.dir or os.path.join(repo, "tutorials")
    rzc = args.rzc or os.path.join(repo, "target", "debug", "rzc")
    # 规范化绝对路径：check_one 在 /tmp 临时项目目录下以 cwd 运行 rzc，
    # 相对路径（如 --rzc target/debug/rzc）在子进程 cwd 下找不到可执行文件
    rzc = os.path.abspath(rzc)
    if not os.path.isfile(rzc):
        rzc = os.path.join(repo, "target", "release", "rzc")
    if not os.path.isfile(rzc):
        print(f"❌ 找不到 rzc 可执行文件（先运行 cargo build）：{rzc}", file=sys.stderr)
        return 2

    # 1. 提取全部块
    all_blocks = []  # (index, fname, start, section, content, expected, behavior, expected_output)
    for path in sorted(glob.glob(os.path.join(tut_dir, "*.md"))):
        for fname, start, section, content in extract_blocks(path):
            expected, behavior, expected_output, content = parse_marks(content)
            all_blocks.append((len(all_blocks), fname, start, section, content, expected, behavior, expected_output))

    # 2. 生成并验证单块
    work = tempfile.mkdtemp(prefix="zrverify_")
    tasks = []
    stats = {"完整程序": 0, "片段": 0, "错误示例": 0, "输出示例": 0, "省略": 0}
    for index, fname, start, section, content, expected, behavior, expected_output in all_blocks:
        kind = classify_task(expected, behavior, expected_output, content)
        stats[kind] += 1
        t = build_task(work, index, fname, start, content, expected, behavior, expected_output, args.serialize, args.lang)
        if t:
            tasks.append(t)

    results = []

    def run(t):
        index, fname, start, kind, expected, behavior, expected_output, src_path, d = t
        ok, rc, out = check_one(rzc, src_path, d, expected, behavior, expected_output)
        codes = actual_error_codes(out)
        verdict = "PASS"
        reason = ""
        if kind == "错误示例":
            if behavior == "通过":
                if not ok:
                    verdict, reason = "FAIL", "预期通过却编译失败"
            elif behavior == "运行失败":
                if not ok:
                    verdict, reason = "FAIL", "预期编译通过但运行失败，实际编译未通过"
            else:
                if ok:
                    verdict, reason = "FAIL", "预期失败却编译通过"
                elif expected and not expected & codes:
                    verdict, reason = "FAIL", f"错误码不匹配：预期 {sorted(expected)}，实际 {sorted(codes) or '无 E 码'}"
        elif kind == "输出示例":
            if not ok:
                verdict, reason = "FAIL", f"编译失败或输出不匹配：{out[-300:]}"
        else:
            if not ok:
                verdict, reason = "FAIL", "编译失败"
                item = allowlist.get((fname, start))
                if item:
                    verdict = "EXPECTED"
                    reason = f"预期失败（{item.get('category', '?')}：{item.get('note', '')}）"
        return {"id": index, "file": fname, "line": start, "kind": kind,
                "verdict": verdict, "reason": reason, "codes": sorted(codes)}

    with ThreadPoolExecutor(max_workers=args.parallel) as ex:
        results = list(ex.map(run, tasks))

    # 3. 串联模式（按章分组拼接；定义按名去重、变量全局提升，
    #    既补全跨节依赖，也消除同名定义冲突噪音）
    serial_results = []
    if args.serialize:
        chapters = {}
        for index, fname, start, section, content, expected, behavior, expected_output in all_blocks:
            chapters.setdefault(fname, []).append(
                (index, fname, start, content, expected, behavior, expected_output))
        for fname, blocks in sorted(chapters.items()):
            # 目录名只保留字母数字下划线中文：`:` 反引号等会破坏 rustc 路径拼接
            base_name = re.sub(r"[^\w一-鿿-]", "-", os.path.splitext(fname)[0])
            deps = block_deps(fname, -1)
            if "第二十五章" in fname:
                deps += 'rustc_lexer = "0.1"\n'
            src_path, d = build_serialized(work, blocks, base_name, allowlist, deps, args.lang)
            ok, rc, out = check_one(rzc, src_path, d, set(), None, timeout=180)
            codes = actual_error_codes(out)
            serial_results.append({
                "file": fname,
                "verdict": "PASS" if ok else "FAIL",
                "reason": "" if ok else "串联编译失败",
                "codes": sorted(codes),
                "output": out[-500:] if not ok else "",
            })

    # 4. 报告
    fails = [r for r in results if r["verdict"] == "FAIL"]
    expecteds = [r for r in results if r["verdict"] == "EXPECTED"]
    print(f"提取 {len(all_blocks)} 个代码块（{stats}）")
    print(f"单块验证：通过 {len(results) - len(fails) - len(expecteds)} / {len(results)}，"
          f"预期失败 {len(expecteds)}，失败 {len(fails)}")
    for r in sorted(fails, key=lambda x: x["id"]):
        print(f"  ❌ b{r['id']:03d} {r['file']} L{r['line']} [{r['kind']}] {r['reason']}")
    # 过期检查仅在单块验证执行后有意义（--serialize 附加模式不跑单块，
    # 白名单条目无从匹配，全部误报过期）
    if allowlist and not args.serialize:
        matched = {(r["file"], r["line"]) for r in expecteds}
        for key in sorted(allowlist):
            if key not in matched:
                print(f"  ⚠️ 白名单过期条目（已不失败，建议删除）：{key[0]} L{key[1]}")
    sfails = []
    if args.serialize:
        sfails = [r for r in serial_results if r["verdict"] != "PASS"]
        print(f"串联验证：通过 {len(serial_results) - len(sfails)} / {len(serial_results)}，失败 {len(sfails)}")
        for r in sfails:
            print(f"  ❌ 串联 {r['file']}：{r['reason']}")
            print(f"     {r['output'].replace(chr(10), chr(10) + '     ')[:400]}")

    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump({"blocks": len(all_blocks), "stats": stats,
                       "single": results, "serial": serial_results},
                      f, ensure_ascii=False, indent=1)

    return 1 if fails or sfails else 0


if __name__ == "__main__":
    sys.exit(main())
