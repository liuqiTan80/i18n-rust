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
- bin+lib 同项目还剩一处墙（原第 ② 条已修、第 ① 条已补译文，均见下两条；教程 18.5 已按实机写明）：
  ③ 入口产物只由被请求的文件写，所以「先 check 库再 check 主程序」是读者
  必须知道的一步——修法候选：`has_cargo_target` 之外再加一条库目标自检（`rzc doctor`），或让
  项目级转译在检测到 `[lib] path`/src/lib.rs 时一并刷新库产物；而 `rzc init 中文名` 是官方鼓励的
  写法，跟非 ASCII 包名这道墙直接冲突（本轮实测：主程序**完全不引用库**也一样撞，只有库或
  只有主程序都能编），真正的根治方向是让入口产物以一个 ASCII crate 名递交给 Cargo
- **（2026-10-28 已落地）** bin+lib 的第 ① 条墙：``crate name `计算器` passed to `--extern` is not a
  valid ASCII identifier`` 已补 10 语言译文（消息翻译键 166 → 167，en 是恒等语言不携带该节）。因为
  工具通道只渲染 `消息模板`、不渲染 `教学提示`，可执行动作写进了模板本身；墙本身未拆，只是读者
  现在看得懂该改什么
- **（2026-10-28 已落地）** en 恒等语言在工具通道上假报「有消息未译」：`gaps::record_gap`
  现对 `语言::is_identity_language` 直接返回，工具通道与 JSON 通道一并止住（两者汇流于此），
  汇总提示由登记非空驱动所以自然消失，渲染路径一行未动（原句照旧输出）。历史遗留的
  `~/.rz/gaps/en.tsv`（1079 行）不代删，`rzc gaps --lang en --clear` 自清；一致性由
  `identity_language_packs_carry_no_message_table` 双向钉住（en 带「消息翻译」节即红，
  其余 10 语言的消息表被清空也红）
- en 的工具通道前缀仍与 cargo 自己不统一（同轮查出）：`cargo_progress_error` 在 en 写作
  `Error: {}`（大写 E），于是经过工具通道的错误行首字母被改写，与 cargo 自己的 `error:`
  以及同屏的 `warning:` / `note:`（这两条键都是小写）不一致——英语读者拿到的不是逐字原句。
  该键同时被 `translate_cargo_progress` 复用（diagnostics.rs:255），改前先量进度行输出与
  en 教程引用（第十八/十六章的 `Error:` 行来自 rzc 自身的 `bail!`，与这条不同路径）。
  现状已被 `test_localize_tool_line_in_identity_lang_shows_original_without_gap` 按字面钉住
- **（2026-10-29 已落地）** 待译清单的 TSV 会被多行消息破坏：消息原文含换行时按原样落盘，于是
  `~/.rz/gaps/en.tsv` 1079 行里有 628 行是不以语言列开头的裸续行，`parse_gap_lines` 把它们
  当作「单列消息」并按**当前界面语言**归类 —— 语言归属与条数双双失真，`rzc gaps` 的读数
  因此偏大。已修：`gap_line` 写入前转义 `\n`/`\r`/`\t`/`\\`，`parse_gap_lines` 读取时还原，
  并拼回历史裸续行到上一条记录；`flush_gaps_to_dir` 整档重写（而非追加）触发格式迁移；
  `rzc gaps --export` 也走转义。一致性由 3 条新测试 + 1 条 CLI 集成用例钉住
- **（已落地）** 消息模板里**作者手写的**反引号英文词会被 `replace_type_names` 按类型映射改写（zh stdlib 表里
  `线程名` = `name`，当时第一版 zh 模板因此输出「请把 Cargo.toml 里的 `线程名` 改成 ASCII 名」）。
  当时靠改措辞 + 用例绕开，没动机制。现已按「注入前本地化」根治：`replace_type_names` 拆成
  `localize_rustc_literals`/`localize_all_backticks`/`localize_token` 三层，模板渲染路径只在写入
  `{期望}`/`{实际}`/`CODE_TEMPLATE_PLACEHOLDERS`/`{qN}` 前对**值**做中文化（`{qN}` 走新增的
  `fill_dynamic_placeholders_with`），全文改写只留给「整段就是 rustc 英文原文」的兜底（两表未命中、
  占位符无法回填、英文残段回拼），也顺带消除了主链上的双次调用。按各语言**真实** `type_map` 扫出的
  存量 17 处模板字面（ar 2 / hi 3 / ja 4 / ko 3 / ru 2 / zh 3，涉词仅 `main`/`Box`/`Rc`/`clone`/`await`）
  逐条复核均为「指认源代码语法」，修复后保持英文即正确，故 TOML 一行未改。help 通道与工具通道
  本就不走该函数，行为不变。
  一致性由 `template_literal_identifiers_survive_localization`（E0601 实机输出从「找不到 `主函数` 函数」
  回到「找不到 `main` 函数」，并把 17 处清单 `TEMPLATE_LITERAL_INVENTORY` 逐行钉住）与反向门禁
  `injected_rustc_types_are_still_localized`（同表同名 token 作为注入值时仍必须被译成母语名，
  `{期望}`/`{实际}`/带路径类型/`{integer}` 同样照旧）成对钉住；两条各带自己的反证：退回旧实现（或
  只把渲染收尾改回全文改写）正面红、退回反面（`localize_token` 改恒等）反面红
