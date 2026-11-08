#!/usr/bin/env python3
"""全中文门禁：清点 .zh 方言源「代码区」里剩下的英文标识符，只许减少不许增加。

为什么需要它
------------
本分支的目标是后端全中文。此前只有「一词多译」与「绑定位劫持」两道审计，
两者都只盯中文用得对不对，**没有一处盯「还有多少英文没中文化」**——
于是「表里明明有词条、却照抄英文」（as_str / into_values / .path() / std::）
这类退化会静默累积，模块转完后没人再回头看。

判定口径
--------
**代码区**（剥掉注释与字面量后的部分）：
1. 等长剥离注释与字符串/字符字面量内容（复用 check-zh-artifacts.strip_literals，
   同一套词法口径，避免两处审计互相打架）；
2. 再剥掉数字字面量（`0x…`、带后缀的数值）——里面的 a-f 不是英文；
3. 再剥掉语言包里**已登记的词形**（keywords/stdlib/module_paths/crates 全部键，
   长词优先）——这样 `是ASCII字母数字` 里的 `ASCII` 不算英文，
   而 `作为字符串` 覆盖不到的 `as_str` 才算；
4. 剩下长度 ≥2 的 ASCII 标识符 = 未中文化的英文，按文件计数并与基线比较。

代码区基线是**棘轮**：新文件/新增英文判红；英文下降后须 `--update` 收紧基线，
这样「全中文」是单调推进的，不靠人记住还剩多少。

**注释区**（`//`、`///`、`//!`、`/* */` 的内容）：同一套剥词形口径，但**零容忍**——
注释是读者主要在读的东西，「代码里没英文、注释里全是英文」不算达标。注释里
允许保留的只有两类，且都必须成文登记，不接受就地豁免：

- **路径/文件名**（含 `/` 或带扩展名的引用，如 `crates/engine/src/error.rs`、
  `zh-findings.md`）——它们是真实存在的文件名，改写就找不到文件；
- **专名**（`tools/zh-comment-allowlist.json` 逐条附理由：工具名 `rzc`/`rustc`、
  格式名 `TOML`/`JSON`、语言名 `Rust`、平台名 `Windows`、算法名 `FNV` 等）。

散文化英文（`stdlib`、`token`、`lint`、`HashMap` 这类**有中文术语却没写**的词）
判红；真实存在的英文 API 名若语言包已收词形，也须改用词形。

用法：
  python3 tools/check-zh-ascii.py            # 门禁：代码区超基线 / 注释区有英文 判红
  python3 tools/check-zh-ascii.py --report    # 打出逐词清单（含出现次数）
  python3 tools/check-zh-ascii.py --update    # 用当前计数覆盖代码区基线
"""
import argparse
import importlib.util
import json
import re
import subprocess
import sys
import tomllib
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve()
REPO = HERE.parents[1]
spec = importlib.util.spec_from_file_location("cza", REPO / "tools/check-zh-artifacts.py")
cza = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cza)

