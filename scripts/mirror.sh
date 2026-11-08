#!/usr/bin/env bash
# GitHub 单向镜像：zh / main / 指定版本标签 → Gitee / GitCode
#
# 设计约束（与分支策略一致）：
#   - GitHub 是唯一写入源；Gitee / GitCode 只读镜像，禁止反向回推。
#   - 只做快进推送：镜像分支被人工改动时直接失败并告警，不静默覆盖。
#   - 标签逐个推送，不用 `--tags`，避免批量改写镜像上的历史标签。
#   - 分支与标签推送后逐项核对 SHA，不一致即失败。
#
# 用法：
#   scripts/mirror.sh                # 同步 zh、main
#   scripts/mirror.sh v0.8.12        # 额外同步该版本标签
#   DRY_RUN=1 scripts/mirror.sh      # 只打印将要执行的 git 命令
#
# 环境变量：
#   SRC             源远程名，默认 github
#   SRC_URL         源远程不存在时用它自动添加（CI 中传带 token 的 HTTPS 地址）
#   TARGETS         目标远程名列表，默认 "gitee gitcode"
#   GITEE_PUSH_URL  Gitee 推送地址（存在时覆盖同名远程的 push URL）
#   GITCODE_PUSH_URL GitCode 推送地址（同上）
#   LOCK            互斥锁文件，默认 $TMPDIR/rzc-mirror.lock

set -euo pipefail

SRC="${SRC:-github}"
TARGETS_STR="${TARGETS:-gitee gitcode}"
TAG="${1:-}"
DRY_RUN="${DRY_RUN:-0}"
LOCK="${LOCK:-${TMPDIR:-/tmp}/rzc-mirror.lock}"

run() {
    if [[ "$DRY_RUN" == "1" ]]; then
        printf '+'
        printf ' %q' "$@"
        printf '\n'
    else
        "$@"
    fi
}

exec 9>"$LOCK"
if ! flock -n 9; then
    echo "已有镜像任务在运行（锁：$LOCK），本次退出" >&2
    exit 1
fi

if ! git remote get-url "$SRC" >/dev/null 2>&1; then
    : "${SRC_URL:?源远程 $SRC 不存在，且未提供 SRC_URL}"
    run git remote add "$SRC" "$SRC_URL"
fi

# 失败重试一次：镜像平台偶发 5xx 时避免整条发布链断掉
fetch_source() {
    run git fetch "$SRC" --prune --tags && return 0
    echo "首次 fetch 失败，3 秒后重试…" >&2
    sleep 3
    run git fetch "$SRC" --prune --tags
}
fetch_source

for dst in $TARGETS_STR; do
    upper=$(printf '%s' "$dst" | tr '[:lower:]' '[:upper:]')
    url_var="${upper}_PUSH_URL"
    url="${!url_var:-}"

    if [[ -n "$url" ]]; then
        if git remote get-url "$dst" >/dev/null 2>&1; then
            run git remote set-url --push "$dst" "$url"
        else
            run git remote add "$dst" "$url"
        fi
    fi
    if ! git remote get-url "$dst" >/dev/null 2>&1; then
        echo "目标远程 $dst 不存在，且未提供 ${upper}_PUSH_URL" >&2
        exit 2
    fi

    echo "== 镜像到 $dst =="
    run git push "$dst" \
        "refs/remotes/$SRC/zh:refs/heads/zh" \
        "refs/remotes/$SRC/main:refs/heads/main"

    if [[ -n "$TAG" ]]; then
        run git push "$dst" "refs/tags/$TAG:refs/tags/$TAG"
    fi

    if [[ "$DRY_RUN" == "1" ]]; then
        continue
    fi

    for br in zh main; do
        src_sha=$(git rev-parse "$SRC/$br")
        dst_sha=$(git ls-remote "$dst" "refs/heads/$br" | cut -f1)
        if [[ "$src_sha" != "$dst_sha" ]]; then
            echo "❌ $dst/$br 校验失败：源 $src_sha，镜像 ${dst_sha:-空}" >&2
            exit 3
        fi
    done

    if [[ -n "$TAG" ]]; then
        src_sha=$(git rev-parse "refs/tags/$TAG")
        dst_sha=$(git ls-remote "$dst" "refs/tags/$TAG" | cut -f1)
        if [[ "$src_sha" != "$dst_sha" ]]; then
            echo "❌ $dst 标签 $TAG 校验失败：源 $src_sha，镜像 ${dst_sha:-空}" >&2
            exit 3
        fi
    fi
    echo "✅ $dst 镜像校验通过"
done

echo "镜像完成：zh / main${TAG:+ / $TAG}"
