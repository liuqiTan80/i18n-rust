#!/usr/bin/env bash
# 校验候选发布树 = zh 方言源执行 rzc eject 的产物（只比代码区）
#
# 用法：
#   scripts/eject-verify.sh --rzc <rzc 二进制> --zh <zh 源码目录> --candidate <候选发布树>
#
# 典型场景：
#   本地：scripts/eject-verify.sh --rzc .zh-build/target/debug/rzc \
#            --zh . --candidate ../main-worktree
#   CI  ：在 main 的 PR 上，candidate 为 PR head（含 .rs），zh 为 origin/zh
#
# 比较范围：crates/、Cargo.toml、Cargo.lock。
# 文档、脚本、CI 配置允许在 main 上独立演进，不参与本校验。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RZC=""
ZH=""
CAND=""

用法() {
    echo "用法：scripts/eject-verify.sh --rzc <rzc> --zh <zh源码目录> --candidate <候选发布树>" >&2
    exit 2
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --rzc) RZC="${2:-}"; shift 2 ;;
        --zh) ZH="${2:-}"; shift 2 ;;
        --candidate) CAND="${2:-}"; shift 2 ;;
        -h|--help) 用法 ;;
        *) echo "未知参数：$1" >&2; 用法 ;;
    esac
done

[[ -x "$RZC" ]] || { echo "❌ rzc 不可执行：${RZC:-<空>}" >&2; exit 2; }
[[ -d "$ZH" ]] || { echo "❌ zh 源码目录不存在：${ZH:-<空>}" >&2; exit 2; }
[[ -d "$CAND" ]] || { echo "❌ 候选发布树不存在：${CAND:-<空>}" >&2; exit 2; }
[[ -f "$CAND/crates/engine/src/lib.rs" ]] || {
    echo "❌ 候选树缺少 crates/engine/src/lib.rs：main 只能是 eject 产物，不能直接合入 zh 源码" >&2
    exit 1
}

TMP=$(mktemp -d "${TMPDIR:-/tmp}/eject-verify.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/zh"

echo "== 复制 zh 源码到临时目录 =="
tar -C "$ZH" -cf - \
    --exclude=./.git \
    --exclude=./target \
    --exclude=./.zh-build \
    --exclude=./.zh-tmp \
    --exclude=./.rz \
    --exclude=./release \
    . | tar -C "$TMP/zh" -xf -

cd "$TMP/zh"

echo "== 执行 rzc eject =="
dir_units=()
for dir in crates/engine/src crates/cli/src crates/lsp/src crates/zh-pure/src; do
    [[ -d "$dir" ]] && dir_units+=("$dir")
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
    [[ -f "$file" ]] && file_units+=("$file")
done
if (( ${#dir_units[@]} == 0 && ${#file_units[@]} == 0 )); then
    echo "❌ zh 源码目录里没有 .zh 方言源" >&2
    exit 1
fi
(( ${#dir_units[@]} > 0 )) && "$RZC" eject -- "${dir_units[@]}"
(( ${#file_units[@]} > 0 )) && "$RZC" eject -- "${file_units[@]}"
find . -name '*.rs.bak' -delete

# 产物树里不应再出现方言源（zh-pure 的 tests-zh 夹具除外，它是数据）
python3 - <<'PY'
import os
for dp, _, fns in os.walk("crates"):
    if os.path.normpath(dp).startswith(os.path.normpath("crates/zh-pure/tests-zh")):
        continue
    for f in fns:
        if f.endswith(".zh"):
            os.remove(os.path.join(dp, f))
PY

echo "== 规范化产物 =="
cargo fmt --all

echo "== 与候选树比对 =="
FAIL=0
for path in crates Cargo.toml Cargo.lock; do
    if ! diff -ru --exclude=target "$TMP/zh/$path" "$CAND/$path" > "$TMP/diff.log" 2>&1; then
        echo "❌ $path 与候选树不一致（前 60 行）：" >&2
        sed -n '1,60p' "$TMP/diff.log" >&2
        FAIL=1
    fi
done

if (( FAIL )); then
    cat >&2 <<'提示'
候选 main 不是当前 zh 的 eject 产物。请：
  1. 在 zh 分支完成开发与验证并 push；
  2. 重新生成 eject 分支（scripts/prepare-eject-branch.sh <版本>）；
  3. 重新打开/更新 PR。
不要手工修改 main 上的 .rs，也不要直接把 zh merge 到 main。
提示
    exit 1
fi

echo "✅ 候选树与 zh 的 eject 产物完全一致"
