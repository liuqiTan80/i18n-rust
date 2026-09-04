#!/usr/bin/env bash
# 一键初始化「第三方库共享注册中心」空仓库骨架。
#
# 用法：
#   ./tools/init-registry.sh [目标目录，默认 zrRust-crates]
#
# 生成的仓库即一个合规的注册中心：根含 index.json 索引，
# 预创建各语言目录；并 git init（可添加远程后供他人 install / publish）。
# 之后他人用 RZ_CRATE_REPO=<该目录绝对路径> 即可使用 rzc crate 子命令。

set -euo pipefail

REGISTRY_DIR="${1:-zrRust-crates}"
echo "==> 初始化注册中心仓库：$REGISTRY_DIR"

mkdir -p "$REGISTRY_DIR"
cd "$REGISTRY_DIR"

# index.json（空索引）
if [ ! -f index.json ]; then
  cat > index.json <<'JSON'
{
  "registry": "zrRust-crate-mappings",
  "updated": "",
  "mappings": []
}
JSON
  echo "    已生成 index.json"
fi

# 预创建常用语言目录（发布时自动生成文件，这里仅占位）
for lang in zh ru ja ko de es fr pt ar hi; do
  mkdir -p "$lang/crates"
done

# 初始化 git 仓库（便于直接作为注册中心推送）
if [ ! -d .git ]; then
  git init -q
  git add -A
  git commit -q -m "init crate registry skeleton" >/dev/null 2>&1 || true
  echo "    已 git init（可添加远程后供他人 install / publish）"
fi

echo "完成。他人可用："
echo "  RZ_CRATE_REPO=$(pwd) rzc crate install <crate> --lang <语言>"
echo "  RZ_CRATE_REPO=$(pwd) rzc crate publish <crate> --lang <语言> [--file <路径>]"
