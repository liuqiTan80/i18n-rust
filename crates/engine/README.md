# i18n-rust-engine

多语言 Rust 教学方言核心引擎：词法转译、映射管理、诊断翻译、增量缓存。

`rzc` 命令行工具与 `i18n-rust-lsp` 语言服务器共同依赖的本库，也可独立嵌入
你自己的教学工具或编辑器插件：

```rust,ignore
use i18n_rust_engine::mapping_manager::MappingManager;

// 加载中文语言包（关键字/模块路径/别名/错误消息翻译）
let manager = MappingManager::load_builtin("zh");

// 母语 Rust → 标准 Rust（逐 token 原地替换，行号恒等）
let english = i18n_rust_engine::transpile_pipeline(
    "函数 主函数() { 打印行!(\"你好\"); }",
    &manager,
)
.output;
```

## 核心能力

- **词法转译**：基于 `rustc_lexer` 的逐 token 替换，不依赖正则，行号恒等、
  列偏移可回放（`column_map`）
- **映射管理**：语言包（关键字/标准库/别名/派生特征/错误消息）多表加载、
  冲突检测、语境指纹缓存失效
- **诊断翻译**：rustc JSON 诊断 → 母语教学诊断（错误码表 + 消息表 +
  类型本地化 + 所有权叙事 + 引用层数提示）
- **Unicode 安全检查**：全角标点检测/修复、零宽与双向控制字符告警

## 设计原则

引擎不硬编码任何具体语言：新增一门自然语言 = 新增一个语言包目录，
零代码改动。项目主页与完整文档见
[仓库根 README](https://github.com/liuqiTan80/i18n-rust)。

## 许可证

MIT
