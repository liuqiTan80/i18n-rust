#!/usr/bin/env python3
"""生产代码 panic 面门禁：src/ 下的 .unwrap()/.expect()/panic! 等只允许出现在
#[cfg(test)] 区域或显式白名单中。

背景：转译器/LSP 长期面对畸形输入（用户方言源码、rustc JSON、第三方语言包
TOML），任何生产路径 panic 都会让编辑器语言服务器整体崩溃。2026-09 全量审计
确认 src/ 生产代码已无裸 unwrap/expect（572 处命中全部位于 #[cfg(test)] 模块、
集成测试或 build.rs），本门禁把该结论固化，防止回归。

检测内容（仅 #[cfg(test)] 区域之外的物理行）：
  .unwrap() / .expect(   （不误报 unwrap_or / unwrap_or_else / unwrap_or_default）
  panic! / unreachable! / unimplemented! / todo!

跳过区域：
  - #[cfg(test)] 属性修饰的整个项（mod tests { … } 或单个 fn/use/const，
    按花括号深度配对，支持行/块注释、字节串/裸字符串/字符字面量等情形）；
  - src 下名为 tests.rs 的独立测试文件（经 #[cfg(test)] mod tests; 引入）；
  - crates/*/tests、benches、examples（集成测试与基准天然允许 panic）；
  - build.rs（cargo 受控构建期环境）。

白名单：编译期嵌入资源等「不可能失败、失败即构建损坏」的不变量，
须在 ALLOW 中写明（文件, 行内特征串, 理由），门禁逐条核对仍存在。

用法：python3 tools/check-prod-panics.py
"""

import pathlib
import re
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent

# (相对路径, 行内必须包含的特征串, 白名单理由)
ALLOW = [
    (
        "crates/cli/src/builtin_lang.rs",
        'builtin_file(lang, file).expect(',
        "include_str! 编译期嵌入的内置语言包；缺失=引擎构建损坏，且 expect 带中文说明",
    ),
]

PANIC_RE = re.compile(r"\.(?:unwrap|expect)\s*\(|(?:panic|unreachable|unimplemented|todo)\s*!")

# 粗扫属性行；精确区域由花括号深度配对决定
CFG_TEST_RE = re.compile(r"#\[cfg\s*\(\s*test\s*\)\]")
RAW_STR_RE = re.compile(r'b?r(#*)"')
PLAIN_STR_RE = re.compile(r'b?"')
CHAR_LITERAL_RE = re.compile(r"'(?:\\.|[^'\\\n])'")


def _skip_literal(text: str, i: int):
    """i 位于字符串/字符字面量起点时返回结束偏移，否则返回 None。

    覆盖普通/字节字符串（含转义）、裸字符串 r"…"/r#"…"#、字符字面量；
    'a 形式的生命周期/标签因不匹配字符字面量模式而按普通字符处理。
    """
    n = len(text)
    m = RAW_STR_RE.match(text, i)
    # 词边界：标识符内部的 r（如 bar"…"）不是裸字符串前缀
    if m and i > 0 and (text[i - 1].isalnum() or text[i - 1] == "_"):
        m = None
    if m:
        closer = '"' + "#" * len(m.group(1))
        end = text.find(closer, m.end())
        return n if end == -1 else end + len(closer)
    if (m := PLAIN_STR_RE.match(text, i)):
        j = m.end()  # 跳过整段前缀（'"' 或 'b"'），勿把字节串内容当代码
        while j < n:
            if text[j] == "\\":
                j += 2
                continue
            if text[j] == '"':
                return j + 1
            j += 1
        return n
    m = CHAR_LITERAL_RE.match(text, i)
    if m:
        return m.end()
    return None


def iter_src_files():
    for crate in sorted((REPO_ROOT / "crates").iterdir()):
        src = crate / "src"
        if not src.is_dir():
            continue
        for p in sorted(src.rglob("*.rs")):
            # 独立测试文件（经父模块 `#[cfg(test)] mod tests;` 引入，
            # 全仓唯一：lsp/src/response_map/tests.rs）整体视为测试代码
            if p.name == "tests.rs":
                continue
            yield p


