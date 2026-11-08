#!/usr/bin/env bash
# 生成 main 的候选发布树（规范流程第 2 步）
#
# 用法：
#   scripts/prepare-eject-branch.sh <版本>        # 例如 0.9.1
#
# 做什么：
#   1. 在 .zh-build/ 隔离目录对当前 zh 工作树执行 rzc eject；
#   2. 从 main 新建 release/eject-<版本> 工作树；
#   3. 用 eject 产物整体替换该工作树（删除所有 .zh，保留 zh-pure 夹具）；
#   4. cargo fmt + 重生成 ui-keys 索引，提交；
#   5. 打印推送与开 PR 的命令（不自动推送）。
#
# 前置：当前在 zh 分支、工作树干净；main 已 fetch 到最新。
set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
    echo "用法：scripts/prepare-eject-branch.sh <版本，如 0.9.1>" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRANCH="release/eject-$VERSION"
WT="$ROOT/.zh-tmp/eject-$VERSION"
cd "$ROOT"

[[ "$(git branch --show-current)" == "zh" ]] || {
    echo "❌ 请在 zh 分支执行（当前：$(git branch --show-current)）" >&2
    exit 1
}
[[ -z "$(git status --porcelain)" ]] || {
    echo "❌ zh 工作树不干净，先提交或清理" >&2
    git status --short >&2
    exit 1
}
git show-ref --verify --quiet "refs/heads/$BRANCH" && {
    echo "❌ 分支已存在：$BRANCH（先删除或换版本号）" >&2
    exit 1
}

EXPECTED=$(grep -m1 '^version = ' crates/engine/Cargo.toml | cut -d'"' -f2)
[[ "$VERSION" == "$EXPECTED" ]] || {
    echo "❌ 版本不一致：参数 $VERSION，crates/engine/Cargo.toml 为 $EXPECTED" >&2
    echo "   先在 zh 分支把三 crate / 扩展 / CHANGELOG 统一到 $VERSION" >&2
    exit 1
}

echo "== 1/5 在 .zh-build 生成 eject 产物 =="
scripts/zh-build.sh

echo "== 2/5 创建发布分支工作树 =="
mkdir -p "$ROOT/.zh-tmp"
git worktree add -b "$BRANCH" "$WT" main

echo "== 3/5 用 eject 产物整体替换工作树 =="
(
    cd "$WT"
    git rm -r -q .
    git clean -fdx -q
)
tar -C "$ROOT/.zh-build/src" -cf - --exclude=./target . | tar -C "$WT" -xf -

python3 - "$WT" <<'PY'
import os, sys
wt = sys.argv[1]
removed = 0
for dp, _, fns in os.walk(os.path.join(wt, "crates")):
    parts = os.path.normpath(dp).split(os.sep)
    if "tests-zh" in parts:      # zh-pure 的方言夹具是数据，保留
        continue
    for f in fns:
        if f.endswith(".zh"):
            os.remove(os.path.join(dp, f))
            removed += 1
print(f"删除 {removed} 个 .zh（保留 zh-pure/tests-zh 夹具）")
PY

echo "== 4/5 格式化 + 重生成 ui 键索引 =="
(
    cd "$WT"
    cargo fmt --all
    python3 tools/ui-keys-index.py --write >/dev/null
)

echo "== 5/5 提交 =="
(
    cd "$WT"
    git add -A
    # zh 的 .gitignore 把转译产物（.rs）与扩展生成物列入忽略，但 main 必须跟踪：
    # 先普通 add 收新文件，再按这两类路径强制添加。不整仓 -f，避免把
    # __pycache__、target 等未跟踪缓存带进发布树。
    git add -f -- 'crates/**/*.rs' 'crates/zh-pure/build.rs' 'crates/engine/build.rs'
    git add -f -- tools/vscode-extension/syntaxes tools/vscode-extension/package-lock.json
    git commit -m "release: v$VERSION —— zh 方言源 eject 产物"
)

cat <<完成

✅ 已生成：$WT
   分支：$BRANCH
   提交：$(git -C "$WT" log --oneline -1)

下一步：
  git push -u github "$BRANCH"
  # 在 GitHub 上向 main 开 PR；等 eject-verify / test / windows-gates / msrv 全绿，
  # 直接 Squash 合并（不要求评审，禁止 merge commit）。
  # 合并后打 tag：git tag v$VERSION && git push github v$VERSION
  # 最后清理：git worktree remove "$WT" && git branch -d "$BRANCH"
完成
