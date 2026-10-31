#!/usr/bin/env python3
"""术语三方一致性检查：总术语表 ↔ 附录B索引 ↔ 各章「本章术语表」。

检查项（任一 HARD 失败退出码非零）：
1. [HARD] 总术语表/附录B 内部：词条不重复；「见「X」」目标必须存在且不自环；
   解释末尾的（第 N 章）章节号合法；见引用词条章节号必须与目标一致。
2. [HARD] 总术语表 ↔ 附录B：两表都收录的词，章节号不得矛盾。
3. [HARD] 总术语表自解释词条：被引章节正文必须真含该词（防“指错章”）；
   确实不命中的概念词记入 tools/glossary-phantoms.json 豁免清单（待下一轮重命名/改写）。
4. [HARD] 覆盖度：各章「本章术语表」的词必须收进总术语表（错误码 E+4位与
   登记在 SKIP_WORDS 的元词除外，错误码查附录 C）。
5. [HARD] 附录B 字面命中：条目词须在被引章正文出现（2026-10-31 清单清零后上锁；
   指向附录的条目天然跳过）。

用法：python3 tools/check-glossary-consistency.py [--tutorials-dir tutorials]
                                  [--phantoms tools/glossary-phantoms.json] [--report]
"""

import argparse
import json
import os
import re
import sys

# 覆盖度检查的跳过规则：错误码词条目不收总术语表（附录 C 是错误码字典）
SKIP_COVERAGE_RE = re.compile(r"^E\d{4}$")
SKIP_COVERAGE_WORDS = {"常见问题"}

# 文件名 → 章节标识；中文数字表
CN_NUM = {
    "零": 0, "一": 1, "二": 2, "三": 3, "四": 4, "五": 5, "六": 6, "七": 7,
    "八": 8, "九": 9, "十": 10,
}


def cn_to_int(s: str):
    """章节编号（阿拉伯数字或中文数字 一~九十九）转 int；失败返回 None。"""
    if not s:
        return None
    if s.isdigit():
        return int(s)
    if "十" in s:
        left, _, right = s.partition("十")
        tens = CN_NUM.get(left, 1) if left else 1
        ones = CN_NUM.get(right, 0) if right else 0
        if left and left not in CN_NUM:
            return None
        if right and right not in CN_NUM:
            return None
        return tens * 10 + ones
    return CN_NUM[s] if len(s) == 1 and s in CN_NUM else None


def chapter_label(m: re.Match):
    """把「第X章 / 附录X / 开篇」匹配转成统一标识。"""
    name = m.group(0)
    if name.startswith("附录"):
        return "附录" + re.sub(r"\s+", "", name)[2]
    if name == "开篇":
        return "开篇"
    inner = re.sub(r"\s+", "", name)[1:-1]
    num = cn_to_int(inner)
    return ("章", num) if num is not None else None


# 兼容「第 17 章」（总术语表，阿拉伯数字带空格）与「第十七章」（附录B，中文数字）
CHAPTER_RE = re.compile(r"第\s*[0-9零一二三四五六七八九十]+\s*章|附录\s*[A-E]|开篇")


def first_chapter(text: str):
    """取文本中第一个章节标识（匹配模式已容忍内部空白）。"""
    m = CHAPTER_RE.search(text)
    return chapter_label(m) if m else None


def norm_term(t: str) -> str:
    """归一化词条：去括号注释、去空白与连字符，保留主干词。

    连字符也剥：「让-否则」与章里标题「让 否则」是同一个词，不能因写法差异判两台。"""
    t = re.sub(r"（[^）]*）", "", t)
    t = re.sub(r"[\s\-_]", "", t)
    return t


def load_chapter_texts(tutorials_dir: str) -> dict:
    """读入各章正文（去空白）：{章节标识: 文本}，供「词必须在被引章节出现」检查。"""
    result = {}
    for fname in os.listdir(tutorials_dir):
        m = re.match(r"^第([零一二三四五六七八九十]+)章：", fname)
        if not m:
            continue
        num = cn_to_int(m.group(1))
        if num is None:
            continue
        with open(os.path.join(tutorials_dir, fname), encoding="utf-8") as f:
            result[("章", num)] = re.sub(r"\s+", "", f.read())
    return result


