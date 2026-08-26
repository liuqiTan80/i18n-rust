# =============================================================
# i18n-rust 完整离线版打包脚本（Windows / PowerShell）
#
# 功能：
#   1. cargo build --release 编译 rzc 与语言服务器
#   2. 下载 VS Code 免安装版（官方，解压即用）
#   3. 组装完整目录：VS Code（便携 data + 预装 i18n-rust 扩展）
#      + rzc + 语言服务器 + 内置工具链 + VC++ 运行库 + 语言包 + 教程
#   4. 打包为完整版 .zip（解压后双击"启动编程环境.cmd"即用）
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File .\release-offline-full.ps1
#
# 产物：
#   release\i18n-rust-<版本>-windows-x86_64-完整版.zip
# =============================================================
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $MyInvocation.MyCommand.Definition)

# ---------- 1. 读取版本号 ----------
$version_match = Select-String -Path 'crates/cli/Cargo.toml' -Pattern '^version = "([^"]+)"'
$version = $version_match.Matches[0].Groups[1].Value

$package_name = "i18n-rust-$version-windows-x86_64-完整版"
$release_dir = 'release'
$temp_root = ".release-tmp-full"
$archive = Join-Path $release_dir "$package_name.zip"

Write-Host "======================================================"
Write-Host "i18n-rust 完整离线版打包"
Write-Host "   版本：v$version"
Write-Host "   产物：$archive"
Write-Host "======================================================"

# ---------- 2. 构建 ----------
Write-Host "`n[1/4] 编译 rzc 与语言服务器（release）..."
cargo build --release -p rzc -p i18n-rust-lsp
if ($LASTEXITCODE -ne 0) { Write-Host "编译失败" -ForegroundColor Red; exit $LASTEXITCODE }

# ---------- 3. 下载 VS Code 免安装版 ----------
Write-Host "`n[2/4] 下载 VS Code 免安装版（官方，约 100MB）..."
$vscode_zip = Join-Path $env:TEMP "vscode-portable.zip"
if (-not (Test-Path $vscode_zip) -or (Get-Item $vscode_zip).Length -lt 50MB) {
    curl.exe -L "https://update.code.visualstudio.com/latest/win32-x64-archive/stable" -o $vscode_zip --silent
    if ($LASTEXITCODE -ne 0) { Write-Host "VS Code 下载失败，请检查网络" -ForegroundColor Red; exit 1 }
}
Write-Host "   下载完成：$([Math]::Round((Get-Item $vscode_zip).Length/1MB,1)) MB"

# ---------- 4. 组装 ----------
Write-Host "`n[3/4] 组装完整目录..."
if (Test-Path $temp_root) { Remove-Item $temp_root -Recurse -Force }
$pack_dir = Join-Path $temp_root $package_name
New-Item -ItemType Directory -Path $pack_dir -Force | Out-Null

# 4.1 VS Code（解压到 pack_dir/vscode；zip 无顶层目录，内容直接在 temp_root）
Write-Host "   解压 VS Code..."
tar -xf $vscode_zip -C $temp_root
$vscode_dest = Join-Path $pack_dir 'vscode'
New-Item -ItemType Directory -Path $vscode_dest -Force | Out-Null
Get-ChildItem $temp_root | Where-Object { $_.Name -ne $package_name } | Move-Item -Destination $vscode_dest -Force

# 4.2 预装扩展（vsix 解压到 vscode/data/extensions，便携模式自动加载）
Write-Host "   预装 i18n-rust 扩展..."
$vsix = Get-ChildItem $release_dir -Filter "i18n-rust-$version.vsix" | Select-Object -First 1
if ($vsix) {
    $ext_dir = Join-Path $pack_dir 'vscode/data/extensions/i18n-rust-extension'
    New-Item -ItemType Directory -Path $ext_dir -Force | Out-Null
    tar -xf $vsix.FullName -C $ext_dir
    # vsix 内是 extension/ 目录，解压到子目录后把内容上移
    $inner = Join-Path $ext_dir 'extension'
    if (Test-Path $inner) {
        Get-ChildItem $inner | Move-Item -Destination $ext_dir -Force
        Remove-Item $inner -Recurse -Force
    }
}

