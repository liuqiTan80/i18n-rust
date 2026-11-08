#!/usr/bin/env bash
# 为 GitHub 仓库应用分支/标签保护规则（需要已登录且具备 admin 权限的 gh CLI）
#
# 用法：
#   scripts/setup-github-rulesets.sh [owner/repo]
# 默认仓库：liuqiTan80/i18n-rust
#
# 应用内容：
#   - main：禁止删除/强推、线性历史、必须走 PR（不要求评审）、
#           必须通过状态检查 test / windows-gates / msrv / eject-verify /
#           覆盖率 / 基准 / 三平台 LSP e2e、要求分支最新（strict）
#   - zh：禁止删除/强推，必须通过方言源门禁与方言源编译门禁（strict）
#   - v* 标签：禁止删除/移动
#
# 说明：GitHub 是本项目唯一写入源；Gitee / GitCode 为只读镜像，
# 其分支保护只需保证只有镜像账号可写（见 docs/strategy/分支保护与发布流程.md）。
set -euo pipefail

REPO="${1:-liuqiTan80/i18n-rust}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v gh >/dev/null 2>&1; then
    cat >&2 <<'提示'
缺少 gh CLI：https://cli.github.com/
任选一种：
  1) 安装并登录后重跑本脚本：gh auth login && scripts/setup-github-rulesets.sh
  2) 在 GitHub 网页 Settings → Rules → New ruleset → Import 逐个导入：
       .github/rulesets/protect-main-release.json
       .github/rulesets/protect-zh-dev.json
       .github/rulesets/protect-release-tags.json
  3) 用 curl + 有 admin 权限的 token 调用 REST API（POST/PUT /repos/:repo/rulesets）
提示
    exit 1
fi

if ! gh auth status >/dev/null 2>&1; then
    echo "gh 未登录：先执行 gh auth login（需要 repo admin 权限）" >&2
    exit 1
fi

apply_ruleset() {
    local file="$1"
    local name id
    name=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "$file")
    id=$(gh api "repos/$REPO/rulesets" --jq '.[] | "\(.id)\t\(.name)"' \
        | awk -F'\t' -v n="$name" '$2==n {print $1; exit}')
    if [[ -n "$id" ]]; then
        gh api -X PUT "repos/$REPO/rulesets/$id" --input "$file" >/dev/null
        echo "已更新规则集：$name（id=$id）"
    else
        gh api -X POST "repos/$REPO/rulesets" --input "$file" >/dev/null
        echo "已创建规则集：$name"
    fi
}

apply_ruleset "$ROOT/.github/rulesets/protect-main-release.json"
apply_ruleset "$ROOT/.github/rulesets/protect-zh-dev.json"
apply_ruleset "$ROOT/.github/rulesets/protect-release-tags.json"

cat <<'完成'
完成。还需要人工确认：
  1. Settings → General → Pull Requests：只勾选 "Allow squash merging"（或 Rebase），
     关闭 "Allow merge commits"——与 required_linear_history 配套；
  2. Settings → Branches / Rules：确认 main 的 required checks 能匹配到
     CI 里的 test / windows-gates / msrv / eject-verify / 测试覆盖率门禁 /
     基准回归对比 / LSP 端到端测试 linux|windows|macos；
     zh 的 required checks 能匹配到「方言源门禁」与「方言源编译门禁」；
  3. Gitee / GitCode：默认分支设为 zh，main / zh 仅允许镜像账号写入。
完成
