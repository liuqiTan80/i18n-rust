<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

[![CI](https://github.com/liuqiTan80/i18n-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/liuqiTan80/i18n-rust/actions)
[![crates.io](https://img.shields.io/crates/v/rzc.svg)](https://crates.io/crates/rzc)
[![Docs](https://img.shields.io/badge/Docs-Online%20documentation-blue)](https://liuqiTan80.github.io/i18n-rust/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

> 🪞 **仓库镜像**：本项目在 GitHub（[liuqiTan80/i18n-rust](https://github.com/liuqiTan80/i18n-rust)）与 GitCode（[tan80/i18n-rust](https://gitcode.com/tan80/i18n-rust)）双平台同步维护。`rzc lang install` 默认优先使用 GitCode 源（国内访问更快），失败自动回退 GitHub。

# rzc —— 用母语编写真正的 Rust

> 🌍 **Write Rust in your native language.** · 10 languages · real Rust, real toolchain · graduate anytime with `rzc eject`

**rzc 是多语言 Rust 方言编译器**：你用母语写代码，rzc 实时翻译为标准 Rust 交给官方工具链编译运行，再把所有报错翻译成母语并附上教学提示。

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

**从这里开始**：🚀 [快速跑通](#快速开始) · 🌐 [在线文档站](https://liuqiTan80.github.io/i18n-rust/)（四语言；托管于 GitHub Pages，国内访问可能不稳定） · 📚 [飞书知识库](https://my.feishu.cn/wiki/space/7689728327082314704)（中文教程，国内免登录阅读） · 📖 [系统学习（中文教程）](#配套教程) · 🤝 [参与贡献](#参与贡献)

---

## 📦 安装

两种方式任选：**方式一 crates.io 一键安装**（推荐，适合绝大多数用户）；**方式二 源码编译**（需要最新开发版，或要修改 rzc 源码的开发者）。

**方式一：crates.io 一键安装（推荐）**——已在 crates.io 发布，一条命令装完全局可用：

```bash
cargo install rzc        # 从 crates.io 获取源码并在本机编译（首次约 1-3 分钟）
rzc --version            # 验证（显示版本号即成功）
rzc init 我的项目 && cd 我的项目 && rzc run src/main.zh
```

> 语言包已内置，无需任何额外配置。crate 页面：<https://crates.io/crates/rzc>；升级已装版本：`cargo install rzc --force`。尚未安装 Rust 工具链？见下方「第一步」。

**方式二：源码编译（开发者）**——获取最新开发版，或修改 rzc 后本地构建。**最短路径**（已装 Rust 工具链，4 条命令跑通第一个程序）：

```bash
git clone https://github.com/liuqiTan80/i18n-rust && cd i18n-rust
cargo build --release --workspace   # 约 1-3 分钟
cargo install --path crates/cli     # 让 rzc 全局可用（也可直接用 ./target/release/rzc）
rzc init 我的项目 && cd 我的项目 && rzc run src/main.zh
```

下方为方式二的完整分步说明（先决条件 → 编译 → 验证）；其中「第一步」两种方式通用。

### 先决条件一览

| 工具 | 用途 | 必需？ |
|---|---|---|
| **Rust 工具链**（rustc + cargo） | 编译 rzc 本体 | ✅ 必需 |
| **git** | 获取源码 | ✅ 必需（也可下载源码 zip） |
| **网络** | 首次编译时下载依赖 | ✅ 必需（首次） |
| **Node.js 18+ 与 npm** | 编译 VS Code 扩展 | 可选（仅 IDE 需要） |
| **rust-analyzer** | IDE 补全/诊断后端 | 可选（仅 IDE 需要） |

### 第一步：安装编译工具（Rust 工具链）

rustup 是官方推荐的安装方式，一条命令装好 rustc、cargo、rustup 全家桶。

**Linux / macOS**（在终端中执行）：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

安装完成后按提示运行 `source "$HOME/.cargo/env"`（或关掉终端重新打开）使命令生效。

**Windows**：下载 [rustup-init.exe](https://rustup.rs) 双击按向导安装（或在 PowerShell 中执行 `winget install --id Rustlang.Rustup`）。

**验证安装**（重开终端后）：

```bash
rustc --version   # 输出类似 rustc 1.98.0 即成功
cargo --version
```

> 💡 Linux 也可用发行版软件源安装（如 Debian/Ubuntu：`sudo apt install rustc cargo`），但版本通常较旧，推荐 rustup。

### 第二步：获取源码

```bash
git clone https://github.com/liuqiTan80/i18n-rust
cd i18n-rust
```

没有 git 可到仓库页面点 `Code → Download ZIP`，解压后进入目录。

### 第三步：编译

```bash
cargo build --release --workspace
```

首次编译需下载依赖并编译全部组件（rzc + 语言服务器），约 1-3 分钟。产物：

- `target/release/rzc` —— 命令行工具
- `target/release/i18n-rust-lsp` —— 语言服务器（VS Code 扩展后端）

验证：

```bash
./target/release/rzc --version
```

### 第四步（可选）：让 rzc 全局可用

```bash
cargo install --path crates/cli    # 本地编译并安装到 ~/.cargo/bin
```

之后任意目录可直接执行 `rzc`（也可手动复制 `target/release/rzc` 到 `~/.cargo/bin/`）。

### 第五步（可选）：安装 rust-analyzer（IDE 智能提示需要）

VS Code 的补全/诊断/悬停由 rust-analyzer 提供，联网执行一次：

```bash
rzc install toolchain --ra-only --force
```

官方 standalone rust-analyzer 安装到 `~/.rz/toolchain`，rzc 与 LSP 自动优先使用。

### 第六步（可选）：编译 VS Code 扩展

先决条件：Node.js 18+ 与 npm（[nodejs.org](https://nodejs.org) 下载安装）。

```bash
cd tools/vscode-extension
npm ci
npm run package    # 生成 i18n-rust-<版本>.vsix
```

在 VS Code 中「Install from VSIX…」安装生成的 `.vsix`，即获得语法高亮、补全、诊断、悬停、所有权可视化等全部功能；语言服务器由 `rzc install lsp` 提供（自动定位内置工具链）。

### 完整功能配置（一条命令各就位）

```bash
rzc install lsp          # 语言服务器（VS Code 补全/诊断/悬停后端）
rzc install toolchain    # 内置官方工具链（standalone rustc/cargo/rust-analyzer 到 ~/.rz/toolchain）
rzc doctor               # 查看工具链环境状态（内置 / PATH / 版本对比）
```

安装后 rzc 与 LSP 自动优先使用内置工具链，单文件项目直调 rustc，无需 cargo 索引。

### 给发布者：打包离线发布版（教学机房离线分发）

仓库提供一键打包脚本（本地编译，生成适合当前平台的发布包，供 Release / 网盘分发）：

| 平台 | 命令 | 产物 |
|---|---|---|
| Linux / macOS | `./release-offline.sh` | `release/rzc-<版本>-linux/macos-<架构>.tar.gz` |
| Windows（PowerShell） | `.\release-offline.ps1` | `release/rzc-<版本>-windows-x86_64.zip` |

前置条件：已按上面第三步编译 release 产物；联网执行一次 `rzc install toolchain --ra-only --force` 下载当前平台的 rust-analyzer（打包脚本会把它复制进包内）。

脚本自动识别平台（Linux / Darwin / Windows），包内含 rzc、i18n-rust-lsp、rust-analyzer、11 个内置语言包（10 种自然语言 + `en` 恒等包）与教程，解压即用（详细步骤见教程附录 D.5）。

### 环境变量（可选，一般无需配置）

| 变量 | 作用 |
|---|---|
| `RZ_LANG_DIR` | 指定语言包目录（默认使用内置语言包） |
| `RUST_ANALYZER_PATH` | 指定 rust-analyzer 路径（自动检测失败时） |

VS Code 设置 `i18n-rust.serverPath` 可显式指定 LSP 二进制路径（自动检测失败时使用）。

### 升级工具链中的某个软件

| 场景 | 命令 |
|---|---|
| 升级 rustc/cargo（如 1.98 → 1.99） | `rzc install toolchain --version 1.99.0 --force` |
| 仅升级 rust-analyzer（免重下 300MB） | `rzc install toolchain --ra-tag <日期tag> --ra-only --force` |
| 查看当前版本与状态 | `rzc doctor` |

### 更换编辑器（如改用 VSCodium / Cursor 等 VS Code 系）

i18n-rust 扩展（.vsix）兼容所有 VS Code 系编辑器；rzc、语言服务器与工具链均与编辑器无关：

1. 新编辑器 → 扩展 → 「Install from VSIX」→ 选 `i18n-rust-<版本>.vsix`；
2. 把组件接入系统标准位置（用编译出的 rzc 执行）：
   ```bash
   rzc install lsp        # 语言服务器 → ~/.cargo/bin
   rzc install toolchain  # 内置工具链 → ~/.rz/toolchain（联网安装；或从发布者分发的离线包复制）
   ```
3. 打开 `.zh` 文件即用（扩展自动定位语言服务器与工具链；也可用环境变量 RUST_ANALYZER_PATH 或设置 i18n-rust.serverPath 显式指定）。

## 🚀快速开始

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
| `rzc transpile <文件>`             | 只转译不编译，标准 Rust 输出到屏幕 |
| `rzc cheat <语言>`                 | 母语 ↔ Rust 映射速查表（`--markdown` 可嵌入文档） |
| `rzc add <库名>[@版本]`            | 添加第三方依赖（封装 `cargo add`，附母语映射提示） |
| `rzc doctor`                       | 诊断工具链环境（内置 / PATH / 版本对比） |
| `rzc lang list`                    | 列出已安装语言包 |
| `rzc lang install <码/目录>`       | 安装语言包（远程仓库或本地目录） |
| `rzc lang search [关键词]`         | 检索可安装的远程语言包 |
| `rzc lang remove <码>`             | 删除用户安装的语言包 |
| `rzc mapping auto <crate名> [--target-version 版本]` | 自动生成第三方库的母语映射（AI/规则），可锁定生成基准版本 |
| `rzc mapping check [目标]`         | 校验映射质量（重复键/关键字碰撞/跨文件冲突/必备节完整性） |
| `rzc mapping coverage [--lang 语言]` | 用真实源码检验语言包覆盖度，列出缺失的母语映射（仓库根运行） |
| `rzc mapping scaffold <源> <目标>` | 生成新语言的翻译骨架，`--provider deepseek` 可 AI 自动翻译 |
| `rzc crate search [关键词]` | 检索社区共享的第三方库母语映射（注册中心） |
| `rzc crate install <库> --lang <语言>` | 从注册中心安装单个第三方库映射到全局语言包 |
| `rzc crate list` | 列出已安装的社区映射 |
| `rzc crate remove <库> --lang <语言>` | 移除已安装的社区映射 |
| `rzc crate update` | 按清单重新拉取所有已安装映射（获取更新） |
| `rzc crate publish <库> --lang <语言>` | 把本地自译映射发布到注册中心（先经质量门禁） |
| `rzc install <lsp\|toolchain>` | 安装配套组件（语言服务器 / 内置官方工具链） |

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

Rust 本身以英文书写，因此英语不作为教学方言（恒等映射无教学价值）；语言包目录中另有一个 `en` 恒等包（扩展名 `.en`），供需要恒等映射的场景与英文界面文案使用。上表 10 种自然语言按文件扩展名自动匹配，同一项目可混用。

### 教学级诊断
- **错误码 + 消息双轨翻译**：覆盖 rustc 错误码、无码 lint 警告、help 短语
- **类型名本地化**：`std::fmt::Display` → `标准库::格式化::可显示`
- **💡 教学提示**：每条错误附带下一步建议；所有权错误附带 📌 移动/借用叙事
- **依赖引导**：识别未声明的第三方库，提示 `rzc add <crate>`

### 完整 IDE 体验（VS Code / Qoder 扩展）
语法高亮、智能补全、悬停文档、定义跳转、引用查找、重命名、代码格式化、一键运行/检查、全角标点自动转半角、AI 辅助翻译。

### 第三方库母语化
`rzc mapping auto` 从已安装 crate 提取公开 API，AI 生成母语名；生产映射用 `--target-version` 锁定生成基准版本（写入文件头）；社区共建映射经 `rzc mapping check` 质量门禁。

### 为什么不是"玩具语言"

| | 常见"母语编程"玩具方案 | rzc |
|---|---|---|
| 代码形态 | 自创语法或伪代码 | 与标准 Rust 完全同构，只有标识符母语化 |
| 编译运行 | 自带解释器 / 只转不跑 | 官方 rustc/cargo，真实编译与运行 |
| 依赖生态 | 封闭或裁剪 | 全量 crates.io（`rzc add` + 母语映射） |
| 报错体验 | 英文原样或自制提示 | 母语翻译 + 错误码 + 教学提示 |
| 退出成本 | 中途放弃要重写 | `rzc eject` 一键导出标准 Rust |

---

## 📖配套教程

面向零基础学习者的完整中文教程：**26 章 + 总术语表 + 5 个附录**，见 [tutorials/](tutorials/)。
从《你好世界》到所有权、闭包、异步、宏，直至综合实战——所有示例全部用中文 Rust 书写。

🌐 **在线阅读**：[在线文档站](https://liuqiTan80.github.io/i18n-rust/)（mdBook 四语言 + 顶栏切换；本地预览 `make site-serve`） · 📚 **国内推荐**：[飞书知识库](https://my.feishu.cn/wiki/space/7689728327082314704)（33 篇中文教程，公网免登录直接阅读）。站点托管于 GitHub Pages，**国内网络可能无法稳定访问**（此时用上方飞书入口）；也可直接在仓库内阅读 [tutorials/](tutorials/)（GitCode 页内可正常浏览），或使用离线发布包随附的 `教程/` 目录。

**多语言译本进行中**：英文（开篇、第 1–13 章、附录 D，15/33）、日本語、Русский 持续滚动；
进度与下一批见 [translation-status.md](docs/translation-status.md)。

> 教程质量由 CI 自动门禁守护（[tools/verify-tutorials.py](tools/verify-tutorials.py)）：
> 每个代码块须可编译，错误示例须报出标注的预期错误码（`// 预期错误: EXXXX`），
> 修改教程后本地验证：`make tutorials`（中文）或 `make tutorials-all`（英日俄）。

> 常见问题与学习路线见[附录E](tutorials/附录E：常见问题、迁移指南与学习路线.md)；欢迎共同翻译**教程**与**映射表**到其他语言，见下文“参与贡献”。

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
| `crates/engine/lang-packs` | 11 个内置语言包（10 种自然语言 + `en` 恒等包：关键字/标准库/模块路径/错误翻译/界面文案） |
| `tools/vscode-extension`   | VS Code / Qoder 扩展 |
| `tools`                    | 门禁与构建脚本：教程验证、基准回归、文档站装配等（见 [tools/README.md](tools/README.md)） |
| `tutorials`                | 26 章中文教程与附录（en/ja/ru 译本进行中） |
| `book`                     | 文档站装配源（mdBook，见 [book/README.md](book/README.md)） |
| `docs`                     | 参考文档、开发文档与[项目地图](docs/project-map.md)；运营与路线图见 [docs/strategy/](docs/strategy/README.md) |

**设计原则**：引擎不硬编码任何具体语言；新增一门自然语言 = 新增一个语言包目录，零代码改动（构建脚本自动嵌入）。想把这套范式移植到其他编程语言（如中文 Python），参见 [方言编程框架生成蓝图](docs/dialect-framework-blueprint.md)。

---

## 🤝参与贡献

- **第一次参与先看**：[项目地图（维护者手册）](docs/project-map.md)——"我想改 X 去哪里、改完怎么验证"一页速查；开发环境与提交规范见 [CONTRIBUTING.md](CONTRIBUTING.md)
- **应用开发者缺词条指引**：[docs/missing-mapping-guide.md](docs/missing-mapping-guide.md)（写应用时补词条 / 定制映射，面向新手）
- **新增语言包**：[docs/contributing-lang-pack.md](docs/contributing-lang-pack.md)（含 `rzc mapping scaffold` AI 翻译流程）
- **第三方库映射**：[docs/third-party-mapping.md](docs/third-party-mapping.md)
- **第三方库共享注册中心**：[docs/third-party-registry.md](docs/third-party-registry.md)（社区上传/下载自译映射）
- **翻译教程**：以 `tutorials/` 为源，保持章节结构一致；进度面板见 [translation-status.md](docs/translation-status.md)
- 提交前请确保 `make gate` 全绿（等价 CI 测试门禁）

---

## ⭐ 支持项目

如果 rzc 对你有帮助或有启发，欢迎给一个 Star；也欢迎把教程翻译成你的母语——这是对项目最好的支持。

---

## 📄 许可证

[MIT](LICENSE) © tan80

