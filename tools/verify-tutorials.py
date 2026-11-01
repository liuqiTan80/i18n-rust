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

可选 --outputs 模式（输出展示复验，CI 门禁）：教程里给读者看的「运行结果」
裸围栏（``` 无语言标注）历史上不受任何校验——正文一改，展示块容易改漏。该模式
把每对「程序块 + 紧随其后的展示块」拿去 `rzc run`，展示块必须与实际 stdout 逐行
严格相等，任何「输出不符」直接判红；「输出一致」的对数受基线下限保护（只增不减，
见 `tools/output-baseline.json`）。固有不确定（哈希容器遍历顺序）可在白名单用 category `输出顺序不定` 放宽——只免行序、
不免内容（按行多重集比对，单块验证的 `// 预期输出:` 同口径）。诊断/命令行/构建记录不该
占裸围栏，应改 ```text。

退出码：非预期失败为 1（CI 门禁用）；白名单里的过期条目（本次扫到的文件中，
登记行已不再失败）同样判 1——只提示不拦的话，教程改好后忘删的登记会静静积在
`tools/expected-failures.json` 里，既误导后来人「这块还得失败」，也会掩盖代码块
本身的变化；--json 输出机器可读报告（含 `stale` 过期清单）。

「问不到答案」不算「编译器说了不」：任何一块出现 rzc 超时、进程没起来、被信号
终止（共享 runner 的 OOM-killer）或 rzc 自身 panic，一律判红，不得折进「预期
失败成立」。旧写法把「退出码非 0」当成编译失败，四种场景（SIGKILL、退出 101、
run 被杀、重试后仍超时）实测都能让断言失败的块白拿 PASS（四语受影响 89 块，
详见 `stage_unknown` 注释）。

用法：
  python3 tools/verify-tutorials.py [--serialize] [--ctx] [--ctx-update] \
      [--outputs] [--outputs-update] \
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
CTX_TIMEOUT = 180# ---------- 输出展示块复验（--outputs） ----------
# 书里给读者看的「运行结果」有两种载体：代码块内的 `// 预期输出:` 注释（单块
# 验证已断言 stdout），和紧随代码块之后的裸围栏（``` 无语言标注）。后者历史上
# 没有任何校验——正文改了展示块跟着改，改漏就一直错着（第九章 9.3 把 推入('，')
# 改成 推入('！') 就漏了展示块那一处）。--outputs 把裸围栏当断言：必须与前面那
# 段程序 `rzc run` 的 stdout 逐行严格相等。
#
# 配套规矩：**裸围栏只放程序打印出来的 stdout**。命令行记录（`$ cargo test`）、
# 编译器诊断、恐慌信息改用 ```text / ```bash。zh 版 41 处已按此清理；en/ja/ru
# 尚未清理的裸诊断围栏归入「非stdout展示」并逐条提示，不判红。
OUT_COLON_END_RE = re.compile(r"[:：]\s*$")
OUT_CUE_WORD_RE = re.compile(
    r"(输出|运行结果|运行后屏幕显示|显示|出力|报告|見えるはずのもの|実行すると画面には"
    r"|output|what you should see|report|вывод|вы увидите|сообщит|сообщение)", re.IGNORECASE)
# 小节标题本身就是「运行结果 / Output / Вывод」样式
OUT_HEAD_RE = re.compile(
    r"^#{1,4}\s+.*(运行结果|输出|出力|見えるはずのもの|output|вывод|вы увидите)", re.IGNORECASE)
# 展示块其实不是 stdout 的形态特征（诊断、命令行、构建/测试记录）
NON_STDOUT_RE = re.compile(
    r"^\s*(?:\$|>)\s|^(?:错误|エラー|警告|warning|error|ошибка)\b|^\s*-->|^\s*💡"
    r"|^线程 |^\s*thread .+ panicked|^test result:|^running \d+ tests"
    r"|^Предупреждение\[|^Ошибка\[|^Поток "
    r"|^\S+ сохранён как ", re.MULTILINE | re.IGNORECASE)
OUT_PASS, OUT_FAIL, OUT_NOSTDOUT, OUT_UNRUN, OUT_REG = (
    "输出一致", "输出不符", "非stdout展示", "无法复验", "已登记跳过")
