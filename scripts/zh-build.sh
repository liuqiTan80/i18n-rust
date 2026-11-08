#!/usr/bin/env bash
# =============================================================
# zh 分支隔离构建：源码树只保留 .zh，.rs 产物生成到 .zh-build/
#
# 背景：cargo 只认得 .rs，而 zh 分支要求「同目录只显示 .zh」。因此本脚本
# 把工作树复制到 .zh-build/（已被 .gitignore 忽略），在副本里用 rzc eject
# 生成全部 .rs，再在副本里跑 cargo。仓库源码目录不会出现任何转译产物。
#
# 用法：
#   scripts/zh-build.sh                          # 只生成产物，保留 .zh-build/src
#   scripts/zh-build.sh test --workspace --locked
#   scripts/zh-build.sh clippy --workspace --all-targets -- -D warnings
#   make zh-build ARGS='test --workspace --locked'
#
# rzc 来源（按顺序）：
#   1. 环境变量 RZC
#   2. target/debug/rzc、target/release/rzc（main 分支构建过即可复用）
#   3. .zh-build/tool/ 下已解包的离线包
#   4. release/rzc-*-linux-*.tar.gz 离线包（解包到 .zh-build/tool）
# 都找不到时请先构建 main 的 rzc：
#   git worktree add .zh-tmp/rzc-src main
#   cargo build --manifest-path .zh-tmp/rzc-src/Cargo.toml -p rzc
#   RZC=.zh-tmp/rzc-src/target/debug/rzc scripts/zh-build.sh test
# =============================================================
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BUILD="$ROOT/.zh-build"
RZC="${RZC:-}"

cd "$ROOT"
rm -rf "$BUILD/src"
mkdir -p "$BUILD/src"

# ---------- 1. 定位 rzc ----------
find_rzc() {
    if [[ -n "$RZC" && -x "$RZC" ]]; then
        printf '%s\n' "$RZC"
        return 0
    fi
    local candidate
    for candidate in "$ROOT/target/debug/rzc" "$ROOT/target/release/rzc"; do
        if [[ -x "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    candidate=$(find "$BUILD/tool" -maxdepth 2 -type f -name rzc 2>/dev/null | head -1 || true)
    if [[ -n "$candidate" ]]; then
        printf '%s\n' "$candidate"
        return 0
    fi
    local pkg
    pkg=$(ls "$ROOT"/release/rzc-*-linux-*.tar.gz 2>/dev/null | tail -1 || true)
    if [[ -n "$pkg" ]]; then
        mkdir -p "$BUILD/tool"
        tar -xzf "$pkg" -C "$BUILD/tool"
        candidate=$(find "$BUILD/tool" -maxdepth 2 -type f -name rzc 2>/dev/null | head -1 || true)
        if [[ -n "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    fi
    return 1
}

if ! RZC=$(find_rzc); then
    cat >&2 <<'提示'
❌ 找不到 rzc，无法生成 .zh 的转译产物。
   任选一种：
   1) 设置 RZC 指向现成二进制：RZC=/path/to/rzc scripts/zh-build.sh …
   2) 从 main 分支构建：
        git worktree add .zh-tmp/rzc-src main
        cargo build --manifest-path .zh-tmp/rzc-src/Cargo.toml -p rzc
        RZC=.zh-tmp/rzc-src/target/debug/rzc scripts/zh-build.sh …
   3) 把 Linux 离线包放到 release/rzc-<版本>-linux-<架构>.tar.gz
提示
    exit 2
fi

# ---------- 2. 复制工作树到隔离目录 ----------
tar -C "$ROOT" -cf - \
    --exclude=./.git \
    --exclude=./target \
    --exclude=./.zh-build \
    --exclude=./.zh-tmp \
    --exclude=./.rz \
    --exclude=./release \
    . | tar -C "$BUILD/src" -xf -

cd "$BUILD/src"

# 构建缓存放在 .zh-build/target：每次重建 src 副本时不丢缓存，
# 也正好落在 check-zh-artifacts.py 的自动查找路径上。
export CARGO_TARGET_DIR="$BUILD/target"

# ---------- 3. 逐编译单元生成 .rs ----------
# src 按目录导出（同 crate 的声明上下文完整）；tests/benches/examples/build.rs
# 必须按单文件导出：目录模式只收集同目录内的声明，会把 src 里已声明为中文的
# API 名（如 输出、分词转译、新建）误译成英文。
# 目录与文件分两次调用，并用 `--` 显式终止选项解析：路径不会被误当成
# rzc 的全局选项（如 --lang-pack / --no-lint），批量目标也不会互相串组。
dir_units=()
for dir in crates/engine/src crates/cli/src crates/lsp/src crates/zh-pure/src; do
    if [[ -d "$dir" ]]; then
        dir_units+=("$dir")
    fi
done

file_units=()
for file in \
    crates/engine/tests/*.zh \
    crates/engine/benches/*.zh \
    crates/engine/examples/*.zh \
    crates/engine/build.zh \
    crates/cli/tests/*.zh \
    crates/lsp/tests/*.zh \
    crates/lsp/benches/*.zh \
    crates/zh-pure/tests/*.zh \
    crates/zh-pure/build.zh; do
    if [[ -f "$file" ]]; then
        file_units+=("$file")
    fi
done

if (( ${#dir_units[@]} == 0 && ${#file_units[@]} == 0 )); then
    echo "❌ 未找到任何 .zh 方言源（当前分支可能不是 zh）" >&2
    exit 2
fi
if (( ${#dir_units[@]} > 0 )); then
    "$RZC" eject -- "${dir_units[@]}"
fi
if (( ${#file_units[@]} > 0 )); then
    "$RZC" eject -- "${file_units[@]}"
fi
find . -name '*.rs.bak' -delete
cargo fmt --all

if [[ $# -eq 0 ]]; then
    echo ""
    echo "✅ 已生成隔离构建树：$BUILD/src"
    echo "   在这里可直接执行 cargo 命令（例如 cd \"$BUILD/src\" && cargo test --workspace --locked）"
    exit 0
fi

# ---------- 4. 在隔离目录执行调用方给的 cargo 命令 ----------
exec cargo "$@"
