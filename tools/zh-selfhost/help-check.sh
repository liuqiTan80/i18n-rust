#!/usr/bin/env bash
# zh 自举：CLI --help 全树本地化门禁
#
# 背景：make gate 批次实证（guide §3 条目 6）——rzc 全部命令路径的 --help
#   内容文案须由 界面.取文 本地化覆盖、无英文散文泄漏；且 clap 内建 help 子命令
#   （其描述 "Print this message or the help of the given subcommand(s)" 是
#   clap 内建英文、取文覆盖够不到）已被 disable_help_subcommand(true) 整体移除。
#   「勿重新启用」不能只靠文档口径：本脚本递归爬取真实二进制的整棵命令树，
#   两条红线任一触犯即退出码 1，纳入 make gate 链防回归——
#     ① Commands: 清单不得出现内建 help 子命令条目；
#     ② 非白名单行不得出现 ≥4 连续英文词的散文（即 help/about 描述未被本地化）。
#   白名单只放 clap 结构层英文（Usage:/段标题/内建 -h/-V flag 行/页脚），
#   不用粗心的「含 -- 即排」——那会把英文化的 flag 描述整片放过（假绿盲点）。
#
# 用法：
#   ./tools/zh-selfhost/help-check.sh               # 用 PATH 中的 rzc
#   ./tools/zh-selfhost/help-check.sh --rzc <路径>  # 指定二进制（如 target/debug/rzc）
#   ./tools/zh-selfhost/help-check.sh --self-test   # 自检检测器非「永远绿」：
#       正样本须通过、两类负样本（英文散文/help 复活）须被拒；不依赖 rzc
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# --- 检测器：对一段聚合 help 文本跑两条红线（stdin 无关，入参为文件路径）---
# 通过返回 0；任一红线触犯打印归属并返回 1。
scan_help_text() {
    local file="$1" fail=0 leak
    # 红线①：内建 help 子命令复活（disable_help_subcommand 被移除/新组漏配）
    if grep -qE '^  help[[:space:]]' "$file"; then
        echo "❌ help 子命令复活：clap 内建 help 条目重新出现在 Commands: 清单" \
             "（其英文描述无法被 界面.取文 覆盖，见 guide §3 条目 6「勿重新启用」）"
        fail=1
    fi
    # 红线②：英文散文泄漏。先排除含中文行与结构层白名单，再查 ≥4 连续英文词
    # 注意：grep 模式必须全部走 -e；一旦用 -e，位置参数就变成文件操作数而非模式
    # （自检负样本1 实测拿掉 -e 会静默假绿）
    leak=$(grep -vP '[\x{4e00}-\x{9fff}]' "$file" |
           grep -vE -e '^==== ' -e '^Usage:' -e '^(Options|Arguments|Commands|Flags):' \
                  -e '^  (-h, )?--help[[:space:]]+Print help' \
                  -e '^  (-V, )?--version[[:space:]]+Print version' \
                  -e 'For more information' \
           || true)
    leak=$(printf '%s\n' "$leak" | grep -E '[A-Za-z]+( [A-Za-z]+){3,}' || true)
    if [ -n "$leak" ]; then
        echo "❌ 用户面英文散文泄漏（该行的 about/help 未被 界面.取文 覆盖）："
        # sort -u 去重：递归爬取时同一泄漏行会在每个子命令 help 里重复出现
        printf '%s\n' "$leak" | sort -u
        fail=1
    fi
    return "$fail"
}

#   防假绿要点（实测定案，勿回退）：
#   ① locale 钉 UTF-8：grep -P '\x{4e00}' 在 LC_ALL=C 下硬报错空输出→红线②静默
#     死掉永假绿（容器/CI 纯 C 环境实测复现）；脚本自带能力探测，不满足即红中止。
#   ② 界面语言无条件钉 RZ_LANG=zh：CLI 界面语言跟 LANG/LC_ALL/RZ_LANG 走，任何
#     非 zh 界面的合法文本都会被红线②误判「泄漏」（实测 RZ_LANG=en 入口整屏
#     假红）——此为门禁内部前提，不容调用方环境穿透，与 locale 同理强制隔离。
#   ③ --help 运行失败带命令路径归属中止；④ 有 Commands: 段却枚举不出子命令=
#     失灵盲点中止；⑤ 上报覆盖路径数且 root 外须至少爬到 1 个子命令。

# --- 环境钉定（自检与真实模式共用，置于所有 grep -P 调用之前）---
# 依次试当前环境与常见 UTF-8 locale，用中文样本实测 grep -P '\x{}' 真生效才 export；
# 全部不可用即红退出——绝不带着静默失效的红线②继续判绿。
set_utf8_locale() {
    local c
    for c in "${LC_ALL:-}" "${LANG:-}" C.UTF-8 en_US.UTF-8 zh_CN.UTF-8; do
        [ -n "$c" ] || continue
        if printf '你好' | LC_ALL="$c" grep -qP '[\x{4e00}-\x{9fff}]' 2>/dev/null; then
            export LC_ALL="$c"
            return 0
        fi
    done
    echo "❌ 找不到能让 grep -P '\\x{}' 中文探测生效的 UTF-8 locale（当前 LC_ALL=${LC_ALL:-未设} LANG=${LANG:-未设}）"
    echo "   红线②的中文过滤在 C locale 下会静默失效成假绿——拒绝在不可靠环境继续判定。"
    echo "   排障：设 LC_ALL=C.UTF-8（或 en_US.UTF-8/zh_CN.UTF-8）重跑；容器镜像缺 locale 时先装。"
    exit 1
}