PACK = REPO / "crates/engine/lang-packs/zh"
BASELINE = REPO / "tools/zh-ascii-baseline.json"
ALLOWLIST = REPO / "tools/zh-comment-allowlist.json"
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
NUM = re.compile(r"0x[0-9A-Fa-f_]+|\b\d[\w.]*")
# 注释里的路径/文件引用：含斜杠的片段，或带已知扩展名的名字（整体豁免，
# 不然 `crates/engine/src/error.rs` 会被拆成四个“英文词”报假账）
PATHISH = re.compile(
    r"[\w.\-/~<>]*[\w.\-/~<>]/[\w.\-~<>*/]+"                                    # a/b 形式（含 ~/.rz）
    r"|[\w\-]*\.(?:rs|zh|toml|md|json|txt|sh|py|html|lock|tape|gif|nls|yml|yaml|csv|rz|exe|cmd|bat|com|ps1|dll|so|ini)\b"
)
# 反引号区间里的代码标点：含这些符号的区间是「把产物文本逐字摆给读者看」
# （`let x = 1;`、`Box::leak`、`matches!`），不是拿英文当行文用词；散文里的
# `HashMap`/`stdlib`/`token` 不带代码标点，依旧判红。
# 空白也算：`fn main`、`impl Debug` 这类两个词以上的区间必然是产物片段
# （单个英文词才可能是「有中文词形却没写」的散文用词），本轮实测 `fn main`
# 不含任何旧标点符号、被误判成散文后补上。
CODEPUNCT = re.compile(r"[;()\[\]{}=!|&*<>:#$]|\s")
# 导入位的 crate 名：Rust crate 名必须是 ASCII（rustc 对 `--extern` 与
# `use` 路径首段都有硬约束），`公开 使用 zh_pure::*;` 这类薄胶水不是“没中文化的
# 英文”，而是语言本身的边界。只豁免 `use/使用` 路径首段，其余标识符照扫。
CRATE_USE = re.compile(r"(?:(?:公开|pub)\s+)?(?:使用|use)\s+([A-Za-z_][A-Za-z0-9_]*)::")
# 反引号区间里的命令引用：以真命令开头（`rzc run`、`cargo test --workspace`）
CMDSTART = re.compile(r"(?:rzc|cargo|rustc|rustup|rustfmt|clippy|git|make|python3)\b")
# 反引号区间里的命令行选项名（`--help`、-v）：与命令同性质，是对外契约的逐字
# 引用——改写就敲不出那个选项。CODEPUNCT 不含 `-`，所以得单列一条。
OPTSTART = re.compile(r"-{1,2}[A-Za-z][\w-]*$")
# 连字符小写名：工具/文件/包名的惯常写法（rust-analyzer、rust-toolchain）
KEBAB = re.compile(r"\b[a-z][a-z0-9]*(?:-[a-z0-9]+)+\b")
# 编号类引用：台账条目 F16、编译错误码 E0308、基准项 B9 等（大写字母 + 数字）
CODE = re.compile(r"\b[A-Z]{1,3}\d+\b")
# 声明位/导入位的单字母 ASCII 名：scan_file 的 len<2 豁免本意是放过泛型参数 T 与
# 闭包参数，但它同时把「有名字的英文 API」也藏了（`pub fn t`/`pub fn f` 及方言侧
# 对它们的调用）。泛型/闭包参数是结构性 ASCII，声明名与导入名是没中文化的英文，
# 同一条 len<2 规则盖不住两种情况——这里单独量出来（台账 F38）。
# F39 起升为判红：`语言` 模块方言化后已无一处命中，留作只报不红的规则等于没有。
SINGLE_DECL = re.compile(
    r"(?:函数|方法|结构体|枚举|常量|静态|特征|模块|类型|类型别名)\s+([A-Za-z])\b"
    r"|::\s*([A-Za-z])\b"
    r"|[{,]\s*([A-Za-z])\s+作为"
)
# 寿命位的 ASCII 寿命名：`'a`/`'static` 以前被归入「结构性 ASCII」，但寿命名对
# rzc 与 rustc 都是普通标识符，`'甲` 已实测可用（台账 F39）——它不是语言约定，
# 是没中文化的名字。`(?!')` 把字符字面量 `'a'` 排除在外。
LIFETIME_ASCII = re.compile(r"(?<![\\\w])'([A-Za-z][A-Za-z0-9_]*)(?!')")
# 类型参数位的单字母名：`<T: 可复制>`、`, F>`、`-> T>`。多字母的英文名
# （`Value` 之类）本来就会被代码区扫到，len<2 只漏了单字母这一种。
GENERIC_PARAM = re.compile(
    r"(?:<|,)\s*([A-Za-z])\s*(?::|,|>)"
    r"|->\s*([A-Za-z])\s*(?=[,>)])"
)
# 方法位的单字母名（`ui.f(...)`、`m.t(...)`）：声明位规则管不到调用位，
# 而全仓 250+ 处 `ui.f(` 证明这类 API 真存在——方言源必须不许写它。
METHOD_ASCII = re.compile(r"\.[A-Za-z]\(")
# 绑定名位：循环变量、闭包参数、`让` 绑定的局部名。单字母在这里不是语言约定
# （只有 `_` 是），而是随手写的英文名——方言源里全部已改中文名，故一并判红。
BINDING_ASCII = re.compile(
    r"对于\s+([A-Za-z])\s+在"
    r"|\|\s*([A-Za-z])\s*[,|]"
    r"|让\s+(?:可变\s+)?([A-Za-z])\s*[:=]"
)


