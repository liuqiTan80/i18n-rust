#!/bin/bash
# 教程代码块回归校验：从教程 markdown 中提取 ```rust 围栏的方言代码块，
# 逐个转译（rzc eject）→ rustc 编译 → 含主函数则运行。
#
# 说明（2026-08 实测 332 个代码块）：
#   - 无主函数的块是讲解片段，自动跳过；
#   - 教程故意包含"找错游戏/错误示例"（错误宏名、全角标点、panic、
#     私有模块、外部 crate 如 futures 等），这些块本就应当编译失败。
#     因此本脚本定位为本地质量工具（人工查看失败清单判断是否为预期错误），
#     尚未接入 CI 硬门禁——接入前需在教程中为"故意错误"代码块加标记。
#
# 用法：verify_blocks.sh [教程目录] [rzc 可执行文件]
#   - 教程目录默认 tutorials/（脚本所在仓库根）
#   - rzc 默认 target/debug/rzc；可用第二个参数覆盖（如 target/release/rzc）
set -uo pipefail

REPO="$(cd "$(dirname "$0")" && pwd)"
DIR="${1:-$REPO/tutorials}"
RZC="${2:-$REPO/target/debug/rzc}"
[ -x "$RZC" ] || { echo "❌ rzc 不存在: $RZC（先运行 cargo build）" >&2; exit 2; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
PASS=0; FAIL=0; SKIP=0; N=0

# 提取所有教程 md 的 ```rust 围栏代码块（方言代码写在 rust 语言标签内），
# 每块写到独立的临时 .zh 文件（文件名 = md 名 + 块序号）
for md in "$DIR"/*.md; do
    [ -f "$md" ] || continue
    base="$(basename "$md" .md)"
    awk -v work="$WORK" -v base="$base" '
        /^```rust[[:space:]]*$/ { in_block=1; block++; file=work "/" base "_b" block ".zh"; next }
        /^```[[:space:]]*$/      { if (in_block) { in_block=0; close(file) }; next }
        in_block                 { print > file }
    ' "$md"
done

for f in "$WORK"/*.zh; do
    [ -f "$f" ] || { echo '未提取到任何 ```rust 代码块'; exit 2; }
    N=$((N+1))
    name="$(basename "$f" .zh)"
    if ! "$RZC" eject "$f" >"$f.eject.log" 2>&1; then
        echo "❌ $name 转译失败: $(head -c 150 "$f.eject.log" | tr '\n' ' ')"
        FAIL=$((FAIL+1)); continue
    fi
    rs="${f%.zh}.rs"
    if ! grep -q "fn main" "$rs"; then
        # 无主函数的代码块是讲解用片段，不参与编译门禁
        SKIP=$((SKIP+1)); continue
    fi
    # --crate-name 固定 crate 名：文件名含全角冒号（章节标题）时 rustc 无法推导
    if rustc --edition 2021 --crate-name i18n_block -o "${f%.zh}.bin" "$rs" 2>"$f.err"; then
        if "${f%.zh}.bin" > "$f.out" 2>&1; then
            echo "✅ $name 编译+运行成功: $(head -c 60 "$f.out" | tr '\n' ' ')"
        else
            echo "❌ $name 运行失败: $(head -c 120 "$f.out" | tr '\n' ' ')"
            FAIL=$((FAIL+1)); continue
        fi
    else
        echo "❌ $name 编译失败: $(head -c 200 "$f.err" | tr '\n' ' ')"
        FAIL=$((FAIL+1))
    fi
    PASS=$((PASS+1))
done
echo "======== 代码块: $N, 完整程序: $((N-SKIP)), 跳过片段: $SKIP, 通过: $PASS, 失败: $FAIL ========"
[ "$FAIL" -eq 0 ]