# 白名单里用来放宽某块输出断言的类别：哈希映射/哈希集合的遍历顺序是 Rust 的
# 固有不确定，登记后该行不再要求顺序，但内容（每行各出现几次）仍然逐行核对。
# 「已登记跳过」= 因这条豁免而对齐了；不匹配照样判红。
OUT_ALLOW_CATEGORY = "输出顺序不定"
OUT_TIMEOUT = 120
# rzc run 把程序 stdout 原样打出，再在尾部追加引擎警告/提示块（`警告[...]:`、
# `  -->`、`   |`、`💡`、`提示：`、`---` 分隔行）。展示块只印程序自己打印的内容，
# 所以比对时要从第一条噪音行处截断。教程里若要印「分隔线」请别让它出现在行首
# （现状：无一块的正文以这些形态开头，实测四语 79+ 对全部对齐）。
NOISE_LINE_RE = re.compile(
    r"^(?:警告\[|错误\[|错误[:：]|警告[:：]|提示[:：]|💡|\s*-->|\s*\||---+$"
    r"|warning\[|error\[|note[:：]|help[:：]|エラー\[|panic[:：]"
    # 尾部的「N warnings emitted」汇总行同属诊断。「错误：」不列——程序自己打印
    # 「错误：xxx」是合法输出，静默吃掉比留下它更危险
    # 俄文诊断前缀（实测 `rzc run` 打到 stdout 的形态：Предупреждение[...]/
    # Предупреждение: N warning emitted）；以及带语言包缺口的块会先打一行
    # 「<路径> сохранён как <路径>.bak」（rzc 的备份提示，不是程序输出）。
    r"|Предупреждение\[|Предупреждение[:：]|Ошибка\["
    # 俄文 panic 行首（"Поток ... panicked"）也是诊断，不是程序 stdout
    r"|Поток "
    r"|\S+ сохранён как )")


def strip_engine_noise(text):
    """逐行剔除引擎噪音，只留程序自己打印的部分。

    不能「从第一条噪音行截断」：`rzc run` 把警告打在程序 stdout 前面还是后面并不
    固定（zh 实测在后、ru 实测在前），截断会把真实输出整段吃掉（ru 第四/十二章
    一度报「实际 0 行」）。噪音行的形态都是诊断专属（`警告[`/`💡`/` -->`/`   |`/
    `---`/`Предупреждение[`/「сохранён как」），教程里程序打印的行不会撞上；正文若
    真要让读者看到 `---` 这类行，请改用 ```text 展示，别占 stdout 围栏。
    """
    keep = [ln for ln in text.split("\n") if not NOISE_LINE_RE.match(ln)]
    return "\n".join(keep)




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
            # 后续连续注释行同属这份输出：两种写法都要收（ja/ru 有 17 块写成
            # `// 预期输出: 首行` + 续行，旧规则只比首行，等于放弃其余行的断言）
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
    """按块所属**文件名**补充外部依赖。

    刻意不用全库块序号判定：序号会随其它章节增删（如新增教程章节）整体漂移，
    硬编码序号会静默失效，导致依赖漏注入、示例编译报 E0432。fname 稳定可作依据。"""
    deps = ""
    if "第二十四章" in fname:
        deps += 'futures = "0.3"\n'
    if "第二十五章" in fname:  # 25.11 彩蛋镜像真实后端模块，用到 rustc_lexer
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


def output_matches(want, actual, ignore_order=False):
    """预期行按序匹配实际输出行（宽容编译警告等附加输出）。

    实际输出可能夹杂编译警告（如 警告[dead_code]）与帮助信息，
    只要预期各行按原顺序出现即为匹配。
    ignore_order=True 用于白名单登记 `输出顺序不定` 的块（哈希容器遍历）：改为
    行多重集相等才算过——顺序不锁，内容照旧一行不能多、一行不能少。"""
    if ignore_order:
        return sorted(want.splitlines()) == sorted(actual.splitlines())
    lines = want.splitlines()
    i = 0
    for ln in actual.splitlines():
        if i < len(lines) and ln == lines[i]:
            i += 1
    return i == len(lines)


# rzc check 只用两种码表态：0 编译通过、1 有诊断（rustc 非零一律折成 FAILURE，
# 见 crates/cli/src/main.rs 的 Check 分支与 diagnostics.rs）。
CHECK_CODES = (0, 1)


def stage_unknown(rc, out, stage):
    """这次提问到底有没有问到答案：问到了返回 None，没问到返回给人看的理由。

    把「编译器说了不」和「没能问到编译器」混为一谈是门禁的经典假绿：`!= 0` 这个
    判据同时接纳了超时（-1）、进程没起来（-2）、被信号杀（负数——共享 runner 上
    OOM-killer 的常态）和 rzc 自身 panic（101）。四例都用最小教程夹具（一个
    `// ❌` 块 + 一个 `// 预期行为: 运行失败` 块）配假 rzc 端到端复现过：check 被
    SIGKILL（returncode -9）时前者判绿、check 以 101 崩溃时判绿、run 被 SIGKILL 时
    后者判绿、check 重试后仍超时（-1）时判绿。按块统计，四语教程里 180 个断言失败的
    块有 89 个没有预期错误码兜底（无码 76 + 运行失败 13），带码的 91 个则因取不到
    预期码而侥幸判红——也就是说这条假绿路径正好盖住了「预期失败」类块的一半。

    stage == "check"：只认 0/1；stage == "run"：教程程序自己的退出码无法穷举
    （0 跑成功、101 panic、`process::exit(3)` 都是合法答案），只有「进程没跑成」
    （rc < 0：超时 / 没起来 / 被信号杀）算没问到答案。
    残留盲区：rzc run 把「子进程被信号杀」折成退出码 1 传播（main.rs 的
    `unwrap_or(FAILURE)`），那一层从码上分不出来，只能靠 rzc 侧改协议。
    """
    if stage == "check":
        if rc in CHECK_CODES:
            return None
    elif rc >= 0:
        return None
    if rc == -1:
        return "无法判定：rzc 超时（重试后仍超时）"
    if rc == -2:
        return f"无法判定：rzc 未能执行（{out[:120]}）"
    if rc < 0:
        return f"无法判定：rzc 被信号终止（returncode {rc}）"
    if rc == 101:
        return f"无法判定：rzc 自身 panic（returncode 101）：{out[:120]}"
    return f"无法判定：rzc {stage} 退出码异常（{rc}）：{out[:120]}"


