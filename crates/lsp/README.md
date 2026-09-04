# i18n-rust-lsp

多语言 Rust 教学方言的语言服务器：将母语 Rust 源码（`.zh`/`.ja`/`.ko` 等）
代理给官方 rust-analyzer，并把响应中的位置与诊断双向还原/翻译为母语。

通常不直接安装本 crate——由 `rzc install lsp` 自动定位部署；
VS Code 扩展（i18n-rust）会自动发现语言服务器。

## 工作原理

1. 收到方言文档后转译为标准 Rust，写入按用户隔离的虚拟项目目录，
   交给 rust-analyzer 分析；
2. rust-analyzer 的响应（诊断/补全/悬停/定义/重命名等）中的
   URI、行列坐标经翻译缓存回译到方言源文件；
3. 诊断消息中文化（与 `rzc` 同源的错误消息表）、所有权错误叙事化、
   补全反向翻译与语言过滤、教学快捷修复注入。

## 架构

- `translation_cache`：文档转译缓存与行列映射（`RwLock` 分桶）
- `response_map`：各 LSP 请求类型的响应映射与诊断翻译
- `analyzer`：rust-analyzer 子进程生命周期管理与 LSP 双向转发

项目主页与完整文档见
[仓库根 README](https://github.com/liuqiTan80/i18n-rust)。

## 许可证

MIT
