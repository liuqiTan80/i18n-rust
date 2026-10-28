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

- 教程滚动：en 第十九章《智能指针》起继续（en 20/33，复用既有翻译 + 门禁流程）；ja/ru 保持节奏
- 原生 Cargo 命令仍不看界面语言（`rzc add` 一侧已补母语提示，`cargo build` 直敲
  时用户依旧看到 `no targets specified in the manifest` 原文）。教程已教会读者把 `Caused by`
  两行读成「还没有 `.rs` 入口」，但 rzc 无法拦截外部 cargo 进程。可选做法：`rzc doctor`
  增加一项「项目是否已有 Cargo 可读入口」的自检，把这条墙从「读得懂」降级为「提前看得见」
- **（2026-10-28 已按方案 ③ 落地）** `rzc init --lib` 仍不存在（`error: unexpected argument
  '--lib' found`），但造方言库已经零配置：`lib` 纳入入口名集合，`src/lib.<ext>` 的产物直写
  `src/lib.rs`，Cargo 自动认出库目标；`[lib] path` 有显式声明时照声明写。剩下的可选做法是给
  `rzc init` 加 `--lib`（生成 `src/lib.<ext>` 骨架 + 库版 README），属易用性增强而非墙
- bin+lib 同项目还有两处墙（原第 ② 条已修，见下一条；教程 18.5 已按实机写明）：
  ① ``crate name `计算器` passed to `--extern` is not a valid ASCII identifier`` 无母语译文
  （记入 `.rz/gaps`），而 `rzc init 中文名` 是官方鼓励的写法——只有库或只有主程序时中文包名
  能编，两者同存必撞；③ 入口产物只由被请求的文件写，所以「先 check 库再 check 主程序」是读者
  必须知道的一步——修法候选：`has_cargo_target` 之外再加一条库目标自检（`rzc doctor`），或让
  项目级转译在检测到 `[lib] path`/src/lib.rs 时一并刷新库产物
- **（2026-10-28 已修）** bin+lib 的第 ② 条墙：主程序 `使用 calc::翻倍;` 报 E0432 时，rustc 的
  `cargo add calc` 建议与 rzc 追加提示都把**自有 crate**说成第三方库，而 Cargo 拒绝包依赖自己
  （`cannot add ... to itself`）。现在按本地包名（`[package] name` 的 `-`→`_` + `[lib] name`）
  剔除该建议并给 `hint_own_crate_lib` / `hint_own_crate_no_lib`。同轮还查出一个真缺陷：项目级
  转译把兄弟 `src/lib.zh` 旁边的 `src/lib.rs` 当旧布局残留清掉，正好废掉「先 check 库」这条配方
- LSP 未建模 bin+lib 项目（本轮登记，未动代码）：编辑器的虚拟聚合项目写死
  `name = "i18n-virtual"` 的单 `[[bin]]`，没有库目标，于是用户项目里明明合法的
  `使用 calc::翻倍;` 在编辑器里恒报 E0432，`lsp_hint_add_dependency` 还会跟着建议 `rzc add calc`。
  要修的是虚拟项目本身：为 src/lib.<ext> 生成 `[lib] path` + 用真实包名，而不是只改提示文案
- `错误：` 前缀取自界面语言（`Ui::global()`），消息正文取自文件扩展名（`ui_for_file`）：
  中文系统下 `rzc run examples/demo.en` 渲染出 `错误：<英文正文>`（`run_not_default_entry`、
  本轮新增的 `run_lib_no_bin` 都如此）。要么让顶层错误打印复用 per-file 的 ui，要么让
  `ui_for_file` 只在消息内部使用而前缀统一走 global——涉及所有 `bail!` 出口，改动小但面广
- AI provider 真实端点联调：OpenAI / Ollama / 通义各跑一次 `rzc mapping auto`，验证超时与分批上限（`RZ_AI_*` 改造目前只有离线测试，未联网验证）
- 贡献 SOP 英文化：`docs/contributing-lang-pack.md` / `missing-mapping-guide.md` 出 en 版
  （键字典部分已由生成物 `docs/ui-keys.en.md` 覆盖，2026-10-28；剩正文翻译）
- 文档站增强：en/ja/ru 书接入参考文档（依赖翻译进度）；en 书开启 Playground 评估
- 扩展剩余内联中文文案入 `ext_` 键表（含按钮标签「查看日志 / 打开设置」等 9 处）：
  需先把按钮的展示文案与返回值解耦（现在靠中文字符串比对用户点了哪个），门禁
  `tools/check-extension-ui-keys.py` 已就位，新键一律走它