def check_one(rzc, src_path, cwd, expected, behavior, expected_output=None, timeout=120,
              out_ignore_order=False):
    """_check_one_once 的超时重试外壳。

    共享 runner 负载抖动会让冷缓存编译偶发越过超时上限——这与块内容无关，
    超时（returncode -1）自动重试一次，重试后仍超时才维持判红
    （2026-10-31 run #241 实证：同内容 #240 绿 #241 红，无重试则误报）。
    重试救不了的那一次会带着「无法判定」的理由返回，不会伪装成断言成立。"""
    res = _check_one_once(rzc, src_path, cwd, expected, behavior,
                          expected_output=expected_output, timeout=timeout,
                          out_ignore_order=out_ignore_order)
    if res[1] == -1:
        res = _check_one_once(rzc, src_path, cwd, expected, behavior,
                              expected_output=expected_output, timeout=timeout,
                              out_ignore_order=out_ignore_order)
    return res


def _check_one_once(rzc, src_path, cwd, expected, behavior, expected_output=None, timeout=120,
                    out_ignore_order=False):
    """rzc check（需要运行时验证时加 rzc run）。

    返回 (ok, returncode, full_output, unknown)：unknown 非 None 表示这一问根本没
    问到答案（超时 / 没起来 / 被信号杀 / rzc 崩溃 / 退出码异常），调用方必须判红，
    判据见 stage_unknown。
    ok 语义：
    - behavior 为 "通过"：编译通过
    - behavior 为 "运行失败"：编译通过且运行非零退出
    - expected_output 非 None：编译通过且 stdout 规范化后与预期一致
      （out_ignore_order=True 时只要求行多重集相同，见 output_matches）
    - 其他（错误示例默认）：编译失败"""
    try:
        r = subprocess.run([rzc, "check", src_path], capture_output=True, text=True,
                           timeout=timeout, cwd=cwd)
        out = (r.stdout + r.stderr).strip()
        unknown = stage_unknown(r.returncode, out, "check")
        if unknown:
            return False, r.returncode, out, unknown
        compiled_ok = r.returncode == 0
        if expected_output is not None:
            if not compiled_ok:
                return False, r.returncode, out, None
            rr = subprocess.run([rzc, "run", src_path], capture_output=True, text=True,
                                timeout=timeout, cwd=cwd)
            rr_out = (rr.stdout + rr.stderr).strip()
            unknown = stage_unknown(rr.returncode, rr_out, "run")
            if unknown:
                return False, rr.returncode, rr_out, unknown
            if rr.returncode != 0:
                return False, rr.returncode, rr_out, None
            actual = normalize_output(strip_engine_noise(rr.stdout))
            want = normalize_output(expected_output)
            if not output_matches(want, actual, out_ignore_order):
                return False, rr.returncode, ("输出不匹配\n"
                                              f"  预期: {want!r}\n"
                                              f"  实际: {actual!r}"), None
            return True, rr.returncode, rr.stdout, None
        if behavior == "运行失败":
            if not compiled_ok:
                return False, r.returncode, out, None
            rr = subprocess.run([rzc, "run", src_path], capture_output=True, text=True,
                                timeout=timeout, cwd=cwd)
            out = (rr.stdout + rr.stderr).strip()
            unknown = stage_unknown(rr.returncode, out, "run")
            if unknown:
                return False, rr.returncode, out, unknown
            return rr.returncode != 0, rr.returncode, out, None
        if behavior == "通过":
            return compiled_ok, r.returncode, out, None
        # 错误示例默认：断言编译失败（预期错误码由调用方核对）
        return compiled_ok, r.returncode, out, None
    except subprocess.TimeoutExpired:
        return False, -1, "超时", stage_unknown(-1, "超时", "check")
    except Exception as e:
        return False, -2, str(e), stage_unknown(-2, str(e), "check")


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

        def build(grp, tag):
            src_path, d = build_serialized(work, grp, f"{base}-{k}-{tag}", allowlist, deps, lang)
            if src_path is None:
                return False, "前文项目生成失败", None
            ok, _rc, out, unknown = check_one(rzc, src_path, d, set(), None, timeout=CTX_TIMEOUT)
            return ok, first_error_line(out), unknown

        # 拿不到判定（超时/被信号杀/rzc 崩溃）时不许记在「本块致错」上——那一格
        # 是在向读者指认正文里真实存在的一堵墙；归进「前文噪音」这个无法归因的
        # 桶，并把理由带进报告。
        ok, err, unknown = build(blocks[:k + 1], "ctx")
        if ok:
            verdict = CTX_PASS
        elif unknown:
            verdict, err = CTX_NOISE, unknown
        else:
            # 归因：本块之前的前文自己就编不过时，错不算在本块头上
            ok0, _err0, unknown0 = build(blocks[:k], "base")
            if unknown0:
                verdict, err = CTX_NOISE, unknown0
            else:
                verdict = CTX_NOISE if not ok0 else CTX_MINE
        return {"file": fname, "line": line, "category": cat, "verdict": verdict,
                "error": "" if verdict == CTX_PASS else err}

    with ThreadPoolExecutor(max_workers=parallel) as ex:
        return list(ex.map(probe, todo))


