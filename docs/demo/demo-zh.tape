# =============================================================
# rzc 演示录屏（中文）— vhs 脚本
#
# 用法（详见 docs/demo/README.md）：
#   1. 准备演示项目：rzc init rzc-demo && cd rzc-demo
#      并覆盖 src/main.zh（可用 docs/demo/samples/main.zh）
#   2. 在项目目录中运行：vhs <仓库>/docs/demo/demo-zh.tape
#   3. 输出的 demo-zh.gif 位于当前目录（可改下方 Output 路径）
#
# 内容脚本（约 15 秒）：
#   ① 母语源码 → ② 母语教学报错（E0384 + 💡）→ ③ eject 导出标准 Rust
# =============================================================

Output demo-zh.gif

Set Shell bash
Set FontSize 18
Set Width 1080
Set Height 620
Set Padding 24
Set TypingSpeed 55ms
Set Theme "Dracula"

# ① 母语源码（4 行中文 Rust）
Type "cat src/main.zh"
Enter
Sleep 2s

# ② 母语教学报错：错误码 + 💡 教学提示 + 修复建议
Type "rzc check src/main.zh"
Enter
Sleep 3.5s

# ③ 一键毕业：导出标准 Rust，零锁定
Type "rzc eject src/main.zh && cat src/main.rs"
Enter
Sleep 3s
