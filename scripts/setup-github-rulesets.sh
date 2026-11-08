#!/usr/bin/env bash
# 为 GitHub 仓库应用分支/标签保护规则（需要已登录且具备 admin 权限的 gh CLI）
#
# 用法：
#   scripts/setup-github-rulesets.sh [owner/repo]
# 默认仓库：liuqiTan80/i18n-rust
#
# 说明：
#   - main：禁止删除/强推、线性历史、必须走 PR（含 CODEOWNERS 审批）
#   - v* 标签：禁止删除/移动
#   - 状态检查上下文（gate 等）由现有 CI 工作流提供，若名称不同请在此脚本中追加
set -euo pipefail

REPO="${1:-liuqiTan80/i18n-rust}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v gh >/dev/null 2>&1; then
    echo "缺少 gh CLI：https://cli.github.com/；或改用 GitHub 网页 Settings → Rules 导入 JSON" >&2
    exit 1
fi

apply_ruleset() {
    local file="$1"
    local name
    name=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "$file")
    if gh api "repos/$REPO/rulesets" --jq '.[].name' | grep -qx "$name"; then
        echo "已存在规则集：$name（跳过；如需更新请先删除或改用 PUT）"
    else
        gh api -X POST "repos/$REPO/rulesets" --input "$file" >/dev/null
        echo "已创建规则集：$name"
    fi
}

apply_ruleset "$ROOT/.github/rulesets/protect-main-release.json"
apply_ruleset "$ROOT/.github/rulesets/protect-release-tags.json"

echo "完成。请再在 GitHub Settings → Environments 新建 release 环境并配置审批人。"