def load_allowlist() -> set:
    """注释区允许保留的专名（小写比较）；没登记就是红"""
    if not ALLOWLIST.exists():
        return set()
    data = json.loads(ALLOWLIST.read_text(encoding="utf-8"))
    return {k.lower() for k in data.get("允许保留", {})}


def registered_forms() -> list:
    """语言包全部键（词形），长词优先；含 ASCII 的词形一并收录

    `ui.toml` 整本排除：它的两节（`["界面消息"]` 622 项消息键、`["解释"]` 191 项
    英文 API 名）键全是**英文原名**，不是母语词形。混进词形登记表等于给
    `main`/`debug`/`clone` 这批「有中文词形却没写」的英文发通行证——注释区
    扫到它们时会被 `strip_forms` 抹空，静默放行（本轮实测出来的第 4 层豁免漏洞）。
    真正该保留的拉丁专名（`JSON`/`HTTP2`/`Actix`/`salvo`）另有
    `tools/zh-comment-allowlist.json` 逐条登记，不靠这本文件。
    """
    forms = set()

    def walk(node):
        if isinstance(node, dict):
            for k, v in node.items():
                forms.add(k)
                walk(v)
        elif isinstance(node, list):
            for v in node:
                walk(v)

    for f in sorted(PACK.rglob("*.toml")):
        if f.name == "ui.toml":
            continue
        try:
            walk(tomllib.loads(f.read_text(encoding="utf-8")))
        except Exception as exc:  # noqa: BLE001 - 词形收集不因单文件失败而中断
            print(f"警告：{f.relative_to(REPO)} 解析失败：{exc}", file=sys.stderr)
    return sorted((x for x in forms if x and x != "版本"), key=len, reverse=True)


def data_keys() -> set:
    """`ui.toml` 的键全集：真实存在的数据标识符，逐字引用时不算「有词形不写」

    与 registered_forms() 排除 ui.toml 是同一件事的两面：那些键（622 个消息键 +
    191 个英文 API 索引名）**不是母语词形**，所以不能进词形登记表当通行证；
    但它们是磁盘上真实存在的键名，注释里引用 `template_main`、
    `panic_msg_index_oob` 与引用 `cache.rs` 同性质。豁免口径因此收紧为
    **整个反引号区间逐字等于一个键**，而不是「长得像 snake_case 就放行」。
    """
    文件 = PACK / "ui.toml"
    if not 文件.exists():
        return set()
    文档 = tomllib.loads(文件.read_text(encoding="utf-8"))
    # 只收 `["界面消息"]`（622 个消息键）：它们是程序内部标识符，改了键就查不到
    # 消息，注释里只能逐字引用。**不收 `["解释"]`**——那 191 项的键是英文 API 名
    # （`clone`、`main`、`Option::unwrap`），个个都有中文词形，给它们发通行证等于
    # 把「有词形却写英文原名」重新藏起来（本轮先收全、量到 3 处后才定此口径）。
    return set(文档.get("界面消息", {}))


def strip_forms(text: str, forms: list) -> str:
    """剥词形时带 ASCII 词边界：纯 ASCII 词形（如表里的 `Level`）不得把
    `DangerLevel` 这类英文标识符切碎——那会把一个真英文词漏报成两个假词。"""
    pat = re.compile(
        r"(?<![A-Za-z0-9_])(?:%s)(?![A-Za-z0-9_])"
        % "|".join(re.escape(f) for f in forms if f)
    )
    out = []
    i = 0
    while True:
        m = pat.search(text, i)
        if not m:
            out.append(text[i:])
            break
        out.append(text[i:m.start()])
        out.append(" " * (m.end() - m.start()))
        i = m.end()
    return "".join(out)


