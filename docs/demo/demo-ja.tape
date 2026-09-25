# =============================================================
# rzc 演示录屏（日本語）— vhs 脚本
#
# 用法（详见 docs/demo/README.md）：
#   1. 準備：rzc init rzc-demo-ja --lang ja && cd rzc-demo-ja
#      src/main.ja を docs/demo/samples/main.ja で上書き
#   2. プロジェクトディレクトリで実行：vhs <repo>/docs/demo/demo-ja.tape
#   3. 出力 demo-ja.gif はカレントディレクトリ（Output で変更可）
#
# 内容脚本（約 15 秒）：
#   ① 母語ソース → ② 母語の教学エラー（E0384 + 💡）→ ③ eject で標準 Rust へ
# =============================================================

Output demo-ja.gif

Set Shell bash
Set FontSize 18
Set Width 1080
Set Height 620
Set Padding 24
Set TypingSpeed 55ms
Set Theme "Dracula"

# ① 母語ソース（4 行の日本語 Rust）
Type "cat src/main.ja"
Enter
Sleep 2s

# ② 母語の教学エラー：エラーコード + 💡 学習ヒント + 修正提案
Type "rzc check src/main.ja"
Enter
Sleep 3.5s

# ③ ワンステップで卒業：標準 Rust をエクスポート
Type "rzc eject src/main.ja && cat src/main.rs"
Enter
Sleep 3s
