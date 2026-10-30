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
- 省略块（代码里出现 ... / …… / 省略，或以 `// ...` 起头占位，且无主函数）：跳过。

Result 的 `错误(...)` 构造、`错误(原因)` 模式匹配、`xxx错误` 类型名等
合法用法不会误判为错误示例（错误标记判定不含裸 `错误` 词）。

可选 --serialize 模式：按章节把非错误示例块按出现顺序拼接成一个大程序
验证（覆盖跨块续写的上下文依赖），顶层声明去重（同名保留第一个），
语句合并进主函数。注意：该模式是尽力而为的实验性验证，教程大量使用
同名函数/特征/枚举分块渐进教学与错误示例块（其变量声明被跳过），与
合并验证本质冲突，个别章节失败属固有噪音，不作为 CI 门禁。

可选 --ctx 模式（上下文复验，CI 门禁）：白名单块「单独编译失败」不等于
「不必编译」——把该章到此块为止的前文按串联规则拼进临时项目再验一次
（读者照着书敲到这一步，用的正是这份前文），每个白名单块得到三种归属
之一：`前文补全通过`（等价于已验证）、`本块致错`（去掉本块前文能编译，
加上就报错——正文里真实存在的墙）、`前文噪音`（前文自己还编不过，无法
归因）。「前文补全通过」数受基线下限保护（只增不减，见
`tools/ctx-baseline.json`），教程改动导致它下降即判红；白名单收缩（条目因修好
而删除）导致的等量下降除外。

退出码：非预期失败为 1（CI 门禁用）；--json 输出机器可读报告。

用法：
  python3 tools/verify-tutorials.py [--serialize] [--ctx] [--ctx-update] \
      [--json 报告.json] [--rzc 路径] [--dir 教程目录] [--parallel N]
