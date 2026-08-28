# =============================================================
# rzc 离线发布包构建脚本（Windows / PowerShell）
#
# 功能：
#   1. cargo build --release 编译 rzc 与语言服务器 i18n-rust-lsp
#   2. 附带 rust-analyzer（官方 Release 资产下载不稳定，随包分发；
#      来源为 ~/.rz/toolchain/bin，即 rzc install toolchain 安装的内置工具链）
#   3. 将可执行文件、语言包、教程、VC++ 运行库、许可证与说明文档组装到临时目录
#   4. 打包为 .zip 压缩包
#   5. 输出压缩包路径
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File .\release-offline.ps1
#   （或直接：.\release-offline.ps1）
#
# 产物：
#   release\rzc-<版本>-windows-<架构>.zip
#
# 提示：
#   - 本包不内置 rustc/cargo 与 VS Code（发布方案已取消）；编译运行中文代码
#     需自行安装 Rust 环境，或联网执行 rzc install toolchain；
#   - 离线包由本地编译，手动上传到 Release / 网盘分发。
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
Write-Host "📦 第 1 步：编译 rzc 与语言服务器（release）..."
cargo build --release -p rzc -p i18n-rust-lsp
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

# 语言服务器（VS Code 扩展后端；与 rzc 同目录，`rzc install lsp` 可免网络直接安装）
Copy-Item 'target/release/i18n-rust-lsp.exe' (Join-Path $pack_dir 'i18n-rust-lsp.exe')

# rust-analyzer：发布方案中唯一保留随包分发的第三方组件（官方资产下载不稳定）；
# 来源为本地 rzc install toolchain 安装的内置工具链（~/.rz/toolchain/bin）
$ra_src = Join-Path $HOME '.rz/toolchain/bin/rust-analyzer.exe'
if (Test-Path $ra_src) {
    $ra_dest = Join-Path $pack_dir 'toolchain/bin'
    New-Item -ItemType Directory -Path $ra_dest -Force | Out-Null
    Copy-Item $ra_src (Join-Path $ra_dest 'rust-analyzer.exe')
    Write-Host "   已附带 rust-analyzer（$(& $ra_src --version)）"
} else {
    Write-Host "   WARN: 未找到 $ra_src，跳过 rust-analyzer（可先执行 rzc install toolchain --ra-only --force 安装后再打包）" -ForegroundColor Yellow
}

# VC++ 运行库（新系统可能缺失，随包分发；从系统 System32 复制）
foreach ($dll in @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll', 'concrt140.dll')) {
    $src = Join-Path $env:WINDIR "System32\$dll"
    if (Test-Path $src) {
        Copy-Item $src (Join-Path $pack_dir $dll)
        Write-Host "   已附带运行库 $dll"
    }
}

# 语言包目录（可选扩展：远程安装的语言包；中文等已内置到可执行文件）；
# 单一数据源为 crates/engine/lang-packs/，包内仍按 lang-packs/ 布局（RZ_LANG_DIR 约定）
if (Test-Path 'crates/engine/lang-packs') {
    Copy-Item 'crates/engine/lang-packs/*' (Join-Path $pack_dir 'lang-packs') -Recurse -Force
    Write-Host "   ✅ 已复制语言包：$((Get-ChildItem 'crates/engine/lang-packs' -Directory).Name -join ' ')"
} else {
    Write-Host "   ⚠️ 未找到 crates/engine/lang-packs/ 目录，跳过（内置中文语言包不受影响）"
}

# 教程（教学场景离线分发）
if (Test-Path 'tutorials') {
    Copy-Item 'tutorials' (Join-Path $pack_dir '教程') -Recurse -Force
    Write-Host "   已附带教程目录"
}

# 许可证与说明文档
if (Test-Path 'LICENSE') { Copy-Item 'LICENSE' $pack_dir }
if (Test-Path 'README.md') { Copy-Item 'README.md' $pack_dir }
# 使用说明
$readme_content = @'
# rzc 离线发布包使用说明

## 快速开始

- **Windows**：在 PowerShell 中进入解压目录执行 `.\rzc.exe --help` 查看帮助（直接双击 `rzc.exe` 也会显示帮助，按任意键退出）；
- **Linux / macOS**：直接运行 `./rzc`

中文语言包已内置到可执行文件中，开箱即用，无需任何配置。

## 常用命令

```powershell
.\rzc.exe init 我的项目      # 创建新项目（含 src/main.zh）
.\rzc.exe run src/main.zh   # 翻译并运行中文代码
.\rzc.exe check src/main.zh # 类型检查（中文错误提示）
.\rzc.exe lang list         # 查看语言包
```

## 包内容

本包为本地编译的离线发布包，包含：

- `rzc.exe` 与 `i18n-rust-lsp.exe`（语言服务器，VS Code 扩展后端）
- `toolchain/bin/rust-analyzer.exe`（官方 rust-analyzer：其发布资产下载不稳定，随包附送）
- `lang-packs/`（可选语言包目录）与 `教程/`
- VC++ 运行库（vcruntime140 等，新系统可能缺失）

## rust-analyzer 接入（VS Code 使用）

包内 rust-analyzer 不会自动注册，二选一：

```powershell
# 方式一：复制到 rzc 内置工具链目录（rzc / LSP 自动优先使用）
New-Item -ItemType Directory -Force -Path "$HOME\.rz\toolchain\bin"
Copy-Item .\toolchain\bin\rust-analyzer.exe "$HOME\.rz\toolchain\bin\"

# 方式二：环境变量指定（无需复制）
$env:RUST_ANALYZER_PATH = "$PWD\toolchain\bin\rust-analyzer.exe"
```

## Rust 编译环境（rustc/cargo）

本包**不再内置 rustc/cargo**（发布方案已取消）：编译运行中文代码需自行安装 Rust 环境：

- 联网机器：到 https://rustup.rs 下载安装（或执行 `winget install Rustlang.Rustup`）；
- 或联网后执行 `rzc.exe install toolchain`（官方 standalone 工具链安装到 ~/.rz/toolchain）。

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
Write-Host "💡 部署方式：将压缩包手动上传到 Release / 网盘，用户解压后直接运行其中的 rzc.exe 即可。"