# 4.3 rzc + 语言服务器 + VC++ 运行库
Copy-Item 'target/release/rzc.exe' (Join-Path $pack_dir 'rzc.exe')
Copy-Item 'target/release/i18n-rust-lsp.exe' (Join-Path $pack_dir 'i18n-rust-lsp.exe')
foreach ($dll in @('vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll','concrt140.dll')) {
    $src = Join-Path $env:WINDIR "System32\$dll"
    if (Test-Path $src) { Copy-Item $src (Join-Path $pack_dir $dll) }
}

# 4.4 内置工具链（~/.rz/toolchain/bin）
$tc_src = Join-Path $HOME '.rz/toolchain/bin'
if (Test-Path $tc_src) {
    $tc_dest = Join-Path $pack_dir 'toolchain/bin'
    New-Item -ItemType Directory -Path $tc_dest -Force | Out-Null
    Copy-Item (Join-Path $tc_src '*') $tc_dest -Recurse -Force
    Write-Host "   已附带内置工具链"
} else {
    Write-Host "   WARN: 未找到 ~/.rz/toolchain/bin（请先执行 rzc install toolchain）" -ForegroundColor Yellow
}

# 4.5 语言包与教程
Copy-Item 'crates/engine/lang-packs' (Join-Path $pack_dir 'lang-packs') -Recurse -Force
if (Test-Path 'tutorials') { Copy-Item 'tutorials' (Join-Path $pack_dir '教程') -Recurse -Force }
if (Test-Path 'LICENSE') { Copy-Item 'LICENSE' $pack_dir }
Copy-Item 'README.md' (Join-Path $pack_dir 'README.md') -Force

# 4.6 启动脚本（双击即用）
@'
@echo off
rem 启动 i18n-rust 编程环境（VS Code 便携模式，扩展已预装）
start "" "%~dp0vscode\Code.exe"
'@ | Out-File -FilePath (Join-Path $pack_dir '启动编程环境.cmd') -Encoding ASCII

# 4.7 使用说明
@'
# i18n-rust 完整离线版

本包包含：VS Code（免安装便携版，已预装 i18n-rust 扩展）+ rzc + 语言服务器
+ 完整内置工具链（rustc/cargo/rust-analyzer）+ VC++ 运行库 + 10 种语言包 + 教程。

## 使用（Windows）

1. 解压本包到任意目录（建议英文路径，如 D:\i18n-rust）；
2. 双击「启动编程环境.cmd」——VS Code 打开即用（语法高亮、补全、诊断已就绪）；
3. 创建项目：菜单栏 终端 → 新终端，输入：
   .\rzc.exe init 我的项目
   cd 我的项目
   ..\rzc.exe run src/main.zh

## 命令行使用

在包目录打开 PowerShell：
   .\rzc.exe --help        查看全部命令
   .\rzc.exe doctor        查看工具链状态
   .\rzc.exe init 我的项目 创建项目

## 说明

- 离线包已内置工具链，无需安装 Rust/rustup，无需联网；
- 教程见「教程」目录；README.md 为项目说明。
'@ | Out-File -FilePath (Join-Path $pack_dir '使用说明.md') -Encoding UTF8

# ---------- 5. 打包 ----------
Write-Host "`n[4/4] 打包压缩（约几分钟）..."
New-Item -ItemType Directory -Path $release_dir -Force | Out-Null
Push-Location $temp_root
try {
    tar -a -c -f "..\$archive" $package_name
    if ($LASTEXITCODE -ne 0) { throw "打包失败" }
} finally {
    Pop-Location
}
Remove-Item $temp_root -Recurse -Force

Write-Host ""
Write-Host "✅ 完整离线版已生成：$((Get-Location).Path)\$archive"
Write-Host "   大小：$([Math]::Round((Get-Item $archive).Length/1MB,1)) MB"
Write-Host "   分发：上传网盘即可；用户解压后双击「启动编程环境.cmd」即用，不会下错版本。"
