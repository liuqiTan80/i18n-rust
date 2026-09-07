#!/usr/bin/env python3
"""方言コード変換ツール（dialect-convert）

標準 Rust の ```rust コードブロックを、指定した言語パックの方言に変換する。
文字列リテラルとコメントの内部は変換しない（安全のため）。

使い方:
    python3 tools/dialect-convert.py --lang ja --src en.md --out ja.md
    # または stdin -> stdout
    cat en.md | python3 tools/dialect-convert.py --lang ja > ja.md

変換は「予約語 + 標準ライブラリ部品名」の逆写像を用いる。
&mut / &str / & は個別に処理し、残りは (?<!\w)TOKEN(?!\w) で一括置換。
"""
import argparse
import os
import re
import sys
import tomllib


def load_pack(lang: str):
    """lang-packs/<lang>/{keywords,stdlib}.toml を読み、
    すべての 値(rust) -> 鍵(母語) 写像を作る（重複はキーワード側を優先）。"""
    base = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        "crates", "engine", "lang-packs", lang)
    rev = {}

    def flatten(value):
        out = {}
        if isinstance(value, dict):
            for k, v in value.items():
                if isinstance(v, dict):
                    out.update(flatten(v))
                elif isinstance(v, str):
                    out[k] = v  # 葉：ネイティブ -> rust
        return out

    # keywords.toml: 鍵=母語, 値=rust
    kw = tomllib.load(open(os.path.join(base, "keywords.toml"), "rb"))
    kw_rev = {}
    for native, rust in flatten(kw).items():
        kw_rev.setdefault(rust, native)
    # stdlib.toml: 鍵=母語, 値=rust（メソッド名等）
    sl = tomllib.load(open(os.path.join(base, "stdlib.toml"), "rb"))
    sl_rev = {}
    for native, rust in flatten(sl).items():
        sl_rev.setdefault(rust, native)

    # キーワード側を優先（構文語は確実）、stdlib で補完
    rev.update(sl_rev)
    rev.update(kw_rev)
    return rev


# パンクチュエーション置換（トークン置換より先）
PUNCT = [
    ("&mut", None),   # 後で lang ごとに上書き
    ("&str", None),
    ("&", None),
]

# 補完マップ（パックに型名が無いもの等）
EXTRA = {
    "ja": {
        "Vec": "ベクタ",
        "&mut": "可変参照",
        "&str": "文字列参照",
        "&": "参照",
    },
}


def make_punct(lang):
    extra = EXTRA.get(lang, {})
    return [
        ("&mut", extra.get("&mut", "&mut")),
        ("&str", extra.get("&str", "&str")),
        ("&", extra.get("&", "&")),
    ]


def convert_buf(buf, rev, punct, token_re, token_map):
    # 1) パンクチュエーション
    for a, b in punct:
        if b is not None:
            buf = buf.replace(a, b)
    # 2) 識別子一括置換
    return token_re.sub(lambda m: token_map[m.group(0)], buf)


def convert_line(line, rev, punct, token_re, token_map):
    out = []
    i, n = 0, len(line)
    buf = []
    while i < n:
        c = line[i]
        # 文字列リテラル "..." （エスケープ考慮）
        if c == '"':
            out.append(convert_buf("".join(buf), rev, punct, token_re, token_map))
            buf = []
            k = i + 1
            while k < n and line[k] != '"':
                if line[k] == '\\':
                    k += 1
                k += 1
            if k < n:
                k += 1
            out.append(line[i:k])
            i = k
            continue
        # 文字リテラル / ライフタイム '...'
        if c == "'" and i + 1 < n and line[i + 1] != ' ':
            out.append(convert_buf("".join(buf), rev, punct, token_re, token_map))
            buf = []
            k = i + 1
            while k < n and line[k] != "'":
                if line[k] == '\\':
                    k += 1
                k += 1
            if k < n:
                k += 1
            out.append(line[i:k])
            i = k
            continue
        # 行コメント // （ツール注記 预期错误 等はそのまま）
        if c == '/' and i + 1 < n and line[i + 1] == '/':
            out.append(convert_buf("".join(buf), rev, punct, token_re, token_map))
            out.append(line[i:])
            i = n
            break
        buf.append(c)
        i += 1
    out.append(convert_buf("".join(buf), rev, punct, token_re, token_map))
    return "".join(out)


def build_token_re(rev):
    # 長い順に並べて部分一致を避ける
    tokens = sorted(rev.keys(), key=len, reverse=True)
    # 正規表現特殊文字をエスケープ
    pat = "(?<![\\w])(" + "|".join(re.escape(t) for t in tokens) + ")(?![\\w])"
    return re.compile(pat), {t: rev[t] for t in tokens}


def convert_markdown(text, lang):
    rev = load_pack(lang)
    token_re, token_map = build_token_re(rev)
    punct = make_punct(lang)

    lines = text.split("\n")
    out = []
    in_rust = False
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("```rust"):
            in_rust = True
            out.append(line)
            continue
        if in_rust and stripped.startswith("```"):
            in_rust = False
            out.append(line)
            continue
        if in_rust:
            out.append(convert_line(line, rev, punct, token_re, token_map))
        else:
            out.append(line)
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--lang", required=True)
    ap.add_argument("--src", default=None)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    if args.src:
        text = open(args.src, encoding="utf-8").read()
    else:
        text = sys.stdin.read()
    result = convert_markdown(text, args.lang)
    if args.out:
        open(args.out, "w", encoding="utf-8").write(result)
    else:
        sys.stdout.write(result)


if __name__ == "__main__":
    main()
