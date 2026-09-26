#!/usr/bin/env python3
"""README 结构 parity 检查：以 README.en.md 为基准，校验 9 份翻译 README 结构对齐。

检查项（任一失败退出码非零）：
1. h1/h2/h3 标题数与基准一致（防翻译版漏章节/多章节漂移）
2. 页内锚点 `](#...)` 必须指向本文档真实存在的标题（锚点失效=点击跳转失败）
3. 禁止 `](#-` 写法（项目统一「emoji 后无空格」标题风格，避免生成前导连字符锚点）
4. 代码围栏 ``` 必须成对（防未闭合代码块吞掉后续正文）
5. 「从这里开始」导航行须含 3 个页内锚点 + 文档站 + 飞书知识库链接
6. 必需结构元素：🪞 双平台镜像说明行、[MIT](LICENSE) 版权行

用法：python3 tools/check-readme-parity.py
"""

import argparse
import os
import re
import sys
import unicodedata

BASELINE = "README.en.md"
TARGETS = ["ja", "ru", "de", "es", "fr", "pt", "ko", "ar", "hi"]

ANCHOR_RE = re.compile(r"\]\(#([^)]*)\)")
LINK_TEXT_RE = re.compile(r"\[([^\]]*)\]\([^)]*\)")


def slugify(heading: str) -> str:
    """按 GitHub 标题锚点规则（github-slugger）生成 slug。

    保留 Unicode 字母/组合记号(M)/数字/连接符(Pc) 与连字符、空格
    （天城文等组合记号属 Mark 类别，用 \\w 会误删，须按类别判断），
    其余标点与 emoji 移除；空格转连字符。与 GitHub 行为对齐的关键点：
    emoji/标点被移除后若留下前导空格，会生成前导连字符锚点（`-xxx`）——
    项目已禁止该写法，此处不做 trim 以如实暴露。
    """
    text = LINK_TEXT_RE.sub(r"\1", heading)  # 行内链接取文本
    text = text.replace("`", "").replace("*", "")
    text = text.lower()
    kept = []
    for ch in text:
        cat = unicodedata.category(ch)
        if ch in " -" or cat[0] in ("L", "M", "N") or cat == "Pc":
            kept.append(ch)
    return "".join(kept).replace(" ", "-")


def parse_readme(path: str) -> dict:
    with open(path, encoding="utf-8") as f:
        text = f.read()
    lines = text.splitlines()
    headings = [ln for ln in lines if ln.startswith("#")]
    h1 = sum(1 for ln in headings if ln.startswith("# "))
    h2 = sum(1 for ln in headings if ln.startswith("## "))
    h3 = sum(1 for ln in headings if ln.startswith("### "))
    slugs = {slugify(ln.lstrip("#").strip()) for ln in headings}
    anchors = ANCHOR_RE.findall(text)
    nav_lines = [ln for ln in lines if ANCHOR_RE.search(ln)]
    return {
        "h1": h1,
        "h2": h2,
        "h3": h3,
        "slugs": slugs,
        "anchors": anchors,
        "fence_count": sum(1 for ln in lines if ln.startswith("```")),
        "nav_lines": nav_lines,
        "text": text,
    }


def check(lang: str, base: dict, data: dict) -> int:
    errors = 0

    for level in ("h1", "h2", "h3"):
        if data[level] != base[level]:
            print(f"❌ {lang}: {level} 标题数 {data[level]} != 基准 {base[level]}")
            errors += 1

    bad_anchors = [a for a in data["anchors"] if a not in data["slugs"]]
    if bad_anchors:
        print(f"❌ {lang}: {len(bad_anchors)} 个页内锚点无对应标题："
              f"{', '.join(bad_anchors[:5])}" + (" …" if len(bad_anchors) > 5 else ""))
        errors += 1

    if "](#-" in data["text"]:
        print(f"❌ {lang}: 存在 `](#-` 前导连字符锚点（违反「emoji 后无空格」标题规范）")
        errors += 1

    if data["fence_count"] % 2 != 0:
        print(f"❌ {lang}: 代码围栏 ``` 数量为奇数（{data['fence_count']}），存在未闭合代码块")
        errors += 1

    if len(data["anchors"]) != len(base["anchors"]):
        print(f"❌ {lang}: 页内锚点链接数 {len(data['anchors'])} != 基准 {len(base['anchors'])}")
        errors += 1

    if not data["nav_lines"]:
        print(f"❌ {lang}: 未找到「从这里开始」导航行（含页内锚点链接）")
        errors += 1
    else:
        nav = data["nav_lines"][0]
        missing = []
        if not ANCHOR_RE.search(nav):
            missing.append("页内锚点")
        if "liuqiTan80.github.io/i18n-rust" not in nav:
            missing.append("文档站")
        if "my.feishu.cn" not in nav:
            missing.append("飞书知识库")
        if missing:
            print(f"❌ {lang}: 导航行缺少：{'、'.join(missing)}")
            errors += 1

    if "🪞" not in data["text"]:
        print(f"❌ {lang}: 缺少 🪞 双平台镜像说明行")
        errors += 1

    if "[MIT](LICENSE) © tan80" not in data["text"]:
        print(f"❌ {lang}: 缺少统一的许可证行 `[MIT](LICENSE) © tan80`")
        errors += 1

    return errors


def main() -> int:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    parser = argparse.ArgumentParser(description="翻译 README 结构 parity 检查")
    parser.add_argument("--repo-root", default=repo_root, help="仓库根目录")
    args = parser.parse_args()

    base_path = os.path.join(args.repo_root, BASELINE)
    if not os.path.exists(base_path):
        print(f"❌ 基准文件不存在：{base_path}")
        return 1
    base = parse_readme(base_path)
    base_errs = 0
    for a in base["anchors"]:
        if a not in base["slugs"]:
            print(f"❌ 基准 {BASELINE}: 锚点 `{a}` 无对应标题")
            base_errs += 1

    total = base_errs
    for lang in TARGETS:
        path = os.path.join(args.repo_root, f"README.{lang}.md")
        if not os.path.exists(path):
            print(f"❌ {lang}: 缺少 README.{lang}.md")
            total += 1
            continue
        data = parse_readme(path)
        errs = check(f"README.{lang}.md", base, data)
        if errs == 0:
            print(f"✅ README.{lang}.md 结构对齐（h2={data['h2']}, h3={data['h3']}, "
                  f"锚点={len(data['anchors'])}, 围栏={data['fence_count']}）")
        total += errs

    if total:
        print(f"\nREADME parity 检查失败：{total} 项问题（基准：{BASELINE}，共 {len(TARGETS)} 份翻译）")
        return 1
    print(f"\n✅ README parity 检查通过：{len(TARGETS)} 份翻译均与 {BASELINE} 结构对齐"
          f"（h2={base['h2']}, h3={base['h3']}, 锚点={len(base['anchors'])}）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
