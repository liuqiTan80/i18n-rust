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
- `response_map`：各 LSP 请求类型的响应映射与诊断翻译（含 RA 噪声过滤）
- `mirror_check`：真实项目镜像 `cargo check`（权威诊断，见下节）
- `analyzer`：rust-analyzer 子进程生命周期管理与 LSP 双向转发

## 诊断双通道

母语源码无法被 rust-analyzer / rustc 直接分析，诊断必须先转译为标准 Rust、
再经检查、最后把坐标与文案回译回方言。本 crate 用**两条互补通路**产出诊断，
二者合并发布：

### 1. 虚拟项目通路（实时，由 rust-analyzer 驱动）

把打开的方言文档转译为 `.rs` 写入按用户隔离的虚拟项目目录，并将
rust-analyzer 的工作区根指向该目录（不透传客户端 `rootUri`，否则 RA 会全量
分析整个真实工作区、发布海量诊断并阻塞消息通道）。此通路提供**秒级**的语法 /
类型 / 补全 / 悬停等实时反馈，但只聚合方言模块、无第三方依赖，与真实工程结构
偏差大——因此**编译期诊断（`check`）在此不可达**：RA 的 `checkOnSave` 在虚拟
项目上常驻却不发布结果。

由于虚拟项目结构失真，RA 会产生成类**固定误报**（如把方言模块名报成「找不到
crate/模块」「main 函数从未使用」等）。`response_map` 用 5 个按消息文本判定的
噪声过滤器（`is_main_fn_hint` / `is_missing_dependency_noise` /
`is_unopened_module_reference` / `is_missing_project_dependency` /
`is_missing_include_asset`）抑制它们。误过滤器是**双刃剑**：漏过滤 → 噪声淹没
用户，误杀 → 真错被静默吞掉。故 `diag_text.rs` 的测试模块对每个过滤器都钉住
「正例（固有误报应滤）× 反例（真错应留）」双向断言，RA 消息文案漂移时第一时间
在此暴露。

### 2. 镜像项目通路（权威，代理自跑 `cargo check`）

所有权可视化等依赖 `E0382` 等 `check` 级诊断，必须由真实编译产出。`mirror_check`
把整个项目树复制为「镜像」（排除 `target`/`.git`/`node_modules`/`dist`），按与
`rzc` 同源的引擎规则把 `src/` 下方言文件转译为 `.rs` 产物（入口写 `main.rs`、
非 ASCII 模块声明补 `#[path]` 注解），再执行 `cargo check --offline
--message-format=json`，复用真实项目 `target/` 的依赖产物。rustc JSON 诊断经
行/列回译（含 `#[path]` 注解行的行号换算与字符列 → UTF-16 列换算）还原为方言
坐标、翻译文案后发布。

关键策略：

- **触发时机**：`didSave` 先跑虚拟草稿（快速反馈）再跑镜像（权威）；`didOpen`
  只跑镜像（镜像已含全部信息，草稿无必要）。
- **并发合并**：`check_running` 保证同时只有一个 check；进行中的新保存置
  `check_pending`，当前 check 结束后补跑一次，合并连续保存的中间态，最终诊断
  不停留在旧版本。
- **超时**：轮询上限 600s（首轮含真实依赖编译，可能数分钟），超时强杀并回退。
- **回退**：镜像不可用（无 `Cargo.toml` 的单文件教学、复制失败、`cargo` 无法
  启动、异常退出且无可归位诊断）时返回 `false`，调用方回退虚拟检查。

两条通路的诊断按**同 `code` 且同起始行**去重合并（镜像结果不覆盖 RA 的语法 /
类型实时诊断），再叠加教学 lint，统一发布。

## 性能基准

LSP 热路径有 criterion 基准与入库回归门禁（覆盖文档同步
`update_document` 与响应映射 `reverse_transpile`，见 `benches/hot_paths.rs`）：

```bash
cargo bench -p i18n-rust-lsp               # 只出结果（-- --quick 快速模式）
tools/bench-check.sh --only lsp            # 对比入库基线（本地阈值 30%）
tools/bench-check.sh --update --only lsp   # 刷新基线（随提交）
```

项目主页与完整文档见
[仓库根 README](https://github.com/liuqiTan80/i18n-rust)。

## 许可证

MIT
