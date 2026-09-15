# 更新日志（Changelog）

所有显著变更记录于此。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增
- zh 语言包：stdlib.toml 标识符节补「文件系统」→「fs」，让函数体内直接调用完整路径
  （如 `文件系统::递归创建目录` → `fs::create_dir_all`）可转译（模块路径节仅 use 语句
  生效）；来自 xiaozs 实战验证
- zh 语言包：数据库.toml 的 rusqlite 段补充「SQLite」→「rusqlite」标识符映射，
  支撑函数体内以 `SQLite::xxx` 限定路径调用（模块路径节仅在 use 语句生效）；
  来自 xiaozs 单机版（SQLite 本地文件存储）实战验证
- zh 语言包补充与修正约百条映射（stdlib、chrono、reqwest、rusqlite、serde_json、salvo、
tauri 及新建 mysql），均来自 xiaozs 实战项目验证；含「阻塞」→「阻塞调用」、
「启动」→「发射」等冲突避让改名
- zh 语言包：salvo 补充跨域装配词条（「跨域」子模块、「跨域策略」Cors、「宽松」
permissive、「转为处理器」into_handler、「装配」hoop），支撑浏览器直连模式的
CORS 中间件；均来自 xiaozs 实战验证
- zh 语言包补齐 rand / serde / tokio 词条（xiaozs 词表回馈收尾）：
工具.toml 补「线程随机」→`thread_rng`（同义）、「随机生成」→`gen`；
序列化.toml 补「序列化器」→`Serializer`、「反序列化器」→`Deserializer`；
异步.toml 补「异步任务」→`spawn`（同义）
- 文档 `docs/missing-mapping-guide.md`：应用开发者缺词条指引（四级来源、项目语言包定制、
常见坑）；contributing-lang-pack.md / third-party-mapping.md / README 增加指路链接

## [0.7.2] - 2026-09-13

### 新增
- VS Code 扩展新增「离线更新提醒」：每天自动检查 GitHub Releases 上的新版本并弹窗提示（可用 `i18n-rust.checkUpdates` 关闭），离线安装（vsix）的用户也能及时获知版本更新

## [0.7.1] - 2026-09-13

### 新增
- `rzc cheat [lang] [--markdown]`：母语 ↔ Rust 映射速查表（关键字/模块路径/别名/派生特征），
  支持 Markdown 输出便于嵌入教程与 README；恒等映射语言（如英文包）提示无需速查
- crates/cli：第三方库共享注册中心 `rzc crate`（search/install/list/remove/update/publish），
  各语言包的 serde/tokio 等第三方库映射跨语言共享发布
- engine：`LoadTarget::ErrorMessages` 结构化错误变体（errors.toml 加载层）
- CLI 临时路径统一安全构建 `temp_guard`（用户隔离 + 符号链接校验，对齐 LSP 防护水位）

### 变更
- errors.toml 加载从 `Result<_, String>` 迁移到结构化 `LoadError`（Display 输出不变）
- CLI 主线程 Mutex 中毒锁改为恢复式处理（后台线程 panic 不再连锁崩溃）
- 仓库元数据统一：三个 crate 的 `repository` 统一为 GitHub 规范地址

### 重构
- 大文件模块化拆分（外部 API 不变）：`mapping_gen`（2475 行 → 6 子模块）、
  `diagnostic`（1822 行 → 6 文件）、`response_map`（2447 行 → 4 文件）
- Cargo workspace：`[workspace.dependencies]` 统一共享依赖、
  `[workspace.lints.clippy] all = deny` 落库、`[profile.release]`（thin LTO + strip）

### 工程
- CLI 集成测试（assert_cmd 冒烟：version/lang list/transpile/cheat）
- CI：MSRV 门禁显式 `cargo +1.88`（防 rust-toolchain.toml 静默绕过）、
  覆盖率基线注释刷新（llvm-cov 实测 73.6%）
- rust-toolchain.toml（stable + clippy/rustfmt 组件）
- 仓库卫生：根目录调试遗留清理、策略文档归档至 docs/strategy/、`.gitignore` 补全

## [0.7.0] - 2026-09-03

### 新增
- 教学 lint 与 code actions（未标注 let、全角标点一键修复、行尾忽略标记）
- 转译预览并排视图（VS Code 扩展）
- 基准回归门禁（criterion 基线入库 + 30% 阈值对比脚本）
- 并行转译与会话缓存（多文件项目 mod 链共享命中）
- 三平台端到端测试（Linux/macOS/Windows）
- 语言包在线市场（`rzc lang install`，GitCode 优先回退 GitHub）
- 扩展侧 AI 诊断讲解（Anthropic/Gemini/OpenAI 三提供商，光标诊断选择）
- CLI 诊断列映射回译（rustc 英文产物列号 → 母语源码列号，run/check 四条路径接入）

### 修复
- 教学提示仅输出首条的回归（现全量输出）
- 缓存语境指纹漏算派生特征表（改表不失效导致旧产物）

## [0.6.2] - 2026-08-30

- 教程代码全中文化 + 方言映射扩展（标准库成员/片段说明符/布局指定符）+ 语言包修复

## [0.6.0] - 2026-08-28

- 方言框架蓝图落地：多语言语言包架构（数据驱动、零代码新增语言）
- LSP 代理服务器、VS Code 扩展、离线发布脚本

## [0.5.0] - 2026-08-15

- 首个公开版本：中文方言转译内核、诊断翻译、25 章教学教程