- E0277 通用码模板套坏 `?` 的专用消息（en 第 16 章采集时发现）：rustc 关于 `?` 的
  两条消息（"the `?` operator can only be used in a function that returns
  `Result` or `Option`" 与 "`?` couldn't convert the error to `X`"）本身就带 E0277 码，
  `render_main_message` 却让错误码表优先，并按反引号 token 顺序回填 `{类型}`/`{特征}`
  ——前者把 `?` 本身当成「类型」，渲染出 "the trait bound `?`: `Result` is not
  satisfied"（语义不通），后者渲染出 "the trait bound `?`: `SignupError` is not
  satisfied"。修法：给 `[消息翻译]` 增两条 `?` 专用键，并把「消息表**精确命中**」提到
  码表模板之前（前缀/通配命中仍走码表优先），11 语言同步；动的是核心优先级，须与既有
  `en_pack_code_templates_reproduce_rustc_own_wording` 一并验证
- `rzc check` 快路径漏 MIR 级 lint：`--emit=metadata` 不跑 `unconditional_panic`，于是
  `let scores = [1,2,3]; scores[9]` 被判为「✅ Compilation succeeded」，要到 `rzc run`
  才报 `error[unconditional_panic]: this operation will panic at runtime`（且不翻译）。
  check 与 run 口径不一致，会让学习者对「编译器会提前拦截」这类正文承诺失去信任。修法
  二选一：① check 路径额外做一次 const-eval 级检查；② 接受差异，但在 CLI 中注明这类
  「仅 run 可见」的诊断，并同步各语言教程措辞。涉及快路径延迟，动手前先量
  `tools/bench-check.sh` 基线
- 其余 10 语言的 `[E0603]` 不给出条目名（en 第 17 章采集时发现）：zh/ja/ru/de/es/fr/pt/
  ko/hi/ar 的模板写作「模块是私有的」一类措辞，不带 `{名称}`。它本就不像 en 那样把种类
  说错（种类名词缺位），但读者拿到「是私有的」却不知**路径上哪一层**断了，只能回头对照
  源码。en/ja/de 本轮已改为「不点名种类 + 保留条目名」的中立措辞（见引擎门禁
  `code_templates_do_not_misname_item_kind`）；其余语言要按各自性/数/格改写「模块」这一
  名词，或改用完全不含种类的句式，属逐语言文案工作，建议配合 `tools/check-lang-packs.py`
  一次性做完并扩测覆盖
- 教程门禁只支持单文件（en 第 17 章采集时确认）：`make_project` 每块只生成
  `src/main.<ext>` 一个源文件，`mod cutlery;` 这类文件模块必然 E0583/E0601，于是各语言
  第十七章的多文件示例只能进 `tools/expected-failures.json` 白名单（zh 现有 61 条里含
  同类）。en 第 17 章改写为自包含程序 + ` ```text ` 展示文件形态，绕开了限制但读者看不到
  可运行的两文件例子。修法：给验证器加 `// 附带文件: cutlery.en` 协议——按声明顺序在
  `src/` 下多写几个文件再整体 `rzc check`/`rzc run`，注意与既有 file+line 白名单键共存、
  四语言（zh/en/ja/ru）同时生效，属工具扩展，先在 en 上验证再回灌 zh 白名单
- E0502 的所有权叙事语序歧义：`diag_ownership_borrowed_in_use`（"变量 `{}` 在第 {}
  行被借用，第 {} 行仍在被使用"）本是给 E0382 类"先借后用"写的，E0502 复用后两个行号
  先后颠倒——en 教程第十五章实机捕获到 "borrowed on line 5 and is still in use on
  line 3"（5 是新的可变借用、3 是仍在结束的不可变借用），初学者会读反。修法：给
  E0502 单独一条键（"第 3 行的不可变借用还没结束，第 5 行又要可变借用"），
  11 语言同步并走 `tools/check-ui-keys.py` 门禁


## 本目录

| 文件 | 性质 |
|---|---|
| [README.md](README.md) | 本页：现状与路线图（随进展更新） |
| [发布准备清单.md](发布准备清单.md) | 维护者凭据类动作清单（全球分发第 1 周执行项） |
| [archive/](archive/README.md) | 历史评估快照（只读） |

## 记录原则

- 新评估 / 新方案写新文件，并登记到本页；被取代的旧版移入 `archive/`，在其 README 备注结论；
- 本页只保留"当前有效"的结论，任何执行流水写 [CHANGELOG](../../CHANGELOG.md)。