# ---------- 主流程 ----------
# ---------- 输出展示块定位与复验 ----------
def iter_fences(lines):
    """扫描围栏，返回 [(开行0-based, 闭行0-based, 语言标注)]（闭行越界表示未闭合）。"""
    spans = []
    i = 0
    while i < len(lines):
        st = lines[i].strip()
        if st.startswith("```"):
            info = st[3:].strip()
            j = i + 1
            while j < len(lines) and lines[j].strip() != "```":
                j += 1
            spans.append((i, j, info))
            i = j + 1
        else:
            i += 1
    return spans


def details_depth(lines, upto):
    """ upto 行之前（不含）还开着几层 `<details>`。练习答案折叠区里的「题目要求」
    块长得和运行结果一样，靠这个把跨区的错误配对挡掉。"""
    depth = 0
    for ln in lines[:upto]:
        s = ln.strip().lower()
        if s.startswith("<details"):
            depth += 1
        elif s.startswith("</details"):
            depth -= 1
    return depth


def find_output_shows(md_path):
    """定位「程序块 + 紧随其后的运行结果展示块」配对。

    返回 [(程序块行号, 展示块首行行号, 展示文本, 引导方式)]。行号口径与
    extract_blocks 一致：程序块行号就是白名单里用的那个键（围栏行号）。
    配对条件：① 展示块是裸围栏；② 上一个围栏是 ```rust；③ 两者没跨过
    `<details>` 边界；④ 中间有引导语（正文以「输出：」收尾，或最近标题是
    「运行结果」）。"""
    with open(md_path, encoding="utf-8") as f:
        lines = f.read().split("\n")
    spans = iter_fences(lines)
    out = []
    for (s, e, info) in spans:
        if info:
            continue
        prev = [x for x in spans if x[1] < s]
        if not prev or prev[-1][2] != "rust":
            continue
        ps, pe = prev[-1][0], prev[-1][1]
        if details_depth(lines, ps) != details_depth(lines, s):
            continue
        between = lines[pe + 1:s]
        head = any(OUT_HEAD_RE.match(l.strip()) for l in between)
        cue = any(OUT_COLON_END_RE.search(l.strip()) and OUT_CUE_WORD_RE.search(l)
                  and not l.strip().startswith(("#", "```")) for l in between)
        if not (head or cue):
            continue
        out.append((ps + 1, s + 2, "\n".join(lines[s + 1:e]), "标题" if head else "正文"))
    return out


def output_diff_note(want, actual, ignore_order=False):
    """人话说明两者差在哪一行（逐行严格比较）。

    ignore_order=True 用于「顺序无关」的登记块：先按行排序再比，否则报告会把
    「少了哪一行」说成「第 N 行不同」，看着像错位其实差在内容。"""
    if ignore_order:
        want = "\n".join(sorted(want.split("\n")))
        actual = "\n".join(sorted(actual.split("\n")))
    wl = want.split("\n") if want else []
    al = actual.split("\n") if actual else []
    if len(wl) != len(al):
        note = f"行数不同：展示 {len(wl)} 行，实际 {len(al)} 行"
    else:
        note = "内容不同"
    for k in range(min(len(wl), len(al))):
        if wl[k] != al[k]:
            return f"{note}；首个差异在第 {k + 1} 行：展示 {wl[k]!r}，实际 {al[k]!r}"
    if len(wl) != len(al):
        extra = (wl[len(al):] or al[len(wl):])[0]
        return f"{note}；多出的一行是 {extra!r}"
    return note