"""
import argparse
import atexit
import glob
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import textwrap
from concurrent.futures import ThreadPoolExecutor

# ---------- 常量 ----------
OMIT_MARKS = ("...", "……", "省略")
# 行尾注释起点：冒号后的 //（协议 URL）不算注释，避免把字符串里的地址当成注释截断
LINE_COMMENT_RE = re.compile(r"(?<!:)//")
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
# 多文件示例协议：块首行 `// 文件: cutlery.en` 声明本块写入 `src/cutlery.en`；
# 同一章节内相邻的此类块合成一个项目整体 check/run（判据是「多文件模块」
# 在单文件项目里必然报 E0583/E0601，静态检查无法覆盖）。四种语言教程通
# 用同一标记（标记本身用中文，不随教程语言变）。
FILE_MARK_RE = re.compile(r"^\s*//\s*文件\s*[:：]\s*(\S+)\s*$")
# 各语言方言里与 `main` 等价的入口文件词干（Cargo 只认 main/lib 两个 crate 根，
# 方言侧把 main 翻译后的词干也算入口——见 crates/cli/src/main.rs 的 is_entry_stem）
MAIN_STEMS = {"zh": "主函数", "ja": "主関数", "ru": "главная"}

# 上下文复验覆盖的白名单类别：这两类的成立条件都是「定义散落在前后块」，
# 拼上前文即可复现读者的真实输入；`故意报错`/`练习答案` 拼上前文也不该编译
# 通过（前者是反例，后者缺的答案留给读者），复验它们只会制造假红
CTX_CATEGORIES = ("上下文依赖", "教学演示")
# 复验三态
CTX_PASS, CTX_MINE, CTX_NOISE = "前文补全通过", "本块致错", "前文噪音"
# 上下文复验的编译耗时上限比单块长（前文越拼越长），与 --serialize 同档
CTX_TIMEOUT = 180

# 片段包裹：顶层声明词头（其后允许空格/泛型参数/括号/!；
# 异步/不安全 为前缀修饰，后跟 函数/结构体/块）；
# 英文 trait 与 collect_decl 的 is_block 正则保持一致（en 教程用）
DECL_WORDS = ("fn ", "struct ", "impl ", "enum ", "trait ", "use ", "mod ", "pub ", "const ", "static ", "type ",
              "使用", "结构体", "实现", "特征", "枚举", "常量",
              "类型", "函数", "外部", "宏规则", "宏", "模块", "异步", "不安全",
              # 日本語 (ja)
              "関数 ", "構造体 ", "実装 ", "列挙型 ", "使用 ", "定数 ", "型 ",
              "モジュール ", "公開 ", "外部 ", "トレイト ", "非同期 ", "安全でない ",
              # Русский (ru)
              "функция ", "структура ", "реализация ", "перечисление ", "используй ", "константа ", "тип ",
              "модуль ", "публичный ", "внешний ", "трейт ", "асинхронно ", "небезопасно ")

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


def main_pattern(lang):
    """主函数签名的匹配式，统一从 MAIN_FUNCS / STD_MAIN 生成。

    以前这里是 `zh / ja / 其余按 fn main` 三分支：ru 的 `функция главная` 在
    `is_complete_program` 里认得到、在这里认不到，于是含主函数的 ru 块拆不出函数体，
    串联出来的前文多出一个孤立 `}`（实测 ru 第十一章因此一直归不进「前文补全通过」）。
    新增方言只需往上填 MAIN_FUNCS。"""
    name = MAIN_FUNCS.get(lang, STD_MAIN)
    return re.compile(r"\s+".join(map(re.escape, name.split())) + r"\s*\(\s*\)")


NATIVE_KEYWORDS = {
    "zh": ("函数", "让 ", "打印行", "如果", "匹配", "循环", "对于"),
    "ja": ("関数", "宣言 ", "表示行", "もし", "マッチ", "ループ", "各"),
    "ru": ("функция", "пусть ", "печатай_строку", "если", "матч", "цикл", "для ", "структура"),
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


def has_omit_mark(content):
    """块内是否真的省略了代码（决定该块跳过验证）。

    省略标记必须出现在**代码**里，或作为**注释的第一个 token**（`// ... 这里略过实现`
    才是作者把内容省了）。写在注释中间的词不算——`让 全部 = &分数[..]; // 两头都省略`
    讲的是范围语法，`} // ❌ 名次还可能是 3、4、5……` 讲的是穷尽匹配，两者都是要验证的
    正文（实测第四章的切片块因此被整块跳过，连累下一块在 `--ctx` 里被误判成本块致错）。
    """
    for line in content.splitlines():
        s = line.strip()
        if s.startswith("//"):
            if s[2:].lstrip().startswith(OMIT_MARKS):
                return True
            continue
        if any(k in LINE_COMMENT_RE.split(line, maxsplit=1)[0] for k in OMIT_MARKS):
            return True
    return False


def classify(content, lang="zh"):
    if has_omit_mark(content) and not is_complete_program(content, lang):
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


def parse_file_mark(content):
    """剥离块首的 `// 文件: 名字` 声明，返回 (文件名或 None, 剩余内容)。

    只认块首注释区（首个非空行开头的连续注释行），避免把正文里的同名注
    释误当作文件声明。文件名必须是单个路径段（不带目录分隔）。
    """
    lines = content.splitlines(keepends=True)
    for i, ln in enumerate(lines):
        st = ln.strip()
        if not st:
            continue
        if not st.startswith("//"):
            break
        m = FILE_MARK_RE.match(ln)
        if m:
            name = m.group(1)
            if "/" in name or "\\" in name:
                return None, content
            return name, "".join(lines[:i] + lines[i + 1:])
    return None, content


def is_entry_file(name, lang="zh"):
    """文件名是否为方言项目的入口（main 或母语 main 词干，且直接在 src/ 下）"""
    stem = os.path.splitext(os.path.basename(name))[0]
    return stem == "main" or stem == MAIN_STEMS.get(lang, "")


# ---------- 片段包裹 ----------
def is_decl_head(st):
    for w in DECL_WORDS:
        base = w.rstrip()
        if st.startswith(base):
            rest = st[len(base):]
            # 词后必须是边界：空行/泛型/括号/感叹/属性，或非 ASCII 母语标识符
            # （排除前缀误配，如 функцияX、结构体学生 → 学生 是合法标识符的一部分）
            if not rest or rest[0] in " <(!" or ord(rest[0]) > 127:
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
        r"^(?:(?:异步|不安全|асинхронно|небезопасно)\s*)?(?:函数|结构体|枚举|实现|特征|外部|宏规则|宏|模块|"
        r"fn|struct|enum|impl|trait|use|mod|extern|"
        r"функция|структура|перечисление|реализация|трейт|модуль|внешний|"
        r"関数|構造体|列挙型|実装|トレイト|モジュール|公開|外部|非同期|安全でない)\b", st))
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


# ---------- 多文件任务 ----------
def build_multifile_task(work, group, file_names, lang="zh"):
    """把一组带 `// 文件:` 声明的相邻块写成一个项目的多个源文件整体验证。

    group 为同章节相邻的块（已按文档顺序），file_names 为 index → 文件名。
    组内必须恰好有一个入口文件（main 或该语言的 main 母语词干）；标记
    （预期错误/输出）从组内第一个非空标记继承。返回 10 元组，末位
    problem 为书写错误的原因（缺入口、重名文件），非 None 时永远判红。
    """
    index, fname, start = group[0][0], group[0][1], group[0][2]
    expected = next((b[5] for b in group if b[5] is not None), None)
    behavior = next((b[6] for b in group if b[6] is not None), None)
    expected_output = next((b[7] for b in group if b[7] is not None), None)
    # 依赖声明可能落在组内任一块（常见是 main），逐块合并去重
    deps = ""
    for b in group:
        d0 = block_deps(b[1], b[0])
        deps += "".join(l + "\n" for l in d0.splitlines() if l + "\n" not in deps)
    d = os.path.join(work, f"b{index:03d}")
    os.makedirs(os.path.join(d, "src"), exist_ok=True)
    with open(os.path.join(d, "Cargo.toml"), "w", encoding="utf-8") as f:
        f.write(CARGO_TMPL.replace("[dependencies]\n", "[dependencies]\n" + deps))
    names = [file_names[b[0]] for b in group]
    entry = None
    for b, name in zip(group, names):
        with open(os.path.join(d, "src", name), "w", encoding="utf-8") as f:
            f.write(b[4])
        if is_entry_file(name, lang):
            entry = os.path.join(d, "src", name)
    if len(set(names)) != len(names):
        problem = "同组多文件示例出现重名文件（后者会静默覆盖前者）"
    elif entry is None:
        problem = "多文件示例缺少入口文件（词干须为 main 或该语言的 main 母语词）"
    else:
        problem = None
    first_path = os.path.join(d, "src", names[0])
    return (index, fname, start, "多文件", expected, behavior, expected_output,
            entry or first_path, d, problem)


# ---------- 串联（章节级） ----------
def extract_main_body(content, lang="zh"):
    """括号配对提取 `函数 主函数() { ... }` 或对应语言的等价主函数，返回 (函数体, 其余部分)。
    主函数签名行整体从其余部分移除（残留签名会让后续 collect_decl 配对错乱）。
    注释行里的主函数签名不匹配（先跳过以 `//` 开头的行）。
    找不到时返回 (None, content)。"""
    main_re = main_pattern(lang)
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


# ---------- 上下文复验（--ctx） ----------
def first_error_line(output):
    """从 rzc 输出里取第一行真错误（剥掉 lint 教学提示行，它们会淹没有用信息）"""
    kept = []
    for line in output.splitlines():
        if "（教学提示）" in line or "[警告]" in line:
            continue
        if ERR_CODE_RE.search(line):
            return line.strip()[:120]
        kept.append(line.strip())
    for line in kept:
        if line.startswith(("错误", "error", "エラー", "Ошибка")) or "error[" in line:
            return line[:120]
    return (kept[-1] if kept else "")[:120]


def run_ctx_check(rzc, work, results, all_blocks, grouped, allowlist, lang, parallel):
    """白名单块的前文上下文复验：把「到此块为止」的章节前文拼进临时项目再验一次

    返回 `[{file, line, category, verdict, error}]`，verdict 取 CTX_PASS /
    CTX_MINE / CTX_NOISE。前文拼接复用 `build_serialized`（与 --serialize 同一
    规则，避免两套口径分叉）；临时目录名带上块序与阶段，否则同章多块并行
    会写同一个 src 互相覆盖（实测会让通过数在两跑之间拖动）。"""
    chapters = {}
    for b in all_blocks:
        index, fname, start, _section, content, expected, behavior, expected_output = b
        if index in grouped:
            # 多文件块自成项目，不参与章节拼接（与 --serialize 同口径）
            continue
        chapters.setdefault(fname, []).append(
            (index, fname, start, content, expected, behavior, expected_output))
    todo = []
    for r in results:
        if r["verdict"] != "EXPECTED":
            continue
        item = allowlist.get((r["file"], r["line"]))
        if not item or item.get("category") not in CTX_CATEGORIES:
            continue
        blocks = chapters.get(r["file"])
        if not blocks:
            continue
        pos = next((i for i, t in enumerate(blocks) if t[2] == r["line"]), None)
        if pos is None:
            continue
        todo.append((r["file"], r["line"], item.get("category"), blocks, pos))

    def probe(t):
        fname, line, cat, blocks, k = t
        base = re.sub(r"[^\w一-鿿-]", "-", os.path.splitext(fname)[0])
        deps = block_deps(fname, -1)
        if "第二十五章" in fname:
            deps += 'rustc_lexer = "0.1"\n'

        def build(grp, tag):
            src_path, d = build_serialized(work, grp, f"{base}-{k}-{tag}", allowlist, deps, lang)
            if src_path is None:
                return False, "前文项目生成失败"
            ok, _rc, out = check_one(rzc, src_path, d, set(), None, timeout=CTX_TIMEOUT)
            return ok, first_error_line(out)

        ok, err = build(blocks[:k + 1], "ctx")
        if ok:
            verdict = CTX_PASS
        else:
            # 归因：本块之前的前文自己就编不过时，错不算在本块头上
            ok0, _err0 = build(blocks[:k], "base")
            verdict = CTX_NOISE if not ok0 else CTX_MINE
        return {"file": fname, "line": line, "category": cat, "verdict": verdict,
                "error": "" if verdict == CTX_PASS else err}

    with ThreadPoolExecutor(max_workers=parallel) as ex:
        return list(ex.map(probe, todo))


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
    ap.add_argument("--ctx", action="store_true",
                    help="对白名单块做前文上下文复验（CI 门禁，见文件头说明）")
    ap.add_argument("--ctx-baseline", default=None,
                    help="上下文复验基线 JSON（默认 tools/ctx-baseline.json）")
    ap.add_argument("--ctx-update", action="store_true",
                    help="把本次「前文补全通过」数写回基线（需同时 --ctx）")
    args = ap.parse_args()

    allowlist = {}
    if args.allowlist and os.path.isfile(args.allowlist):
        with open(args.allowlist, encoding="utf-8") as f:
            for item in json.load(f):
                allowlist[(item["file"], item["line"])] = item
    elif args.allowlist:
        print(f"⚠️ 白名单不存在：{args.allowlist}（格式见 tools/README.md：条目为"
              f" {{file, line, category, note}}）", file=sys.stderr)

    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    tut_dir = args.dir or os.path.join(repo, "tutorials")
    # 候选可执行文件：依次探测 debug / release（Windows 产物带 .exe）
    exe = ".exe" if os.name == "nt" else ""
    if args.rzc:
        rzc = os.path.abspath(args.rzc)
    else:
        candidates = [os.path.join(repo, "target", p, "rzc" + exe)
                      for p in ("debug", "release")]
        rzc = next((c for c in candidates if os.path.isfile(c)), candidates[0])
    if not os.path.isfile(rzc):
        print(f"❌ 找不到 rzc 可执行文件（先运行 cargo build）：{rzc}", file=sys.stderr)
        return 2

    # 1. 提取全部块
    all_blocks = []  # (index, fname, start, section, content, expected, behavior, expected_output)
    file_names = {}  # index → 多文件协议声明的文件名
    for path in sorted(glob.glob(os.path.join(tut_dir, "*.md"))):
        for fname, start, section, content in extract_blocks(path):
            declared, content = parse_file_mark(content)
            expected, behavior, expected_output, content = parse_marks(content)
            index = len(all_blocks)
            if declared:
                file_names[index] = declared
            all_blocks.append((index, fname, start, section, content, expected, behavior, expected_output))

    # 1.5 多文件分组：同文档相邻、且都带 `// 文件:` 声明的块合成一个项目
    #    （单块带声明不开组：它仍按普通块处理，避免读者用单文件写模块时被误判）
    groups = []
    grouped = set()
    i = 0
    while i < len(all_blocks):
        if all_blocks[i][0] not in file_names:
            i += 1
            continue
        grp = [all_blocks[i]]
        j = i + 1
        while (j < len(all_blocks) and all_blocks[j][1] == all_blocks[i][1]
               and all_blocks[j][0] in file_names):
            grp.append(all_blocks[j])
            j += 1
        if len(grp) >= 2:
            groups.append(grp)
            grouped.update(b[0] for b in grp)
        i = j

    # 2. 生成并验证单块
    work = tempfile.mkdtemp(prefix="zrverify_")
    # 临时工作目录随进程退出统一清理（正常结束、验证失败退出、KeyboardInterrupt
    # 均覆盖；被 SIGKILL 强杀时无法保证），避免本地反复运行在 /tmp 残留
    atexit.register(shutil.rmtree, work, ignore_errors=True)
    tasks = []
    stats = {"完整程序": 0, "片段": 0, "错误示例": 0, "输出示例": 0, "省略": 0, "多文件": 0}
    for grp in groups:
        stats["多文件"] += len(grp)
        tasks.append(build_multifile_task(work, grp, file_names, args.lang))
    # 声明了文件名却没和相邻块组成项目的孤块：不静默忽略，直接判红
    for index in sorted(set(file_names) - grouped):
        b = all_blocks[index]
        stats["多文件"] += 1
        tasks.append((b[0], b[1], b[2], "多文件", b[5], b[6], b[7], None, None,
                      "多文件块声明了文件名，却没有和相邻块组成项目（至少两个相邻块，中间不要插入其他代码块）"))
    for index, fname, start, section, content, expected, behavior, expected_output in all_blocks:
        if index in grouped:
            continue
        kind = classify_task(expected, behavior, expected_output, content)
        stats[kind] += 1
        t = build_task(work, index, fname, start, content, expected, behavior, expected_output, args.serialize, args.lang)
        if t:
            tasks.append(t)

    results = []

    def run(t):
        index, fname, start, kind, expected, behavior, expected_output, src_path, d = t[:9]
        problem = t[9] if len(t) > 9 else None
        if problem:
            # 项目本身写错了（缺入口/重名/孤块），不必浪费一次编译
            return {"id": index, "file": fname, "line": start, "kind": kind,
                    "verdict": "FAIL", "reason": problem, "codes": []}
        ok, rc, out = check_one(rzc, src_path, d, expected, behavior, expected_output)
        codes = actual_error_codes(out)
        verdict = "PASS"
        reason = ""
        if kind == "多文件":
            # 多文件项目默认断言编译通过；误差允许白名单兜底（与单块同口径）
            if expected is not None or behavior == "运行失败":
                if ok:
                    verdict, reason = "FAIL", "预期失败却编译通过"
                elif expected and not expected & codes:
                    verdict, reason = "FAIL", f"错误码不匹配：预期 {sorted(expected)}，实际 {sorted(codes) or '无 E 码'}"
            elif expected_output is not None:
                if not ok:
                    verdict, reason = "FAIL", f"编译失败或输出不匹配：{out[-300:]}"
            elif not ok:
                verdict, reason = "FAIL", "多文件项目编译失败"
                item = allowlist.get((fname, start))
                if item:
                    verdict = "EXPECTED"
                    reason = f"预期失败（{item.get('category', '?')}：{item.get('note', '')}）"
        elif kind == "错误示例":
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

    # 2.5 上下文复验（--ctx）：白名单块补上前文再验一次，归属三态
    ctx_results = run_ctx_check(rzc, work, results, all_blocks, grouped, allowlist,
                                args.lang, args.parallel) if args.ctx else []

    # 3. 串联模式（按章分组拼接；定义按名去重、变量全局提升，
    #    既补全跨节依赖，也消除同名定义冲突噪音）
    serial_results = []
    if args.serialize:
        chapters = {}
        for index, fname, start, section, content, expected, behavior, expected_output in all_blocks:
            if index in grouped:
                # 多文件块单独成组验证，不参与章节拼接（同一文件会被声明成模块）
                continue
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
        # 只检查本次实际扫描到的文件：多语言白名单合并存放时（zh + en/ja/ru
        # 条目），跑单语言目录不会误报其它语言的条目过期
        run_files = {r["file"] for r in results}
        for key in sorted(allowlist):
            if key[0] not in run_files:
                continue
            if key not in matched:
                print(f"  ⚠️ 白名单过期条目（已不失败，建议删除）：{key[0]} L{key[1]}")
    sfails = []
    if args.serialize:
        sfails = [r for r in serial_results if r["verdict"] != "PASS"]
        print(f"串联验证：通过 {len(serial_results) - len(sfails)} / {len(serial_results)}，失败 {len(sfails)}")
        for r in sfails:
            print(f"  ❌ 串联 {r['file']}：{r['reason']}")
            print(f"     {r['output'].replace(chr(10), chr(10) + '     ')[:400]}")

    # 4.5 上下文复验报告与基线（只约束「前文补全通过」下限，不拦新发现的堆坑）
    ctx_fail = False
    if args.ctx:
        counts = {CTX_PASS: 0, CTX_MINE: 0, CTX_NOISE: 0}
        for r in ctx_results:
            counts[r["verdict"]] = counts.get(r["verdict"], 0) + 1
        print(f"上下文复验：{len(ctx_results)} 块白名单 → {CTX_PASS} {counts[CTX_PASS]}，"
              f"{CTX_MINE} {counts[CTX_MINE]}，{CTX_NOISE} {counts[CTX_NOISE]}")
        for r in sorted((x for x in ctx_results if x["verdict"] == CTX_MINE),
                        key=lambda x: (x["file"], x["line"])):
            print(f"  🔴 {r['file']} L{r['line']}（{r['category']}）{r['error']}")
        baseline_path = args.ctx_baseline or os.path.join(repo, "tools", "ctx-baseline.json")
        baseline = {}
        if os.path.isfile(baseline_path):
            with open(baseline_path, encoding="utf-8") as f:
                baseline = json.load(f)
        langs = baseline.setdefault("languages", {})
        if args.ctx_update:
            langs[args.lang] = {CTX_PASS: counts[CTX_PASS], "复验块数": len(ctx_results)}
            baseline["说明"] = ("教程门禁 --ctx 的「前文补全通过」下限（只增不减）。改完教程跑"
                               " `python3 tools/verify-tutorials.py --ctx --ctx-update`（非 zh 附加"
                               " `--dir tutorials/en --lang en`）上调下限；低于下限即判红。"
                               "「复验块数」参与判定：白名单比基线每少一条，下限等量放宽"
                               "（那条多半已能单块编译，不再需要上下文复验）。")
            with open(baseline_path, "w", encoding="utf-8") as f:
                json.dump(baseline, f, ensure_ascii=False, indent=2)
                f.write("\n")
            print(f"  已写入基线：{baseline_path}（{args.lang} {CTX_PASS}={counts[CTX_PASS]}）")
        else:
            entry = langs.get(args.lang) or {}
            floor = entry.get(CTX_PASS)
            if floor is None:
                print(f"  ⚠️ 基线没有 {args.lang} 条目（先 --ctx-update 记录一次）")
            else:
                # 白名单收缩的合法扣减：条目被删（多是因为该块已能单块编译）后，它
                # 不再进入上下文复验，通过数会跟着少一个——这是进步不是回退。故按
                # 「复验块数」的下降量等量放宽下限；跌幅超过收缩量仍然判红。
                shrink = max(0, (entry.get("复验块数") or len(ctx_results)) - len(ctx_results))
                base = floor
                floor -= shrink
                shrink_note = (f"（基线 {base}，白名单收缩 −{shrink}）" if shrink else "")
                if counts[CTX_PASS] < floor:
                    ctx_fail = True
                    print(f"  ❌ {CTX_PASS} 低于下限 {floor} → {counts[CTX_PASS]}"
                          f"{shrink_note}（教程正文或前文拼接规则回退了）")
                elif counts[CTX_PASS] > floor:
                    print(f"  ✅ {CTX_PASS} 高于下限 {floor} → {counts[CTX_PASS]}"
                          f"{shrink_note}（可 --ctx-update 上调基线）")
                else:
                    print(f"  ✅ {CTX_PASS} 与下限持平：{counts[CTX_PASS]}{shrink_note}")

    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump({"blocks": len(all_blocks), "stats": stats,
                       "single": results, "serial": serial_results, "ctx": ctx_results},
                      f, ensure_ascii=False, indent=1)

    return 1 if fails or sfails or ctx_fail else 0


# Windows 控制台/管道默认 GBK：输出里的 ✅/❌ 无法编码会触发
# UnicodeEncodeError，把「检查通过」变成崩溃；保留终端本地编码、
# 仅在不支持时替换字符（中文可读，emoji 退化为 ?）
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(errors="replace")


if __name__ == "__main__":
    sys.exit(main())
