# Changelog

## 0.5.6

### 新增
- `rzc install lsp` 版本校验：已安装的语言服务器与 rzc 版本不一致或无法确认时，提示执行 `--force` 重装；`i18n-rust-lsp` 新增 `--version` 输出（10 语言提示文案）。

### 变更
- 移除英文语言包：Rust 本身以英文书写，恒等映射无教学价值，内置语言由 11 种调整为 10 种（`.en` 方言文件不再支持）。

### 内部
- cargo fmt 全量修复 + clippy 清零（CI 的 fmt 检查长期失效已修复）；CI Node 20→22。

## 0.5.5

### 修复
- `rzc init` 生成的 `rust-toolchain.toml` 不再硬编码 1.85：改为动态探测本机当前生效工具链版本（components 含 rust-analyzer/rust-src），避免 rust-analyzer 报"工具链过于陈旧"导致方言文件语义着色（变量颜色）失效。
- 测试环境变量竞态修复：依赖/修改 `RZ_LANG_DIR`、`LANG` 等环境变量的测试统一持进程级互斥锁，消除并行测试互相污染。

### 文档
- 重构 README：新增功能特性、命令速查、项目结构与工作原理等章节，补充 CI/crates.io/许可证徽章。
- 新增 `docs/dialect-framework-blueprint.md`（方言编程框架生成蓝图）。

## 0.5.3

### 修复
- LSP 在 Windows 上找不到 rust-analyzer 导致启动即退出、反复重启：查找逻辑跨平台化（PATH 扫描替代 Unix 专属 which，支持 PATHEXT/.exe；主目录定位回退 USERPROFILE），并补充 ~/.cargo/bin 候选。

## 0.5.2

### 新增
- `rzc install lsp` 一键安装语言服务器（VS Code 扩展的补全/诊断后端）：优先从离线包同目录免网络复制，否则从 crates.io 安装与 rzc 版本严格一致的版本。

### 改进
- LSP 缺失时的错误提示改为引导安装：`rzc install lsp`（或 `cargo install i18n-rust-lsp`）。

## 0.4.0

### 新增
- 对齐引擎全部 11 种方言语言：中文、English、日本語、Deutsch、Español、Français、Português、Русский、한국어、हिन्दी、العربية（语法高亮文件由 `scripts/gen-grammars.mjs` 从语言包自动生成）。
- 新增配置 `i18n-rust.rzcPath`：指定 rzc 命令行工具路径。
- AI 对话支持进度通知中取消；新会话自动中止上一个会话。
- 扩展日志输出通道「i18n-rust 日志」。
- 新增 `node --test` 单元测试（shell 引用、插入位置计算、TOML 解析、提示词生成）。

### 修复
- AI 系统提示词语言包定位失效（目录英文化后仍按显示名拼路径，导致永远回退英文提示词）：现按语言代码目录读取，并从 `lang_info.toml` 读取显示名。
- 全角符号自动转换在粘贴含换行的多行文本时替换位置跨行错位：改为逐字符换行感知的位置计算。
- 终端命令拼接存在引号/反引号注入风险：所有 `sendText` 参数改为平台感知安全引用；`eject` 改用无 shell 的 `execFile`。
- LSP 二进制查找仅支持 Unix `which` 且只看 `target/debug`：改为跨平台 PATH 扫描（Windows 支持 PATHEXT/.exe）并补充 `target/release`；找不到二进制时给出明确错误与设置入口。
- Run/Check 每次新建终端导致终端刷屏：改为复用单一 `i18n-rust` 终端。

### 变更
- AI 密钥从明文设置迁移到 VS Code SecretStorage（激活时自动一次性迁移，原设置项标记弃用）。
- 使用 esbuild 打包扩展（vsix 不再捆绑 node_modules，体积显著减小）。
- 语言包选择器列出全部 11 种语言。

## 0.3.1

- 右键菜单覆盖 rust-zh/rust-en/rust-de 三种方言并补充格式化命令。
- AI 提供商抽象层（OpenAI/DeepSeek/通义千问/智谱 GLM/Ollama/自定义）。
- 所有权错误可视化装饰器与全角符号自动转换。
