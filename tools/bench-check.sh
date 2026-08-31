#!/usr/bin/env bash
# =============================================================
# 基准回归对比：运行转译管线基准并与入库基线对比，超过阈值时退出非零。
#
# 用法：
#   tools/bench-check.sh              # 运行基准并对比基线（CI 用）
#   tools/bench-check.sh --update     # 运行基准并刷新基线文件（开发者提交前用）
#
# 环境变量：
#   BENCH_REGRESSION_PCT   回归判定阈值（百分比，默认 30；仅对比模式生效）
#   BENCH_EXTRA_ARGS       透传给 cargo bench 的额外参数（如 -- --quick 加速调试）
#
# 基线文件：crates/engine/benches/baseline.json（随仓库提交）
# 每次运行后 criterion 在 target/criterion/<组>/<函数>/new/estimates.json
# 输出本次结果（mean.point_estimate，纳秒），脚本归一化后与基线逐项对比。
# =============================================================
set -euo pipefail

cd "$(dirname "$0")/.."
BENCH="${BENCH:-cargo}"
THRESHOLD_PCT="${BENCH_REGRESSION_PCT:-30}"
BASELINE_FILE="crates/engine/benches/baseline.json"
PYTHON="${PYTHON:-python3}"

if [ "${1:-}" = "--update" ]; then
    echo "▶ 运行基准并刷新基线..."
    ${BENCH} bench -p i18n-rust-engine --bench transpile ${BENCH_EXTRA_ARGS:-}
    "${PYTHON}" - "$BASELINE_FILE" <<'PYEOF'
import json, pathlib, sys, time
baseline_file = sys.argv[1]
results = {}
for p in pathlib.Path("target/criterion").rglob("new/estimates.json"):
    parts = p.parts
    # target/criterion/<组>/<函数>/<new>/estimates.json
    group = parts[2]
    func = parts[3]
    data = json.loads(p.read_text(encoding="utf-8"))
    results[f"{group}/{func}"] = data["mean"]["point_estimate"]
if not results:
    print("❌ 未找到基准结果，请确认 cargo bench 运行成功")
    sys.exit(1)
baseline = {
    "benchmark": "i18n-rust-engine/transpile",
    "created": time.strftime("%Y-%m-%d"),
    "unit": "ns（mean point_estimate）",
    "threshold_pct": 30,
    "results": dict(sorted(results.items())),
}
pathlib.Path(baseline_file).write_text(
    json.dumps(baseline, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
)
print(f"✅ 基线已写入 {baseline_file}（{len(results)} 项）")
for k, v in sorted(results.items()):
    print(f"   {k}: {v:.1f} ns")
PYEOF
    exit 0
fi

if [ ! -f "$BASELINE_FILE" ]; then
    echo "❌ 未找到基线文件 $BASELINE_FILE，请先运行 tools/bench-check.sh --update"
    exit 1
fi

echo "▶ 运行基准（对比模式，回归阈值 ${THRESHOLD_PCT}%）..."
${BENCH} bench -p i18n-rust-engine --bench transpile ${BENCH_EXTRA_ARGS:-}

"${PYTHON}" - "$BASELINE_FILE" "$THRESHOLD_PCT" <<'PYEOF'
import json, pathlib, sys
baseline_file, threshold = sys.argv[1], float(sys.argv[2])
baseline = json.loads(pathlib.Path(baseline_file).read_text(encoding="utf-8"))["results"]
results = {}
for p in pathlib.Path("target/criterion").rglob("new/estimates.json"):
    parts = p.parts
    group, func = parts[2], parts[3]
    data = json.loads(p.read_text(encoding="utf-8"))
    results[f"{group}/{func}"] = data["mean"]["point_estimate"]
if not results:
    print("❌ 未找到基准结果，请确认 cargo bench 运行成功")
    sys.exit(1)

failures = []
print("基准对比（ns，越小越好）：")
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
    sys.exit(1)
print(f"\n✅ 全部基准项在回归阈值 {threshold:.0f}% 内")
PYEOF
