#!/usr/bin/env bash
# =============================================================
# 基准回归对比：运行基准集并与入库基线对比，超过阈值时退出非零。
#
# 基准集（TARGETS 定义，四段以 | 分隔：包名|bench 目标|基线文件|组名前缀）：
#   engine  i18n-rust-engine / transpile   转译管线（组前缀 transpile / teaching_checks）
#   lsp     i18n-rust-lsp    / hot_paths   LSP 热路径（组前缀 lsp_update_document / lsp_reverse_transpile）
#
# 用法：
#   tools/bench-check.sh                  # 运行全部基准并对比基线（CI 用）
#   tools/bench-check.sh --update         # 运行全部基准并刷新基线文件（开发者提交前用）
#   tools/bench-check.sh --only lsp       # 仅运行指定基准集（engine | lsp）
#   tools/bench-check.sh --update --only lsp
#
# 环境变量：
#   BENCH_REGRESSION_PCT   回归判定阈值（百分比，默认 30；仅对比模式生效）
#   BENCH_RUNS             每套基准重复运行次数（默认 1；CI 取 3）。多次结果
#                          逐项取【最小均值】聚合：共享 runner 的邻居负载只会
#                          把耗时放大、不可能压低，最小值最接近无干扰性能，
#                          从而在不放松阈值的前提下排除偶发噪声（CI 配合
#                          BENCH_REGRESSION_PCT=40 使用，取代旧的单次 100%）
#   BENCH_EXTRA_ARGS       透传给 cargo bench 的额外参数（如 -- --quick 加速调试）
#
# 每次运行后 criterion 在 target/criterion/<组>/<函数>/new/estimates.json
# 输出本次结果（mean.point_estimate，纳秒）。多个基准集共用同一
# target/criterion 目录，脚本按各组名前缀过滤（避免互相串读）。BENCH_RUNS>1
# 时每跑完一遍即刻收割到聚合文件取最小值，避免下一遍覆盖 new/ 结果。
#
# 收割只认【本次真的跑出来的】结果：以每遍 cargo bench 的启动时刻为下限按
# 文件 mtime 过滤。criterion 不清理已删/已改名基准的目录，不过滤就会把历史
# 数值当本次结果收进来（详见 harvest_run 注释）。配套地，基线里本次没有结果
# 的项在全量运行时判失败——那意味着基准被改名或删掉了，门禁对它已经失明。
#
# 同一方向的另一个洞：组名前缀清单是人维护的。新写一个 benchmark_group 而忘了
# 往 TARGETS 第四段登记，那个组的数值既进不了回归对比、也永远收不进 --update
# 写出的基线，而且全程一声不吭（实测：往 target/criterion 塞一个 mtime 新鲜的
# zz_probe_group，旧 harvest_run 直接丢弃它并 EXIT=0）。现在陌生组响亮失败。
# =============================================================
set -euo pipefail

cd "$(dirname "$0")/.."
BENCH="${BENCH:-cargo}"
THRESHOLD_PCT="${BENCH_REGRESSION_PCT:-30}"
RUNS="${BENCH_RUNS:-1}"
PYTHON="${PYTHON:-python3}"

case "$RUNS" in
    ''|*[!0-9]*)
        echo "❌ BENCH_RUNS 必须是正整数（当前：${RUNS}）"
        exit 2
        ;;
esac
if [ "$RUNS" -lt 1 ]; then
    echo "❌ BENCH_RUNS 必须 ≥ 1（当前：${RUNS}）"
    exit 2
fi

# 基准集：包名|bench 目标|基线文件|criterion 组名前缀（逗号分隔）
TARGETS=(
    "i18n-rust-engine|transpile|crates/engine/benches/baseline.json|transpile,teaching_checks"
    "i18n-rust-lsp|hot_paths|crates/lsp/benches/baseline.json|lsp_update_document,lsp_reverse_transpile"
)

# TARGETS 里登记过的全部组名前缀（harvest_run 用它分辨「本遍产出了陌生组」和
# 「产出的是另一个基准集」）。--only 单跑一套时，另一套的目录 mtime 早于本遍
# cutoff，不会被当成陌生组误报。
ALL_PREFIXES=""
for _spec in "${TARGETS[@]}"; do
    IFS='|' read -r _pkg _name _baseline _ps <<<"$_spec"
    ALL_PREFIXES="${ALL_PREFIXES}${_ps},"