def scan_file(path: Path, forms: list) -> Counter:
    code = cza.strip_literals(path.read_text(encoding="utf-8"))
    code = NUM.sub(" ", code)
    # crate 名必须 ASCII，只豁免导入路径首段（见 CRATE_USE 注释）。
    # 必须在 strip_forms 之前做：`使用`/`公开` 本身也是登记词形，先剥就匹配不到了。
    code = CRATE_USE.sub(lambda m: " " * len(m.group(0)), code)
    code = strip_forms(code, forms)
    hits = Counter()
    for m in IDENT.finditer(code):
        tok = m.group(0)
        if len(tok) < 2 or set(tok) <= {"_"}:
            continue  # 单字母泛型参数/占位符：结构性 ASCII，不是「没中文化的英文」
        hits[tok] += 1
    return hits


def comment_lines(text: str) -> list:
    """按与 strip_literals 同一套词法取注释内容，返回 [(行号, 文本)]

    字符串字面量里的 `//`（如 URL）不当成注释起笔，反过来注释里的引号
    也不会把后续代码误判成字面量。
    """
    out = []
    i, n = 0, len(text)
    line = 1
    depth = 0
    while i < n:
        c = text[i]
        if depth:
            if text.startswith("*/", i):
                depth -= 1
                i += 2
                continue
            if c == "\n":
                out.append((line, text[line_start:i]))
                line += 1
                i += 1
                line_start = i
                continue
            i += 1
            continue
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append((line, text[i:j]))
            i = j
            continue
        if text.startswith("/*", i):
            depth += 1
            line_start = i + 2
            i += 2
            continue
        m = re.match(r'(?:b?r#*)?"|(?:b?")', text[i:])
        if m:
            tok = m.group(0)
            if "r" in tok:
                hashes = tok[tok.index("r") + 1 : -1]
                closer = '"' + hashes
                j = text.find(closer, i + len(tok))
                end = n if j < 0 else j + len(closer)
            else:
                j = i + len(tok)
                while j < n:
                    if text[j] == "\\":
                        j += 2
                        continue
                    if text[j] == '"':
                        j += 1
                        break
                    j += 1
                end = min(j, n)
            line += text.count("\n", i, end)
            i = end
            continue
        m = re.match(r"'(?:\\.|[^\\'])", text[i:])
        if m and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            i += len(m.group(0))
            continue
        if c == "\n":
            line += 1
        i += 1
    if depth:  # 未闭合的块注释，收尾到文件末尾
        out.append((line, text[line_start:]))
    return out


def strip_phrases(body: str, allow: set) -> str:
    """先剥掉带空格的多字专名（如 `VS Code`），免得拆成两个词各报一次"""
    for phrase in sorted((a for a in allow if " " in a), key=len, reverse=True):
        body = re.sub(r"(?<![A-Za-z0-9_])" + re.escape(phrase) + r"(?![A-Za-z0-9_])",
                      " ", body, flags=re.IGNORECASE)
    return body


def strip_spans(text: str, keys: frozenset = frozenset()) -> str:
    """按反引号配对剖开，只把「代码形/命令形/选项名/数据键名」的区间内容抹空

    不用正则扫整行：rustdoc 链接 [`X`] 与命令引用 `rzc run` 相邻时，正则
    会把「上一个区间的结束反引号」当成「下一个区间的开始反引号」，跨区间
    误抹真实内容（本分支实测踩到）。配对剖开没有这个歧义。
    """
    parts = text.split("`")
    for i in range(1, len(parts), 2):
        seg = parts[i]
        if (CODEPUNCT.search(seg) or CMDSTART.match(seg) or OPTSTART.match(seg)
                or seg.strip() in keys):
            parts[i] = " " * len(seg)
    return "`".join(parts)


