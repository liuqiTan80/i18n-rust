<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc：母语编写 Rust 配套了中文教程

用中文编写 Rust 程序，rzc 自动翻译为标准 Rust 并编译运行——学编程，不背英语。

```rust
// src/main.zh —— 中文 Rust 教学方言
函数 主函数() {
    让 可变 数量 = 10;
    数量 = 数量 + 1;
    打印行!("数量是：{}", 数量);
}
```

```bash
$ rzc run src/main.zh
数量是：11
```

## 📦 安装

一条命令，装完即可全局使用：

```bash
cargo install rzc
```

> 需要已安装 [Rust 工具链](https://www.rust-lang.org/tools/install)（rustup 的 stable 即可）。

安装后 `rzc` 命令全局可用，语言包已内置，**无需任何额外配置**。

> VS Code 扩展的智能提示（高亮/补全/诊断）还需要语言服务器 `i18n-rust-lsp`，运行一次即可自动安装：
>
> ```bash
> rzc install lsp    # 或 cargo install i18n-rust-lsp
> ```
>
> 若使用离线发布包，`rzc install lsp` 会直接从包内免网络安装。

也可以从源码构建：

```bash
git clone https://gitcode.com/tan80/zrRust.git   # 或 github.com/liuqiTan80/i18n-rust
cd zrRust
cargo build --release --workspace                # 二进制在 target/release/rzc
```

## 🚀 快速开始

```bash
rzc init 我的项目
cd 我的项目
rzc run src/main.zh
```

`rzc init` 生成完整可运行的项目骨架（`Cargo.toml` + `src/main.zh`），直接运行即可。

## 🛠️ 常用命令

| 命令                               | 说明                                                        |
|------------------------------------|------------------------------------------------------------|
| `rzc init <项目名>`                | 创建新项目 |
| `rzc run <文件>`                   | 翻译并运行 `.zh` 源码 |
| `rzc check <文件>`                 | 类型检查，输出中文教学诊断 |
| `rzc eject <文件>`                 | 导出为标准 Rust 代码 |
| `rzc lang list`                    | 列出已安装语言包 |
| `rzc lang install <码/目录>`       | 安装语言包（远程仓库或本地目录，支持 `RZ_LANG_REPO` 自定义源） |
| `rzc install [lsp]`               | 安装配套组件（语言服务器 `i18n-rust-lsp`，VS Code 扩展后端；省略子命令默认安装全部） |
| `rzc mapping auto <crate名>`       | 自动生成第三方库映射 |
| `rzc mapping check [目标]`         | 第三方库映射质量校验（冲突/关键字避让/跨语言一致性） |
| `rzc mapping scaffold <源> <目标>` | 新语言翻译脚手架，`--provider deepseek` 可 AI 自动翻译键名 |

### 同时欢迎大家共同翻译**教程**以及**映射表**到其他语言
> 新增语言与第三方库映射维护见 [docs/contributing-lang-pack.md](docs/contributing-lang-pack.md) 与 [docs/third-party-mapping.md](docs/third-party-mapping.md)。

## ✨ 功能特性

- **母语编程**：用中文关键字（`函数`、`让`、`如果`、`返回`…）编写完整 Rust 程序
- **多语言原生**：内置 11 种语言包（中/英/德/日/俄/西/法/葡/韩/阿/印地），扩展名自动匹配（`.zh`、`.ja`、`.ru`…）
- **本地化诊断**：`rzc check` 把 rustc 错误翻译为对应语言，附带 💡 教学提示
- **VS Code 扩展**：从 [百度网盘](https://pan.baidu.com/s/19EGFN7kTS-ASNXvwbXINJQ?pwd=i18n) 下载 `i18n-rust.vsix`，在 VS Code 中选择「从 VSIX 安装」即可（详见[安装说明](tools/vscode-extension/)）。提供语法高亮、智能补全、所有权可视化（颜色高亮变量移动与再使用）、一键运行/检查、AI 对话、语言包工具（映射校验/翻译脚手架/语言包安装）等完整 IDE 体验
- **完整 LSP 支持**：补全、悬停、定义跳转、引用查找、重命名、代码格式化
- **渐进过渡**：`rzc eject` 一键导出标准 Rust 代码，平滑迁移生态

## 📖 配套教程

面向零基础新手的完整中文教程，共 24 章 + 4 个附录，见 [tutorials/](tutorials/)。

## 📄 许可证

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
