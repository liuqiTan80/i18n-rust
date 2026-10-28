# 战略与运营：现状与路线图

> **一页纸现状**（2026-09-26 更新）：工程侧全部就绪，剩余的是"凭据与素材"类
> 维护者动作（P0）与按需启动的 P1 增强。
> 历史评估快照见 [archive/](archive/README.md)（只读）；执行流水以 [CHANGELOG](../../CHANGELOG.md) 为准。

## 已就绪（工程侧，无需动作）

| 能力 | 说明 |
|---|---|
| 全球品牌与双平台 | GitHub / GitCode 同名 `i18n-rust`，双平台同步推送；语言包安装源自动回退 |
| 发布链路 | **crates.io 三 crate 最新 0.8.4（2026-10-28 `cargo search` 实测；0.8.2 首发 2026-09-21），已落后源码版本**；`vsce package` 本机一次通过；release.yml 三平台产物 + SHA256SUMS |
| 教程资产 | zh 全量（26 章 + 术语表 + 5 附录，逐块编译门禁）；en 15/33；ja/ru 滚动中（[translation-status](../translation-status.md)） |
| 文档站 | `book/` 装配源 + `tools/build-site.py` 四语言组装 + [pages.yml](../../.github/workflows/pages.yml) 自动部署（[book/README.md](../../book/README.md)） |
| 演示素材 | `docs/demo/` 三语言 vhs 脚本 + 演示样本（zh/ja/ru），差一次录屏 |
| 质量门禁 | CI 七 job（test / msrv / coverage / audit / bench / build / vsix）+ 本地 `make gate`；四语言教程门禁 0 失败；维护者入口见 [项目地图](../project-map.md) |

## 待维护者动作（P0：凭据 / 素材 / 网页设置）

| # | 事项 | 卡点 |
|---|---|---|
| 1 | VS Code Marketplace + OpenVSX 发布 | **CI 已就绪**（release.yml `publish-extension` job，推 tag 自动双商店发布）；卡点仅剩人工：注册 i18n-rust 发布者/命名空间 + 配置 `VSCE_PAT`、`OVSX_PAT` 两个 Secret |
| 2 | ~~演示 GIF 录屏（15 秒，放 README 顶部）~~ | **已完成（2026-06）**：`docs/demo/demo-zh.gif` 已录制入库并被中/英 README 顶部引用 |
| 3 | 启用 GitHub Pages（Settings → Pages → Source 选 "GitHub Actions"） | 网页操作；启用后站点：https://liuqiTan80.github.io/i18n-rust/ |
| 4 | GitHub 仓库元数据（描述 / Topics / Homepage） | 网页操作（推广露面；Homepage 依赖 Pages 启用） |
| 5 | 飞书知识库教程发布（国内公网免登录阅读） | **已完成（2026-09-26）**：33 篇全量发布、已开启互联网公开；入口 <https://my.feishu.cn/wiki/space/7689728327082314704>（操作记录见[发布准备清单](发布准备清单.md)第 5 节） |

逐条命令与发布后核验步骤见 [发布准备清单.md](发布准备清单.md)。

## 候选下一步（P1，按需启动）

- 教程滚动：en 第十五章《集合类型》起继续（en 16/33，复用既有翻译 + 门禁流程）；ja/ru 保持节奏
- AI provider 真实端点联调：OpenAI / Ollama / 通义各跑一次 `rzc mapping auto`，验证超时与分批上限（`RZ_AI_*` 改造目前只有离线测试，未联网验证）
- 贡献 SOP 英文化：`docs/contributing-lang-pack.md` / `missing-mapping-guide.md` 出 en 版
  （键字典部分已由生成物 `docs/ui-keys.en.md` 覆盖，2026-10-28；剩正文翻译）
- 文档站增强：en/ja/ru 书接入参考文档（依赖翻译进度）；en 书开启 Playground 评估
- 扩展剩余内联中文文案入 `ext_` 键表（含按钮标签「查看日志 / 打开设置」等 9 处）：
  需先把按钮的展示文案与返回值解耦（现在靠中文字符串比对用户点了哪个），门禁
  `tools/check-extension-ui-keys.py` 已就位，新键一律走它

## 本目录

| 文件 | 性质 |
|---|---|
| [README.md](README.md) | 本页：现状与路线图（随进展更新） |
| [发布准备清单.md](发布准备清单.md) | 维护者凭据类动作清单（全球分发第 1 周执行项） |
| [archive/](archive/README.md) | 历史评估快照（只读） |

## 记录原则

- 新评估 / 新方案写新文件，并登记到本页；被取代的旧版移入 `archive/`，在其 README 备注结论；
- 本页只保留"当前有效"的结论，任何执行流水写 [CHANGELOG](../../CHANGELOG.md)。
