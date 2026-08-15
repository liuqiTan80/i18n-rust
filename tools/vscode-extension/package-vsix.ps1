# =============================================================
# i18n-rust VS Code 扩展离线打包脚本（Windows / PowerShell）
#
# 功能：
#   1. 检查 Node.js 与 npm 是否已安装
#   2. 安装依赖（node_modules 不存在时执行 npm install）
#   3. 编译 TypeScript（npm run compile）
#   4. 使用 vsce 打包为 .vsix（未安装时自动 npm install -g @vscode/vsce）
#   5. 输出 .vsix 文件路径
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File .\package-vsix.ps1
#   （或直接：.\package-vsix.ps1）
#
# 产物：
#   release\i18n-rust-<版本>.vsix
# =============================================================
$ErrorActionPreference = 'Stop'

# 切换到扩展目录（脚本所在位置）
Set-Location (Split-Path -Parent $MyInvocation.MyCommand.Definition)

# ---------- 1. 检查 Node.js 与 npm ----------
Write-Host "🔍 第 1 步：检查 Node.js 与 npm..."
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Host "❌ 未检测到 Node.js。请先安装 Node.js 18 或更高版本（https://nodejs.org/）" -ForegroundColor Red
    exit 1
}
if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
    Write-Host "❌ 未检测到 npm。请先安装 npm（通常随 Node.js 一同安装）" -ForegroundColor Red
    exit 1
}
Write-Host "   ✅ Node.js $((node --version))，npm $((npm --version))"

# ---------- 2. 安装依赖 ----------
Write-Host "📦 第 2 步：安装依赖..."
if (Test-Path 'node_modules') {
    Write-Host "   ✅ node_modules 已存在，跳过 npm install"
} else {
    npm install --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

# ---------- 3. 编译 TypeScript ----------
Write-Host "🛠️ 第 3 步：编译 TypeScript..."
npm run compile
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# ---------- 4. 打包 .vsix ----------
Write-Host "📦 第 4 步：打包 .vsix..."
New-Item -ItemType Directory -Path 'release' -Force | Out-Null
$version = node -p "require('./package.json').version"

# 优先使用本地 vsce（node_modules/.bin），否则使用全局 vsce，否则自动安装
if (Test-Path 'node_modules/.bin/vsce.cmd') {
    $VSCE = 'node_modules/.bin/vsce.cmd'
} elseif (Get-Command vsce -ErrorAction SilentlyContinue) {
    $VSCE = 'vsce'
} else {
    Write-Host "   ⚠️ 未检测到 vsce，正在通过 npm 全局安装（npm install -g @vscode/vsce）..."
    npm install -g @vscode/vsce
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $VSCE = 'vsce'
}

& $VSCE package --out release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# ---------- 5. 输出结果 ----------
Write-Host ""
Write-Host "✅ 打包完成：$((Get-Location).Path)\release\i18n-rust-$version.vsix"
Write-Host "💡 安装方式：打开 VS Code → 扩展侧边栏 → “...” 菜单 → “从 VSIX 安装...” → 选择该文件"
