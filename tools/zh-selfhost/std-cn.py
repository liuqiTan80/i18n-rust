#!/usr/bin/env python3
"""zh 自举：把 .zh 源真相里的英文 std/三方名替换为 lang-pack 的中文词条（反向中文化）。

与 rename-abi.py（自定义 ABI 改名，选非词表中文名使 eject 透传）相反：
本工具用词表【已有】的 zh→en 词条，把 .zh 里的英文方法名换成中文方言词，
eject 时词表再将其转回英文。

验收金标准：regen 后产物 .rs 的**代码区字节不变**（注释中文化是额外收益）。
关键陷阱：候选词的中文若与 engine 既有自定义中文标识符**撞名**（如 is_empty→是否为空
与自定义方法 是否为空 同名），eject 无法区分，会破坏 ABI —— 用 precheck 先筛掉。

用法：
  std-cn.py precheck <en> [<en> ...]   # 只报告：词表是否唯一映射 + 产物代码区是否撞名，不改文件
  std-cn.py apply    <en> [<en> ...]   # 对 15 个 engine .zh 应用 en→zh（仅唯一映射且不撞名的词）
  std-cn.py revert   <en> [<en> ...]   # 反向 zh→en 回退（CJK 安全边界）
"""
import re, sys, tomllib, pathlib, collections

ROOT = pathlib.Path(__file__).resolve().parents[2]
PACK = ROOT / 'crates/engine/lang-packs/zh'
SRC = ROOT / 'crates/engine/src'

CJK = '\u4e00-\u9fff'
# 字符串字面量感知（含 r#""#、字节串、字符、生命周期）——替换时跳过其中内容
# 原始字符串 r#"..."# 可含内嵌 " 与换行，须按 # 数配平收尾（反向引用 h），
# 切勿用 [^"]* —— 那会在 JSON 键的第一个内嵌引号处提前截断，误改串内英文。
STR_SPAN = re.compile(
    r'r(?P<h>#+)"[\s\S]*?"(?P=h)|r"[^"]*"|b?"(?:[^"\\\n]|\\[\s\S])*"'
    r"|b?'(?:[^'\\\n]|\\[ntbr'\\]|\\x[0-9a-fA-F]{2}|\\u\{[0-9a-fA-F]+\})'"
    r"|'[_A-Za-z][A-Za-z0-9_]*")


def load_en2zh():
    en2zh = collections.defaultdict(set)

    def walk(x):
        for k, v in x.items():
            if isinstance(v, str):
                en2zh[v].add(k)
            elif isinstance(v, dict):
                walk(v)

    for f in ['keywords.toml', 'stdlib.toml', 'module_paths.toml']:
        walk(tomllib.loads((PACK / f).read_text()))
    for f in (PACK / 'crates').glob('*.toml'):
        walk(tomllib.loads(f.read_text()))
    return en2zh


def code_idents_in_products(zh_word):
    """在权威产物 .rs 代码区（去注释、去字符串）中统计某中文词作为独立标识符的出现。"""
    hits = 0
    for rs in SRC.glob('*.rs'):
        t = rs.read_text()
        t = re.sub(r'/\*.*?\*/', '', t, flags=re.S)
        t = re.sub(r'//[^\n]*', '', t)
        t = STR_SPAN.sub('', t)  # 复用同一串感知正则（含 r#".."# 内嵌引号），避免测试夹具字符串误判为代码区撞名
        hits += len(re.findall(r'(?<![' + CJK + r'0-9A-Za-z_])' + re.escape(zh_word) + r'(?![' + CJK + r'0-9A-Za-z_])', t))
    return hits


def resolve_mapping(en2zh, words):
    ok, skip = {}, {}
    for w in words:
        zs = en2zh.get(w, set())
        if len(zs) != 1:
            skip[w] = f'非唯一/无词条 {sorted(zs) if zs else "∅"}'
            continue
        ok[w] = next(iter(zs))
    return ok, skip


def sub_word(text, src, dst):
    """CJK 安全的全词替换：src/dst 相邻不得是中文字母数字下划线。"""
    pat = r'(?<![' + CJK + r'0-9A-Za-z_])' + re.escape(src) + r'(?![' + CJK + r'0-9A-Za-z_])'
    return re.sub(pat, dst, text)


def transform(t, pairs):
    def seg_apply(seg):
        for src, dst in sorted(pairs, key=lambda p: -len(p[0])):
            seg = sub_word(seg, src, dst)
        return seg
    out, last = [], 0
    for m in STR_SPAN.finditer(t):
        out.append(seg_apply(t[last:m.start()]))
        out.append(m.group(0))
        last = m.end()
    out.append(seg_apply(t[last:]))
    return ''.join(out)


def apply_files(pairs):
    n = 0
    for zh in sorted(SRC.glob('*.zh')):
        s = zh.read_text()
        r = transform(s, pairs)
        if r != s:
            zh.write_text(r)
            n += 1
    print(f'落盘 {n} 个 .zh')


def main():
    mode = sys.argv[1]
    words = sys.argv[2:]
    en2zh = load_en2zh()
    ok, skip = resolve_mapping(en2zh, words)
    for w, why in skip.items():
        print(f'  跳过 {w}: {why}')

    if mode == 'precheck':
        print(f'{"en":16} {"zh":10} 产物代码区撞名数')
        for w, zh in sorted(ok.items()):
            c = code_idents_in_products(zh)
            flag = '  ⚠️撞名(不可用)' if c else '  ✓安全'
            print(f'{w:16} {zh:10} {c}{flag}')
    elif mode == 'apply':
        pairs = []
        for w, zh in ok.items():
            if code_idents_in_products(zh):
                print(f'  拒绝 {w}→{zh}：与既有自定义标识符撞名')
                continue
            pairs.append((w, zh))
        print(f'apply 映射({len(pairs)}):', dict(pairs))
        apply_files(pairs)
    elif mode == 'revert':
        apply_files([(zh, w) for w, zh in ok.items()])
    else:
        sys.exit('模式: precheck | apply | revert')


if __name__ == '__main__':
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    main()
