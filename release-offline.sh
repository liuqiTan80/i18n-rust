#!/usr/bin/env bash
# =============================================================
# rzc 离线发布包构建脚本（Linux / macOS）
#
# 功能：
#   1. cargo build --release 编译 rzc 可执行文件
#   2. 将可执行文件、lang-packs/ 目录、许可证与说明文档组装到临时目录
#   3. 打包为 .tar.gz（Windows 下运行本脚本时自动改用 .zip）
#   4. 输出压缩包路径
#
# 用法：
#   ./release-offline.sh
#
# 产物：
#   release/rzc-<版本>-<平台>-<架构>.tar.gz
#
# 提示：
#   - 中文语言包已内置到可执行文件，解压后开箱即用；
#   - 包内附带的 lang-packs/ 目录为可选扩展（远程安装的语言包），
#     可通过环境变量 RZ_LANG_DIR 指向该目录使用。
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
echo "📦 第 1 步：编译 rzc（release）..."
cargo build --release -p rzc

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

# 语言包目录（可选扩展：远程安装的语言包；中文等已内置到可执行文件）
if [ -d "lang-packs" ]; then
    cp -r lang-packs/. "$PACK_DIR/lang-packs/"
    echo "   ✅ 已复制语言包：$(ls lang-packs | tr '\n' ' ')"
else
    echo "   ⚠️ 未找到 lang-packs/ 目录，跳过（内置中文语言包不受影响）"
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
echo "💡 部署方式：将压缩包拷贝到目标机器，解压后直接运行其中的 rzc（或 rzc.exe）即可。"
