# Changelog

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
