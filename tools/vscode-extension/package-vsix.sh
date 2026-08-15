#!/usr/bin/env bash
# =============================================================
# i18n-rust VS Code 扩展离线打包脚本（Linux / macOS）
#
# 功能：
#   1. 检查 Node.js 与 npm 是否已安装
#   2. 安装依赖（node_modules 不存在时执行 npm install）
#   3. 编译 TypeScript（npm run compile）
#   4. 使用 vsce 打包为 .vsix（未安装时自动 npm install -g @vscode/vsce）
#   5. 输出 .vsix 文件路径
#
# 用法：
#   ./package-vsix.sh
#
# 产物：
#   release/i18n-rust-<版本>.vsix
# =============================================================
set -euo pipefail

# 切换到扩展目录（脚本所在位置）
cd "$(dirname "$0")"

# ---------- 1. 检查 Node.js 与 npm ----------
echo "🔍 第 1 步：检查 Node.js 与 npm..."
if ! command -v node >/dev/null 2>&1; then
    echo "❌ 未检测到 Node.js。请先安装 Node.js 18 或更高版本（https://nodejs.org/）"
    exit 1
fi
if ! command -v npm >/dev/null 2>&1; then
    echo "❌ 未检测到 npm。请先安装 npm（通常随 Node.js 一同安装）"
    exit 1
fi
echo "   ✅ Node.js $(node --version)，npm $(npm --version)"

# ---------- 2. 安装依赖 ----------
echo "📦 第 2 步：安装依赖..."
if [ -d node_modules ]; then
    echo "   ✅ node_modules 已存在，跳过 npm install"
else
    npm install --no-audit --no-fund
fi

# ---------- 3. 编译 TypeScript ----------
echo "🛠️ 第 3 步：编译 TypeScript..."
npm run compile

# ---------- 4. 打包 .vsix ----------
echo "📦 第 4 步：打包 .vsix..."
mkdir -p release
VERSION=$(node -p "require('./package.json').version")

# 优先使用本地 vsce（node_modules/.bin），否则使用全局 vsce，否则自动安装
if [ -x node_modules/.bin/vsce ]; then
    VSCE="node_modules/.bin/vsce"
elif command -v vsce >/dev/null 2>&1; then
    VSCE="vsce"
else
    echo "   ⚠️ 未检测到 vsce，正在通过 npm 全局安装（npm install -g @vscode/vsce）..."
    npm install -g @vscode/vsce
    VSCE="vsce"
fi
"$VSCE" package --out release \
    --baseContentUrl https://gitcode.com/tan80/zrRust/raw/main/tools/vscode-extension/ \
    --baseImagesUrl https://gitcode.com/tan80/zrRust/raw/main/tools/vscode-extension/

# ---------- 5. 输出结果 ----------
echo ""
echo "✅ 打包完成：$(pwd)/release/i18n-rust-$VERSION.vsix"
echo "💡 安装方式：打开 VS Code → 扩展侧边栏 → “...” 菜单 → “从 VSIX 安装...” → 选择该文件"