def find_test_region_offsets(text: str):
    """返回所有 #[cfg(test)] 项覆盖的字符区间 [(start, end_exclusive), …]。

    轻量词法扫描：跳过行/块注释与各类字符串字面量（含裸字符串 r#"…"#），
    在正常代码中统计花括号深度。命中属性时记录起点，等待被修饰项的主体
    花括号打开并回到属性所在深度即结束；无主体项（use/const，以 ; 结束）
    在该深度的分号处结束。
    """
    regions = []
    depth = 0
    i = 0
    n = len(text)
    pending = None  # (start, attr_depth)
    while i < n:
        ch = text[i]
        # 行注释
        if ch == "/" and i + 1 < n and text[i + 1] == "/":
            j = text.find("\n", i)
            i = n if j == -1 else j
            continue
        # 块注释（支持嵌套）
        if ch == "/" and i + 1 < n and text[i + 1] == "*":
            nest = 1
            i += 2
            while i < n and nest:
                if text[i] == "/" and i + 1 < n and text[i + 1] == "*":
                    nest += 1
                    i += 2
                elif text[i] == "*" and i + 1 < n and text[i + 1] == "/":
                    nest -= 1
                    i += 2
                else:
                    i += 1
            continue
        # 字符串/字符字面量：整段跳过（其中的括号不计深度）
        literal_end = _skip_literal(text, i)
        if literal_end is not None:
            i = literal_end
            continue
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if pending is not None and depth == pending[1]:
                regions.append((pending[0], i + 1))
                pending = None
        elif ch == ";" and pending is not None and depth == pending[1]:
            regions.append((pending[0], i + 1))
            pending = None
        elif ch == "#" and pending is None and CFG_TEST_RE.match(text, i):
            # 记录属性起点（回退到行首，连同行内前导空白一并跳过）
            line_start = text.rfind("\n", 0, i) + 1
            pending = (line_start, depth)
            i += len("#[cfg(test)]")
            continue
        i += 1
    return regions


def line_offsets(text: str):
    offsets = [0]
    for m in re.finditer("\n", text):
        offsets.append(m.end())
    return offsets


def offset_to_line(offsets, offset):
    import bisect

    return bisect.bisect_right(offsets, offset)


def main():
    violations = []
    for path in iter_src_files():
        rel = str(path.relative_to(REPO_ROOT))
        text = path.read_text(encoding="utf-8")
        offsets = line_offsets(text)
        regions = find_test_region_offsets(text)

        def in_test_region(pos):
            for start, end in regions:
                if start <= pos < end:
                    return True
            return False

        for m in PANIC_RE.finditer(text):
            if in_test_region(m.start()):
                continue
            line_no = offset_to_line(offsets, m.start())
            line = text.splitlines()[line_no - 1].strip()
            # 白名单核对：特征串必须仍在该行
            allowed = any(
                rel == f and marker in line
                for f, marker, _reason in ALLOW
            )
            if allowed:
                continue
            violations.append((rel, line_no, line))

    # 白名单防腐：条目标记的现场必须仍存在，否则删除白名单条目
    missing_allow = []
    for f, marker, reason in ALLOW:
        p = REPO_ROOT / f
        alive = False
        if p.is_file():
            alive = any(marker in ln for ln in p.read_text(encoding="utf-8").splitlines())
        if not alive:
            missing_allow.append((f, marker, reason))

    errors = 0
    if violations:
        errors += 1
        print(f"❌ 发现 {len(violations)} 处生产代码 panic 面（须改用错误传播/Option 处理）：")
        for rel, line_no, line in violations:
            print(f"   {rel}:{line_no}: {line}")
        print("   - 仅测试使用的代码必须放入 #[cfg(test)] 项；")
        print("   - 确属编译期不变量的，在 tools/check-prod-panics.py 的 ALLOW 中登记并写明理由；")
        print("   - 确需 panic 且不属于上述情形的，请先在评审中说明再扩展本门禁。")
    if missing_allow:
        errors += 1
        print("❌ 白名单条目现场已消失，请删除或更新 ALLOW：")
        for f, marker, reason in missing_allow:
            print(f"   {f}（{marker}）——原理由：{reason}")

    if errors:
        sys.exit(1)
    scanned = sum(1 for _ in iter_src_files())
    print(f"✅ 生产 panic 面门禁通过（扫描 {scanned} 个 src/ Rust 文件，白名单 {len(ALLOW)} 条）")


if __name__ == "__main__":
    main()
