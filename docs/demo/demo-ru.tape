# =============================================================
# rzc 演示录屏（Русский）— vhs 脚本
#
# 用法（详见 docs/demo/README.md）：
#   1. 准备：rzc init rzc-demo-ru --lang ru && cd rzc-demo-ru
#      用 docs/demo/samples/main.ru 覆盖 src/main.ru
#   2. 在项目目录中运行：vhs <仓库>/docs/demo/demo-ru.tape
#   3. 输出的 demo-ru.gif 位于当前目录（可改下方 Output 路径）
#
# 内容脚本（约 15 秒）：
#   ① 母语源码 → ② 母语教学报错（E0384 + 💡）→ ③ eject 导出标准 Rust
# =============================================================

Output demo-ru.gif

Set Shell bash
Set FontSize 18
Set Width 1080
Set Height 620
Set Padding 24
Set TypingSpeed 55ms
Set Theme "Dracula"

# ① 母语源码（4 行俄语 Rust）
Type "cat src/main.ru"
Enter
Sleep 2s

# ② 母语教学报错：错误码 + 💡 教学提示 + 修复建议
Type "rzc check src/main.ru"
Enter
Sleep 3.5s

# ③ 一键毕业：导出标准 Rust，零锁定
Type "rzc eject src/main.ru && cat src/main.rs"
Enter
Sleep 3s
