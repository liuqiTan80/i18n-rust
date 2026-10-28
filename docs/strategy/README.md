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
- **方言项目的「无 target」错误没人话（en 第 18 章实机确认，用户必经之路）**：`rzc init demo -l en`
  只生成 `src/main.en`，而 Cargo 只认 `src/main.rs`，于是**新建项目里第一条 Cargo 命令一定失败**：
  `rzc add rand` 与 `cargo build` 都原样吐出 `error: failed to parse manifest at .../Cargo.toml
  Caused by: no targets specified in the manifest / either src/lib.rs, src/main.rs, a [lib]
  section, or [[bin]] section must be present`（`handle_add_command` 直接 spawn cargo 并传播
  退出码，未做任何前置判定）。中文读者看到一堆英文构建器内部术语，且**修复动作（先跑一次
  `rzc check`）完全看不出来**。修法：`rzc add` 在 spawn 前判定项目有无 target（Cargo.toml 无
  `[lib]`/`[[bin]]` 且 src/ 下无 `.rs` 入口），命中就用界面消息键给出「先 `rzc check src/main.<ext>`
  生成标准 Rust 入口」；`cargo build` 侧无法拦截，可在失败后追加同样的提示行。需 11 语言新键 +
  `check-ui-keys.py`，并用 `rzc init` 实机回归
- `rzc init --lib` 不存在（en 第 18 章实机确认，**zh 教程第十八章 18.5 声称存在**）：
  `rzc init booklib --lib` 报 `error: unexpected argument '--lib' found`。当前造库的唯一办法是
  手写 `src/lib.en`（`rzc check src/lib.en` 实测通过）。修法二选一：① 给 init 加 `--lib`
  （生成 lib 骨架 + Cargo.toml 注释说明产物仍需 rzc 转译）；② 不加参数，改教程与
  `rzc init --help` 的说法。无论哪种，zh 第十八章正文都要同步
- zh 教程第十八章的构建流程对**所有语言读者都少了一步**：18.7 直接给
  `rzc init → 写代码 → rzc run → cargo build --release`，而 `cargo build --release` 在从未跑过
  `rzc check`/`rzc run` 的项目里必然报「no targets」（见上条）。en 第 18 章已改为「先 check
  一次，再交给 Cargo」并把该错误列为本章头号常见错误；zh/ja/ru 三语需要同步这一步骤，属教程
  正确性修复，优先级高于常规滚动
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
- `rzc lang list` 相关测试 `test_lang_list_lists_builtin_packs` 在全量并行下偶发失败
  （en 第 18 章收尾时观测到一次：单跑与 `--test-threads=1` 稳定通过，其后 3 轮全量 release
  + 1 轮 debug 全绿，未再复现）。该测试直接断言 `rzc lang list` 输出含 "11"，而
  `test_lang_local_install_list_remove_lifecycle` 同一批次里在**改全局状态**
  （`~/.rz/lang-packs/` 安装/移除语言包），两者并发即竞争。修法：让 list 类断言走
  `RZ_HOME`/临时目录隔离（或给这两个测试加同一把文件锁），否则 CI 会间歇性红灯且难以归因
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
