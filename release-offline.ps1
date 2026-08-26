# =============================================================
# rzc 离线发布包构建脚本（Windows / PowerShell）
#
# 功能：
#   1. cargo build --release 编译 rzc 可执行文件
#   2. 将可执行文件、lang-packs/ 目录、许可证与说明文档组装到临时目录
#   3. 打包为 .zip 压缩包
#   4. 输出压缩包路径
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File .\release-offline.ps1
#   （或直接：.\release-offline.ps1）
#
# 产物：
#   release\rzc-<版本>-windows-<架构>.zip
#
# 提示：
#   - 中文语言包已内置到可执行文件，解压后开箱即用；
#   - 包内附带的 lang-packs/ 目录为可选扩展（远程安装的语言包），
#     可通过环境变量 RZ_LANG_DIR 指向该目录使用。
# =============================================================
$ErrorActionPreference = 'Stop'

# 切换到项目根目录（脚本所在位置）
Set-Location (Split-Path -Parent $MyInvocation.MyCommand.Definition)

# ---------- 1. 解析版本号（来自 crates/cli/Cargo.toml） ----------
$version_match = Select-String -Path 'crates/cli/Cargo.toml' -Pattern '^version = "([^"]+)"'
if (-not $version_match) {
    Write-Host "❌ 无法从 crates/cli/Cargo.toml 读取版本号" -ForegroundColor Red
    exit 1
}
$version = $version_match.Matches[0].Groups[1].Value

# ---------- 2. 检测架构 ----------
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'aarch64' } else { 'x86_64' }

$package_name = "rzc-$version-windows-$arch"
$release_dir = 'release'
$temp_root = ".release-tmp-$package_name"
$archive = Join-Path $release_dir "$package_name.zip"

Write-Host "======================================================"
Write-Host "🔨 rzc 离线发布包构建"
Write-Host "   版本：v$version"
Write-Host "   平台：windows-$arch"
Write-Host "   产物：$archive"
Write-Host "======================================================"

# ---------- 3. 编译 release 二进制 ----------
Write-Host ""
Write-Host "📦 第 1 步：编译 rzc（release）..."
cargo build --release -p rzc
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ cargo build 失败（退出码 $LASTEXITCODE）" -ForegroundColor Red
    exit $LASTEXITCODE
}

# ---------- 4. 组装发布目录 ----------
Write-Host "📁 第 2 步：组装发布目录..."
if (Test-Path $temp_root) { Remove-Item $temp_root -Recurse -Force }
$pack_dir = Join-Path $temp_root $package_name
New-Item -ItemType Directory -Path (Join-Path $pack_dir 'lang-packs') -Force | Out-Null

# 可执行文件
Copy-Item 'target/release/rzc.exe' (Join-Path $pack_dir 'rzc.exe')

# 语言包目录（可选扩展：远程安装的语言包；中文等已内置到可执行文件）；
# 单一数据源为 crates/engine/lang-packs/，包内仍按 lang-packs/ 布局（RZ_LANG_DIR 约定）
if (Test-Path 'crates/engine/lang-packs') {
    Copy-Item 'crates/engine/lang-packs/*' (Join-Path $pack_dir 'lang-packs') -Recurse -Force
# 语言服务器（离线包内自带，rzc install 优先复制同目录二进制）
if (Test-Path 'target/release/i18n-rust-lsp.exe') {
    Copy-Item 'target/release/i18n-rust-lsp.exe' (Join-Path $pack_dir 'i18n-rust-lsp.exe')
    Write-Host "   ? 已附带语言服务器 i18n-rust-lsp.exe"
}

# 内置工具链（~/.rz/toolchain/bin，若已安装则一并打包；离线包即开即用）

# VC++ 运行库（新系统可能缺失，随包分发；从系统 System32 复制）
foreach ($dll in @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll', 'concrt140.dll')) {
    $src = Join-Path $env:WINDIR "System32\$dll"
    if (Test-Path $src) {
        Copy-Item $src (Join-Path $pack_dir $dll)
        Write-Host "   已附带运行库 $dll"
    }
}

$tc_src = Join-Path $HOME '.rz/toolchain/bin'
if (Test-Path $tc_src) {
    $tc_dest = Join-Path $pack_dir 'toolchain/bin'
    New-Item -ItemType Directory -Path $tc_dest -Force | Out-Null
    Copy-Item (Join-Path $tc_src '*') $tc_dest -Recurse -Force
    Write-Host "   ? 已附带内置工具链（rustc/cargo/rust-analyzer），安装后无需 rustup"
} else {
    Write-Host "   ? 未找到 ~/.rz/toolchain/bin（可先执行 rzc install toolchain 再打包，离线包将包含完整工具链）"
}
    Write-Host "   ✅ 已复制语言包：$((Get-ChildItem 'crates/engine/lang-packs' -Directory).Name -join ' ')"
} else {
    Write-Host "   ⚠️ 未找到 crates/engine/lang-packs/ 目录，跳过（内置中文语言包不受影响）"
}

