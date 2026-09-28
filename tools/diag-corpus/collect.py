#!/usr/bin/env python3
"""诊断语料采集：把反例源码喂给真实 rustc，抽出主消息与全部子诊断原文。

为什么需要它：消息表覆盖哪些句子，看语言包本身永远看不全——只有真跑编译器才知道
rustc 今天会吐哪些话（同一错误码在不同上下文下的 help/note 完全不同）。本脚本把
`tools/diag-corpus/fixtures/*.rs` 逐个编译，收集 `--error-format=json` 里的
message + children[].message，产出去重后的消息清单，交给 `diag_audit` 示例判定
哪些句子在语言包里译不出来。

用法：
  python3 tools/diag-corpus/collect.py --out /tmp/diag-msgs.txt
  cargo run -q -p i18n-rust-engine --example diag_audit -- \
      --errors crates/engine/lang-packs/zh/errors.toml --messages /tmp/diag-msgs.txt
  # 退出码非零 ＝ 存在未译全的句子（可直接当门禁）

注意：rustc 在仓库外运行（fixtures 复制到临时目录），避免转译产物或元数据
落到宿主项目里。
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.join(HERE, "fixtures")


def extract(path: str):
    """返回该文件产生的、**会被翻译层渲染**的诊断文本。

    引擎只翻译主消息与 `level == "help"` 的子诊断（见 translator.rs），
    `note`/`warning` 子句不进翻译管线，因此不采集——避免为看不到的句子补死键。
    主消息另优先由 E0xxx 错误码表（完整桩）翻译，因此一并输出 code 供下游分流。
    返回 (level, code, message) 列表（code 可为空串）。
    """
    # --out-dir 指向源文件所在临时目录：rustc 默认把 --crate-type lib 产物
    # （lib<名>.rlib）写到**进程 CWD**而非源目录，不设 --out-dir 会让采样
    # 垃圾落到仓库根（正是此前根目录一堆 *.rlib 的来源）。
    workdir = os.path.dirname(path)
    proc = subprocess.run(
        [
            "rustc", "--edition", "2021", "--crate-type", "lib",
            "--error-format=json", "--out-dir", workdir, path,
        ],
        capture_output=True, text=True, cwd=workdir,
    )
    texts = []
    # rustc 的 JSON 诊断走 stderr（cargo 才把部分信息分流到 stdout），两侧都扫以防万一
    lines = (proc.stderr or "").splitlines() + (proc.stdout or "").splitlines()
    for line in lines:
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            diag = json.loads(line)
        except json.JSONDecodeError:
            continue
        # 顶层对象只有 error/warning 才是真正的主诊断；note 级顶层对象是
        # rustc 脚注（如 “For more information about this error…”），不进翻译层
        top_level = diag.get("level")
        msg = diag.get("message")
        code = (diag.get("code") or {}).get("code") or ""
        if msg and top_level in ("error", "warning"):
            texts.append(("main", code, msg))
        for child in diag.get("children", []) or []:
            if child.get("level") != "help":
                continue
            cmsg = child.get("message")
            if cmsg:
                texts.append(("help", "", cmsg))
    return texts


def main() -> int:
    parser = argparse.ArgumentParser(description="采集真实 rustc 诊断消息原文")
    parser.add_argument("--out", default="/tmp/diag-msgs.txt", help="消息清单输出路径")
    parser.add_argument("--fixtures", default=FIXTURES, help="反例源码目录")
    parser.add_argument("--keep-temp", action="store_true", help="保留临时编译目录")
    args = parser.parse_args()

    if not os.path.isdir(args.fixtures):
        print(f"❌ 反例目录不存在：{args.fixtures}")
        return 1
    sources = sorted(f for f in os.listdir(args.fixtures) if f.endswith(".rs"))
    if not sources:
        print(f"❌ 反例目录为空：{args.fixtures}")
        return 1

    tmp = tempfile.mkdtemp(prefix="i18n-rust-diag-corpus-")
    seen, collected, broken = set(), [], []
    try:
        for name in sources:
            src = os.path.join(tmp, name)
            shutil.copyfile(os.path.join(args.fixtures, name), src)
            texts = extract(src)
            if not texts:
                broken.append(name)
            for level, code, t in texts:
                flat = " ".join(t.split())  # rustc 消息不含换行，保守归一空白
                key = (level, flat)
                if key not in seen:
                    seen.add(key)
                    collected.append(f"{level}\t{code}\t{flat}")
    finally:
        if args.keep_temp:
            print(f"临时目录保留：{tmp}")
        else:
            shutil.rmtree(tmp, ignore_errors=True)

    with open(args.out, "w", encoding="utf-8") as f:
        f.write("\n".join(collected) + "\n")

    print(f"✅ 采集完成：{len(sources)} 个反例 → {len(collected)} 条去重消息原文（main+help）→ {args.out}")
    if broken:
        print(f"⚠️  以下反例未产生可翻译诊断（只出 note 或不报错）：{', '.join(broken)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
