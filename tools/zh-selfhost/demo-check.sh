#!/usr/bin/env bash
# zh 自举：.zh-demo 演示项目的方言改写往返校验
#
# 背景：.zh-demo 是面向学习者的「用中文方言写真实 Rust」最小示例，
#   主打模块路径替换（使用 标准库::集合::哈希映射 → use std::collections::HashMap）。
#   它不在 regen 覆盖内（用户风格 demo，非编译器源真相），故用本脚本单独佐证
#   rzc 对该演示的改写确实生效：eject 后产物须同时具备
#     ① 预期英文改写结果（关键字/宏/方法/模块路径/第三方透传）
#     ② 用户自定义中文标识符原样存活（不被误劫持成英文）
#   任一条不满足即退出码 1，纳入 make gate 链防止演示随源码演化而失效。
#
# 用法：
#   ./tools/zh-selfhost/demo-check.sh            # 用 PATH 中的 rzc
#   ./tools/zh-selfhost/demo-check.sh --rzc <路径>  # 指定 rzc 二进制（如 target/debug/rzc）
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

RZC="$(command -v rzc || true)"
if [ "${1:-}" = "--rzc" ]; then RZC="${2:?--rzc 需紧跟二进制路径}"; fi
if [ -z "$RZC" ]; then
    echo "❌ 找不到 rzc：PATH 中无 rzc 且未传 --rzc <路径>"
    exit 1
fi

SRC=".zh-demo/src/main.zh"
[ -f "$SRC" ] || { echo "❌ 缺少演示源 $SRC"; exit 1; }

# 隔离到 /tmp 独立项目 eject：rzc 靠 Cargo.toml 定位项目根，直接在 .zh-demo/
# 内 eject 会向上命中宿主根并覆盖其源文件（嵌套夹具坑），且污染 git 跟踪的演示目录。
STAGE="$(mktemp -d /tmp/zh-demo-check-XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT
cp "$SRC" "$STAGE/main.zh"
cat > "$STAGE/Cargo.toml" <<'STUB'
[package]
name = "zh-demo-check"
version = "0.0.0"
edition = "2021"
STUB
(cd "$STAGE" && "$RZC" eject main.zh >/dev/null 2>&1)
OUT="$STAGE/main.rs"
[ -f "$OUT" ] || { echo "❌ eject 未产出 main.rs"; exit 1; }

# ① 预期英文改写结果（缺任一即演示核心能力回退）
MUST_HAVE=(
    "use std::collections::HashMap;"      # 模块路径替换（README 主打能力）
    "use rustc_lexer::{TokenKind, tokenize};"  # 第三方库无母语映射→透传保留
    "fn main()"                            # 关键字 函数/主函数 → fn/main
    "println!"                             # 宏 打印行! → println!
    "HashMap::new()"                       # 新建 → new（std 构造）
    "TokenKind::Ident"                     # 外部枚举变体原样透传
)
# ② 用户自定义中文标识符须原样存活（证明未被词条/别名误劫持）
MUST_KEEP_ZH=(
    "struct 替换结果"
    "struct 源码映射项"
    "fn 替换模块路径带表"
)

fail=0
for s in "${MUST_HAVE[@]}"; do
    if grep -qF "$s" "$OUT"; then
        echo "✅ 改写生效: $s"
    else
        echo "❌ 缺失预期改写: $s"
        fail=1
    fi
done
for s in "${MUST_KEEP_ZH[@]}"; do
    if grep -qF "$s" "$OUT"; then
        echo "✅ 中文标识符存活: $s"
    else
        echo "❌ 中文标识符被劫持/丢失: $s"
        fail=1
    fi
done

if [ "$fail" -ne 0 ]; then
    echo "❌ .zh-demo 往返校验失败：方言改写结果与预期不符（详见上方缺失项）"
    exit 1
fi
echo "全部通过：.zh-demo 方言改写往返校验一致。"
