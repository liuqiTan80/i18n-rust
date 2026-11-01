#!/usr/bin/env bash
# CI 失败诊断注解：把 cargo test / cargo llvm-cov 失败日志的关键部分转成
# ::error:: 工作流注解。
#
# 背景：GitHub Actions 的 job 日志需要登录才能查看，而工作流注解可经
# check-runs annotations API（匿名可读）获取——单测类步骤判红时整步只剩
# 「exit code 1」（run #241/#242 实证），外部排查无从下手。本脚本与
# e2e-failure-annotations.sh 同构：抓编译错误行、libtest 失败摘要与
# failures 段（含捕获的测试输出 dump），转为注解输出，便于无 GitHub
# 登录时定位到具体失败测试。
#
# 用法：cargo-test-annotations.sh <日志文件> [标签]
set -u

log_file="${1:?用法: $0 <日志文件> [标签]}"
label="${2:-cargo-test}"

if [ ! -f "$log_file" ]; then
  echo "::error::[$label] 日志文件不存在：$log_file"
  exit 0
fi

# CRLF 归一（Windows 日志可能含 \r，会破坏 grep 的行尾锚点）
clean_file="$(mktemp)"
key_file="$(mktemp)"
tr -d '\r' < "$log_file" > "$clean_file"

# 关键段：编译错误行 + libtest 结果摘要（passed; N failed）+ failures 段
# （含 panic 位置与被捕获的测试输出）；无 failures 段（如编译失败）取日志末尾兜底。
# 末尾剔掉空白行：CRLF 归一后 CR 进度残留会变成整片空行，它们白占「每块 12 行」
# 的额度，把真正的内容挤出注解上限。
{
  grep -E '^error(\[[A-Za-z0-9]+\])?:' "$clean_file" | head -n 10 || true
  grep -E '^test result: FAILED' "$clean_file" || true
  if grep -q '^failures:$' "$clean_file"; then
    awk '/^failures:$/ { found = 1 } found { print }' "$clean_file" | head -n 200
  else
    tail -n 80 "$clean_file"
  fi
} | sed '/^[[:space:]]*$/d' > "$key_file"

# 提取不到任何有效内容不是「没有需要报告的失败」，而是明细出口哑了：本步骤是失败
# 详情的唯一对外通道（job 日志要登录才能看），注解空白等于排查者什么都看不到。
# 旧版只防了「日志文件不存在」——0 字节日志（主步骤超时被杀 / 磁盘满）和 4 KB 的
# CR 进度残留都一路走到 awk、产出 0 条注解后 exit 0（实测两者皆「注解 0 条 /
# 输出 0 字节 / EXIT=0」，而同情形下文件不存在反而有明确注解）。
key_lines="$(wc -l < "$key_file")"
if [ "$key_lines" -eq 0 ]; then
  echo "::error::[$label] 失败明细为空：$log_file（$(wc -c < "$log_file") 字节，去 CRLF 与空白行后 0 行有效内容）"
  echo "::error::[$label] 注解缺失不等于没有失败——请回到该步骤的原始日志查看，别把「无注解」当成绿灯"
  rm -f "$clean_file" "$key_file"
  exit 0
fi

# 分块输出注解：每块至多 12 行且长度适中（注解有数量/长度上限）；
# 换行转义为 %0A、% 转义为 %25，整块保持单行
awk -v os="$label" -v total="$key_lines" '
  function esc(s) { gsub(/%/, "%25", s); gsub(/\r/, "", s); return s }
  {
    line = esc(substr($0, 1, 1200))
    buf = (buf == "" ? line : buf "%0A" line)
    n = n + 1
    len = len + length(line)
    if (n >= 12 || len >= 10000) {
      blocks = blocks + 1
      printf "::error::[%s] 单测失败关键日志（%d）：%%0A%s\n", os, blocks, buf
      buf = ""; n = 0; len = 0
      if (blocks >= 8) exit
    }
  }
  END {
    if ((buf != "") && (blocks < 8)) {
      blocks = blocks + 1
      printf "::error::[%s] 单测失败关键日志（%d）：%%0A%s\n", os, blocks, buf
    }
    # 触到上限时必须说清还剩多少：否则 8 块会被读成「失败明细就这些」。
    # 实测（605 行日志、100 个失败测试的 failures 段 → 提取 121 行有效内容）：
    # 旧版出满 8 块共 96 行就收尾，剩下 25 行一声不响地丢了，末块停在「测试17」，
    # 读注解的人会把「十几个失败」当成全部失败明细。
    if (NR < total) {
      printf "::error::[%s] 注解已到 %d 块上限，还有 %d 行关键日志未输出（共 %d 行）——完整明细见该步骤原始日志\n", os, blocks, total - NR, total
    }
  }
' "$key_file"

rm -f "$clean_file" "$key_file"
exit 0