def scan_single_names(path: Path) -> list:
    """代码区的单字母英文名：声明/导入位、寿命位、类型参数位、方法位、绑定名位

    只量代码区：测试里内嵌的英文 Rust 样例（字符串字面量）属数据区，不在此射程。
    """
    text = cza.strip_literals(path.read_text(encoding="utf-8"))
    out = []
    for lineno, line in enumerate(text.split("\n"), 1):
        seg = re.sub(r"//.*", "", line)
        for pat in (SINGLE_DECL, LIFETIME_ASCII, GENERIC_PARAM, METHOD_ASCII,
                    BINDING_ASCII):
            if pat.search(seg):
                out.append((lineno, seg.strip()[:70]))
                break
    return out


def scan_comments(path: Path, forms: list, allow: set, keys: frozenset) -> dict:
    """注释区未中文化的英文：{词: [(行号, 该词所在片段)]}"""
    text = path.read_text(encoding="utf-8")
    hits = {}
    for lineno, raw in comment_lines(text):
        body = re.sub(r"^[\s/*]*", "", raw)
        body = PATHISH.sub(" ", body)
        body = strip_spans(body, keys)
        body = KEBAB.sub(" ", body)
        body = strip_phrases(body, allow)
        body = CODE.sub(" ", body)
        body = strip_forms(NUM.sub(" ", body), forms)
        for m in IDENT.finditer(body):
            tok = m.group(0)
            if len(tok) < 2 or set(tok) <= {"_"} or tok.lower() in allow:
                continue
            lo = max(0, m.start() - 24)
            hits.setdefault(tok, []).append((lineno, body[lo:m.end() + 16].strip()))
    return hits


def zh_sources() -> list:
    """门禁射程内的方言源。

    三类都要管：`crates/**/*.zh` 是后端本体；`.zh-demo/**/*.zh` 是对外演示 crate、
    `docs/**/*.zh` 是文档里的可运行样例——它们写的就是「方言能写到什么程度」的
    门面，门面里留英文等于替英文背书。（glob 的通配段不匹配点开头目录，故
    `.zh-demo` 写成字面量段。）
    """
    found = [p for p in REPO.glob("crates/**/*.zh") if ".zh-" not in p.name]
    found += list(REPO.glob(".zh-demo/**/*.zh"))
    found += list(REPO.glob("docs/**/*.zh"))
    return sorted(found)