done

MODE="check"
ONLY=""
while [ $# -gt 0 ]; do
    case "$1" in
        --update) MODE="update" ;;
        --only)
            shift
            ONLY="${1:-}"
            case "$ONLY" in
                engine | lsp) ;;
                *)
                    echo "❌ --only 仅支持 engine 或 lsp（当前：${ONLY:-空}）"
                    exit 2
                    ;;
            esac
            ;;
        -h | --help)
            echo "用法：tools/bench-check.sh [--update] [--only engine|lsp]"
            echo "  --update          运行基准并刷新基线文件"
            echo "  --only engine|lsp 仅处理指定基准集（默认全部）"
            echo "环境：BENCH_RUNS（重复次数取最小值）、BENCH_REGRESSION_PCT（阈值）"
            exit 0
            ;;
        *)
            echo "❌ 未知参数：$1（-h 查看用法）"
            exit 2
            ;;
    esac
    shift
done

# 对比模式先整体校验基线文件存在，避免第一套跑完才发现第二套缺基线
if [ "$MODE" = "check" ]; then
    for spec in "${TARGETS[@]}"; do
        IFS='|' read -r pkg _bench_name baseline _prefixes <<<"$spec"
        key="${pkg#i18n-rust-}"
        if [ -n "$ONLY" ] && [ "$ONLY" != "$key" ]; then
            continue
        fi
        if [ ! -f "$baseline" ]; then
            echo "❌ 未找到基线文件 $baseline"
            echo "   请先运行：tools/bench-check.sh --update --only $key"
            exit 1
        fi
    done
fi

# 每跑完一遍即刻收割【本遍产出】的 new/estimates.json，与聚合文件逐项取最小值
# $1=聚合文件 $2=组名前缀（逗号分隔）$3=本遍 cargo bench 的启动时刻（epoch 秒）
# 下限按 mtime 卡：target/criterion 里长期留着已从源码删除/改名的基准目录
# （criterion 只写不清），不限定时间就会把它们的历史数值当成本次结果。实测：
# 临时基准 tmp_scale_* 从 benches/transpile.rs 删掉后，下一次门禁仍连续把四项
# 以「新增基准项」列进报告（实测新鲜项 mtime ≤ 4 分钟、僵尸项 ≥ 42 分钟，两个
# 量级差得远）；更糟的是 --update 模式会把这类僵尸数值写进入库基线。
harvest_run() {
    "${PYTHON}" - "$1" "$2" "$3" "${ALL_PREFIXES}" <<'PYEOF'
import json, pathlib, sys
agg_file, prefixes = pathlib.Path(sys.argv[1]), tuple(x for x in sys.argv[2].split(",") if x)
cutoff = float(sys.argv[3])
# TARGETS 第四段登记过的全部组名前缀（不是只有本基准集的）
registered = tuple(x for x in sys.argv[4].split(",") if x)
agg = {}
if agg_file.exists():
    agg = json.loads(agg_file.read_text(encoding="utf-8"))
fresh = 0
foreign = set()
for p in pathlib.Path("target/criterion").rglob("new/estimates.json"):
    parts = p.parts
    # target/criterion/<组>/<函数>/<new>/estimates.json
    group, func = parts[2], parts[3]
    if p.stat().st_mtime < cutoff:
        continue  # 历史遗留目录：本遍没跑它
    if not group.startswith(prefixes):
        # 本遍真产出了、却不属于本基准集：要么是同脚本里的另一个基准集，
        # 要么是 TARGETS 漏登记的陌生组——后者必须响亮失败，否则新写的基准
        # 永远不进回归对比、也进不了 --update 的基线
        if not group.startswith(registered):
            foreign.add(group)
        continue
    fresh += 1
    value = json.loads(p.read_text(encoding="utf-8"))["mean"]["point_estimate"]
    key = f"{group}/{func}"
    if key not in agg or value < agg[key]:
        agg[key] = value
if foreign:
    print(
        f"\n❌ 本遍产出了 TARGETS 未登记的基准组：{', '.join(sorted(foreign))}\n"
        "   收割只认 TARGETS 第四段列出的组名前缀：未登记的组既进不了回归对比，\n"
        "   也永远收不进 --update 写出的基线——门禁对它完全失明。\n"
        "   请把新组名补进 tools/bench-check.sh 的 TARGETS（确属遗留目录则删掉它）。"
    )
    sys.exit(1)
if not fresh:
    print(f"❌ 本遍未产出基准结果（组前缀 {prefixes}），请确认 cargo bench 运行成功")
    sys.exit(1)
agg_file.write_text(json.dumps(agg, ensure_ascii=False, indent=2), encoding="utf-8")
PYEOF
}