def run_output_check(rzc, work, tut_dir, allowlist, lang, parallel):
    """裸围栏「运行结果」复验：把前面那段程序单独跑一遍，展示块必须与 stdout 相等。

    代码块内已有 `// 预期输出:` 的，stdout 已由单块验证断言，这里只做「注释与
    展示块互相对齐」的静态比对（不再烧一次编译）。"""
    pairs = []
    for path in sorted(glob.glob(os.path.join(tut_dir, "*.md"))):
        fname = os.path.basename(path)
        blocks = {st: ct for (_f, st, _sec, ct) in extract_blocks(path)}
        for code_line, shown_line, shown, how in find_output_shows(path):
            content = blocks.get(code_line)
            if content is None:
                continue
            _exp, _beh, marks, _rest = parse_marks(content)
            pairs.append((fname, code_line, shown_line, shown, content, marks, how))

    def probe(idx, item):
        fname, code_line, shown_line, shown, content, marks, how = item
        base = {"file": fname, "line": code_line, "shown_line": shown_line, "cue": how}
        entry = allowlist.get((fname, code_line)) or {}
        loose = entry.get("category") == OUT_ALLOW_CATEGORY
        base["loose"] = loose
        want = normalize_output(shown)
        if NON_STDOUT_RE.search(want):
            base.update(verdict=OUT_NOSTDOUT,
                        reason="这不是程序打印的 stdout（诊断/命令行/构建记录），建议围栏改成 ```text")
            return base
        if marks is not None:
            got = normalize_output(marks)
            if got == want:
                base.update(verdict=OUT_PASS, reason="与块内 预期输出 注释对齐")
                return base
            if loose and sorted(got.split("\n")) == sorted(want.split("\n")):
                # 同一份程序的两处展示（块内注释 + 裸围栏），顺序类块允许行序不同
                base.update(verdict=OUT_REG, reason="与块内 预期输出 注释内容一致（行序不同）")
                return base
            base.update(verdict=OUT_FAIL,
                        reason="展示块与块内 `// 预期输出:` 注释不一致：" + output_diff_note(want, got))
            return base
        t = build_task(work, 900000 + idx, fname, code_line, content,
                       None, None, None, False, lang)
        if t is None:
            base.update(verdict=OUT_UNRUN, reason="省略块，不单独运行")
            return base
        src_path, d = t[7], t[8]
        # 与 check_one 同一口径：负载抖动型超时与内容无关，重试一次仍超时才判
        rr = None
        for _ in (1, 2):
            try:
                r = subprocess.run([rzc, "check", src_path], capture_output=True,
                                   text=True, timeout=OUT_TIMEOUT, cwd=d)
                if r.returncode != 0:
                    base.update(verdict=OUT_UNRUN,
                                reason="程序单独跑不通：" + first_error_line((r.stdout + r.stderr).strip()))
                    return base
                rr = subprocess.run([rzc, "run", src_path], capture_output=True,
                                    text=True, timeout=OUT_TIMEOUT, cwd=d)
                if rr.returncode != 0:
                    base.update(verdict=OUT_UNRUN, reason="运行非零退出（展示的多是 stderr，应改用 ```text）")
                    return base
                break
            except subprocess.TimeoutExpired:
                rr = None
        if rr is None:
            base.update(verdict=OUT_UNRUN, reason="运行超时（重试后仍超时）")
            return base
        actual = normalize_output(strip_engine_noise(rr.stdout))
        if actual == want:
            base.update(verdict=OUT_PASS, reason="")
        elif loose and sorted(want.split("\n")) == sorted(actual.split("\n")):
            base.update(verdict=OUT_REG, reason="顺序不定登记项：本次行序不同，内容逐行一致")
        elif loose:
            base.update(verdict=OUT_FAIL,
                        reason="已登记顺序不定，但内容对不上（按行排序后比）："
                               + output_diff_note(want, actual, ignore_order=True))
        else:
            base.update(verdict=OUT_FAIL, reason=output_diff_note(want, actual))
        return base

    with ThreadPoolExecutor(max_workers=parallel) as ex:
        return [base for base in ex.map(lambda c: probe(c[0], c[1]), list(enumerate(pairs)))]


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
    ap.add_argument("--outputs", action="store_true",
                    help="复验裸围栏「运行结果」展示块是否与实际 stdout 一致（CI 门禁，见文件头说明）")
    ap.add_argument("--outputs-baseline", default=None,
                    help="输出展示复验基线 JSON（默认 tools/output-baseline.json）")
    ap.add_argument("--outputs-update", action="store_true",
                    help="把本次「输出一致」数写回基线（需同时 --outputs）")
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

    # 1.1 检查对象为空绝不是「没有需要检查的东西」，而是门禁瞎了。目录写错（CI 用
    # `--dir "tutorials/$lang"` 拼路径，方言名笔误就会指到不存在的目录）、教程文件
    # 被移动，旧版都会一路跑到基线比对，再把「0 块复验」按收缩扣减算成
    # 「✅ 高于下限 -9」报绿（实测：`--dir tutorials/zz-不存在 --ctx --outputs` 配
    # 仓库真基线跑出 EXIT=0，提取 0 块、复验 0 块、对齐 0 对）。
    if not os.path.isdir(tut_dir):
        print(f"❌ 教程目录不存在：{tut_dir}\n"
              "   本门禁检查的对象就是这份目录，目录拿不到时所有断言恒成立——不能算通过。",
              file=sys.stderr)
        return 2
    if not all_blocks:
        print(f"❌ 目录 {tut_dir} 里没提取到任何代码块（*.md 里没有 rust 围栏？）\n"
              "   空对象上的「失败 0」不是通过：请确认 --dir 指对、教程文件在位。",
              file=sys.stderr)
        return 2

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
        kind = classify_task(expected, behavior, expected_output, content, args.lang)
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
        item = allowlist.get((fname, start)) or {}
        ok, rc, out, unknown = check_one(rzc, src_path, d, expected, behavior, expected_output,
                                  out_ignore_order=item.get("category") == OUT_ALLOW_CATEGORY)
        if unknown:
            # 「没能问到编译器」不是「编译器说了不」：见 stage_unknown 的实测四例。
            # 白名单条目也救不了——登记的含义是「这块编不过」，不是「这块跑不起来」。
            return {"id": index, "file": fname, "line": start, "kind": kind,
                    "verdict": "FAIL", "reason": unknown, "codes": []}
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
    # 2.6 输出展示复验（--outputs）：裸围栏「运行结果」必须等于程序实际 stdout
    out_results = run_output_check(rzc, work, tut_dir, allowlist,
                                   args.lang, args.parallel) if args.outputs else []


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
            src_path, d = build_serialized(work, blocks, base_name, allowlist, deps, args.lang)
            ok, rc, out, unknown = check_one(rzc, src_path, d, set(), None, timeout=180)
            codes = actual_error_codes(out)
            serial_results.append({
                "file": fname,
                "verdict": "PASS" if ok else "FAIL",
                "reason": "" if ok else (unknown or "串联编译失败"),
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
    stale = []
    if allowlist and not args.serialize:
        matched = {(r["file"], r["line"]) for r in expecteds}
        # 只检查本次实际扫描到的文件：多语言白名单合并存放时（zh + en/ja/ru
        # 条目），跑单语言目录不会误报其它语言的条目过期
        run_files = {r["file"] for r in results}
        for key in sorted(allowlist):
            if key[0] not in run_files:
                continue
            if allowlist[key].get("category") == OUT_ALLOW_CATEGORY:
                # 该类别是 --outputs 的豁免登记，不参与单块失败匹配
                continue
            if key not in matched:
                stale.append(key)
    for stale_file, stale_line in stale:
        print(f"  ❌ 白名单过期条目（已不失败）：{stale_file} L{stale_line}")
    if stale:
        print(f"   共 {len(stale)} 条登记已不再命中：要么删掉该条（教程已修好），"
              "要么代码块改位置后把行号改对；\n"
              "   留着它们只会误导后来人，并掩盖代码块自身的变化。")
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
        baseline_loaded = os.path.isfile(baseline_path)
        if baseline_loaded:
            with open(baseline_path, encoding="utf-8") as f:
                baseline = json.load(f)
        langs = baseline.setdefault("languages", {})
        if args.ctx_update:
            prev_floor = (langs.get(args.lang) or {}).get(CTX_PASS)
            langs[args.lang] = {CTX_PASS: counts[CTX_PASS], "复验块数": len(ctx_results)}
            if prev_floor is not None and counts[CTX_PASS] < prev_floor:
                # 不拦（--ctx-update 就是主动重记的出口），但要把「这是下调」说出口，
                # 免得下调混在一大片输出里看不见
                print(f"  ⚠️ 本次下调了 {args.lang} 的下限：{prev_floor} → "
                      f"{counts[CTX_PASS]}（复验块数 {len(ctx_results)}）——"
                      "请在提交说明里写清为什么")
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
                # 拿不到下限 = 这条门禁本轮没有生效。旧版只打一行 ⚠️ 且不影响退出码，
                # 于是基线文件改名/移动、或 `--lang` 写得和基线里的键不一致（CI 新增
                # 方言时最容易撞上）都会让「前文补全通过」不再有任何下限地报绿
                # （实测：`--ctx-baseline` 指向不存在的文件→只报 ⚠️、EXIT=0）。
                ctx_fail = True
                where = baseline_path if baseline_loaded else f"{baseline_path}（文件不存在）"
                print(f"  ❌ 基线里没有 {args.lang} 的条目：{where}——下限拿不到时"
                      "本门禁等于没跑。核对 --lang 与基线键一致后用 --ctx-update 显式记录一次")
            else:
                # 白名单收缩的合法扣减：条目被删（多是因为该块已能单块编译）后，它
                # 不再进入上下文复验，通过数会跟着少一个——这是进步不是回退。故按
                # 「复验块数」的下降量等量放宽下限；跌幅超过收缩量仍然判红。
                shrink = max(0, (entry.get("复验块数") or len(ctx_results)) - len(ctx_results))
                base = floor
                # 但扣减不许把下限拖没：整批块退出复验（类别被改成不参与复验的类别、
                # 白名单被重新生成）时 floor 会掉到 0 甚至负数，「0 块通过」反而打印
                # 「✅ 高于下限 -9」（实测：`--dir` 指向不存在的目录，配仓库真基线跑出
                # EXIT=0）。一次掉一半以上只可能是口径变化，必须显式 --ctx-update 重记。
                floor = max(base - shrink, -(-base // 2))
                shrink_note = (f"（基线 {base}，白名单收缩 −{shrink}）" if shrink else "")
                if counts[CTX_PASS] < floor:
                    ctx_fail = True
                    cap_note = ("；跌幅超过基线的一半，收缩扣减已封顶——多半是口径变化"
                                "（类别改登记/白名单重生成），确认后用 --ctx-update 重记"
                                if base - shrink < floor else "")
                    print(f"  ❌ {CTX_PASS} 低于下限 {floor} → {counts[CTX_PASS]}"
                          f"{shrink_note}（教程正文或前文拼接规则回退了）{cap_note}")
                elif counts[CTX_PASS] > floor:
                    print(f"  ✅ {CTX_PASS} 高于下限 {floor} → {counts[CTX_PASS]}"
                          f"{shrink_note}（可 --ctx-update 上调基线）")
                else:
                    print(f"  ✅ {CTX_PASS} 与下限持平：{counts[CTX_PASS]}{shrink_note}")

    # 4.6 输出展示复验报告与基线（不符即判红；「非stdout展示」只提示，计数只减不增）
    out_fail = False
    if args.outputs:
        counts = {}
        for r in out_results:
            counts[r["verdict"]] = counts.get(r["verdict"], 0) + 1
        line = (f"输出展示复验：{len(out_results)} 对 → {OUT_PASS} {counts.get(OUT_PASS, 0)}，"
                f"{OUT_FAIL} {counts.get(OUT_FAIL, 0)}，{OUT_UNRUN} {counts.get(OUT_UNRUN, 0)}，"
                f"{OUT_NOSTDOUT} {counts.get(OUT_NOSTDOUT, 0)}，顺序无关一致 {counts.get(OUT_REG, 0)}")
        print(line)
        for r in sorted((x for x in out_results if x["verdict"] == OUT_FAIL),
                        key=lambda x: (x["file"], x["shown_line"])):
            print(f"  ❌ {r['file']} 展示块L{r['shown_line']}（程序块L{r['line']}，{r['cue']}引导）{r['reason']}")
            out_fail = True
        for r in sorted((x for x in out_results if x["verdict"] == OUT_UNRUN),
                        key=lambda x: (x["file"], x["shown_line"])):
            print(f"  ⚪ {r['file']} L{r['shown_line']}：{r['reason']}")
        for r in sorted((x for x in out_results if x["verdict"] == OUT_NOSTDOUT),
                        key=lambda x: (x["file"], x["shown_line"])):
            print(f"  🔧 {r['file']} L{r['shown_line']}：{r['reason']}")
        # 豁免条目若已用不上（该配对没了），提示清理，不判红
        pairs_keys = {(r["file"], r["line"]) for r in out_results}
        run_files = {r["file"] for r in out_results}
        for key, item in sorted(allowlist.items()):
            if item.get("category") == OUT_ALLOW_CATEGORY and key[0] in run_files and key not in pairs_keys:
                print(f"  ⚠️ 输出豁免条目已用不上（建议删除）：{key[0]} L{key[1]}")

        ob_path = args.outputs_baseline or os.path.join(repo, "tools", "output-baseline.json")
        obase = {}
        obase_loaded = os.path.isfile(ob_path)
        if obase_loaded:
            with open(ob_path, encoding="utf-8") as f:
                obase = json.load(f)
        olangs = obase.setdefault("languages", {})
        # 下限看的是「对齐的对数」：顺序完全一致 + 顺序无关一致都算对齐。哈希
        # 容器的行序每次可能不同，只数 OUT_PASS 会让基线随抖动忽涨忽落、报假红。
        aligned = counts.get(OUT_PASS, 0) + counts.get(OUT_REG, 0)
        # 顺序无关下限看的是「登记为顺序不定的块是否仍对齐」，与随机行序无关：
        # 同一哈希块某次恰好按序会落成 输出一致，不能因此判配对消失。
        loose_aligned = sum(1 for r in out_results
                            if r.get("loose") and r["verdict"] in (OUT_PASS, OUT_REG))
        if args.outputs_update:
            prev_aligned = (olangs.get(args.lang) or {}).get(OUT_PASS)
            olangs[args.lang] = {OUT_PASS: aligned, "复验对数": len(out_results),
                                   "顺序无关": loose_aligned}
            if prev_aligned is not None and aligned < prev_aligned:
                # 同 --ctx-update：不拦主动重记，但下调必须说出口
                print(f"  ⚠️ 本次下调了 {args.lang} 的下限：{prev_aligned} → "
                      f"{aligned}（复验对数 {len(out_results)}）——请在提交说明里写清为什么")
            obase["说明"] = ("教程门禁 --outputs 的对齐下限（只增不减）= 「输出一致」+「顺序无关一致」两类的对数。改完教程跑"
                            " `python3 tools/verify-tutorials.py --outputs --outputs-update`（非 zh 附加"
                            " `--dir tutorials/en --lang en`）上调下限；低于下限即判红。"
                            "「复验对数」参与判定：配对数比基线每少一对，下限等量放宽（那对多半被"
                            "改成了 ```text 或挪进了答案区）。另有一条硬规则：出现任何"
                            "「输出不符」直接判红，不受基线保护。")
            with open(ob_path, "w", encoding="utf-8") as f:
                json.dump(obase, f, ensure_ascii=False, indent=2)
                f.write("\n")
            print(f"  已写入基线：{ob_path}（{args.lang} 对齐 {aligned}）")
        else:
            oentry = olangs.get(args.lang) or {}
            ofloor = oentry.get(OUT_PASS)
            if ofloor is None:
                # 同 --ctx：拿不到下限就是这条门禁没生效，不能只提一行就算过
                out_fail = True
                where = ob_path if obase_loaded else f"{ob_path}（文件不存在）"
                print(f"  ❌ 基线里没有 {args.lang} 的条目：{where}——下限拿不到时本门禁"
                      "等于没跑。核对 --lang 与基线键一致后用 --outputs --outputs-update 显式记录一次")
            else:
                oshrink = max(0, (oentry.get("复验对数") or len(out_results)) - len(out_results))
                obase_floor = ofloor
                # 收缩扣减不许把下限拖没（与 --ctx 同口径）：把展示围栏整批改成
                # ```text / 删掉展示块会让配对成批消失，旧规则下限随之等量降到 0 甚至负数，
                # 「0 对对齐」反而打印「✅ 高于下限 -3」（实测：`--dir` 指向不存在的目录，
                # 配仓库真基线跑出 EXIT=0）。一次掉一半以上只可能是口径变化。
                ofloor = max(obase_floor - oshrink, -(-obase_floor // 2))
                onote = f"（基线 {obase_floor}，配对收缩 −{oshrink}）" if oshrink else ""
                if aligned < ofloor:
                    out_fail = True
                    ocap_note = ("；跌幅超过基线的一半，收缩扣减已封顶——多半是口径变化"
                                 "（展示块改成 text 围栏/白名单重生成），确认后用"
                                 " --outputs --outputs-update 重记"
                                 if obase_floor - oshrink < ofloor else "")
                    print(f"  ❌ 对齐对数 低于下限 {ofloor} → {aligned}{onote}"
                          f"（展示块或程序改动把已对齐的输出改坏了）{ocap_note}")
                elif aligned > ofloor:
                    print(f"  ✅ 对齐对数 高于下限 {ofloor} → {aligned}{onote}"
                          f"（可 --outputs --outputs-update 上调基线）")
                else:
                    print(f"  ✅ 对齐对数 与下限持平：{aligned}{onote}")
                # 顺序无关登记项兜底：少了就判红（顺序可以变，配对不能没）。
                # 看 loose_aligned（登记块里仍对齐的对数），随机行序落 PASS 还是 REG 都不误伤；
                # 内容真对不上时该块会判 输出不符，已由上面「不符即判红」硬规则兜住。
                oloose_floor = oentry.get("顺序无关")
                if oloose_floor is not None:
                    # 同样给收缩扣减封个底：登记的哈希顺序块整批消失时，
                    # 不许「0 对」靠负下限报绿
                    oloose_floor = max(oloose_floor - oshrink, -(-oloose_floor // 2))
                    if loose_aligned < oloose_floor:
                        out_fail = True
                        print(f"  ❌ 顺序无关登记块对齐数 低于下限 {oloose_floor} → "
                              f"{loose_aligned}（登记的哈希顺序块内容对不上了，"
                              f"或配对已不成立——请修展示块，或删掉白名单里的该条）")

    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump({"blocks": len(all_blocks), "stats": stats,
                       "single": results, "serial": serial_results, "ctx": ctx_results,
                       "outputs": out_results,
                       "stale": [{"file": f, "line": ln} for f, ln in stale]},
                      f, ensure_ascii=False, indent=1)

    return 1 if fails or sfails or ctx_fail or out_fail or stale else 0


# Windows 控制台/管道默认 GBK：输出里的 ✅/❌ 无法编码会触发
# UnicodeEncodeError，把「检查通过」变成崩溃；保留终端本地编码、
# 仅在不支持时替换字符（中文可读，emoji 退化为 ?）
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(errors="replace")


if __name__ == "__main__":
    sys.exit(main())