def load_master(path: str) -> dict:
    """解析总术语表：返回 {归一化词: {raw, chapter, ref(见目标), line}}。"""
    entries = {}
    dupes = []
    with open(path, encoding="utf-8") as f:
        for lineno, line in enumerate(f, 1):
            m = re.match(r"^- \*\*(.+?)\*\* → (.*)$", line.strip())
            if not m:
                continue
            term, gloss = m.group(1), m.group(2)
            key = norm_term(term)
            chapter = first_chapter(gloss)
            ref = None
            rm = re.match(r"^见「(.+?)」", gloss)
            if rm:
                ref = norm_term(rm.group(1))
            if key in entries:
                dupes.append((key, entries[key]["line"], lineno))
            else:
                entries[key] = {"raw": term, "chapter": chapter, "ref": ref, "line": lineno}
    return {"entries": entries, "dupes": dupes}


def load_index(path: str) -> dict:
    """解析附录B索引：返回 {归一化词: {raw, chapter, line}}。"""
    entries = {}
    dupes = []
    with open(path, encoding="utf-8") as f:
        for lineno, line in enumerate(f, 1):
            m = re.match(r"^- (.+?)\s*—\s*(.+)$", line.strip())
            if not m:
                continue
            term, loc = m.group(1), m.group(2)
            chapter = first_chapter(loc)
            key = norm_term(term)
            if key in entries:
                dupes.append((key, entries[key]["line"], lineno))
            else:
                entries[key] = {"raw": term, "chapter": chapter, "line": lineno}
    return {"entries": entries, "dupes": dupes}


def load_chapter_tables(tutorials_dir: str) -> dict:
    """解析各章「本章术语表」表格：返回 {章节标识: {归一化词: 行号}}。"""
    result = {}
    for fname in sorted(os.listdir(tutorials_dir)):
        if not fname.endswith(".md"):
            continue
        m = re.match(r"^第([零一二三四五六七八九十]+)章：", fname)
        if not m:
            continue
        num = cn_to_int(m.group(1))
        label = ("章", num)
        path = os.path.join(tutorials_dir, fname)
        terms = {}
        inside = False
        with open(path, encoding="utf-8") as f:
            for lineno, line in enumerate(f, 1):
                s = line.strip()
                if re.match(r"^## \d+\.\d+ 本章术语表", s) or re.match(r"^## 本章术语表", s):
                    inside = True
                    continue
                if inside and s.startswith("## "):
                    break
                if inside:
                    rm = re.match(r"^\| (.+?) \| .+ \|$", s)
                    if rm and rm.group(1).strip() not in ("术语", "词语", ":-", "---"):
                        t = rm.group(1).strip()
                        if set(t) <= set("-:"):
                            continue
                        key = norm_term(t)
                        if key:
                            terms[key] = lineno
        if terms:
            result[label] = terms
    return result


def fmt_chapter(ch) -> str:
    if ch is None:
        return "（无章节号）"
    if isinstance(ch, tuple):
        return f"第{ch[1]}章"
    return str(ch)