- **（2026-10-29 已修）** 诊断译文的「同义名胜出」跨进程不稳定：
  `build_type_map` 把语言包正表反转为「英文名 → 母语名」时，用 `HashMap` 遍历顺序上的
  `entry(英文).or_insert(母语)` / `insert` 取舍，而真表允许同一英文值被多个母语同义词映射——
  按引擎自身的加载口径统计，二义英文值 zh 99 / hi 70 / ar 65 / ja 64 / ko 62 / ru 61（共 421），
  含 `String` ← 字符串/字符串值、`Display` ← 可显示/显示、`clone` ← 克隆/克隆值、`fmt` ←
  格式化/格式化层/格式化模块。后果是同一份代码两次 `rzc check` 可能给出
  「`标准库::格式化::可显示`」与「`标准库::格式化::显示`」两种译文——读者拿译文去搜文档时
  会偶尔搜不到，且任何拿字面串做期望的用例都会变成掷随机数。
  修法取「来源优先序 + 同来源内 TOML 书写序，同一英文名取首个命中」：`MappingManager` 加载时按
  「stdlib 模块路径 → 其他模块路径（`module_paths.toml` + `crates/*.toml`）→ `keywords.toml` 的
  `["类型"]` 节 → stdlib 标识符 → crates 标识符」收集有序候选（`reverse_candidates`），
  `build_type_map` 只做 first-wins。用优先序而不是读取顺序，是为了让 `load_from_dir` 与
  `load_from_builtin` 得到同一张表；前两条复刻旧版「模块路径覆盖一切」，stdlib 优先压过
  第三方 crate 别名（跨文件二义如 `SocketAddr` / `TcpListener` / `Json` 由此定）。
  顺序可行的前提：toml 0.8 的 `Value::Table` 是 BTreeMap（按键排序），但 serde 反序列化通道
  按文档序投递键值对，故新增 `parse_toml_sections_ordered`（自定义 visitor 收 `Vec`）；它同时成为
  **唯一解析实现**，`parse_toml_sections` / `merge_module_and_ident_sections` / `parse_danger_sections`
  三个旧入口收敛为它的视图，加载器改走 `ParsedDocument`（一个文件只解析一遍，而 zh 包 26 个文件
  原本要解析 2~3 遍），debug 下 `load_from_dir` 20 次由 1.567s 降到 0.956s、`load_from_builtin`
  1.229s→0.861s（ja 0.938s→0.553s）。字典序与短词优先两个候选规则被数据否掉：ja/ko/ar 的
  字典序首位是格式化区短词（`表示`/`표시`/`عرض`），而六语言书写序首位都是特征区主词条。
  `check-lang-packs.py` 不加「同英文值多母语名」告警：421 项绝大多数是有意保留的同义写法，
  告警只会淹没清单，改为写进 `docs/contributing-lang-pack.md` 2.9 的「主词条在前」约定。
  影响面：11 语言反查表**键集与旧版完全一致**（无少译），改值 zh 36 / ja 36 / ko 31 / ru 36 /
  hi 25 / ar 22（en de es fr pt 为 0），zh 的 `Display` 取回可显示、`String` 取回字符串、
  `clone` 取回克隆。实机同一沙箱项目各跑 20 轮：修复前 E0277 译文 11:9 翻转，修复后 20/20 稳定。
  门禁：新增 `synonym_winner_is_deterministic_and_follows_pack_order`（六语言硬写 `Display` 主词条，
  再比对两次独立加载与目录/内置两口径，失败只列差异项而不打整表）与
  `reverse_candidates_cover_every_forward_entry`（候选漏收某来源时拦住「译文凭空多出英文」）；
  `injected_rustc_types_are_still_localized` 的期望值由「本进程真表反查」**改回硬写字面名**，
  恢复防回退力度。反证两组：`build_type_map` 退回遍历 HashMap → 两条门禁同红（且直接量化出
  zh 45 项跨进程差异）；节内条目改按母语键字典序 → ja 断言转红（`表示` ≠ `表示可能`）。
  另补 `parsed_document_views_match_standalone_functions`（zh/ja/ar 逐文件比对一次解析与旧三入口
  的视图一致）与 `parsed_document_skips_non_string_entries_like_legacy_parser`（非表节/非字符串值/
  空节的跳过口径一致）。语言包 TOML 一行未改，正向映射一行未改
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