# 许可证与说明文档
if (Test-Path 'LICENSE') { Copy-Item 'LICENSE' $pack_dir }
if (Test-Path 'README.md') { Copy-Item 'README.md' $pack_dir }

# 语言服务器（离线包内自带，rzc install 优先复制同目录二进制）
if (Test-Path 'target/release/i18n-rust-lsp.exe') {
    Copy-Item 'target/release/i18n-rust-lsp.exe' (Join-Path $pack_dir 'i18n-rust-lsp.exe')
    Write-Host "   已附带语言服务器 i18n-rust-lsp.exe"
}

# 内置工具链（~/.rz/toolchain/bin，若已安装则一并打包；离线包即开即用）
$tc_src = Join-Path $HOME '.rz/toolchain/bin'
if (Test-Path $tc_src) {
    $tc_dest = Join-Path $pack_dir 'toolchain/bin'
    New-Item -ItemType Directory -Path $tc_dest -Force | Out-Null
    Copy-Item (Join-Path $tc_src '*') $tc_dest -Recurse -Force
    Write-Host "   已附带内置工具链（rustc/cargo/rust-analyzer），安装后无需 rustup"
} else {
    Write-Host "   未找到 ~/.rz/toolchain/bin（可先执行 rzc install toolchain 再打包，离线包将包含完整工具链）"
}
# 使用说明
$readme_content = @'
# rzc 离线发布包使用说明

## 快速开始

- **Windows**：双击运行 `rzc.exe`，或在 PowerShell 中执行 `.\rzc.exe --help` 查看命令
- **Linux / macOS**：直接运行 `./rzc`

中文语言包已内置到可执行文件中，开箱即用，无需任何配置。

## 常用命令

```powershell
.\rzc.exe init 我的项目      # 创建新项目（含 src/main.zh）
.\rzc.exe run src/main.zh   # 翻译并运行中文代码
.\rzc.exe check src/main.zh # 类型检查（中文错误提示）
.\rzc.exe lang list         # 查看语言包
```

## 可选：使用包内语言包目录

包内附带的 `lang-packs/` 目录用于扩展（例如后续远程安装的其他语言包）。
如需使用，将环境变量 `RZ_LANG_DIR` 指向该目录：

```powershell
$env:RZ_LANG_DIR = "$PWD\lang-packs"
```

或在用户主目录下建立全局语言包目录：

```powershell
New-Item -ItemType Directory -Force -Path "$HOME\.rz\lang-packs"
Copy-Item .\lang-packs\* "$HOME\.rz\lang-packs" -Recurse
```

## 系统要求

- 需要 Rust 编译运行环境（rzc 通过 cargo 调用 rustc）
- Windows 10 1803+（含 Windows 11）
'@
Set-Content -Path (Join-Path $pack_dir '使用说明.md') -Value $readme_content -Encoding UTF8

Write-Host "   ✅ 发布目录组装完成：$pack_dir"

# ---------- 5. 打包压缩 ----------
Write-Host "🗜️ 第 3 步：打包压缩..."
New-Item -ItemType Directory -Path $release_dir -Force | Out-Null
if (Get-Command tar.exe -ErrorAction SilentlyContinue) {
    # Windows 10 1803+ 自带 bsdtar，-a 按 .zip 扩展名自动选择 zip 格式
    Push-Location $temp_root
    try {
        tar -a -c -f "..\$archive" $package_name
        if ($LASTEXITCODE -ne 0) { throw "tar 打包失败（退出码 $LASTEXITCODE）" }
    } finally {
        Pop-Location
    }
} else {
    # 回退：PowerShell 内置 Compress-Archive（较慢但无需外部工具）
    Compress-Archive -Path $pack_dir -DestinationPath $archive -Force
}

# ---------- 6. 清理临时目录并输出结果 ----------
Remove-Item $temp_root -Recurse -Force
Write-Host ""
Write-Host "✅ 离线发布包已生成：$((Get-Location).Path)\$archive"
Write-Host "💡 部署方式：将压缩包拷贝到目标机器，解压后直接运行其中的 rzc.exe 即可。"
