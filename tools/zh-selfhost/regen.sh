#!/usr/bin/env bash
# zh 自举：以仓库内 .zh 为源真相，再生成供 cargo 编译的 .rs 产物
#
# 背景：exp/zh-selfhost 分支推进「用 rzc 的母语方言写 rzc 自己」。
#   .zh 是源真相，.rs 是转译产物；改完 .zh 必须跑本脚本再编译/测试。
# 流程：复制到 /tmp 独立项目 eject（规避嵌套夹具向上命中宿主根覆盖源文件的坑）
#       → rustfmt 规范化 → 拷回原位。
#
# 用法：
#   ./tools/zh-selfhost/regen.sh            # 生成模式：再生全部登记的 .zh
#   ./tools/zh-selfhost/regen.sh --check    # 校验模式：产物漂移则退出码 1（CI 门禁，不写文件）
#
# 引导约束：由 PATH 中的 rzc 执行转译；指向工作区 target/ 时警告不阻断
#   （本地验收环境经软链接使用发布构建属预期；CI 应用独立发布的 rzc）。
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

MODE="write"
[ "${1:-}" = "--check" ] && MODE="check"

# 登记表：递归枚举 crates/ 下全部 .zh 源真相（一一对应生成同目录同名 .rs）。
# 自举范围已从 engine 顶层 .zh 扩展到子目录（诊断/）与 cli/lsp 各 crate，
# 故不再硬编码清单，改为发现（作用域限 crates/，排除 .zh-demo、docs 演示夹具）。
ZH_SOURCES=()
while IFS= read -r _zh; do
    ZH_SOURCES+=("$_zh")
done < <(find crates -name '*.zh' | LC_ALL=C sort)

# 引导编译器来源提示：指向本工作区 target/ 时警告不阻断
#   ——本地验收经软链接使用发布构建（v0.8.3 快照）属预期引导；
#   若工作区重编过 release 则为自举中间态，需知情使用。
RZC="$(command -v rzc || true)"
if [ -z "$RZC" ]; then
    echo "❌ PATH 中找不到 rzc（引导编译器）。请安装已发布版本：cargo install rzc"
    exit 1
fi
RZC_REAL="$(readlink -f "$RZC")"
case "$RZC_REAL" in
    *"$(pwd)"/target/*)
        echo "⚠️ 当前 rzc 指向本工作区构建产物：$RZC_REAL"
        echo "   （发布后未重编过则为纯净引导；重编过则为自举中间态，注意知情）";;
esac
echo "引导编译器: $RZC_REAL（$("$RZC" --version 2>/dev/null | head -1)）"

STAGE="$(mktemp -d /tmp/zh-selfhost-stage-XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT

# 中转项目根标记：rzc 靠 Cargo.toml 定位项目根，eject 不解析其内容
cat > "$STAGE/Cargo.toml" <<'STUB'
[package]
name = "zh-selfhost-stage"
version = "0.0.0"
edition = "2021"
STUB

drift=0
for zh in "${ZH_SOURCES[@]}"; do
    [ -f "$zh" ] || { echo "❌ 缺少源文件 $zh"; exit 1; }
    base="$(basename "$zh" .zh)"
    target_dir="$(dirname "$zh")"
    rs="$target_dir/$base.rs"
    # 所属 crate 根；edition 必须与所属 crate 的 Cargo.toml 一致，否则与 cargo fmt 产生漂移
    crate_dir="$(echo "$zh" | cut -d/ -f1,2)"
    src_root="$crate_dir/src"
    edition="$(grep -m1 '^edition' "$crate_dir/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"
    case "$zh" in
      "$src_root"/*)
        # src 下：拷入整棵 src 树（兄弟 .rs/.zh 齐全），使含 mod 声明的文件
        # （crate 根 lib.rs、中间模块 诊断.rs）eject 后 rustfmt 可递归解析整棵模块树；
        # 目标 .rs 随后被本次 eject 覆盖（旧逻辑仅在 base=lib 时拷兄弟，现统一镜像）
        rel="${zh#"$src_root"/}"        # 相对 src 根：lib.zh / 诊断/模型.zh
        rm -rf "$STAGE/src"
        mkdir -p "$STAGE/src"
        cp -r "$src_root"/. "$STAGE/src/"
        cp "$zh" "$STAGE/src/$rel"
        (cd "$STAGE" && rzc eject "src/$rel" >/dev/null 2>&1)
        stage_rs="$STAGE/src/${rel%.zh}.rs"
        ;;
      *)
        # 非-src（tests/benches/examples/build.rs）：cargo 视作各自独立的 crate，
        # 文件内无 mod 声明，单文件镜像即可（stub Cargo.toml 供 rzc 定位项目根）
        rel="${zh#"$crate_dir"/}"       # 相对 crate 根：tests/fuzz_transpile.zh / build.zh
        rm -rf "$STAGE/ns"
        mkdir -p "$(dirname "$STAGE/ns/$rel")"
        printf '[package]\nname="zh-selfhost-ns"\nversion="0.0.0"\nedition="%s"\n' "$edition" > "$STAGE/ns/Cargo.toml"
        cp "$zh" "$STAGE/ns/$rel"
        (cd "$STAGE/ns" && rzc eject "$rel" >/dev/null 2>&1)
        stage_rs="$STAGE/ns/${rel%.zh}.rs"
        ;;
    esac
    rustfmt --edition "$edition" "$stage_rs"
    if [ "$MODE" = "check" ]; then
        if [ ! -f "$rs" ] || ! cmp -s "$stage_rs" "$rs"; then
            echo "⚠️ 产物漂移: $rs 与 $zh 再生成结果不一致"
            drift=1
        else
            echo "✅ 一致: $rs"
        fi
    else
        cp "$stage_rs" "$rs"
        echo "✅ 已生成: $rs"
    fi
done

# 覆盖面断言：crates 下每个 .rs 都须有同目录同名 .zh。regen 只遍历 .zh 生成产物，
# 若有人塞进一个无 .zh 配对的英文 .rs（绕过自举），--check 根本看不到它——故此处
# 反向校验“全部模块源真相反转”不变式，孤儿 .rs 即退出码 1。
orphan=0
while IFS= read -r _rs; do
    [ -f "${_rs%.rs}.zh" ] || { echo "❌ 孤儿 .rs（无配对 .zh，未纳入源真相）: $_rs"; orphan=1; }
done < <(find crates -name '*.rs' | LC_ALL=C sort)
if [ "$orphan" -ne 0 ]; then
    echo "❌ 自举覆盖面断言失败：存在未由 .zh 生成的 .rs（应翻转为 .zh 源真相或删除）"
    exit 1
fi

if [ "$MODE" = "check" ]; then
    [ "$drift" -eq 0 ] || { echo "❌ zh 自举门禁失败：请运行 make zh-regen 并提交再生成的产物"; exit 1; }
    echo "全部产物与 .zh 源真相一致。"
fi
