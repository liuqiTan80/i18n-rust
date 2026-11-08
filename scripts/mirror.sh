#!/usr/bin/env bash
# GitHub 单向镜像：zh / main / 指定版本标签 → Gitee / GitCode
#
# 设计约束（与分支策略一致）：
#   - GitHub 是唯一写入源；Gitee / GitCode 只读镜像，禁止反向回推。
#   - 只做快进推送：镜像分支被人工改动时直接失败并告警，不静默覆盖。
#   - 标签逐个推送，不用 `--tags`，避免批量改写镜像上的历史标签；
#     镜像已存在同名标签时跳过（一致视为已同步，不一致只告警、不覆盖）。
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
    # 不带 --tags：--tags 会用远端强制更新本地全部同名标签，本地存在与远端
    # 不同的历史标签（如 v0.5.8）时整次 fetch 会失败并中断镜像。分支照常
    # 快进，新标签仍会被 git 自动跟随（auto-follow）取到；需要的版本标签在
    # 下方按名单独取、单独推。
    run git fetch "$SRC" --prune && return 0
    echo "首次 fetch 失败，3 秒后重试…" >&2
    sleep 3
    run git fetch "$SRC" --prune
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
        tag_pushed=0
        src_tag=$(git rev-parse -q --verify "refs/tags/$TAG" 2>/dev/null || true)
        if [[ -z "$src_tag" ]]; then
            # 本地没有该标签：按名单独取（不带 --tags，避免历史标签冲突）
            run git fetch "$SRC" "refs/tags/$TAG:refs/tags/$TAG" || true
            src_tag=$(git rev-parse -q --verify "refs/tags/$TAG" 2>/dev/null || true)
        fi
        if [[ -z "$src_tag" ]]; then
            echo "⚠️ 本地与 $SRC 均无标签 $TAG，跳过标签同步" >&2
        else
            dst_tag=$(git ls-remote "$dst" "refs/tags/$TAG" | cut -f1)
            if [[ -n "$dst_tag" ]]; then
                if [[ "$dst_tag" == "$src_tag" ]]; then
                    echo "标签 $TAG 已存在且一致，跳过推送"
                else
                    echo "⚠️ 标签 $TAG 在 $dst 已存在但指向不同提交（源 $src_tag，镜像 $dst_tag），按策略跳过、不覆盖" >&2
                fi
            else
                run git push "$dst" "refs/tags/$TAG:refs/tags/$TAG"
                tag_pushed=1
            fi
        fi
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

    if [[ -n "$TAG" && "${tag_pushed:-0}" == "1" ]]; then
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
