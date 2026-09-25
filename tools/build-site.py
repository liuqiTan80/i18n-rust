#!/usr/bin/env python3
# =============================================================
# rzc 文档站构建脚本（mdBook 多语言装配）
#
# 把仓库中的教程与参考文档组装成多语言静态站点：
#   tutorials/*.md         → 中文书（另含 docs/ 参考文档，仓库相对链接改写）
#   tutorials/<lang>/*.md  → 对应语言书（en/ja/ru；README.md 为仓库导航，不入站）
#
# 站点装配源（book/<lang>/ 下的 book.toml / SUMMARY.md / index.md 与
# book/lang-switch.js）随仓库提交；本脚本只做组装与构建，不改动教程源文件。
#
# 产物（默认）：
#   _site/<lang>/     每种语言一本书（mdbook 输出）
#   _site/index.html  站根落地页（语言卡片）
#   _site/404.html    站根 404（跳回落地页）
#   _site/.nojekyll   GitHub Pages 标记
#
# 用法：
#   python3 tools/build-site.py [--out _site] [--work build/site-work]
#   python3 tools/build-site.py --serve zh     # 本地预览（mdbook serve :3000）
#
# 依赖：mdbook（cargo install mdbook --locked --version 0.4.52，与 CI 同款）
# =============================================================
import argparse
import os
import re
import shutil
import subprocess
import sys

REPO_URL = "https://github.com/liuqiTan80/i18n-rust"
BLOB = REPO_URL + "/blob/main"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# 语言注册表：code → (显示名, book/ 下书目录名, 教程源目录)
# 新增语言时同步更新 book/lang-switch.js 的 LANGS 与 book/README.md 的步骤
LANGS = {
    "zh": ("中文", "zh", "tutorials"),
    "en": ("English", "en", "tutorials/en"),
    "ja": ("日本語", "ja", "tutorials/ja"),
    "ru": ("Русский", "ru", "tutorials/ru"),
}

# 中文书附加的参考文档（docs/ 下文件名；复制到站内 参考/ 子目录）
REFERENCE_DOCS = [
    "contributing-lang-pack.md",
    "dialect-framework-blueprint.md",
    "missing-mapping-guide.md",
    "third-party-mapping.md",
    "third-party-registry.md",
    "translation-status.md",
]

MDBOOK_HINT = "cargo install mdbook --locked --version 0.4.52"


def run(cmd):
    subprocess.run(cmd, check=True)


def assemble(lang, work_dir):
    """组装单个语言的工作区，返回 src 目录路径。"""
    _label, book_name, src_rel = LANGS[lang]
    book_dir = os.path.join(ROOT, "book", book_name)
    src_dir = os.path.join(work_dir, "src")
    os.makedirs(src_dir, exist_ok=True)

    # 1. 站点装配文件（配置 / 目录 / 首页 / 语言切换脚本）
    shutil.copy2(os.path.join(book_dir, "book.toml"), os.path.join(work_dir, "book.toml"))
    for name in ("SUMMARY.md", "index.md"):
        shutil.copy2(os.path.join(book_dir, name), os.path.join(src_dir, name))
    shutil.copy2(
        os.path.join(ROOT, "book", "lang-switch.js"),
        os.path.join(work_dir, "lang-switch.js"),
    )

    # 2. 教程正文（README.md 是仓库目录导航，站内由 SUMMARY 承担）
    src_root = os.path.join(ROOT, src_rel)
    for name in sorted(os.listdir(src_root)):
        if name.endswith(".md") and name != "README.md":
            shutil.copy2(os.path.join(src_root, name), os.path.join(src_dir, name))

    # 3. 参考文档（仅中文书）：../ 仓库相对链接改写为 GitHub blob 绝对链接
    if lang == "zh":
        ref_dir = os.path.join(src_dir, "参考")
        os.makedirs(ref_dir, exist_ok=True)
        for name in REFERENCE_DOCS:
            with open(os.path.join(ROOT, "docs", name), encoding="utf-8") as f:
                text = f.read()
            text = re.sub(r"\]\(\.\./", f"]({BLOB}/", text)
            with open(os.path.join(ref_dir, name), "w", encoding="utf-8") as f:
                f.write(text)

    return src_dir


def build(lang, work_dir, out_dir):
    """执行 mdbook build，返回 SUMMARY 条目数（页数）。"""
    dest = os.path.abspath(os.path.join(out_dir, lang))
    rel_dest = os.path.relpath(dest, work_dir)
    run(["mdbook", "build", work_dir, "-d", rel_dest])
    summary = os.path.join(work_dir, "src", "SUMMARY.md")
    with open(summary, encoding="utf-8") as f:
        return sum(1 for line in f if line.startswith("- ["))


