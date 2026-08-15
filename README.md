<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc：多语言 Rust 教学方言编译器

用你的母语编写 Rust 程序，由 rzc 自动翻译为标准 Rust 并编译运行——编程教育回归逻辑思维，而非英语记忆。

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

## ✨ 功能特性

- **母语编程**：用中文关键字（`函数`、`让`、`如果`、`返回`…）编写完整 Rust 程序
- **多语言原生**：架构天生支持任意自然语言，内置 11 种语言包（中文、英文、德文、日文、俄文、西班牙文、法文、葡萄牙文、韩文、阿拉伯文、印地文），可远程安装其他语言
- **扩展名自动识别**：`.zh`、`.en`、`.de` 等扩展名自动匹配对应语言包，无需手动指定
- **本地化错误诊断**：解析 rustc 输出翻译为对应语言（中文/英文/德文），附带 💡 教学提示；所有权错误（E0382/E0502/E0507）输出叙事化提示，并附结构化 JSON 供可视化
- **所有权可视化**：配合 VS Code 扩展，用颜色高亮变量的移动（黄）、再次使用（红）与生命周期（绿）
- **完整 LSP 支持**：补全、悬停、定义跳转、引用查找、重命名、代码操作、文档符号、格式化
- **宏自动补全**：宏调用可省略感叹号（`打印行(...)`），转译时自动补全
- **渐进过渡**：`eject` 一键导出标准 Rust 代码，平滑迁移到原生生态
- **完整教程**：24 章零基础教程 + 4 个附录，从零基础到综合实战

## 📦 安装

### 通过 crates.io 安装（推荐）

```bash
cargo install rzc
```

安装后可全局使用 `rzc` 命令。需要 Rust 工具链（rustup 安装的 stable 即可）。

### 从源码构建

```bash
# 中国镜像
git clone https://gitcode.com/tan80/zrRust.git
# 国际
git clone https://github.com/liuqiTan80/i18n-rust.git

cd zrRust 或 i18n-rust
cargo build --release --workspace
# 二进制位于 target/release/rzc
```

## 🚀 快速开始

```bash
rzc init 我的项目
cd 我的项目
rzc run src/main.zh
```

`rzc init` 会生成完整可运行的项目骨架（Cargo.toml + `src/main.zh`），内置中文语言包无需任何额外配置。

## 🛠️ 常用命令

| 命令 | 说明 |
|------|------|
| `rzc init <项目名>` | 创建新项目（内置中文语言包） |
| `rzc run <文件>` | 翻译并运行 `.zh` 源码 |
| `rzc check <文件>` | 类型检查，输出中文教学诊断 |
| `rzc eject <文件>` | 导出为标准 `.rs` 代码 |
| `rzc lang list` | 列出已安装语言包 |
| `rzc lang install <来源>` | 安装语言包（本地目录或远程语言代码） |
| `rzc lang remove <语言代码>` | 卸载用户安装的语言包 |
| `rzc mapping auto <crate名>` | 自动生成第三方库映射 |

## 🌍 语言包管理

rzc 内置 **11 种语言包**（中文、英文、德文、日文、俄文、西班牙文、法文、葡萄牙文、韩文、阿拉伯文、印地文），开箱即用（中文包含 98 关键字、538 标准库映射；
每种语言包均含 53 个错误码翻译与本地化教学提示）。

- **扩展名自动识别**：`main.zh`/`main.de` 等 11 种扩展名自动加载对应内置语言包；错误提示随文件语言本地化
- **远程安装**：`rzc lang install <语言代码>` 默认从 GitCode 下载，失败自动回退 GitHub（全部 11 种语言包已内置，无需安装）
- **自定义源**：设置环境变量 `RZ_LANG_REPO` 使用指定仓库地址
- **本地安装**：`rzc lang install ./我的语言包` 直接复制到全局目录
- **加载优先级**：`--语言包` 参数 > 项目内 `lang-packs/<代码>/` > 全局用户目录 > 内置语言包

语言包目录结构：

```
lang-packs/zh/          # 中文（en/、de/ 结构相同）
├── lang_info.toml     # 元数据
├── keywords.toml      # 关键字映射
├── stdlib.toml        # 标准库标识符别名
├── errors.toml        # 错误码翻译 + 教学提示
├── module_paths.toml  # 模块路径映射
└── crates/            # 第三方库映射
```

## 🤖 自动映射生成

`rzc mapping auto` 可从已安装的 crate 自动提取公开 API 并生成映射文件：

```bash
rzc mapping auto anyhow                 # 默认 AI 模式（需 DEEPSEEK_API_KEY）
rzc mapping auto serde --provider 规则  # 离线规则模式
```

| 选项 | 说明 |
|------|------|
| `--lang <语言>` | 目标语言（默认按系统语言检测） |
| `--provider <服务商>` | `deepseek`（默认）或 `规则`（离线） |
| `--output <路径>` | 输出文件路径 |