# --- 自检模式：证明检测器判对（正样本过、两类负样本拒），不依赖 rzc ---
if [ "${1:-}" = "--self-test" ]; then
    set_utf8_locale
    ST="$(mktemp -d /tmp/zh-help-selftest-XXXXXX)"
    trap 'rm -rf "$ST"' EXIT
    # 正样本：真实形态的中文 help 片段 + 全部白名单结构行
    cat > "$ST/good.txt" <<'GOOD'
==== rzc --help ====
多语言 Rust 教学方言编译器
Usage: rzc [OPTIONS] <COMMAND>
Commands:
  init       创建新项目（生成对应语言的主文件模板）
  mapping    自动生成第三方库映射文件：提取 crate 公开 API
Options:
      --屏蔽教学提醒  关闭教学提示
  -h, --help         Print help
  -V, --version      Print version
==== rzc mapping auto --help ====
Arguments:
  <依赖包名>  
  -h, --help  Print help
GOOD
    if ! scan_help_text "$ST/good.txt" >/dev/null; then
        echo "❌ 自检失败：正样本（应全通过）竟判失败——白名单误杀或断言逻辑有误"
        exit 1
    fi
    echo "✅ 自检·正样本：全中文+结构白名单的 help 被判通过"
    # 负样本1：about 未本地化（英文散文描述行）
    cat > "$ST/leak.txt" <<'LEAK'
==== rzc --help ====
Commands:
  init       创建新项目
  mapping    Auto-generate third-party crate mapping files: extract the public API
LEAK
    if scan_help_text "$ST/leak.txt" >/dev/null; then
        echo "❌ 自检失败：英文散文描述行竟通过——检测器失灵（门禁形同虚设）"
        exit 1
    fi
    echo "✅ 自检·负样本1：未本地化的英文散文描述被正确判失败"
    # 负样本2：内建 help 子命令复活
    cat > "$ST/helpsub.txt" <<'HSUB'
==== rzc mapping --help ====
Commands:
  auto      自动生成第三方库映射文件
  check     校验第三方库映射质量
  help      Print this message or the help of the given subcommand(s)
HSUB
    if scan_help_text "$ST/helpsub.txt" >/dev/null; then
        echo "❌ 自检失败：help 子命令复活竟通过——检测器失灵（门禁形同虚设）"
        exit 1
    fi
    echo "✅ 自检·负样本2：内建 help 子命令复活被正确判失败"
    echo "自检通过：help-check 检测器判对可靠。"
    exit 0
fi

# --- 正常模式：解析 rzc 二进制（沿用 demo-check 惯例）---
set_utf8_locale
# 界面语言无条件钉 zh：红线②的断言对象就是「中文界面输出」，属门禁内部前提，
# 必须像 locale 一样强制隔离——若尊重调用方环境里的 RZ_LANG（如 en），合法
# 英文 UI 文本会被红线②整屏误判「泄漏」假红（实测坐实），门禁将不确定。
export RZ_LANG=zh
RZC="$(command -v rzc || true)"
if [ "${1:-}" = "--rzc" ]; then RZC="${2:?--rzc 需紧跟二进制路径}"; fi
if [ -z "$RZC" ]; then
    echo "❌ 找不到 rzc：PATH 中无 rzc 且未传 --rzc <路径>"
    exit 1
fi
if [ "${RZC:0:1}" != "/" ]; then RZC="$PWD/$RZC"; fi
[ -x "$RZC" ] || { echo "❌ rzc 二进制不可执行：$RZC"; exit 1; }

ALL="$(mktemp -d /tmp/zh-help-check-XXXXXX)/all.txt"
trap 'rm -rf "$(dirname "$ALL")"' EXIT

# 递归爬命令树：root → Commands: 段下缩进 2 格的子命令名 → 逐层 --help。
# 防假绿要点：① --help 运行失败带命令路径归属中止（不吞错）；
# ② 有 Commands: 段却枚举不出任何子命令 = 枚举失灵盲点，中止；
# ③ 最终上报覆盖路径数，root 之外须至少爬到 1 个子命令。
COUNT=0
crawl() {
    local path="$1" depth="$2" h subs s
    if ! h="$("$RZC" $path --help 2>&1)"; then
        echo "❌ rzc $path --help 运行失败（二进制：$RZC）"
        printf '%s\n' "$h" | tail -5
        exit 1
    fi
    COUNT=$((COUNT + 1))
    { echo "==== rzc $path --help ===="; printf '%s\n' "$h"; } >> "$ALL"
    [ "$depth" -ge 3 ] && return
    if printf '%s\n' "$h" | grep -qE '^Commands:'; then
        subs=$(printf '%s\n' "$h" |
               awk '/^Commands:/{f=1;next} f&&/^  [a-z][a-z0-9-]*[[:space:]]/{print $1} f&&/^[^ ]/{f=0}' |
               sort -u)
        if [ -z "$subs" ]; then
            echo "❌ 枚举失灵：rzc $path --help 有 Commands: 段却一个子命令都没爬到（检测盲点）"
            exit 1
        fi
        for s in $subs; do crawl "$path $s" $((depth + 1)); done
    fi
}
crawl "" 0
[ "$COUNT" -gt 1 ] || { echo "❌ 命令树爬取异常：仅覆盖 $COUNT 个路径（顶层无任何子命令？）"; exit 1; }

if scan_help_text "$ALL"; then
    echo "✅ 全树 --help 本地化检查通过（覆盖 $COUNT 条命令路径，无 help 子命令复活、无英文散文泄漏）"
else
    echo "（排障：重跑本脚本并观察爬取输出，泄漏行已带命令归属；快照由 trap 清理不留存）"
    exit 1
fi