LANDING_TEMPLATE = """<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>rzc 文档站 · 用母语写 Rust</title>
<style>
  :root { color-scheme: light dark; }
  body { font-family: system-ui, "Segoe UI", "Noto Sans CJK SC", sans-serif;
         max-width: 720px; margin: 8vh auto 4vh; padding: 0 1.2rem; color-scheme: inherit; }
  h1 { font-size: 1.6rem; margin-bottom: .2rem; }
  .tag { color: #888; margin-top: 0; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
          gap: 14px; margin: 2rem 0; }
  .card { display: block; border: 1px solid #d0d0d0; border-radius: 10px;
          padding: 1rem 1.2rem; text-decoration: none; color: inherit; }
  .card:hover { border-color: #3b82f6; box-shadow: 0 2px 8px rgba(0,0,0,.08); }
  .card h2 { margin: 0 0 .3rem; font-size: 1.15rem; }
  .card p { margin: 0; color: #888; font-size: .9rem; }
  footer { color: #888; font-size: .85rem; border-top: 1px solid #eee; padding-top: 1rem; }
</style>
</head>
<body>
<h1>rzc 文档站</h1>
<p class="tag">用母语写 Rust · Write Rust in your native language</p>
<div class="grid">
{{CARDS}}
</div>
<footer>
  <a href="{{REPO_URL}}">GitHub 仓库</a> ·
  <a href="{{REPO_URL}}/releases">下载 rzc</a> ·
  教程代码块均经逐块编译验证，随仓库同步构建
</footer>
</body>
</html>
"""

NOT_FOUND_TEMPLATE = """<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>页面不存在 · rzc 文档站</title>
<script>
(function () {
  // GitHub Pages 项目页：路径首段是仓库名；自定义域名时不存在该段
  var parts = location.pathname.split("/").filter(Boolean);
  var base = "/" + (parts[0] === "i18n-rust" ? "i18n-rust/" : "");
  location.replace(base);
})();
</script>
</head>
<body>
<p>页面不存在，正在返回首页……若未自动跳转，请访问
<a href="https://liuqiTan80.github.io/i18n-rust/">文档站首页</a>。</p>
</body>
</html>
"""


def write_extras(out_dir, counts):
    """生成站根落地页与 .nojekyll / 404.html。"""
    cards = []
    for code, (label, _book, _src) in LANGS.items():
        cards.append(
            '<a class="card" href="./{code}/"><h2>{label}</h2><p>{n} 篇文档</p></a>'.format(
                code=code, label=label, n=counts.get(code, 0)
            )
        )
    html = (
        LANDING_TEMPLATE
        .replace("{{CARDS}}", "\n".join(cards))
        .replace("{{REPO_URL}}", REPO_URL)
    )
    with open(os.path.join(out_dir, "index.html"), "w", encoding="utf-8") as f:
        f.write(html)
    with open(os.path.join(out_dir, "404.html"), "w", encoding="utf-8") as f:
        f.write(NOT_FOUND_TEMPLATE)
    open(os.path.join(out_dir, ".nojekyll"), "w").close()


def main():
    parser = argparse.ArgumentParser(description="rzc 文档站构建（多语言 mdBook）")
    parser.add_argument("--out", default="_site", help="输出目录（默认 _site）")
    parser.add_argument("--work", default="build/site-work", help="组装工作区（默认 build/site-work）")
    parser.add_argument(
        "--serve", metavar="LANG", nargs="?", const="zh",
        help="本地预览：组装指定语言并执行 mdbook serve（默认 zh，3000 端口）",
    )
    args = parser.parse_args()

    if not shutil.which("mdbook"):
        sys.exit(f"错误：未找到 mdbook。安装：{MDBOOK_HINT}")

    out_dir = os.path.abspath(os.path.join(ROOT, args.out))
    work_root = os.path.abspath(os.path.join(ROOT, args.work))

    # 全新组装，避免残留文件进入站点
    shutil.rmtree(work_root, ignore_errors=True)

    if args.serve:
        lang = args.serve
        if lang not in LANGS:
            sys.exit(f"错误：未知语言 {lang}（可选：{', '.join(LANGS)}）")
        work_dir = os.path.join(work_root, lang)
        assemble(lang, work_dir)
        print(f"预览 {LANGS[lang][0]}：mdbook serve（http://localhost:3000，Ctrl-C 退出）")
        run(["mdbook", "serve", work_dir, "-p", "3000"])
        return

    shutil.rmtree(out_dir, ignore_errors=True)
    os.makedirs(out_dir, exist_ok=True)
    counts = {}
    for lang in LANGS:
        work_dir = os.path.join(work_root, lang)
        assemble(lang, work_dir)
        counts[lang] = build(lang, work_dir, out_dir)
    write_extras(out_dir, counts)

    print("站点构建完成：")
    for lang, (label, _book, _src) in LANGS.items():
        print(f"  {label:<9} {counts[lang]:>2} 篇 → {args.out}/{lang}/")
    print(f"  落地页 → {args.out}/index.html（本地预览：python3 tools/build-site.py --serve zh）")


if __name__ == "__main__":
    main()
