# 贡献指南

感谢参与 i18n-rust（rzc）！本项目让开发者用母语编写 Rust 程序。
以下是最小必要的贡献路径说明。

## 开发环境

- **Rust stable**：`rust-toolchain.toml` 已锁定 channel 与组件（clippy/rustfmt）；MSRV 为 1.88；
- **Python 3**：教程与术语表校验脚本（`tools/`）；
- **Node.js 18+**：仅 VS Code 扩展开发与打包需要（`tools/vscode-extension/`）。

## 提交前门禁

一条命令等价于 CI 测试门禁（提交前建议必跑）：

```bash
make gate
```

依次执行：格式检查 → Clippy 零告警 → 全部测试（debug + release）→
映射质量门禁 → 教程代码块编译验证 → 术语表一致性。
全部可用目标与手动等价命令见 `make help`。

涉及转译热路径的改动建议额外跑基准回归：

```bash
make bench-check     # 对比入库基线（本地阈值 30%）
make bench-update    # 有意改变性能特征时刷新基线，并随提交
```

VS Code 扩展改动：`cd tools/vscode-extension && npm ci && npm run lint && npm test`。

## 提交规范

采用约定式提交（Conventional Commits），参考历史风格：

```
fix(cli): 入口词干判定覆盖母语主函数名
feat(engine): zh 词表扩充（三新表 + 六表补条）
perf(engine,cli,lsp): 批量转译 I/O 与按键延迟优化
docs(tutorial): 附录 C 补充常见原因
test(lsp): mirror_check 补充纯逻辑单测
ci(bench): 基准回归失败附加工作流注解
```

- 一次提交只做一件事；修复类提交请说明「根因 + 修复策略」；
- 面向用户的行为变更须同步 `CHANGELOG.md` 的 `[Unreleased]` 节；
- 教程/文档改动请与本语言包事实核对（见 `make tutorials` 门禁）。

## 贡献方向

| 方向 | 入口文档 |
|---|---|
| 新增 / 完善语言包（第 11 种语言等） | [docs/contributing-lang-pack.md](docs/contributing-lang-pack.md) |
| 教程翻译与扩章 | [docs/translation-status.md](docs/translation-status.md) · [tutorials/en/README.md](tutorials/en/README.md) |
| 第三方库映射补充 | [docs/missing-mapping-guide.md](docs/missing-mapping-guide.md) · [docs/third-party-mapping.md](docs/third-party-mapping.md) |
| 引擎 / CLI / LSP / 扩展代码 | [docs/project-map.md](docs/project-map.md)（改哪儿、怎么验）· [docs/dev/i18n-rust.md](docs/dev/i18n-rust.md)（早期设计愿景） |

## 测试要求

- Rust 侧：修复与新功能须附 `#[cfg(test)]` 单测；
- 涉及语言包/词表的改动跑 `make mapping-check`；
- 涉及教程的改动跑 `make tutorials`（预期失败白名单见 `tools/expected-failures.json`，仅登记结构性噪音）。

## 许可

本项目采用 [MIT 许可](LICENSE)。提交贡献即表示同意以该许可发布。