def main() -> int:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    parser = argparse.ArgumentParser(description="术语三方一致性检查")
    parser.add_argument("--tutorials-dir", default=os.path.join(repo_root, "tutorials"))
    parser.add_argument(
        "--phantoms",
        default=os.path.join(repo_root, "tools", "glossary-phantoms.json"),
        help="总术语表外观检查豁免清单（概念词全书未命中的登记簿）",
    )
    parser.add_argument("--report", action="store_true", help="打印 SOFT 覆盖度明细")
    args = parser.parse_args()

    phantoms = {}
    if os.path.exists(args.phantoms):
        with open(args.phantoms, encoding="utf-8") as f:
            phantoms = set(json.load(f).get("总术语表", {}))

    master = load_master(os.path.join(args.tutorials_dir, "总术语表.md"))
    index = load_index(os.path.join(args.tutorials_dir, "附录B：索引.md"))
    chapters = load_chapter_tables(args.tutorials_dir)
    chapter_texts = load_chapter_texts(args.tutorials_dir)

    errors = []  # HARD
    soft = []    # SOFT

    # 1. 总术语表内部
    for key, l1, l2 in master["dupes"]:
        errors.append(f"总术语表词条重复：「{key}」（行 {l1} 与 {l2}）")
    for key, l1, l2 in index["dupes"]:
        errors.append(f"附录B词条重复：「{key}」（行 {l1} 与 {l2}）")
    for key, e in master["entries"].items():
        if e["ref"] is not None:
            if e["ref"] == key:
                errors.append(f"总术语表「{key}」的见引用指向自己（行 {e['line']}）")
            elif e["ref"] not in master["entries"]:
                errors.append(f"总术语表「{key}」的见引用目标「{e['ref']}」不存在（行 {e['line']}）")
        if e["chapter"] is None and e["ref"] is None:
            errors.append(f"总术语表「{key}」缺章节号且非见引用（行 {e['line']}）")
        if isinstance(e["chapter"], tuple) and not (1 <= e["chapter"][1] <= 27):
            errors.append(f"总术语表「{key}」章节号越界：{fmt_chapter(e['chapter'])}（行 {e['line']}）")

    # 2. 总术语表 ↔ 附录B 章节号冲突
    for key, me in master["entries"].items():
        ie = index["entries"].get(key)
        if ie and me["chapter"] and ie["chapter"] and me["chapter"] != ie["chapter"]:
            errors.append(
                f"「{key}」章节号冲突：总术语表 {fmt_chapter(me['chapter'])}（行 {me['line']}）"
                f" vs 附录B {fmt_chapter(ie['chapter'])}（行 {ie['line']}）"
            )

    # 2b. 被引章节必须真的包含该词（只检自带解释的词条；见引用词条是同义词指针，豁免）
    #     总术语表豁免记于 phantoms；附录B 清单已清零，2026-10-31 起同为 HARD（防回潮）。
    for src, entries in (("总术语表", master["entries"]), ("附录B", index["entries"])):
        for key, e in entries.items():
            ch = e["chapter"]
            if not isinstance(ch, tuple) or e.get("ref"):
                continue
            text = chapter_texts.get(ch)
            if text is not None and key not in text:
                msg = f"{src}「{key}」声称在{fmt_chapter(ch)}详解，但该章正文没有这个词（行 {e['line']}）"
                if src == "附录B":
                    errors.append(msg)
                elif key not in phantoms:
                    errors.append(msg)

    # 3. 覆盖度：章表词必在总表（HARD，错误码等按跳过规则豁免）；附录B 有意只收部分词（SOFT 不报）
    only_master = sorted(set(master["entries"]) - set(index["entries"]))
    only_index = sorted(set(index["entries"]) - set(master["entries"]))
    # 章表 ↔ 总表
    ch_all = {}
    for label, terms in chapters.items():
        for t, ln in terms.items():
            ch_all.setdefault(t, []).append((label, ln))
    missing_in_master = sorted(
        t for t in set(ch_all) - set(master["entries"])
        if not SKIP_COVERAGE_RE.match(t) and t not in SKIP_COVERAGE_WORDS)
    for t in missing_in_master:
        where = "、".join(fmt_chapter(c) for c, _ in ch_all[t])
        errors.append(f"章术语表词「{t}」（{where}）未收进总术语表，请补词条或登记豁免")

    if args.report:
        print(f"— 在总术语表、不在附录B（{len(only_master)}）：", "、".join(only_master))
        print(f"— 在附录B、不在总术语表（{len(only_index)}）：", "、".join(only_index))
        print(f"— 在章术语表、不在总术语表（{len(missing_in_master)}）：",
              "、".join(f"{t}({'+'.join(fmt_chapter(c) for c, _ in v)})" for t, v in
                        ((t, ch_all[t]) for t in missing_in_master)))

    # 见引用链的章节号应与目标一致（HARD）
    for key, e in master["entries"].items():
        if e["ref"] and e["chapter"] and e["ref"] in master["entries"]:
            tc = master["entries"][e["ref"]]["chapter"]
            if tc and e["chapter"] != tc:
                errors.append(
                    f"总术语表「{key}」见引用「{e['ref']}」但章节号不一致："
                    f"{fmt_chapter(e['chapter'])} vs {fmt_chapter(tc)}（行 {e['line']}）"
                )

    if errors:
        print(f"❌ 术语一致性检查失败，共 {len(errors)} 处：")
        for e in errors:
            print("  -", e)
        return 1
    print(f"✅ 术语三方一致性通过：总术语表 {len(master['entries'])} 词 / "
          f"附录B {len(index['entries'])} 词 / 章术语表 {len(chapters)} 章 {len(ch_all)} 词")
    return 0


if __name__ == "__main__":
    sys.exit(main())
