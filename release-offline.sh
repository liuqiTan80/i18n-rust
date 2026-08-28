#!/usr/bin/env bash
# =============================================================
# rzc 离线发布包构建脚本（Linux / macOS）
#
# 功能：
#   1. cargo build --release 编译 rzc 与语言服务器 i18n-rust-lsp
#   2. 附带 rust-analyzer（官方 Release 资产下载不稳定，随包分发；
#      来源为 ~/.rz/toolchain/bin，即 rzc install toolchain 安装的内置工具链）
#   3. 将可执行文件、语言包、教程、许可证与说明文档组装到临时目录
#   4. 打包为 .tar.gz（Windows 下运行本脚本时自动改用 .zip）
#   5. 输出压缩包路径
#
# 用法：
#   ./release-offline.sh
#
# 产物：
#   release/rzc-<版本>-<平台>-<架构>.tar.gz
#
# 提示：
#   - 本包不内置 rustc/cargo 与 VS Code（发布方案已取消）；编译运行中文代码
#     需自行安装 Rust 环境，或联网执行 rzc install toolchain；
#   - 离线包由本地编译，手动上传到 Release / 网盘分发。
# =============================================================
set -euo pipefail

# 切换到项目根目录（脚本所在位置）
cd "$(dirname "$0")"

# ---------- 1. 解析版本号（来自 crates/cli/Cargo.toml） ----------
RZC_VERSION=$(grep -m1 '^version = ' crates/cli/Cargo.toml | cut -d'"' -f2)
if [ -z "$RZC_VERSION" ]; then
    echo "❌ 无法从 crates/cli/Cargo.toml 读取版本号"
    exit 1
fi

# ---------- 2. 检测平台与架构 ----------
UNAME_S=$(uname -s)
ARCH_RAW=$(uname -m)
case "$UNAME_S" in
    Linux)  PLATFORM="linux" ;;
    Darwin) PLATFORM="macos" ;;
    # 在 Git Bash / MSYS2 下运行时按 Windows 处理，打包为 zip
    MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
    *) echo "⚠️ 无法识别的系统：$UNAME_S，将按 linux 处理"; PLATFORM="linux" ;;
esac
case "$ARCH_RAW" in
    arm64)  ARCH="aarch64" ;;
    x86_64|amd64) ARCH="x86_64" ;;
    *) ARCH="$ARCH_RAW" ;;
esac

PACKAGE_NAME="rzc-$RZC_VERSION-$PLATFORM-$ARCH"
RELEASE_DIR="release"
TEMP_ROOT=".release-tmp-$PACKAGE_NAME"
ARCHIVE="$RELEASE_DIR/$PACKAGE_NAME.tar.gz"
if [ "$PLATFORM" = "windows" ]; then
    # Windows 下打包 zip（Git Bash 的 tar 为 bsdtar，-a 按扩展名自动选择格式）
    ARCHIVE="$RELEASE_DIR/$PACKAGE_NAME.zip"
fi

echo "======================================================"
echo "🔨 rzc 离线发布包构建"
echo "   版本：v$RZC_VERSION"
echo "   平台：$PLATFORM-$ARCH"
echo "   产物：$ARCHIVE"
echo "======================================================"

# ---------- 3. 编译 release 二进制 ----------
echo ""
echo "📦 第 1 步：编译 rzc 与语言服务器（release）..."
cargo build --release -p rzc -p i18n-rust-lsp

# ---------- 4. 组装发布目录 ----------
echo "📁 第 2 步：组装发布目录..."
rm -rf "$TEMP_ROOT"
PACK_DIR="$TEMP_ROOT/$PACKAGE_NAME"
mkdir -p "$PACK_DIR/lang-packs"

# 可执行文件（Windows 下为 rzc.exe）
if [ -f "target/release/rzc.exe" ]; then
    cp "target/release/rzc.exe" "$PACK_DIR/rzc.exe"
else
    cp "target/release/rzc" "$PACK_DIR/rzc"
fi

# 语言服务器（VS Code 扩展后端；与 rzc 同目录，`rzc install lsp` 可免网络直接安装）
if [ -f "target/release/i18n-rust-lsp.exe" ]; then
    cp "target/release/i18n-rust-lsp.exe" "$PACK_DIR/i18n-rust-lsp.exe"
else
    cp "target/release/i18n-rust-lsp" "$PACK_DIR/i18n-rust-lsp"
fi

# rust-analyzer：发布方案中唯一保留随包分发的第三方组件（官方资产下载不稳定）；
# 来源为本地 rzc install toolchain 安装的内置工具链（~/.rz/toolchain/bin）
RA_SRC="$HOME/.rz/toolchain/bin/rust-analyzer"
if [ -f "$RA_SRC" ]; then
    mkdir -p "$PACK_DIR/toolchain/bin"
    cp "$RA_SRC" "$PACK_DIR/toolchain/bin/"
    echo "   ✅ 已附带 rust-analyzer（$("$RA_SRC" --version 2>/dev/null || echo 未知版本)）"
