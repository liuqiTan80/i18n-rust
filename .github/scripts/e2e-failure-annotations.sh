#!/usr/bin/env bash
# CI 失败诊断注解：把 e2e 失败日志的关键部分转成 ::error:: 工作流注解。
#
# 背景：GitHub Actions 的 job 日志需要登录才能查看，而工作流注解可经
# check-runs annotations API（匿名可读）获取——这里把失败日志的关键
# 片段（编译错误、失败测试段与捕获的测试输出 dump）转为注解，便于
# 无 GitHub 登录时定位失败原因。
#
# 用法：e2e-failure-annotations.sh <日志文件> [平台名]
set -u

log_file="${1:?用法: $0 <日志文件> [平台名]}"
os_name="${2:-unknown}"

if [ ! -f "$log_file" ]; then
  echo "::error::e2e 日志文件不存在：$log_file"
  exit 0
fi

# CRLF 归一（Windows 日志可能含 \r，会破坏 grep 的行尾锚点）
clean_file="$(mktemp)"
key_file="$(mktemp)"
tr -d '\r' < "$log_file" > "$clean_file"

# 关键段：编译错误 + libtest 失败摘要段（含捕获的测试输出 dump）；
# 无 failures 段（如编译失败）时取日志末尾兜底
{
  grep -E '^error(\[[A-Za-z0-9]+\])?:' "$clean_file" | head -n 10 || true
  if grep -q '^failures:$' "$clean_file"; then
    awk '/^failures:$/ { found = 1 } found { print }' "$clean_file" | head -n 200
  else
    tail -n 80 "$clean_file"
  fi
} > "$key_file"

# 分块输出注解：每块至多 12 行且长度适中（注解有数量/长度上限）；
# 换行转义为 %0A、% 转义为 %25，整块保持单行
awk -v os="$os_name" '
  function esc(s) { gsub(/%/, "%25", s); gsub(/\r/, "", s); return s }
  {
    line = esc(substr($0, 1, 1200))
    buf = (buf == "" ? line : buf "%0A" line)
    n = n + 1
    len = len + length(line)
    if (n >= 12 || len >= 10000) {
      blocks = blocks + 1
      printf "::error::[%s] e2e 失败关键日志（%d）：%%0A%s\n", os, blocks, buf
      buf = ""; n = 0; len = 0
      if (blocks >= 8) exit
    }
  }
  END {
    if ((buf != "") && (blocks < 8)) {
      blocks = blocks + 1
      printf "::error::[%s] e2e 失败关键日志（%d）：%%0A%s\n", os, blocks, buf
    }
  }
' "$key_file"

rm -f "$clean_file" "$key_file"
exit 0