## 🎨 所有权可视化

配合 VS Code 扩展，rzc 将所有权错误渲染为颜色装饰器：

| 颜色 | 含义 |
|------|------|
| 🟡 黄色背景 | 移动/借用发生的位置 |
| 🔴 红色背景 | 再次使用变量的位置 |
| 🟢 浅绿背景 | 生命周期区间 |

## 💻 VS Code 扩展

支持中文（`.zh`）/英文（`.en`）/德文（`.de`）等 11 种语言 Rust 语法高亮、智能补全、错误提示、一键运行/检查/导出与所有权可视化。在 VS Code 扩展市场搜索 `i18n-rust` 安装。

## 📖 配套教程

面向零基础新手的完整中文教程，共 24 章 + 4 个附录：

| 阶段 | 章节 |
|------|------|
| **基础入门** | [第1章 你好世界](tutorials/第一章：你好世界.md) · [第2章 变量与类型](tutorials/第二章：变量与类型.md) · [第3章 复合类型](tutorials/第三章：复合类型.md) · [第4章 控制流](tutorials/第四章：控制流.md) · [第5章 函数与方法](tutorials/第五章：函数与方法.md) |
| **核心概念** | [第6章 所有权](tutorials/第六章：所有权.md) · [第7章 引用与借用](tutorials/第七章：引用与借用.md) · [第8章 字符串与文本](tutorials/第八章：字符串与文本.md) · [第9章 结构体](tutorials/第九章：结构体.md) · [第10章 枚举与模式匹配](tutorials/第十章：枚举与模式匹配.md) |
| **泛型与抽象** | [第11章 泛型](tutorials/第十一章：泛型.md) · [第12章 特征](tutorials/第十二章：特征.md) · [第13章 生命周期](tutorials/第十三章：生命周期.md) · [第14章 集合类型](tutorials/第十四章：集合类型.md) |
| **错误与模块** | [第15章 错误处理](tutorials/第十五章：错误处理.md) · [第16章 模块系统](tutorials/第十六章：模块系统.md) · [第17章 包管理与构建](tutorials/第十七章：包管理与构建.md) |
| **进阶实战** | [第18章 智能指针](tutorials/第十八章：智能指针.md) · [第19章 并发与多线程](tutorials/第十九章：并发与多线程.md) · [第20章 测试](tutorials/第二十章：测试.md) |
| **高级特性** | [第21章 闭包与迭代器](tutorials/第二十一章：闭包与迭代器.md) · [第22章 宏与元编程](tutorials/第二十二章：宏与元编程.md) · [第23章 异步编程](tutorials/第二十三章：异步编程.md) |
| **综合实战** | [第24章 命令行计算器](tutorials/第二十四章：综合实战.md) |
| **附录** | [A 映射表参考](tutorials/附录A：映射表参考.md) · [B 术语表](tutorials/附录B：术语表.md) · [C 迁移指南](tutorials/附录C：迁移指南.md) · [D 常见问题与学习路线](tutorials/附录D：常见问题与学习路线.md) |

## ❓ 常见问题

**Q：为什么使用宏不需要加感叹号？**
为了让初学者少记一个符号，教学方言允许省略宏调用后的 `!`。转译器会自动补全，导出为标准 Rust 时恢复原样。

**Q：可以用中文变量名和函数名吗？**
可以。Rust 支持 Unicode 标识符，`数量`、`主函数` 都是合法标识符。

**Q：编译错误提示是英文怎么办？**
使用 `rzc check`，它会将 rustc 错误翻译并附带教学提示。翻译语言与源文件语言一致：
`.zh` 文件输出中文提示，`.de`/`.ja`/`.ru` 等其他语言文件输出对应语言提示（共 11 种）。

**Q：如何安装其他语言包？**
`rzc lang install ./语言包目录`（本地）或 `rzc lang install <语言代码>`（远程）。
zh/en/de 已内置，无需安装。

**Q：学完之后如何迁移到标准 Rust？**
`rzc eject src/main.zh` 生成标准 `src/main.rs`，过渡平滑。

## 🤝 贡献指南

中国：欢迎通过 [GitHub Issues](https://gitcode.com/tan80/zrRust/issues) 提交反馈、通过 Pull Request 贡献代码。
国际：欢迎通过 [GitHub Issues](https://github.com/liuqiTan80/i18n-rust/issues) 提交反馈、通过 Pull Request 贡献代码。
- **代码贡献**：核心引擎（`crates/engine/`）、CLI（`crates/cli/`）、LSP 代理（`crates/lsp/`）、VS Code 扩展（`tools/vscode-extension/`）
- **语言包贡献**：新增语言包目录 + `lang_info.toml`，欢迎提交到语言包仓库
- **教程贡献**：`tutorials/` 目录下的章节与附录

## 📄 许可证

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
