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
| `crates/engine` | 语言无关核心：转译管线、映射管理、诊断翻译（`诊断/待译清单.zh`）、`产物布局.zh` 产物托管布局（`.rz/gen/`）、增量缓存 | 转译规则、诊断翻译、产物落点 |
| `crates/engine/lang-packs/<lang>/` | 语言包（数据非代码）：`keywords/stdlib/module_paths/errors/ui/lang_info.toml` + `crates/`（第三方映射） | 补词条、改文案 |
| `crates/cli` | rzc：`main.zh` 命令层、`诊断输出.zh` 诊断翻译、`翻译缺口.zh`（`rzc gaps`）、`映射检查.zh` / `映射覆盖.zh` / `映射生成/` 子命令 | 命令与行为 |
| `crates/lsp` | 代理 rust-analyzer：`响应映射/` 反向翻译、`界面消息.zh` 界面消息本地化 | IDE 侧翻译 |
| `crates/zh-pure` | 纯函数方言测试样例（不发布）：`tests-zh/*.zh` 用方言与 `#[测试]` 写用例，`build.zh` 转译后随 `cargo test` 执行 | 方言测试写法示例 |
| `tools/vscode-extension` | 扩展（TypeScript） | 高亮 / 补全 / 诊断 UI |
| `tools/` | 验证与构建脚本（索引见 [tools/README.md](../tools/README.md)） | 门禁逻辑 |
| `scripts/` | 分支与发布工具：`zh-build.sh`（zh 方言源隔离构建）、`mirror.sh`（三仓镜像） | 构建/镜像流程 |
| `tutorials/` | 中文教程母本；`tutorials/<lang>/` 译本 | 章节内容 |
| `book/` | 文档站装配源（mdBook，见 [book/README.md](../book/README.md)） | SUMMARY 与站点结构 |
| `docs/` | 参考文档 + `dev/` + 本页 + `strategy/`（运营）+ `demo/`（演示素材） | 各类文档 |
| `third-party/` | 第三方映射注册中心数据与协议示例 | 注册中心协议 |
| `.github/workflows/` | CI / 发布 / 文档站 / zh 门禁 / 三仓镜像 / 漂移预警 | 门禁与发布 |

> 🌿 分支模型：`main` 是发布树（`.rs` 入库，直接 cargo）；`zh` 是方言源
> （只保留 `.zh`，产物不入库）。`zh` 的构建与阅读入口见
> [zh 分支代码阅读顺序与实操教程](zh-reading-guide.md)。

## 常见任务 → 改哪里 → 怎么验证

| 我想… | 改哪里 | 验证 |
|---|---|---|
| 读懂 zh 分支源码 | [zh 分支代码阅读顺序与实操教程](zh-reading-guide.md) | 按教程跑通 `make zh-build` |
| 改教程章节 | `tutorials/`（zh 母本）；译本 `tutorials/<lang>/` | main：`make tutorials` / `make tutorials-all`；zh：先 `make zh-build ARGS='build --bin rzc'`，再用 `--rzc .zh-build/target/debug/rzc` |
| 补语言包词条 | `lang-packs/<lang>/{keywords,stdlib,module_paths}.toml` | `make mapping-check` + 相关单测 |
| 补错误翻译 | `lang-packs/<lang>/errors.toml` | 带错误码的实机复现 + `make test` |
| 改界面 / 教学文案 | 文案在 `lang-packs/<lang>/ui.toml`；解析、`{}` 替换与缺键回退统一在 `crates/engine/src/界面消息.zh`（CLI / LSP 各自只保留加载链）。键出现在哪条命令、`{}` 填什么见生成物 [ui-keys.en.md](ui-keys.en.md) | `make test` + `make ui-index-write`（改键后重生成索引） + 实机复现 |
| 改转译管线 | `crates/engine/src/*.zh` | main：`make gate`；zh：`make zh-build ARGS='test --workspace --locked'` + `make zh-artifacts`；热路径配套 `make bench-check` |
| 改产物落点（`.rz/gen/` 布局） | `crates/engine/src/产物布局.zh`（CLI 与 LSP 镜像共用） | `make test`（`cli_basic` 嵌套模块用例）+ 实机 `rzc check` 看 `.rz/gen/` 与诊断回译 |
| 补「暂无译文」的消息 | `rzc gaps --export` 取清单 → `lang-packs/<lang>/errors.toml` | 清单能清零 + `make test`（语料门禁含 main/help 两级下限） |
| 加 / 改 rzc 命令 | `crates/cli/src/main.zh`（诊断输出在 `诊断输出.zh`） | `make test` + 手动跑通 |
| 用方言写测试（`.zh`） | `crates/zh-pure/tests-zh/*.zh` | main：`cargo test -p zh-pure`；zh：`make zh-build ARGS='test -p zh-pure'` |
| 改 IDE 体验 | `crates/lsp/` + `tools/vscode-extension/` | 扩展 `npm test`；LSP `make test` |
| 调文档站 | `book/` + `tools/build-site.py` | `make site` · `make site-serve` |
| 发教程到飞书（国内阅读入口） | `tools/publish-feishu.py` | `make feishu`（平台侧配置见[发布准备清单](strategy/发布准备清单.md)第 5 节） |
| 改 CI / 发布 | `.github/workflows/{ci,release,pages,mirror,zh-gates,rustc-drift}.yml` | main 本地等价：`make gate`；zh：`make zh-build …` + `make zh-artifacts zh-ascii` |
| 出发布包 | `release-offline.{sh,ps1}` | 见 [发布准备清单](strategy/发布准备清单.md) |
| 新增一门语言 | [docs/contributing-lang-pack.md](contributing-lang-pack.md) | `make mapping-check` + 教程门禁 |

