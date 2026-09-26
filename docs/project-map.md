# 项目地图（维护者手册）

> 本页回答两个问题：**我想改 X，去哪儿改？改完怎么验证？**
> 面向维护者与贡献者；使用者请看 [README](../README.md)、教程与[在线文档站](https://liuqiTan80.github.io/i18n-rust/)。
> 相关入口：[CONTRIBUTING.md](../CONTRIBUTING.md)（提交规范）· [tools/README.md](../tools/README.md)（脚本索引）· [docs/strategy/README.md](strategy/README.md)（现状与路线图）

## 60 秒认识项目

rzc 是**多语言 Rust 方言编译器**：用母语写代码 → 翻译为标准 Rust → 官方工具链编译运行 → 诊断翻译回母语并附教学提示。

```text
母语源码 (.zh/.ja/…)
   │  engine：词法转译 → 模块路径 → 别名替换
   ▼
标准 Rust 源码 ── cargo/rustc（官方工具链） ──► JSON 诊断
                                                  │  错误码/文案翻译 + 类型本地化 + 教学提示
                                                  ▼
                                              母语输出
```

| 交付物 | 位置 | 面向 |
|---|---|---|
| `rzc` 命令行 | `crates/cli` | 所有用户 |
| `i18n-rust-lsp` 语言服务器 | `crates/lsp` | VS Code 扩展后端 |
| 语言无关引擎 + 11 语言包 | `crates/engine` | 被 CLI / LSP 复用 |
| VS Code / Qoder 扩展 | `tools/vscode-extension` | IDE 用户 |
| 教程（4 语言）与文档站 | `tutorials/` + `book/` | 学习者 |

## 目录速查

| 目录 | 是什么 | 常见改动 |
|---|---|---|
| `crates/engine` | 语言无关核心：转译管线、映射管理、诊断翻译、增量缓存 | 转译规则、诊断翻译 |
| `crates/engine/lang-packs/<lang>/` | 语言包（数据非代码）：`keywords/stdlib/module_paths/errors/ui/lang_info.toml` + `crates/`（第三方映射） | 补词条、改文案 |
| `crates/cli` | rzc：`main.rs` 命令层、`diagnostics.rs` 诊断翻译、`mapping_*` 子命令 | 命令与行为 |
| `crates/lsp` | 代理 rust-analyzer：`response_map/` 反向翻译、`ui.rs` 界面消息本地化 | IDE 侧翻译 |
| `tools/vscode-extension` | 扩展（TypeScript） | 高亮 / 补全 / 诊断 UI |
| `tools/` | 验证与构建脚本（索引见 [tools/README.md](../tools/README.md)） | 门禁逻辑 |
| `tutorials/` | 中文教程母本；`tutorials/<lang>/` 译本 | 章节内容 |
| `book/` | 文档站装配源（mdBook，见 [book/README.md](../book/README.md)） | SUMMARY 与站点结构 |
| `docs/` | 参考文档 6 篇 + `dev/` + 本页 + `strategy/`（运营）+ `demo/`（演示素材） | 各类文档 |
| `third-party/` | 第三方映射注册中心数据与协议示例 | 注册中心协议 |
| `.github/workflows/` | CI / 发布 / 文档站三条流水线 | 门禁与发布 |

## 常见任务 → 改哪里 → 怎么验证

| 我想… | 改哪里 | 验证 |
|---|---|---|
| 改教程章节 | `tutorials/`（zh 母本）；译本 `tutorials/<lang>/` | `make tutorials`（zh）· `make tutorials-all`（en/ja/ru） |
| 补语言包词条 | `lang-packs/<lang>/{keywords,stdlib,module_paths}.toml` | `make mapping-check` + 相关单测 |
| 补错误翻译 | `lang-packs/<lang>/errors.toml` | 带错误码的实机复现 + `make test` |
| 改界面 / 教学文案 | CLI 侧 `lang-packs/<lang>/ui.toml`；LSP 侧 `crates/lsp/src/ui.rs`（读同一 `ui.toml`） | `make test` + 实机复现 |
| 改转译管线 | `crates/engine/src/` | `make gate`；热路径配套 `make bench-check` |
| 加 / 改 rzc 命令 | `crates/cli/src/main.rs`（诊断区 `diagnostics.rs`） | `make test` + 手动跑通 |
| 改 IDE 体验 | `crates/lsp/` + `tools/vscode-extension/` | 扩展 `npm test`；LSP `make test` |
| 调文档站 | `book/` + `tools/build-site.py` | `make site` · `make site-serve` |
| 发教程到飞书（国内阅读入口） | `tools/publish-feishu.py` | `make feishu`（平台侧配置见[发布准备清单](strategy/发布准备清单.md)第 5 节） |
| 改 CI / 发布 | `.github/workflows/{ci,release,pages}.yml` | 本地等价：`make gate` |
| 出发布包 | `release-offline.{sh,ps1}` | 见 [发布准备清单](strategy/发布准备清单.md) |
| 新增一门语言 | [docs/contributing-lang-pack.md](contributing-lang-pack.md) | `make mapping-check` + 教程门禁 |

## 门禁体系

**本地唯一入口 = `make`**（[Makefile](../Makefile)，`make help` 列全）；与 CI 的对应关系：

| 命令 | CI 对应 | 何时跑 |
|---|---|---|
| `make gate` | ci.yml `test` job 全链（fmt → clippy → test → mapping-check → tutorials(zh) → glossary） | 提交前必跑 |
| `make tutorials-all` | 本地增补（CI 仅验 zh）：en / ja / ru 逐语验证 | 动教程或转译规则后 |
| `make bench-check` | ci.yml `bench` job（本地阈值 30%） | 动引擎 / LSP 热路径后 |
| `make site` | pages.yml 构建步骤 | 动 `book/` 或教程结构后 |

流水线：**ci.yml** 七个 job（test / msrv / coverage / audit / bench / build / vsix）·
**release.yml** 四个 job（check / vsix / publish 三平台产物 + 双平台 Release / crates crates.io 发布）·
**pages.yml** 文档站构建与部署。

**教程白名单**：`tools/expected-failures.json` 只登记结构性噪音（跨块依赖等无法单独编译的块）；
新增条目须随教程改动并说明原因；验证失败时先判断"内容错"还是"需要白名单"。

## 文档体系

| 文档 | 回答 |
|---|---|
| [README.md](../README.md) / [README.en.md](../README.en.md) | 这是什么、怎么装、怎么用（访客） |
| [tutorials/](../tutorials/) 与[文档站](https://liuqiTan80.github.io/i18n-rust/) | 怎么学会（学习者） |
| [docs/project-map.md](project-map.md) | 改哪儿、怎么验（维护者，本页） |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | 开发环境与提交规范 |
| 参考文档 6 篇：[contributing-lang-pack](contributing-lang-pack.md) / [missing-mapping-guide](missing-mapping-guide.md) / [third-party-mapping](third-party-mapping.md) / [third-party-registry](third-party-registry.md) / [dialect-framework-blueprint](dialect-framework-blueprint.md) / [translation-status](translation-status.md) | 专项流程 |
| [docs/dev/i18n-rust.md](dev/i18n-rust.md) | 早期设计愿景稿（部分已演进；架构现状见本页与代码） |
| [docs/strategy/README.md](strategy/README.md) | 现状与路线图（维护者） |
| [CHANGELOG.md](../CHANGELOG.md) | 逐版本变更流水 |

## 修改纪律

1. **数据驱动**：语言差异只进语言包，不改引擎代码；新增语言 = 新增目录 + 门禁通过。
2. **文档同步**：用户可见变更 → `CHANGELOG` 的 `[Unreleased]`；教程 / 语言包变更 → 对应门禁，必要时更新 `translation-status`。
3. **先门禁后提交**：`make gate` 全绿再提交；一次提交只做一件事（见 CONTRIBUTING）。
