# rzc — 多语言 Rust 教学方言编译器

用你的母语编写 Rust 程序，rzc 自动翻译为标准 Rust 并编译运行——学编程，不背英语。

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

## 安装

一条命令，装完即可全局使用：

```bash
cargo install rzc
```

> 需要已安装 Rust 工具链（rustup 的 stable 即可）。语言包已内置，无需任何额外配置。

## 快速开始

```bash
rzc init 我的项目
cd 我的项目
rzc run src/main.zh
```

`rzc init` 生成完整可运行的项目骨架（`Cargo.toml` + `src/main.zh`），直接运行即可。

项目语言缺省跟随系统语言（`LC_ALL` / `LC_MESSAGES` / `LANG`）；系统语言没有对应语言包时，
终端里会列出语言菜单让你选（回车默认中文），CI / 管道场景则提示已回退中文。
显式指定用 `rzc init 我的项目 --lang ja`；指定的语言没有语言包时立即报错并列出可用代码。

## 常用命令

| 命令 | 说明 |
|------|------|
| `rzc init <项目名>` | 创建新项目 |
| `rzc run <文件>` | 翻译并运行方言源码（须为项目默认入口 `src/main`） |
| `rzc check <文件>` | 类型检查，输出中文教学诊断 |
| `rzc eject <文件或目录>` | 导出为标准 Rust 代码（目录递归批量导出；混合语言目录按扩展名分组） |
| `rzc lang` | 语言包管理（list / install / remove） |
| `rzc install lsp` | 安装语言服务器 `i18n-rust-lsp`（VS Code 扩展的补全/诊断后端，按当前 rzc 版本精确安装） |
| `rzc doctor` | 环境体检：rustc / cargo / rust-analyzer / 语言服务器是否就位、版本与 rzc 是否一致；在项目目录里还自检「Cargo 读不读得到入口产物」，缺就点名该跑的 `rzc check` |
| `rzc mapping auto <crate>` | 自动生成第三方库映射 |

## AI 映射生成的服务商配置

`rzc mapping auto <crate>` 与 `rzc mapping scaffold <源> <目标> --provider <ai>`
默认走 DeepSeek，`--provider` 也可以写 `openai` / `compatible`——三者是同一条
OpenAI 兼容链路，区别只在环境变量里填的端点与模型。

| 环境变量 | 作用 | 未设置时 |
|------|------|------|
| `RZ_AI_BASE_URL` | chat/completions 的基础 URL（带 `/v1` 或不带后缀都能跑） | `https://api.deepseek.com` |
| `RZ_AI_API_KEY` | Bearer 密钥 | 默认端点回退 `DEEPSEEK_API_KEY`；自定义端点还可用 `OPENAI_API_KEY` |
| `RZ_AI_MODEL` | 请求体的 model 字段 | `deepseek-chat` |

```bash
# OpenAI
export RZ_AI_BASE_URL=https://api.openai.com/v1 RZ_AI_API_KEY=sk-... RZ_AI_MODEL=gpt-4o-mini
# 本地 Ollama（不校验密钥，给个占位值即可通过前置检查）
export RZ_AI_BASE_URL=http://127.0.0.1:11434/v1 RZ_AI_API_KEY=ollama RZ_AI_MODEL=qwen2.5-coder
# 通义千问兼容模式（密钥仍用 RZ_AI_API_KEY 提供）
export RZ_AI_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode/v1 RZ_AI_MODEL=qwen-plus
```

两条行为约定：

- **密钥不跨站发送**：端点保持默认（DeepSeek）时只读 `RZ_AI_API_KEY` /
  `DEEPSEEK_API_KEY`，即使机器上已有 `OPENAI_API_KEY` 也不会被发到 DeepSeek；
  只有显式改了 `RZ_AI_BASE_URL` 才启用该回退。
- **参数问题在动手前报**：`--provider` 拼写未知立即报错；scaffold 的 AI 模式在
  写第一个文件前就检查密钥，失败不会留下一堆半成品映射。`mapping auto` 不同，
  AI 调用失败时回退规则模式（映射仍生成，解释为空）。

## 功能特性

- **母语编程**：用中文关键字（`函数`、`让`、`如果`、`返回`…）编写完整 Rust 程序
- **多语言原生**：内置 10 种语言包（中/德/日/俄/西/法/葡/韩/阿/印地），扩展名自动匹配
- **本地化诊断**：`rzc check` 把 rustc 错误翻译为对应语言，附带 💡 教学提示
- **所有权可视化**：配合 VS Code 扩展，颜色高亮变量的移动与再使用
- **完整 LSP 支持**：补全、悬停、定义跳转、引用查找、重命名
- **渐进过渡**：`rzc eject` 一键导出标准 Rust 代码

## 许可证

MIT
