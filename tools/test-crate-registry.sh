#!/usr/bin/env bash
# 端到端冒烟测试：第三方库共享注册中心（publish / search / install / list / remove）
#
# 用法：
#   RZC=target/debug/rzc bash tools/test-crate-registry.sh
#
# 使用本地路径注册中心（RZ_CRATE_REPO 指向临时 git 仓库），
# 全程不依赖网络；验证 rzc crate 子命令的闭环。

set -euo pipefail

RZC="${RZC:-target/debug/rzc}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ ! -x "$RZC" ]; then
  echo "未找到可执行 rzc：请先 cargo build -p rzc（或指定 RZC=路径）" >&2
  exit 1
fi

WORK="$(mktemp -d)"
REG="$WORK/registry"
LANGDIR="$WORK/lang"
SAMPLE="$WORK/serde.toml"

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

# 1. 初始化本地注册中心（git 仓库骨架）
bash tools/init-registry.sh "$REG" >/dev/null

# 2. 准备一个待发布的映射文件
cat > "$SAMPLE" <<'TOML'
["模块路径"]
"序列化" = "serde"

["标识符"]
"序列化器" = "Serializer"

["解释"]
"序列化器" = "serde 的序列化 trait 实现入口"
TOML

echo "=== publish ==="
RZ_LANG_DIR="$LANGDIR" RZ_CRATE_REPO="$REG" "$RZC" crate publish serde --lang zh --file "$SAMPLE" --author tester

echo "=== registry index.json ==="
cat "$REG/index.json"

echo "=== search ==="
RZ_LANG_DIR="$LANGDIR" RZ_CRATE_REPO="$REG" "$RZC" crate search

echo "=== install ==="
RZ_LANG_DIR="$LANGDIR" RZ_CRATE_REPO="$REG" "$RZC" crate install serde --lang zh

echo "=== installed file ==="
test -f "$LANGDIR/zh/crates/serde.toml" && echo "INSTALL OK" || { echo "INSTALL FAIL"; exit 1; }

echo "=== list ==="
RZ_LANG_DIR="$LANGDIR" RZ_CRATE_REPO="$REG" "$RZC" crate list

echo "=== remove ==="
RZ_LANG_DIR="$LANGDIR" RZ_CRATE_REPO="$REG" "$RZC" crate remove serde --lang zh
test ! -f "$LANGDIR/zh/crates/serde.toml" && echo "REMOVE OK" || { echo "REMOVE FAIL"; exit 1; }

echo "ALL SMOKE TESTS PASSED"
