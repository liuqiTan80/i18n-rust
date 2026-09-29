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

# 每跑完一遍即刻收割当前 new/estimates.json，与聚合文件逐项取最小值
# $1=聚合文件 $2=组名前缀（逗号分隔）
harvest_run() {
    "${PYTHON}" - "$1" "$2" <<'PYEOF'
import json, pathlib, sys
agg_file, prefixes = pathlib.Path(sys.argv[1]), tuple(sys.argv[2].split(","))
agg = {}
if agg_file.exists():
    agg = json.loads(agg_file.read_text(encoding="utf-8"))
found = False
for p in pathlib.Path("target/criterion").rglob("new/estimates.json"):
    parts = p.parts
    # target/criterion/<组>/<函数>/<new>/estimates.json
    group, func = parts[2], parts[3]
    if not group.startswith(prefixes):
        continue
    found = True
    value = json.loads(p.read_text(encoding="utf-8"))["mean"]["point_estimate"]
    key = f"{group}/{func}"
    if key not in agg or value < agg[key]:
        agg[key] = value
if not found:
    print(f"❌ 未找到基准结果（组前缀 {prefixes}），请确认 cargo bench 运行成功")
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
        run_bench "$1" "$2" "$i"
        harvest_run "$4" "$3"
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
agg_file, baseline_file, label = map(pathlib.Path, sys.argv[1:4])
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

compare_baseline() { # $1=基线文件 $2=聚合文件
    "${PYTHON}" - "$1" "$2" "$THRESHOLD_PCT" <<'PYEOF'
import json, os, pathlib, sys
baseline_file, agg_file = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
threshold = float(sys.argv[3])
baseline = json.loads(baseline_file.read_text(encoding="utf-8"))["results"]
results = json.loads(agg_file.read_text(encoding="utf-8"))
if not results:
    print("❌ 聚合结果为空，请确认 cargo bench 运行成功")
    sys.exit(1)

failures = []
print(f"基准对比（ns，越小越好；基线 {baseline_file}）：")
for key in sorted(set(baseline) | set(results)):
    base = baseline.get(key)
    new = results.get(key)
    if base is None:
        print(f"   {key}: 新增基准项 {new:.1f}（基线无此项，仅提示）")
        continue
    if new is None:
        print(f"   {key}: 基线 {base:.1f}（本次无结果，仅提示）")
        continue
    pct = (new - base) / base * 100
    mark = "✅" if pct <= threshold else "❌"
    print(f"   {key}: 基线 {base:.1f} → 本次 {new:.1f}（{pct:+.1f}%）{mark}")
    if pct > threshold:
        failures.append((key, base, new, pct))

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
    if [ "$MODE" = "update" ]; then
        write_baseline "$agg_file" "$baseline" "i18n-rust-${key}/${bench_name}"
    elif ! compare_baseline "$baseline" "$agg_file"; then
        FAILED=1
    fi
done

if [ "$MODE" = "update" ]; then
    echo "✅ 基线刷新完成"
elif [ "$FAILED" = "1" ]; then
    exit 1
fi