def tracked_zh() -> set:
    """git 跟踪的 .zh 全集（射程自校验用）；不在 git 仓库里时返回空集，自校验跳过

    用 `-z` 取 NUL 分隔的原始字节：默认输出会把非 ASCII 路径转成 `"\345\210"`
    这种 C 引用形式，中文文件名全部对不上号。
    """
    跑 = subprocess.run(
        ["git", "ls-files", "-z", "--", "*.zh"], cwd=REPO, capture_output=True
    )
    if 跑.returncode != 0:
        return set()
    return {REPO / 项.decode("utf-8") for 项 in 跑.stdout.split(b"\0") if 项}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--report", action="store_true", help="打印逐词清单")
    ap.add_argument("--update", action="store_true", help="用当前计数覆盖基线")
    args = ap.parse_args()

    forms = registered_forms()
    allow = load_allowlist()
    keys = frozenset(data_keys())
    print(f"词形登记表收录 {len(forms)} 个词形，注释区专名豁免表收录 {len(allow)} 词，"
          f"数据键名豁免 {len(keys)} 项")

    # 射程自校验：漏文件比漏词更危险——不在射程内的方言源，英文可以整篇隐身
    # （`.zh-demo` 曾以「门禁只扫 crates/」为由带着成排英文活了很久）
    sources = zh_sources()
    漏 = sorted(str(p.relative_to(REPO)) for p in tracked_zh() - set(sources))
    if 漏:
        print(f"\n🔴 {len(漏)} 个被跟踪的方言源不在门禁射程内：")
        for rel in 漏:
            print(f"   {rel}")
        print("   把对应目录加进 zh_sources()，或删掉这个方言源")
        return 1

    current = {}
    detail = {}
    for p in sources:
        rel = str(p.relative_to(REPO))
        c = scan_file(p, forms)
        current[rel] = sum(c.values())
        detail[rel] = c

    total = sum(current.values())
    comment_hits = {}
    for p in sources:
        rel = str(p.relative_to(REPO))
        c = scan_comments(p, forms, allow, keys)
        if c:
            comment_hits[rel] = c
    comment_total = sum(sum(len(v) for v in c.values()) for c in comment_hits.values())

    if args.report or args.update:
        for rel, c in sorted(detail.items()):
            if not c:
                continue
            print(f"\n{rel}：代码区 {sum(c.values())} 处 / {len(c)} 词")
            for tok, n in sorted(c.items(), key=lambda kv: (-kv[1], kv[0])):
                print(f"   {n:3} × {tok}")

    if args.update:
        BASELINE.write_text(json.dumps(
            {"说明": "各方言源代码区残余英文标识符计数（棘轮：只许降不许升）",
             "files": {k: v for k, v in sorted(current.items()) if v}},
            ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"✅ 代码区基线已更新：共 {total} 处")
        if comment_hits:
            print(f"⚠️ 注释区仍有英文 {comment_total} 处（不写基线，零容忍）：")
            for rel, c in sorted(comment_hits.items()):
                for tok, places in sorted(c.items(), key=lambda kv: -len(kv[1])):
                    print(f"   {rel}: {len(places)} × {tok}  如 L{places[0][0]} {places[0][1][:60]}")
        return 0

    if not BASELINE.exists():
        print("❌ 缺基线 tools/zh-ascii-baseline.json（先跑 --update）")
        return 1
    base = json.loads(BASELINE.read_text(encoding="utf-8"))["files"]
    bad = 0
    for rel, n in sorted(current.items()):
        b = base.get(rel)
        if b is None and n:
            print(f"🔴 {rel}：基线里没有这个方言源，且含 {n} 处英文（新增方言文件必须从零开始）")
            bad += 1
        elif n > (b or 0):
            print(f"🔴 {rel}：英文 {b} → {n}（+{n - b}）——退化，请改用词表词条或补词条")
            bad += 1
        elif n < (b or 0):
            print(f"🟡 {rel}：英文 {b} → {n}（{n - b}）——已改善，记得 --update 收紧基线")
            bad += 1
    if bad:
        print(f"\n❌ {bad} 个文件与基线不符（当前共 {total} 处英文）")
        return 1
    if comment_hits:
        print(f"\n🔴 注释区残余英文 {comment_total} 处（零容忍，已登记专名除外）：")
        for rel, c in sorted(comment_hits.items()):
            for tok, places in sorted(c.items(), key=lambda kv: -len(kv[1])):
                lines = "、".join(str(x[0]) for x in places[:6])
                print(f"   {rel}: {len(places)} × {tok}  （L{lines}）")
        print("   改写为中文术语；确需保留的往 tools/zh-comment-allowlist.json 附理由登记")
        return 1
    single = {}
    for p in sources:
        rel = str(p.relative_to(REPO))
        s = scan_single_names(p)
        if s:
            single[rel] = s
    if single:
        n = sum(len(v) for v in single.values())
        print(f"\n🔴 代码区单字母英文名 {n} 处（len<2 豁免的漏网，台账 F38/F39）：")
        for rel, places in sorted(single.items()):
            lines = "、".join(str(x[0]) for x in places[:6])
            print(f"   {rel}: {len(places)} 处（L{lines}）如 {places[0][1][:56]}")
        print("   声明/导入名、方法名、循环/闭包/局部名一律中文化（外部模块可加导入别名）；"
              "寿命名用 `'甲`、类型参用中文名词（均已实测）；`'_` 占位与字符字面量不在射程")
        return 1
    print(
        f"✅ 全中文棘轮达标：{len(current)} 个方言源，代码区残余英文 {total} 处与基线一致，"
        f"注释区残余英文 0 处"
    )
    return 0


sys.exit(main()) if __name__ == "__main__" else None