## 门禁体系

**本地唯一入口 = `make`**（[Makefile](../Makefile)，`make help` 列全）；按分支区分：

| 命令 | 适用分支 | CI 对应 | 何时跑 |
|---|---|---|---|
| `make gate` | main | ci.yml `test` job 全链（fmt → clippy → test → prod-panics → ui-keys/ui-index/ext-ui-keys → lang-packs → readme-parity → mapping-check → tutorials-all → glossary/glossary-consistency → zh-artifacts → zh-ascii） | 提交前必跑 |
| `make zh-build ARGS='test --workspace --locked'` | zh | zh-gates 只跑源码级 Python 门禁；编译链在 main 验证 | zh 构建/测试 |
| `make zh-build ARGS='build --bin rzc'` | zh | —（产出 `.zh-build/target/debug/rzc`，供后续门禁/教程验证） | 需要 rzc 时 |
| `make zh-artifacts` | zh | zh-gates `check-zh-artifacts.py`（可转译性 + 一词多译 + 绑定位） | 改词表 / 方言源后 |
| `make zh-ascii` | zh | zh-gates `check-zh-ascii.py`（代码区 / 注释区零英文） | 改方言源后 |
| `make tutorials-all` | main | ci.yml 与 release.yml 均跑 zh/en/ja/ru 四语 | 动教程或转译规则后 |
| `make bench-check` | main | ci.yml `bench` job（本地阈值 30%） | 动引擎 / LSP 热路径后 |
| `make site` | main | pages.yml 构建步骤 | 动 `book/` 或教程结构后 |

流水线：**ci.yml** 八个 job（test / windows-gates / msrv / coverage / audit / bench / build / vsix，仅 main 触发）·
**release.yml** 五个 job（check 全链门禁 / vsix / publish 三平台产物 + GitHub/GitCode Release / publish-extension 双商店 / crates crates.io 发布）·
**pages.yml** 文档站构建与部署 ·
**zh-gates.yml** zh 分支源码级门禁（中文棘轮 + 方言源纪律）·
**mirror.yml** GitHub→Gitee/GitCode 单向镜像（分支 + 版本标签）·
**rustc-drift.yml** 诊断语料漂移预警（每周 stable+beta 实跑采集器比对入库语料；stable 漂移失败、beta 仅警告）。

仓库换行策略由 [.gitattributes](../.gitattributes) 钉住全仓 LF：转译链对 CRLF 敏感
（.zh 字符串续行掩码、eject 逐字节自比、语料/教程文本匹配），不得随个人
 core.autocrlf 漂移；二进制产物（gif/png/vsix 等）已标记不转换。

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
| [docs/dev/i18n-rust.md](dev/i18n-rust.md) | 早期设计愿景稿（**历史文档**，路径与 API 已演进；现状见本页与代码） |
| [docs/zh-reading-guide.md](zh-reading-guide.md) | zh 分支代码阅读顺序与实操教程 |
| [docs/strategy/README.md](strategy/README.md) | 现状与路线图（维护者） |
| [CHANGELOG.md](../CHANGELOG.md) | 逐版本变更流水 |

## 修改纪律

1. **数据驱动**：语言差异只进语言包，不改引擎代码；新增语言 = 新增目录 + 门禁通过。
2. **文档同步**：用户可见变更 → `CHANGELOG` 的 `[Unreleased]`；教程 / 语言包变更 → 对应门禁，必要时更新 `translation-status`。
3. **先门禁后提交**：main 上 `make gate` 全绿再提交；zh 上
   `make zh-build ARGS='test --workspace --locked'` + `make zh-artifacts zh-ascii`；
   一次提交只做一件事（见 CONTRIBUTING）。
