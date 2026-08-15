# i18n-rust - 中文 Rust 教学方言 VS Code 扩展

在 VS Code 中使用中文编写 Rust 代码：语法高亮、智能补全、错误提示与所有权可视化。

## 功能特性

- **语法高亮**：`.zh` 文件的中文 Rust 方言语法高亮（`函数`、`让`、`如果` 等关键字）
- **智能补全**：通过 LSP 代理（i18n-rust-lsp）提供补全、悬停、定义跳转、引用查找、重命名等
- **错误提示**：中文教学诊断，所有权错误（E0382/E0502/E0507）附带叙事化说明
- **所有权可视化**：用颜色高亮变量的移动（黄）、再次使用（红）与生命周期（绿）
- **代码片段**：内置常用代码片段（`函数`、`让`、`如果`、`循环` 等）
- **一键命令**：运行（`Ctrl+Shift+R`）、类型检查（`Ctrl+Shift+C`）、导出标准 Rust

## 离线安装

本扩展尚未发布到 VS Code 扩展市场（Marketplace）。推荐通过 `.vsix` 文件离线安装：

### 1. 下载 .vsix 文件

从项目 Release 页面下载已编译好的 `.vsix` 文件（无需安装任何开发环境）：

- **GitCode**（国内推荐）：https://gitcode.com/tan80/zrRust/releases
- **GitHub**：https://github.com/liuqiTan80/i18n-rust/releases

下载最新版本中的 `i18n-rust-<版本>.vsix` 文件即可。

> 备选：如需从源码打包，在 `tools/vscode-extension/` 目录下运行 `./package-vsix.sh`（Linux/macOS）或 `powershell -ExecutionPolicy Bypass -File .\package-vsix.ps1`（Windows），需要 Node.js 18+。

### 2. 安装 .vsix

1. 打开 VS Code，点击左侧活动栏的 **扩展** 图标（或按 `Ctrl+Shift+X`）打开扩展侧边栏
2. 点击侧边栏右上角的 **“...”** 菜单
3. 选择 **“从 VSIX 安装...”**（Install from VSIX...）
4. 在文件选择对话框中，选中下载的 `i18n-rust-<版本>.vsix` 文件
5. 点击 **安装**，安装完成后按提示 **重新加载** 窗口

> 命令行方式：`code --install-extension i18n-rust-0.1.0.vsix`

### 3. 验证安装

安装并重载窗口后：

- 新建 `main.zh` 文件，应看到中文语法高亮
- 打开命令面板（`Ctrl+Shift+P`），输入 `i18n`，应能看到“i18n: 运行 (Run)”、“i18n: 类型检查 (Check)”等命令
- 编写所有权错误代码（如移动后再使用变量），应看到黄色/红色/绿色高亮装饰器

### 卸载

扩展侧边栏中找到 i18n-rust，点击齿轮图标 → 卸载；或在命令面板执行 `扩展：显示已安装的扩展` 后卸载。

## 从源码开发

```bash
cd tools/vscode-extension
npm install        # 安装依赖
npm run compile    # 编译 TypeScript
npm run watch      # 监听模式编译（开发时使用）
```

按 `F5` 打开扩展开发宿主窗口进行调试（需配合 LSP 代理 `i18n-rust-lsp` 可执行文件，路径可通过设置 `i18n-rust.serverPath` 指定）。

## 许可证

[MIT](LICENSE)