run_bench() { # $1=包名 $2=bench 目标 $3=运行序号（1-based）
    if [ "$RUNS" -gt 1 ]; then
        echo "▶ 运行基准：$1 --bench $2（第 $3/$RUNS 遍）"
    else
        echo "▶ 运行基准：$1 --bench $2"
    fi
    ${BENCH} bench -p "$1" --bench "$2" ${BENCH_EXTRA_ARGS:-}
}

# 跑完 BENCH_RUNS 遍并收割最小值到聚合文件，打印聚合结果
# $1=包名 $2=bench 目标 $3=组名前缀 $4=聚合文件
run_and_aggregate() {
    for i in $(seq 1 "$RUNS"); do
        # 启动时刻作为本遍收割下限（判据依据见 harvest_run 注释）
        cutoff="$("${PYTHON}" -c 'import time; print(f"{time.time():.6f}")')"
        run_bench "$1" "$2" "$i"
        harvest_run "$4" "$3" "$cutoff"
    done
    "${PYTHON}" - "$4" <<'PYEOF'
import json, pathlib, sys
agg = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
for k in sorted(agg):
    print(f"   聚合（{k}）: {agg[k]:.1f} ns")
PYEOF
}

write_baseline() { # $1=聚合文件 $2=基线文件 $3=标签
    "${PYTHON}" - "$1" "$2" "$3" <<'PYEOF'
import json, pathlib, sys, time
# label 是纯文本标签（如 i18n-rust-engine/transpile），不能转 Path——
# 否则 json.dumps 遇到 PosixPath 报 “Object of type PosixPath is not JSON serializable”。
agg_file, baseline_file = map(pathlib.Path, sys.argv[1:3])
label = sys.argv[3]
results = json.loads(agg_file.read_text(encoding="utf-8"))
if not results:
    print("❌ 聚合结果为空，无法刷新基线")
    sys.exit(1)
baseline = {
    "benchmark": label,
    "created": time.strftime("%Y-%m-%d"),
    "unit": "ns（mean point_estimate；BENCH_RUNS 多样本取最小值）",
    "threshold_pct": 30,
    "results": dict(sorted(results.items())),
}
baseline_file.write_text(
    json.dumps(baseline, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
)
print(f"✅ 基线已写入 {baseline_file}（{len(results)} 项）")
for k, v in sorted(results.items()):
    print(f"   {k}: {v:.1f} ns")
PYEOF
}

compare_baseline() { # $1=基线文件 $2=聚合文件 $3=是否容忍基线项缺本次结果（1=容忍）
    "${PYTHON}" - "$1" "$2" "$THRESHOLD_PCT" "$3" <<'PYEOF'
import json, os, pathlib, sys
baseline_file, agg_file = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
threshold = float(sys.argv[3])
allow_missing = sys.argv[4] == "1"
baseline = json.loads(baseline_file.read_text(encoding="utf-8"))["results"]
results = json.loads(agg_file.read_text(encoding="utf-8"))
if not results:
    print("❌ 聚合结果为空，请确认 cargo bench 运行成功")
    sys.exit(1)

failures = []
missing = []
print(f"基准对比（ns，越小越好；基线 {baseline_file}）：")
for key in sorted(set(baseline) | set(results)):
    base = baseline.get(key)
    new = results.get(key)
    if base is None:
        print(f"   {key}: 新增基准项 {new:.1f}（基线无此项，仅提示）")
        continue
    if new is None:
        # 基线里有、本次却没跑出来：基准被改名或删除了。全量运行时这就是
        # 门禁静默失去覆盖（以前只打一行「仅提示」就放行）；只有用
        # BENCH_EXTRA_ARGS 跑子集调试时才是预期情形。
        if allow_missing:
            print(f"   {key}: 基线 {base:.1f}（本次无结果；BENCH_EXTRA_ARGS 跑了子集，仅提示）")
        else:
            print(f"   {key}: 基线 {base:.1f}（本次无结果）❌")
            missing.append(key)
        continue
    pct = (new - base) / base * 100
    mark = "✅" if pct <= threshold else "❌"
    print(f"   {key}: 基线 {base:.1f} → 本次 {new:.1f}（{pct:+.1f}%）{mark}")
    if pct > threshold:
        failures.append((key, base, new, pct))

if missing:
    print(
        f"\n❌ {len(missing)} 项基线基准本次无结果：{', '.join(missing)}\n"
        "   这些项本次没被跑到（基准函数被改名/删除，或 cargo bench 部分失败），"
        "回归门禁对它们已失明；\n"
        "   确认是有意的增删后跑 tools/bench-check.sh --update 刷新基线；"
        "只想跑子集时用 BENCH_EXTRA_ARGS 表明意图。"
    )

if failures:
    print(f"\n❌ {len(failures)} 项超过回归阈值 {threshold:.0f}%：")
    for key, base, new, pct in failures:
        print(f"   {key}: {base:.1f} → {new:.1f} ns（{pct:+.1f}%）")
    print("   若为硬件差异导致的误报，请调整 BENCH_REGRESSION_PCT 或刷新基线；")
    print("   确认为真实回归时，请优化相关代码后重新运行。")
    # CI 环境附加工作流注解：job 日志需登录才能查看，注解经
    # check-runs annotations API 匿名可读（与 e2e 失败注解同一思路），
    # 使维护者在无法登录 GitHub 时也能拿到超阈值明细
    if os.environ.get("GITHUB_ACTIONS") == "true":
        parts = [
            f"{key}: {base:.1f} → {new:.1f} ns（{pct:+.1f}%）"
            for key, base, new, pct in failures[:10]
        ]
        msg = " | ".join(parts)
        if len(failures) > 10:
            msg += f" | … 共 {len(failures)} 项"
        msg = msg.replace("%", "%25")
        print(f"::error title=基准回归超阈值::共 {len(failures)} 项超过 {threshold:.0f}%：{msg}")
    sys.exit(1)
if missing:
    sys.exit(1)
print(f"\n✅ 全部基准项在回归阈值 {threshold:.0f}% 内")
PYEOF
}

AGG_DIR="$(mktemp -d)"
trap 'rm -rf "$AGG_DIR"' EXIT

# 全部基准集跑完再决定退出码：两套基准的失败明细一次性呈现
FAILED=0
for spec in "${TARGETS[@]}"; do
    IFS='|' read -r pkg bench_name baseline prefixes <<<"$spec"
    key="${pkg#i18n-rust-}"
    if [ -n "$ONLY" ] && [ "$ONLY" != "$key" ]; then
        continue
    fi
    agg_file="$AGG_DIR/$key.json"
    run_and_aggregate "$pkg" "$bench_name" "$prefixes" "$agg_file"
    # 基线项缺本次结果只在【全量跑】时判红：BENCH_EXTRA_ARGS 可以透传过滤
    # 参数（如 -- --quick 或只跑某几个基准），那种情形缺项是用户本意
    allow_missing=0
    if [ -n "${BENCH_EXTRA_ARGS:-}" ]; then
        allow_missing=1
    fi
    if [ "$MODE" = "update" ]; then
        if [ "$allow_missing" = "1" ]; then
            echo "⚠️ BENCH_EXTRA_ARGS 过滤了基准：--update 会重写整份基线，本次没跑到的项将从基线消失"
        fi
        write_baseline "$agg_file" "$baseline" "i18n-rust-${key}/${bench_name}"
    elif ! compare_baseline "$baseline" "$agg_file" "$allow_missing"; then
        FAILED=1
    fi
done

if [ "$MODE" = "update" ]; then
    echo "✅ 基线刷新完成"
elif [ "$FAILED" = "1" ]; then
    exit 1
fi
