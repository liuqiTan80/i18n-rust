#!/usr/bin/env python3
"""扫描 engine 15 个 .zh 代码区（去注释、去字符串）剩余的英文标识符，按词频降序报告。

用于发现尚未中文化的 std/三方/自定义英文名。合法例外（数字后缀、'static、编码名、
$tt 宏片段、build.rs 生成符号等）需人工甄别，脚本只做粗筛。"""
import re, sys, pathlib, collections

ROOT = pathlib.Path(__file__).resolve().parents[2]
SRC = ROOT / 'crates/engine/src'
CJK = '\u4e00-\u9fff'
STR_SPAN = re.compile(
    r'r(?P<h>#+)"[\s\S]*?"(?P=h)|r"[^"]*"|b?"(?:[^"\\\n]|\\[\s\S])*"'
    r"|b?'(?:[^'\\\n]|\\[ntbr'\\]|\\x[0-9a-fA-F]{2}|\\u\{[0-9a-fA-F]+\})'"
    r"|'[_A-Za-z][A-Za-z0-9_]*")

IDENT = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')


def code_only(text):
    # 去块注释、行注释，再去字符串字面量
    text = re.sub(r'/\*.*?\*/', '', text, flags=re.S)
    text = re.sub(r'//[^\n]*', '', text)
    text = STR_SPAN.sub(' ', text)
    return text


def main():
    counts = collections.Counter()
    where = collections.defaultdict(set)
    for zh in sorted(SRC.glob('*.zh')):
        text = zh.read_text()
        # 逐行以定位行号（仅代码区，跳过整行注释）
        for lineno, line in enumerate(text.splitlines(), 1):
            stripped = line.lstrip()
            if stripped.startswith('//') or stripped.startswith('/*') or stripped.startswith('*'):
                continue
            code = code_only(line)
            for m in IDENT.finditer(code):
                w = m.group(0)
                # 跳过纯数字后缀跟在标识符后的情况已被 IDENT 处理；跳过单字母以下划线包
                counts[w] += 1
                where[w].add(f'{zh.name}:{lineno}')
    show = int(sys.argv[1]) if len(sys.argv) > 1 else 80
    for w, c in counts.most_common(show):
        locs = sorted(where[w])[:3]
        print(f'{c:4d}  {w:28s} {", ".join(locs)}')
    print(f'--- 共 {len(counts)} 个不同英文标识符，总出现 {sum(counts.values())} 次 ---')


if __name__ == '__main__':
    main()
