# 战略与运营：现状与路线图

> **一页纸现状**（2026-09-26 更新）：工程侧全部就绪，剩余的是"凭据与素材"类
> 维护者动作（P0）与按需启动的 P1 增强。
> 历史评估快照见 [archive/](archive/README.md)（只读）；执行流水以 [CHANGELOG](../../CHANGELOG.md) 为准。

## 已就绪（工程侧，无需动作）

| 能力 | 说明 |
|---|---|
| 全球品牌与双平台 | GitHub / GitCode 同名 `i18n-rust`，双平台同步推送；语言包安装源自动回退 |
| 发布链路 | 三 crate 元数据 / 发布顺序 / include 白名单就绪；`vsce package` 本机一次通过（0.8.2）；release.yml 三平台产物 + SHA256SUMS |
| 教程资产 | zh 全量（26 章 + 术语表 + 5 附录，逐块编译门禁）；en 15/33；ja/ru 滚动中（[translation-status](../translation-status.md)） |
| 文档站 | `book/` 装配源 + `tools/build-site.py` 四语言组装 + [pages.yml](../../.github/workflows/pages.yml) 自动部署（[book/README.md](../../book/README.md)） |
| 演示素材 | `docs/demo/` 三语言 vhs 脚本 + 演示样本（zh/ja/ru），差一次录屏 |
| 质量门禁 | CI 七 job（test / msrv / coverage / audit / bench / build / vsix）+ 本地 `make gate`；四语言教程门禁 0 失败；维护者入口见 [项目地图](../project-map.md) |

## 待维护者动作（P0：凭据 / 素材 / 网页设置）

| # | 事项 | 卡点 |
|---|---|---|
| 1 | crates.io 三个 crate 发布（之后全球 `cargo install rzc`） | `cargo login <token>` + 发布机 `cargo publish --dry-run` |
| 2 | VS Code Marketplace + OpenVSX 发布 | Azure PAT / OpenVSX token |
| 3 | 演示 GIF 录屏（15 秒，放 README 顶部） | 人工录屏（脚本已就绪） |
| 4 | 启用 GitHub Pages（Settings → Pages → Source 选 "GitHub Actions"） | 网页操作；启用后站点：https://liuqiTan80.github.io/i18n-rust/ |
| 5 | GitHub 仓库 about 栏文案 + Topics 标签 | 网页操作（推广露面） |

逐条命令与发布后核验步骤见 [发布准备清单.md](发布准备清单.md)。

## 候选下一步（P1，按需启动）

- 教程滚动：en 第十四章《生命周期》起继续（复用既有翻译 + 门禁流程）；ja/ru 保持节奏
- CLI 侧 OpenAI 兼容 provider（`base_url / key / model` 三环境变量与扩展侧对齐）
- `ui.toml` 键英文字典（第 11 种语言贡献者参考）+ 贡献 SOP 英文化
- `rzc init` 交互式语言选择（locale 不在 10 语言集合时列出菜单而非静默回退）
- 文档站增强：en/ja/ru 书接入参考文档（依赖翻译进度）；en 书开启 Playground 评估
- 渠道投放：按 [推广方案.md](推广方案.md) 执行并回收数据（dev.to / r/rust / Show HN 等全球渠道）

## 本目录

| 文件 | 性质 |
|---|---|
| [README.md](README.md) | 本页：现状与路线图（随进展更新） |
| [推广方案.md](推广方案.md) | 8 周零预算推广执行计划与渠道文案 |
| [发布准备清单.md](发布准备清单.md) | 维护者凭据类动作清单（全球分发第 1 周执行项） |
| [archive/](archive/README.md) | 历史评估快照（只读） |

## 记录原则

- 新评估 / 新方案写新文件，并登记到本页；被取代的旧版移入 `archive/`，在其 README 备注结论；
- 本页只保留"当前有效"的结论，任何执行流水写 [CHANGELOG](../../CHANGELOG.md)。