- **（2026-10-29 已修）** E0277 通用码模板套坏 `?` 的专用消息（en 第 16 章采集时发现）：rustc 关于 `?`
  的两条消息（"the `?` operator can only be used in a function that returns
  `Result` or `Option`" 与 "`?` couldn't convert the error to `X`"）本身就带 E0277 码，
  `render_main_message` 却让错误码表优先，并按反引号 token 顺序回填 `{类型}`/`{特征}`
  ——前者把 `?` 本身当成「类型」，渲染出 "the trait bound `?`: `Result` is not
  satisfied"（语义不通），后者渲染出 "the trait bound `?`: `SignupError` is not
  satisfied"。落地时把「精确命中优先」收紧成可辩护的规约：**消息表命中整句（精确键或通配段键）
  且码表桩含 `CODE_TEMPLATE_PLACEHOLDERS`（靠 token 顺序猜槽位）时用消息表，其余维持桩优先**
  （`diagnostic::message::message_table_wins`，CLI/LSP 共用）。三次量化决定了这条边界：
  放开「任意形态命中都优先」→ zh 语料 12 条翻转、其中 6 条把半句前缀键（「特征约束 `」
  「无法借用 `」）之后的英文原话拼回译文；放开「整句命中就优先」→ `[E0308]` 丢期望/实际、
  `[E0283]` 丢「存在多个可满足要求的实现」；加上桩形态条件后现有主消息语料**0 翻转**。
  11 语言各补两条 `?` 键（第二条是通配段键，`{q1}` 是目标错误类型），en 破例收录
  **同文守卫键**（恒等语言不许做真翻译：模板把 `{qN}` 还原成 `?` 后必须与键逐字相同）。
  门禁：`question_mark_e0277_messages_bypass_token_guessing_stub`（11 语言 × 两条消息，断言
  ≠ 桩的按序回填、无裸占位符、有提示、CLI=LSP、zh 硬写译文）、反证
  `precise_code_stub_keeps_priority_over_coarse_message_entry`（E0308/E0283/前缀键仍走桩）、
  `en_pack_code_templates_reproduce_rustc_own_wording` 扩两条、恒等语言门禁改名
  `identity_language_packs_carry_only_identity_guards`，语料新增反例
  `tools/diag-corpus/fixtures/e0277_question_mark.rs`（用自定义 `SignupError` 避开一条
  无译文的 `From` help）。反证三组：去掉桩形态条件 → E0308 断言转红；`whole_sentence_hit`
  接受前缀/后缀键 → 语料门禁 10 语言 60 处英文残留；删 en 两条守卫键 → en 又渲染出
  "the trait bound `?`: `Result` is not satisfied"
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
  11 语言同步并走 `tools/check-ui-keys.py` 门禁。**已修（2026-10-29）**：实现上由 span
  标签区分两端（`BorrowConflict`），新键 `diag_ownership_borrow_conflict` 只陈述「可变第
  几行 / 不可变第几行重叠」，不暗示先后；两种 span 形状（for 循环里改、先借后改）实机
  行号均正确，zh/ja/ru/en 四处教程旧输出已换成实机数字


- 「按需求描述给 API」检索（2026-10-28 做过命中率样品，**结论：暂不实现**）：用 20 条高频
  中文需求跑现有语料（候选池 = zh stdlib 标识符 715 + keywords 宏/声明/控制流/类型，共 807
  条），top-5 命中仅 **8/20 = 40%**；给其中 29 个 API 手写「需求说法」后升到 70%，但换成
  闭卷（10 条训练 / 10 条不准备）反而从 70% 掉到 **60%**——语料不覆盖全池时，加字段会让检索
  变差。另两条实测事实：现有中文解释只覆盖 104/807 = **12%**，且 20 条需求的目标 API 里 8 个
  连一句说明都没有；同一份语料仅改打分归一化口径就能从 70% 跳到 100%，所以任何绝对命中率在
  有人工判定评测集之前都不可引用。真要做的门槛是：① 索引必须一次吃进 stdlib/keywords/
  third-party 三张表（`println` 只在 keywords 宏段，单表方案不成立）；② 唯一键取
  `(英文名, 路径)`，方言名只做倒排入口（`join` 一名两义 → `连接` 与 `等待线程` 互相污染）；
  ③ 先清畸形方言名（`unwrap_or_else` → `取值或_else`）；④ 语料由 `mapping_gen/ai.rs` 现有
  管线批量产出，但必须全池覆盖 + 示例进编译门禁才允许对外；⑤ 输出形态给候选 + 出处 +
  反问，不写「就该用这个」。运行时调 LLM 作默认路径已否决（不可复现、门禁失效、离线包作废）

## 本目录

| 文件 | 性质 |
|---|---|
| [README.md](README.md) | 本页：现状与路线图（随进展更新） |
| [发布准备清单.md](发布准备清单.md) | 维护者凭据类动作清单（全球分发第 1 周执行项） |
| [archive/](archive/README.md) | 历史评估快照（只读） |

## 记录原则

- 新评估 / 新方案写新文件，并登记到本页；被取代的旧版移入 `archive/`，在其 README 备注结论；
- 本页只保留"当前有效"的结论，任何执行流水写 [CHANGELOG](../../CHANGELOG.md)。