else
    echo "   ⚠️ 未找到 $RA_SRC，跳过 rust-analyzer"
    echo "      可先执行 rzc install toolchain --ra-only --force 安装后再打包"
fi

# 教程（教学场景离线分发）
if [ -d "tutorials" ]; then
    cp -r tutorials "$PACK_DIR/教程"
    echo "   ✅ 已附带教程目录"
fi

# 语言包目录（可选扩展：远程安装的语言包；中文等已内置到可执行文件）；
# 单一数据源为 crates/engine/lang-packs/，包内仍按 lang-packs/ 布局（RZ_LANG_DIR 约定）
if [ -d "crates/engine/lang-packs" ]; then
    cp -r crates/engine/lang-packs/. "$PACK_DIR/lang-packs/"
    echo "   ✅ 已复制语言包：$(ls crates/engine/lang-packs | tr '\n' ' ')"
else
    echo "   ⚠️ 未找到 crates/engine/lang-packs/ 目录，跳过（内置中文语言包不受影响）"
fi

# 许可证与说明文档
[ -f LICENSE ] && cp LICENSE "$PACK_DIR/" || true
[ -f README.md ] && cp README.md "$PACK_DIR/" || true

# 使用说明
cat > "$PACK_DIR/使用说明.md" <<'说明'
# rzc 离线发布包使用说明

## 快速开始

- **Linux / macOS**：直接运行 `./rzc`（终端中执行 `./rzc --help` 查看命令）
- **Windows**：运行 `rzc.exe`（PowerShell 中执行 `.\rzc.exe --help`）

中文语言包已内置到可执行文件中，开箱即用，无需任何配置。

## 常用命令

```bash
./rzc init 我的项目      # 创建新项目（含 src/main.zh）
./rzc run src/main.zh   # 翻译并运行中文代码
./rzc check src/main.zh # 类型检查（中文错误提示）
./rzc lang list         # 查看语言包
```

## 包内容

本包为本地编译的离线发布包，包含：

- `rzc`（命令行工具）与 `i18n-rust-lsp`（语言服务器，VS Code 扩展后端）
- `toolchain/bin/rust-analyzer`（官方 rust-analyzer：其发布资产下载不稳定，随包附送）
- `lang-packs/`（可选语言包目录）与 `教程/`

## rust-analyzer 接入（VS Code 使用）

包内 rust-analyzer 不会自动注册，二选一：

```bash
# 方式一：复制到 rzc 内置工具链目录（rzc / LSP 自动优先使用）
mkdir -p ~/.rz/toolchain/bin
cp toolchain/bin/rust-analyzer ~/.rz/toolchain/bin/

# 方式二：环境变量指定（无需复制）
export RUST_ANALYZER_PATH="$(pwd)/toolchain/bin/rust-analyzer"
```

## Rust 编译环境（rustc/cargo）

本包**不再内置 rustc/cargo**（发布方案已取消）：编译运行中文代码需自行安装 Rust 环境：

- 联网机器：`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`（或使用发行版软件包）；
- 或联网后执行 `rzc install toolchain`（官方 standalone 工具链安装到 ~/.rz/toolchain）。

## 可选：使用包内语言包目录

包内附带的 `lang-packs/` 目录用于扩展（例如后续远程安装的其他语言包）。
如需使用，将环境变量 `RZ_LANG_DIR` 指向该目录：

```bash
export RZ_LANG_DIR="$(pwd)/lang-packs"
```

或在用户主目录下建立全局语言包目录：

```bash
mkdir -p ~/.rz/lang-packs
cp -r lang-packs/* ~/.rz/lang-packs/
```

## 系统要求

- 需要 Rust 编译运行环境（rzc 通过 cargo 调用 rustc）
- Linux：glibc 2.31+；macOS：11.0+；Windows：10 1803+
说明

echo "   ✅ 发布目录组装完成：$PACK_DIR"

# ---------- 5. 打包压缩 ----------
echo "🗜️ 第 3 步：打包压缩..."
mkdir -p "$RELEASE_DIR"
if [ "$PLATFORM" = "windows" ]; then
    # bsdtar 的 -a 标志按 .zip 扩展名自动选择 zip 格式
    (cd "$TEMP_ROOT" && tar -a -c -f "../$ARCHIVE" "$PACKAGE_NAME")
else
    tar -czf "$ARCHIVE" -C "$TEMP_ROOT" "$PACKAGE_NAME"
fi

# ---------- 6. 清理临时目录并输出结果 ----------
rm -rf "$TEMP_ROOT"
echo ""
echo "✅ 离线发布包已生成：$(pwd)/$ARCHIVE"
echo "   包内结构："
tar -tzf "$ARCHIVE" 2>/dev/null | head -20 || unzip -l "$ARCHIVE" 2>/dev/null | head -20 || true
echo ""
echo "💡 部署方式：将压缩包手动上传到 Release / 网盘，用户解压后直接运行其中的 rzc（或 rzc.exe）即可。"
