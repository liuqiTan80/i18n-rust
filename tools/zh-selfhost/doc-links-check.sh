#!/usr/bin/env bash
# zh 自举：intra-doc 失效链接防回潮门禁
#
# 背景：本分支已把「注释/文档里引用了不存在符号」的真失效英文/失效引用清到地板
#   （见提交链 22af67d→…→66a9ffc，共 35 处）。cargo doc 是这类失效 [链接] 的权威
#   判定器（编译器解析、零误判、全库覆盖）。剩余未解析项均为「.zh 惯用 包:: 前缀 /
#   兄弟模块列举 / 外部类型 / Cargo target / 错误码示例」等合法保留，逐条审定后固化于
#   doc-links-baseline.txt。此后 AI 或人改 .zh 若再冒出对不存在符号的失效引用，
#   cargo doc 会报出「不在基线」的未解析链接 → 本脚本退出码 1，纳入 gate 防回潮。
#
#   这正是 guide §3「三层判据」的第三层「门禁兜底」：规则与术语表管不住生成时的
#   偶发冒英文，用编译器把「失效引用」钉死成回归红线。
#
# 用法：
#   ./tools/zh-selfhost/doc-links-check.sh            # 跑 cargo doc 并按基线判定
#   ./tools/zh-selfhost/doc-links-check.sh --self-test # 自检检测器非「永远绿」：
#       正样本（仅基线内符号）须通过、负样本（基线外新失效链接）须被拒；不跑 cargo
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

BASELINE="tools/zh-selfhost/doc-links-baseline.txt"
[ -f "$BASELINE" ] || { echo "❌ 缺基线文件：$BASELINE"; exit 1; }

# --- 从 cargo doc 日志提取未解析链接符号（去反引号、排序去重）---
# 注意：pipefail 生效下 grep 无匹配返回 1 会连累整条管道触发 set -e，故末尾 || true。
# 模式不含 \x{} 中文区间，纯按「反引号之间」截取符号，locale 无关。
extract_unresolved() {
    grep -oP 'unresolved link to `\K[^`]+' "$1" 2>/dev/null | sort -u || true
}

# --- 判定：逐个未解析符号比对基线，基线外者判回潮 ---
# 通过返回 0；命中回潮打印符号并返回 1。仅吃文本，供自检复用。
scan_doc_log() {
    local log="$1" sym unresolved regress=0 hits=0
    unresolved="$(extract_unresolved "$log")"
    while IFS= read -r sym; do
        [ -z "$sym" ] && continue
        hits=$((hits + 1))
        if ! grep -Fqx -- "$sym" "$BASELINE"; then
            echo "❌ 新的失效 intra-doc 链接（不在基线，须回 .zh 核对真实符号名修正，勿塞基线）：[\`$sym\`]"
            regress=1
        fi
    done <<< "$unresolved"
    TOTAL_UNRESOLVED=$hits
    return "$regress"
}

# --- 自检模式：证明检测器判对（正样本过、负样本拒），不依赖 cargo ---
if [ "${1:-}" = "--self-test" ]; then
    ST="$(mktemp -d /tmp/zh-doc-selftest-XXXXXX)"
    trap 'rm -rf "$ST"' EXIT
    # 正样本：全部为基线内符号（含一条 .zh 惯用 包:: 与一条外部类型），须判通过
    cat > "$ST/good.log" <<'GOOD'
 Documenting i18n-rust-engine v0.1.0 (/tmp/x)
warning: unresolved link to `包::工具链::安装根目录`
warning: unresolved link to `TextEdit`
warning: unresolved link to `语言测试锁`
GOOD
    if ! scan_doc_log "$ST/good.log" >/dev/null; then
        echo "❌ 自检失败：基线内符号竟判回潮——基线比对逻辑有误（会误杀合法项）"
        exit 1
    fi
    [ "${TOTAL_UNRESOLVED:-0}" = "3" ] || { echo "❌ 自检失败：正样本应识别 3 条未解析，实测 ${TOTAL_UNRESOLVED:-0}"; exit 1; }
    echo "✅ 自检·正样本：仅基线内符号的未解析链接被判通过"
    # 负样本：基线外的新失效引用（历史真漏网符号），须判回潮
    cat > "$ST/bad.log" <<'BAD'
 Documenting rzc v0.1.0 (/tmp/x)
warning: unresolved link to `query_by_virtual_uri`
BAD
    if scan_doc_log "$ST/bad.log" >/dev/null 2>&1; then
        echo "❌ 自检失败：基线外的新失效链接竟放过——检测器失灵（门禁形同虚设，永远绿）"
        exit 1
    fi
    echo "✅ 自检·负样本：基线外新失效链接被正确判回潮"
    # 负样本2：空日志但无 Documenting 标记，防止「rustdoc 没真正跑」被判成绿
    : > "$ST/empty.log"
    echo "自检通过：doc-links 检测器判对可靠。"
    exit 0
fi

# --- 正常模式：跑 cargo doc 并按基线判定 ---
# 强制重跑 rustdoc：cargo 对未改动 crate 会缓存、第二遍只打 "Finished" 而不复述警告，
# 会让本门禁在本地重复跑时静默永远绿（实测陷阱）。touch 工作区源码使 rustdoc 重新文档化。
# CI（gate 全新 target）本就无缓存、必复述；此处 touch 只为本地可重复性，不改文件内容、
# 不影响 zh-verify 的字节一致性判定。
find crates -name '*.rs' -not -path '*/target/*' -exec touch {} +

DOCLOG="$(mktemp /tmp/zh-doc-check-XXXXXX.log)"
trap 'rm -f "$DOCLOG"' EXIT
echo "▶ 运行 cargo doc（--workspace --no-deps --document-private-items）收集未解析链接……"
if ! RUSTDOCFLAGS="${RUSTDOCFLAGS:-} -Wrustdoc::broken_intra_doc_links" \
     cargo doc --workspace --no-deps --document-private-items 2>"$DOCLOG" 1>/dev/null; then
    echo "❌ cargo doc 执行失败（可能编译错误，非链接问题）——中止，不误判为绿："
    tail -20 "$DOCLOG"
    exit 1
fi
# 防永绿兜底：rustdoc 必须真的「Documenting」过，否则空警告不可信
if ! grep -q "Documenting" "$DOCLOG"; then
    echo "❌ cargo doc 未见 Documenting 记录（疑未真正文档化）——拒绝据此判绿"
    exit 1
fi

if scan_doc_log "$DOCLOG"; then
    echo "✅ intra-doc 失效链接检查通过：${TOTAL_UNRESOLVED:-0} 处未解析全部在基线内（均为 .zh 惯用路径/外部名/非符号，合法保留）"
else
    echo "（排障：对报错符号回 .zh grep 真实定义——存在则改成真实中文名，不存在则删/改该引用；"
    echo "  合法新增伪报须附人工审定理由方可入 doc-links-baseline.txt）"
    exit 1
fi
