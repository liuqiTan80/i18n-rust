<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

[![CI](https://github.com/liuqiTan80/i18n-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/liuqiTan80/i18n-rust/actions)
[![crates.io](https://img.shields.io/crates/v/rzc.svg)](https://crates.io/crates/rzc)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

# rzc —— 用母语编写真正的 Rust

**rzc 是多语言 Rust 教学方言编译器**：你用母语写代码，rzc 实时翻译为标准 Rust 交给官方工具链编译运行，再把所有报错翻译成母语并附上教学提示。

- 🌍 **不是伪代码**：母语代码与标准 Rust 完全同构，编译、运行、依赖、生态 100% 真实
- 🎓 **面向初学者**：错误信息不再是一堆英文，而是"下一步该做什么"的教学引导
- 🚪 **随时毕业**：`rzc eject` 一键导出标准 Rust，平滑回到主流生态，没有任何锁定

```rust
// src/main.zh
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

写错了也没关系——报错也是母语：

```
错误[E0384]: 不可变变量 `数量` 被重复赋值
  --> src/main.zh:3:5
💡 如果需要修改变量的值，请使用 `让 可变` 声明变量。
```

---

## 📦 安装

**前置条件**：已安装 [Rust 工具链](https://www.rust-lang.org/tools/install)（rustup 的 stable 即可）。

| 方式              | 命令                                               | 适用场景        |
|-------------------|----------------------------------------------------|----------------|
| crates.io（推荐） | `cargo install rzc`                                | 有网络 |
| 离线发布包        | 下载 Release 离线包后运行 `rzc install`             | 教学机房/无网络 |
| 源码构建          | `git clone` 后 `cargo build --release --workspace` | 参与开发 |

装完即用：**10 种语言包已内置在二进制中，无需任何额外配置**。

> 要在 VS Code / Qoder 里获得补全、跳转、悬停等完整智能提示，还需语言服务器（一条命令，自动安装）：
>
> ```bash
> rzc install lsp

> **推荐：内置官方工具链**（脱离 rustup，无需 PATH 配置、无组件管理、无网络索引卡死）：
>
> ```bash
> rzc install toolchain   # 一键安装 standalone rustc/cargo/rust-analyzer 到 ~/.rz/toolchain
> rzc doctor              # 查看工具链环境状态（内置 / PATH / 版本对比）
> ```
>
> 安装后 rzc 与 LSP 自动优先使用内置工具链；单文件项目直调 rustc，无需 cargo 索引。> ```

**IDE 扩展**：从 [GitHub Releases](https://github.com/liuqiTan80/i18n-rust/releases)（或[百度网盘](https://pan.baidu.com/s/19EGFN7kTS-ASNXvwbXINJQ?pwd=i18n)）下载 `i18n-rust-*.vsix`，在编辑器中选择「从 VSIX 安装」。

---

## 🚀 快速开始

```bash
rzc init 我的项目        # 生成完整项目骨架（Cargo.toml + src/main.zh）
cd 我的项目
rzc run src/main.zh      # 翻译 → 编译 → 运行
```

三步之内，你的第一个母语 Rust 程序就跑起来了。

---

## 🛠️ 命令速查

| 命令 | 说明 |
|------------------------------------|------------------------------------------------------|
| `rzc init <项目名>`                | 创建新项目（工具链版本锁定到本机当前版本，IDE 开箱可用） |
| `rzc run <文件>`                   | 翻译并运行；警告/错误/构建进度全部母语化 |
| `rzc check <文件>`                 | 类型检查，输出母语教学诊断 |
| `rzc eject <文件>`                 | 导出为标准 Rust 代码（渐进过渡） |
| `rzc add <库名>[@版本]`            | 添加第三方依赖（封装 `cargo add`，附母语映射提示） |
| `rzc lang list`                    | 列出已安装语言包 |
| `rzc lang install <码/目录>`       | 安装语言包（远程仓库或本地目录） |
| `rzc lang remove <码>`             | 删除用户安装的语言包 |
| `rzc mapping auto <crate名>`       | 自动生成第三方库的母语映射（AI/规则） |
| `rzc mapping check [目标]`         | 校验映射质量（重复键/关键字碰撞/跨文件冲突） |
| `rzc mapping scaffold <源> <目标>` | 生成新语言的翻译骨架，`--provider deepseek` 可 AI 自动翻译 |
| `rzc install [lsp]` | 安装配套组件（语言服务器） |

完整参考见 [附录D：rzc命令速查](tutorials/附录D：rzc命令速查.md)。

---

## ✨ 功能特性

### 母语编程
用母语关键字（`函数`、`让`、`如果`、`匹配`…）、母语标准库（`字符串`、`向量::新建()`、`使用 标准集合::哈希映射`）编写完整程序；宏、生命周期、泛型、特征全部支持。

### 10 种语言内置

| 语言    | 扩展名 | 语言      | 扩展名 |
|---------|-------|-----------|--------|
| 中文    | `.zh` | Español   | `.es`  |
| Deutsch | `.de` | Français  | `.fr`  |
| 日本語  | `.ja` | Português | `.pt`  |
| 한국어  | `.ko` | العربية   | `.ar`  |
| Русский | `.ru` | हिन्दी    | `.hi`  |

Rust 本身以英文书写，因此不提供英语方言（恒等映射无教学价值）；其余 10 种自然语言按文件扩展名自动匹配，同一项目可混用。

### 教学级诊断
- **错误码 + 消息双轨翻译**：覆盖 rustc 错误码、无码 lint 警告、help 短语
- **类型名本地化**：`std::fmt::Display` → `标准库::格式化::可显示`
- **💡 教学提示**：每条错误附带下一步建议；所有权错误附带 📌 移动/借用叙事
- **依赖引导**：识别未声明的第三方库，提示 `rzc add <crate>`

### 完整 IDE 体验（VS Code / Qoder 扩展）
语法高亮、智能补全、悬停文档、定义跳转、引用查找、重命名、代码格式化、一键运行/检查、全角标点自动转半角、AI 辅助翻译。

### 第三方库母语化
`rzc mapping auto` 从已安装 crate 提取公开 API，AI 生成母语名；社区共建映射经 `rzc mapping check` 质量门禁。

---

## 📖 配套教程

面向零基础学习者的完整中文教程：**25 章 + 总术语表 + 5 个附录**，见 [tutorials/](tutorials/)。
从《你好世界》到所有权、闭包、异步、宏，直至综合实战——所有示例全部用中文 Rust 书写。

> 欢迎共同翻译**教程**与**映射表**到其他语言，见下文"参与贡献"。

---

## 🏗️ 项目结构与工作原理

```text
母语源码 (.zh)
   │  词法转译 → 模块路径替换 → 别名替换        ← engine（语言无关）
   ▼
标准 Rust 源码
   │  cargo build / run（官方工具链）
   ▼
JSON 诊断 → 错误码/消息表翻译 + 类型本地化 + 教学提示 → 母语输出
```

| 目录                       | 职责                                                                   |
|----------------------------|------------------------------------------------------------------------|
| `crates/engine`            | 语言无关核心引擎：转译管线、映射管理、诊断翻译、增量缓存、Unicode 安全检查 |
| `crates/cli`               | `rzc` 命令行工具 |
| `crates/lsp`               | `i18n-rust-lsp`：代理官方语言服务器（rust-analyzer），双向翻译位置与诊断 |
| `crates/engine/lang-packs` | 11 个自然语言包（关键字/标准库/模块路径/错误翻译/界面文案） |
| `tools/vscode-extension`   | VS Code / Qoder 扩展 |
| `tutorials`                | 25 章中文教程与附录 |
| `docs`                     | 语言包贡献指南、第三方映射工具链、方言框架生成蓝图 |

**设计原则**：引擎不硬编码任何具体语言；新增一门自然语言 = 新增一个语言包目录，零代码改动（构建脚本自动嵌入）。想把这套范式移植到其他编程语言（如中文 Python），参见 [方言编程框架生成蓝图](docs/dialect-framework-blueprint.md)。

---

## 🤝 参与贡献

- **新增语言包**：[docs/contributing-lang-pack.md](docs/contributing-lang-pack.md)（含 `rzc mapping scaffold` AI 翻译流程）
- **第三方库映射**：[docs/third-party-mapping.md](docs/third-party-mapping.md)
- **翻译教程**：以 `tutorials/` 为源，保持章节结构一致
- 提交前请确保 `cargo test --workspace` 全部通过

---

## 📄 许可证

[MIT](LICENSE) © tan80

