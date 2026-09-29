# 更新日志（Changelog）

所有显著变更记录于此。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.8.3] - 2026-09-29

### 修复
- LSP：主消息翻译不再只走 `errors.toml` 消息表，改为**复用引擎与 CLI 同口径的
  `DiagnosticTranslator::render_main_message`**（错误码表优先 → 消息表 → 占位符
  回填 → `type_map` 类型中文化）——此前 CLI 对同一诊断给「类型不匹配：期望…实际…」
  的完整桩而 LSP 只给略短的「类型不匹配」且不中文化类型名；现 LSP 服务器启动时由
  映射管理器经共享的 `build_type_map` 装填 type_map 并构建 `DiagnosticTranslator`，
  镜像检查（真实 rustc JSON）额外透传主 span 标签以回填期望/实际，与 CLI 逐字一致
  （新增 parity 单测 `cli_lsp_main_message_parity`；无法完整回填时回退消息表而非英文，
  保证译文不劣于旧行为）；rust-analyzer 直连路径亦从 `relatedInformation` 归集
  「expected …, found …」候选标签喂给同一渲染器，令 E0308 等类型错误在编辑器内也
  拿到与 CLI 一致的完整译文（新增单测 `test_find_expected_found_label_from_related`）
- CLI：修 `entry_output_path` **越界写出**——此前凡入口词干（`main`/母语主函数词）
  的源文件都聚合到 `project_root/src/main.rs`，导致子目录/示例里恰好名为 `main` 的
  文件（`src/sub/main.zh`、`examples/main.zh`）经 `rzc check` 会越界覆盖宿主真实入口；
  现仅当文件父目录确为 `project_root/src`（src 直属入口）时才聚合，其余产物跟随自身
  路径（新增回归单测 `test_entry_output_path_no_clobber_outside_src`）
- 引擎：修复**单 `?` 通配段键永远匹配不上**——`match_segments` 的 `rest_literals.len() < 2`
  守卫误拒仅含一个动态段的键（如 `consider specifying…type parameter `?``、
  `variable `?` is assigned to, but never used`），恢复「允许空尾锚」的原算法并只保留首段
  非空守卫；回归单测 `wildcard_segment_single_capture`
- 诊断：修复**审计残留词串检测的假阴性**——`english_word_runs` 把 `` `x` ``、`to,` 等相邻
  分隔符切出的**空串**误当词串边界，使整句未译的英文被拆成 <4 碎串而漏报（实跑对照
  `rzc check` 才发现门禁与用户所见背离）；改为空串跳过、英文词跨标点连续计数，并据此
  补 10 语言 `errors.toml` 8 条此前被掩盖的中英混排缺口（`variable … is assigned to, but
  never used`、`maybe it is overwritten before being read?`、`call … first`、`consider
  introducing lifetime …`、`consider annotating … with #[derive(…)]`、`consider removing …
  from the pattern`、`type … is private`、`remove the whole … item`），消息翻译 142→150 键齐平
- CLI：`rzc doctor` 语言包健康检查补**端到端故障注入测试**（真实文件系统注入同名遮蔽 /
  `.stale…bak` 劫持 / 未加载备份 / 自研包不误报 / 目录名与声明扩展名不符五场景），
  确保四类隐患一次查全
- 诊断：10 语言 `errors.toml` 再补 5 条高频 help 精确键（未使用变量的「if this is
  intentional, prefix it with an underscore」、E0382/E0505「consider cloning the value
  if the performance cost is acceptable」、E0308「consider adding an `else` block…」）、
  E0599「items from traits…」）——由 9 个反例实跑 `rzc check` 采集得到，静态检查查不出
  此类缺口；实测确认另有 2 类消息（E0106 「…borrowed from one of `a`'s 2 lifetimes or
  one of `b`'s…」与 E0277「the trait `X` is not implemented for `Y`」）属「两端动态」，
  现有前缀/后缀匹配器无法完整翻译（已由下述通配段键解决，带 `2 lifetimes` 的闭包
  变体在当前 rustc 已不复现，仅在单测锁定语义）
- 诊断：10 语言 `errors.toml` 补 3 条 rustc help 短语精确键（E0384
  「consider making this binding mutable」/ E0596「consider changing this to be
  mutable」/ E0106「consider introducing a named lifetime parameter」）——此前仅命中
  `consider ` 前缀兜底键，输出「修复建议：考虑 making this binding mutable」这类
  中英混排（首屏演示 GIF 可见）；en 语言包无「消息翻译」节（rustc 原文即英文），无需补
- 引擎：路径根位置不再豁免字段名与变量名（office-zb #27 字段场景）——字段名与
  词表词撞名（如「天数」=Days）时，`天数::新建()` 曾因字段豁免保留「天数」、
  只替换段位（产物 `天数::new` 报 E0433）；现在 `::` 前缀位置只豁免本文件项名与
  项目项名/模块名，字面量与字段访问等普通使用处豁免不变
- 引擎：use 之外的模块词路径前缀转译（office-zb #10/#18）——表达式与属性里的
  `模块词::项`（`异步运行时::睡眠`、`#[异步运行时::主函数]`）在别名阶段回退
  模块路径表（产物 `tokio::sleep`、`#[tokio::main]`）；use 语句仍由模块路径阶段
  处理，别名表命中优先（「路径」=Path 优先于 path 模块），替换值做 crate 名
  连字符规范化
- 引擎：教学 lint「未标注类型」大幅收窄误报——仅当初始化式的类型编译器
  **无法自行推导**时才提示：无 turbofish 的关联构造器（`X::new()`/
  `X::default()` 及方言词「新建」「缺省」）与无 turbofish 的 `.collect()`；
  字面量、普通函数/方法调用、宏、结构体/元组/数组构造、路径、块表达式，
  以及模式解构（元组/结构体/数组解构、让-否则、let 链）一律不再提示
  （真歧义仍有 rustc E0282 兜底）。此前 `让 数量 = 5`、`让 x = 字符串::新建()`
  这类可自动推导的写法在 IDE 里满屏蓝色波浪线；歧义词集由 MappingManager
  从语言包反查（zh：新建→new、默认值/缺省→default），CLI/LSP/引擎三入口同口径
- 诊断：修复 E0583 等模板消息残留**裸占位符**——`{模块名}`/`{变量名}`/`{类型}`
  等占位符此前在模板缺词时直接出现在译文里；现统一从 rustc 原文反引号片段
  按序回填（LSP `render_main_message` 与 CLI `translate_diagnostic` 两路径
  同口径），回填不出时回退 rustc 原文/消息表而非裸占位符（回归单测
  `e0583_module_name_placeholder_filled`）
- 多层模块：rzc 源码管理由扁平布局升级为**支持多层目录**（贴合标准 Rust
  模块树 `src/领域.rs` + `src/领域/工具.zh`）——CLI 递归收集 src 各方言文件、
  非入口产物按自身层级补 `#[path="父词干/子词干.rs"]` 注解并原位写回；
  LSP 镜像检查同步全层级词干与层级注解；虚拟项目（rust-analyzer 分析目标）
  兄弟同步改为按模块树规则 DFS（仅同名子目录入队、visited 防环），聚合 main.rs
  只声明顶层模块，嵌套子声明以 `#[path]` 追加在父虚拟文件末尾（统一 pub 防
  E0603，追加行不影响原文行列映射）。此前多层项目报 E0754/E0583/E0603 与
  连带的「未解析的导入」假红（新增 CLI 集成测试 `test_check_nested_module_project_succeeds`、
  LSP 单测 `test_nested_module_tree_aggregated`、真实 rust-analyzer e2e
  `e2e_nested_module_no_false_diagnostics`）
- 语言包：11 语言 `ui.toml` 键完备性补齐（zh +25 / en +126 / 其余 9 语言
  各 +66，含 `mc_cov_*` 键组段位归位）——修复 en 等语言 `rzc crate --help`
  一类界面直接显示原始键名（如 `cmd_crate_about`）的整域缺口
- 语言包：9 语言 `errors.toml` 各补 14 键（E0583 模块文件缺失 / E0761 同名
  歧义 / trait 未导入 / `expected item, found keyword` 等语法与导入提示的
  教学文案）；ja 修正「分支」中文残留与 `.zh` 扩展名外露（应为 `.ja`）；
  zh 教学提示移除已不支持的 `名字/模.rs` 目录式模块写法
- 语言包：en 与 9 翻译语言 stdlib 覆盖缺口按 zh 基准清零（各 +46~50 行，
  含 `Path` / `from_utf8_lossy` 与 u8「字节」族）；配套修复引擎 use 段
  让位态——词形判定由硬编码 `use`/`使用` 改关键字映射反查（es `usar` 等
  语言让位态此前从未开启，`formato` 一类冲突词残留英文宏名）
- 语言包：crates 映射修正——de/es/fr/ru `salvo` 表 `untertyp_javascript`
  值由展示文案改回常量名 `JAVASCRIPT`；de `standardwert` 更名
  `argument_standardwert`；ru stdlib 解析错误条目改名
- CLI：`doctor` / `cheat` / `toolchain` 子命令帮助文本纳入 `localize_clap`
  本地化（此前 `--help` 硬编码中文，与其余命令的 ui.toml 机制不一致）
- 教程门禁：第十九章白名单条目行号漂移（400 → 401）——该章早前将一处输出
  示例由 1 行改为 2 行，`make tutorials` 曾因此误报失败

### 新增
- 门禁：新增语言包完整性检查 `tools/check-lang-packs.py`（逐包 TOML 真回读、消息/错误码/
  关键词/stdlib 键数结构齐平、占位符形态、版本一致、短前缀吞整句告警），接入 `make lang-packs`
  与 CI `gate` 链
- 门禁：新增**诊断语料回归**——`tools/diag-corpus/collect.py` 把 `rustc --error-format json`
  实跑采样出的高频反例固化为 `crates/engine/tests/data/diag-corpus.tsv`，配 `diag_audit` 示例
  与集成测试 `crates/engine/tests/diag_corpus_gate.rs`（遍历全部内置语言包断言译文零残留），
  把「只有实跑才暴露」的中英混排纳入可复现门禁
- 工具：`collect.py` 采样加 `--out-dir`/`cwd` 隔离，避免 `rustc --crate-type lib` 产物 `.rlib`
  逃逸污染仓库根；`.gitignore` 补 `/lib*.rlib` `/lib*.rmeta` `/rlib/`
- 文档：`contributing-lang-pack.md` 修正 2.6 节诊断匹配顺序（补入「最长通配段」层，与引擎
  及 2.7 一致），并在提交前自查清单加入 `make lang-packs`、`diag_corpus_gate` 两条可执行门禁项
- 引擎：消息表支持**通配段键**（键内 `?` 逐段捕获，第 n 个对应 `{q0}`/`{q1}`/`{q2}`），
  匹配顺序为「精确 → 最长通配段 → 最长前缀 → 最长后缀」——解决 E0277/E0106
  这类**两端以上动态**的 help 无法用单一前缀/后缀键完整翻译的限制；`{qN}` 占位
  编号随之放开到 q2（仅对通配段键有意义，前缀/后缀键行为不变）
- 诊断：10 语言 `errors.toml` 各增 3 条通配段键（E0277「the trait `?` is not
  implemented for `?`」、E0106「…borrowed from `?` or `?`」、「`?` doesn't implement `?`」），
  消息翻译 116→119 键齐平；实跑 `rzc check` 验证中英混排已消（基线整句英文残留 →
  「特征「std::fmt::Display」未对「Foo」实现」），ja 同步抽查通过
- 文档：`contributing-lang-pack.md` 新增 2.7 通配段键节，并记录两个踩坑点：锚点子串
  在消息中出现两次会吞掉整段前缀；含撇号的键不能用 TOML 单引号字面量串（会炸掉整包）
- zh 语言包补条：`stdlib.toml`「懒静态」=LazyLock（office-zb #21）；
  `crates/网络.toml`「取构建器」=builder /「连接超时」=connect_timeout；
  `crates/salvo.toml`「提取」=extract 子模块 /「JSON请求体」=JsonBody
  （office-zb 服务端引入实战：HTTP 客户端封装与 JsonBody 提取）
- 教程：附录 C 补充 if-let 漏写 `让` 与模块路径前缀常见原因；附录 E 新增
  「匹配字段简写为何不转译」FAQ（显式写法 `{ 字段: 绑定, .. }`）
- 教程：开篇补「项目地址」导读框与符号图例「项目」行、第二章与附录 D 的
  获取源码命令改国内 GitCode 优先（`https://gitcode.com/tan80/i18n-rust`，
  国际线路 GitHub 同步保留）——国内读者不翻墙即可找到仓库；三篇已重导同步
  至飞书知识库（目录顺序不变）
- 工程：LSP 热路径基准入库——`crates/lsp/benches/hot_paths.rs`（criterion 4 项：
  `update_document` 打开/连续编辑、`reverse_transpile` 补全片段/整文档）；
  `tools/bench-check.sh` 升级为 engine + lsp 双基准集（组名前缀隔离，支持
  `--only engine|lsp`），LSP 每键热路径纳入回归门禁（基线 ≈1.56ms/0.96ms/0.50µs/73µs）
- 工程：CLI 诊断子系统拆分——`crates/cli/src/main.rs` 抽出 `diagnostics.rs`
  （约 700 行纯移动：直调 rustc 快速路径、cargo 进度行翻译、JSON 诊断解析与
  教学化翻译、位置回译 `DiagLocationFixer`、未声明 crate 提取），与 LSP 侧
  `response_map/diag_text.rs` 职责对称；main.rs 2899 → 2086 行，诊断测试随
  模块迁移（5 项移入 `diagnostics::tests`），行为不变
- 教程：en 第 13 章《特征》入库（en 15/33）——685 行转创（trait 定义/默认
  方法/泛型约束与 `impl Trait` 两种写法/特征对象/关联类型/运算符重载/
  超特征/`derive` 真相），代码块全部实机编译验证；验证器 DECL_WORDS 补
  英文 `trait ` 声明头（纯 trait 定义块此前被误包裹进 main 致编译失败）；
  白名单 +5（与 zh 版同位置），四语言门禁（zh/en/ja/ru）0 失败
- 工程：多语言教程门禁落地——`make tutorials-all`（en/ja/ru 三语依次验证）；
  期望失败清单 `tools/expected-failures.json` 合并三语条目（368 → 908 行），
  白名单过期检查改为按本次实际扫描文件过滤（同一清单跑单语言目录不再
  误报其它语言条目过期）
- 文档站：mdBook 多语言站点骨架（`book/` 装配源 + `tools/build-site.py`）——
  中文（教程/附录/术语表 + 6 份参考文档）与 en/ja/ru 四语言书从仓库 md 原位
  组装到 `_site/`（落地页、404 与 `.nojekyll` 一并生成）；顶栏语言切换脚本
  注入所有语言书；`make site` / `make site-serve` 本地入口；
  `.github/workflows/pages.yml` 部署 GitHub Pages（固定 mdBook 0.4.52，
  与本地同款；启用需在仓库 Settings → Pages 选择 "GitHub Actions"）
- 教程发布：飞书知识库发布工具 `tools/publish-feishu.py`（`make feishu`）——
  经官方开放平台 API 四步流水线（上传素材 → md 导入为 docx → 移入知识库节点）
  将 33 篇中文教程按文档站 SUMMARY 顺序发布至知识库，国内公网免登录阅读
  （语雀公网可见需专业会员、Gitee Pages 已停服）；同名文档幂等跳过、限频与
  网络抖动自动重试；正文首行 H1 默认移除（`--keep-h1` 保留）；`--dry-run`
  预览 / `--only <子串>` 定向重导 / `--force` 全量重发；App Secret 仅经环境
  变量传入不落盘；平台侧一次性配置与核验见[发布准备清单](docs/strategy/发布准备清单.md)第 5 节
  ——2026-09-26 首次全量发布完成：33 篇 0 失败、顺序与文档站一致，知识库已开启
  「互联网公开」（外部访客免登录可阅读）；建库默认「首页」节点已移除，
  空间链接自动跳转首篇
- CLI：程序 stderr 流式本地化——运行子进程（cargo 与直调 rustc 两条
  路径）的 stderr 统一经 `StreamTranslator` 过滤：cargo 进度行照旧翻译；
  运行时 panic 框头（`thread 'main' (TID) panicked at file:line:col:`）
  本地化——线程 ID 剥离、主线程名映射为各语言主函数词、自定义线程名保留；
  六型 panic 消息（下标越界 / 字节索引越界 ×2 / 字符边界 / RefCell 借用
  冲突 ×2）与 `RUST_BACKTRACE` note 行按 ui.toml 模板翻译，其余行原样
  透传（lossy 解码不断流）；stdout/stdin 保持继承，交互式程序不受影响

### 工程
- 本地门禁统一入口 `Makefile`（`make gate` = CI test job 全链，`make help`
  列出全部目标）；新增 CONTRIBUTING.md（开发环境 / 提交规范 / 测试要求）
  与 GitHub Issue 模板（bug 报告 / 功能建议）
- 发布产物随附 `SHA256SUMS` 校验和（GitHub Release 附件与 GitCode Release
  均自动包含），GitCode 附件上传后自动回查核验
- crates 模块文档规范化：engine / CLI / LSP 模块级 `//!` 文档补全与措辞修订
- 语言包 / README 门禁三件套：`tools/check-ui-keys.py`（ui.toml 键完备性：
  11 语言与 zh 对齐 + `{}` 占位符一致性）、`tools/check-mapping-warnings.py`
  + `tools/mapping-baseline.json`（mapping check 告警数只减不增，基线 2 条
  为已明示的结构性差异）、`tools/check-readme-parity.py`（9 份翻译 README
  结构 parity：章节数 / GFM 锚点有效性（含 Unicode Mark 类）/ 围栏配对）
- `make gate` 升级：教程验证由 zh 单语扩为四语 `tutorials-all`
  （zh/en/ja/ru），并纳入 `ui-keys` / `readme-parity` / 告警基线三项门禁
- CI test job 同步强化：新增 ui.toml 键完备性与 README parity 步骤、教程
  验证循环 en/ja/ru、mapping 门禁改经告警基线脚本；bench 作业首次失败自动
  重跑一次（共享 runner 噪声）后仍超阈值才判失败
- 新增生产 panic 面门禁 `tools/check-prod-panics.py`：扫描 `crates/*/src` 下
  `#[cfg(test)]` 区域外物理行的 `.unwrap()`/`.expect(`/`panic!`/
  `unreachable!`/`unimplemented!`/`todo!`（自带词法器排除行/块注释、裸串、
  字节串、字符字面量，tests.rs 与 tests/benches/examples 整目录豁免，
  白名单仅 1 条启动期语言包加载 `.expect`），接入 `make gate` 与 CI——
  防止向用户发布路径新增「运行时 panic 即崩溃」面；附 7 个合成对抗样例
  锁定词法器行为
- 基准门禁抗噪声加固：`tools/bench-check.sh` 新增 `BENCH_RUNS` 多遍机制
  （逐遍收割 criterion `estimates.json` 均值并逐项取最小值，消除 runner
  偶发抖动被当回归），回归阈值 100%→40%（真实回归必破、抖动不破），
  CI bench 作业以 `BENCH_RUNS=3` 三遍聚合
- `check-readme-parity.py` 目标语种由硬编码 9 语种改为 glob `README.*.md`
  目录派生（排除基准 en）——新增翻译语种时不可能再漏配门禁；新增
  `--lang` 显式覆盖参数，空目标告警并以非零退出
- 胶水层补测 30 项（crate_registry 双布局/索引损坏/清单往返、mirror_check
  根目录发现/目录净化/树复制排除项/span 行列换算、install 哈希与压缩包
  往返、mapping_gen 早退路径、LSP 语言包回退/配置形状/`is_symlink` lstat
  语义/进程探活/虚拟目录幂等），工作区行覆盖 74.2%→76.16%，CI 覆盖率
  门禁 70%→75%
- 覆盖率门禁把 7 条 `#[ignore]` 的 LSP 端到端测试纳入统计：coverage job
  与 lsp-e2e 同口径先 `rzc install toolchain --ra-only` 装 rust-analyzer，
  再以 `--include-ignored` 跑真实协议链路（子进程 LSP 二进制覆盖率经
  LLVM_PROFILE_FILE 合并），工作区行覆盖 76.16%→**79.95%**、分支
  78.65%→81.96%（server.rs 28.56%→55.86%、mirror_check.rs 50.14%→
  87.54%），阈值上调到 79；rust-analyzer 缺失时 e2e 静默跳过会令覆盖
  率跌破阈值显式失败，不会假装通过
- 覆盖率第二档补测（纯函数边界缝 + 子命令集成），工作区行覆盖
  79.95%→**86.11%**、分支 81.96%→**86.56%**（server.rs 行
  55.86%→82.19%、diagnostics.rs 91.69%、mapping_check.rs 85.98%、
  mapping_source.rs 91.54%、responses.rs 92.89%），CI 阈值 79→85：
  - CLI 集成测试 `tests/cli_basic.rs`（22 项）：init 骨架、直调
    rustc 路径 run（输出「输出：42」）、好/坏程序 check（E0384
    本地化 + 💡）、eject、doctor、mapping check/coverage/scaffold、
    本地语言包装/列/删/--force/路径逃逸拒绝、缺源码文件干净失败；
    HOME 经临时目录逐进程隔离，mapping coverage 用例固定 cwd 为
    工作区根（`find_project_root_upward` 命中首个 Cargo.toml 即停）
  - `diagnostics::tests`（模块内 19 项）：cargo 五类进度前缀/panic
    框头畸形拒绝/源码行边界/产物路径解析顺序/非方言位置直通/子模块
    方言 .zh→.rs 回译/E0432 教学翻译/语言包损坏降级内置表/
    cargo_ok×silent 四组合空分支
  - `mapping_check::tests`（模块内 33 项，新增 5）：节解析错误、
    干净/警告/失败三态报告与未知 code 渲染不 panic、scaffold 首跑
    写 TODO 复跑幂等跳过、crates 目录覆盖与回退、check 目标三分支
  - engine `mapping_source::tests`（新增 4）：FileMissing/
    ParseFailed 错误分类、第三方 crates 多文件多子节归类（非 toml
    忽略）、load_all 的 module_paths 仅补 stdlib 缺失键（stdlib
    同键优先）、分类元信息与 UTF-8 文件名快路径
  - LSP `responses::tests`（新增 2）：补全响应全链路（label/
    newText/insertText/detail/labelDetails 母语化、方法项 snippet
    补括号、严格母语过滤丢弃未翻译第三方英文项、用户英文标识保留、
    文档命中「解释」表白话替换）、null/空 items 不 panic
  - LSP `server::tests`（模块内 21 项，新增 7）：analyzer 响应
    13 方法分发（含未知方法静默）、错误码透传（空 error 落
    -32603）、虚拟 URI 诊断路由回 file:// 方言 URI、杂项通知转发、
    主动请求回送 RA 不进客户端通道、超时条目只清理过期者、
    restart 时全部待办以 "rust-analyzer restarted" 排空；新增
    `#[cfg(test)] Sender::null_for_test` 测试缝（writer 恒 None）
  - 明确不补：CLI 安装/registry、mapping auto 生成等 AI/网络/下载
    路径与需真实 TTY 的首启引导，继续由 e2e 与人工验证覆盖；
    锁定语义不变——编译错误只随 didOpen/didSave 镜像检查发布，
    分发/超时/重启的应答行为经本档单测固定
- LSP e2e：修正 ru/ar 两语言 E0425 特征词陈旧——新版 rust-analyzer
  （2026-08-24/0.3.3025）对未解析名诊断直接上报 `code: "E0425"`，LSP 改
  走错误码表渲染（ru「Имя … не найдено в этой области видимости」、
  ar「الاسم … غير موجود في هذا النطاق」），测试却仍在等旧短语替换路径的
  «найти значение» / «العثور» 而超时失败；关键词改为取自 `[E0425]`
  消息模板并收敛为 `LANG_KEYWORDS` 单一事实来源（新增 `lang_keyword`
  查找函数，四个语言测试不再各自硬编码，杜绝再次漂移）
- 审计热路径补对抗性不变量测试：`match_segments` 单/双/三捕获、空首锚
  拒绝、空捕获返回 `Some([""])` 等 17 例（`match_segments_adversarial`）；
  `english_word_runs` 固定「4 词阈值、纯数字断词、非 ASCII 空串不断词」
  的宁可误报语义（`english_word_runs_threshold_and_splitters`）；fuzz
  `source_strategy` 注入 `?`/`` ` ``/`::`/中文路径/`->`/`=>` 对抗词；
  入口聚合补真实 tempdir 回归（`test_entry_output_path_aggregates_direct_src_entries`，
  锁定 src 直属多文件聚合、非入口不聚合、ghost 路径词法回退三条不变量）
- 发布闭环：release.yml 新增 `publish-extension` job，推 tag 后同一 vsix
  自动发布 VS Code Marketplace 与 OpenVSX（仅 GitHub 镜像仓库执行避免双发；
  `VSCE_PAT`/`OVSX_PAT` 缺失各自告警跳过、不阻塞 Release）；演示 GIF
  已用 vhs 实录入库 `docs/demo/demo-zh.gif`（309KB，三场景：母语源码 →
  教学报错 E0384+💡 → eject 标准 Rust），中/英 README 顶部引用生效

### 新增
- CLI：`rzc doctor` 新增语言包健康检查——全局目录（`~/.rz/lang-packs/`）下报三项：
  同名包遮蔽内置表（附两侧版本与 `rzc lang remove` 解除命令）、备份/改名遗留目录
  仍被当语言包吃掉、目录名与声明扩展名不符；第三方自研包（目录名与扩展名一致，
  如 `vi`）不属异常不报警。遮蔽与译文陈旧均只在用户环境发生，静态门禁查不出，
  现可一条命令自查（判定为纯函数 `lang_pack_health_lines`，附 6 项单测）

### 文档
- 语言包贡献指南补两类「只有实践才暴露」的陷阱：2.5 全局旧语言包遮蔽内置表（含改名
  遗留目录仍被当语言包吃掉、`rzc doctor` 自查示例输出）、2.6 诊断译文缺口静态检查查不出
  （含取 help 原文与免重编译试键的命令、前缀键吞掉整句与前缀优先于后缀两个误用点），
  并加入提交前自查清单
- 首屏演示 GIF 入库：`docs/demo/demo-zh.tape` 经 charm vhs 渲染产出
  [docs/demo/demo-zh.gif](docs/demo/demo-zh.gif)（1080×620、约 12.8 秒、262 KB，三幕：
  母语源码 → 母语教学报错 → `rzc eject` 标准 Rust），挂在 11 份 README 主标题前作为
  首屏展示（alt 文本按语言本地化）；tape 补 `Set FontFamily "Noto Sans Mono CJK SC"`
  避免中文出方块，`docs/demo/README.md` 补全实测安装步骤（ttyd 须进 PATH、GitHub 443
  不通时的加速代理）与两个陷阱（转译缓存命中使教学 lint 跑到第三幕、陈旧全局语言包
  遮蔽内置新词条）
- 维护者入口整理：新增 [项目地图](docs/project-map.md)（任务→文件→命令速查 +
  目录/工作流/门禁/文档体系一页）与 [tools/README.md](tools/README.md)（脚本索引，
  `make` 为本地唯一入口）；`docs/strategy/` 收敛为"现状与路线图 +
  发布准备清单"（原《推广方案》已移除），5 份历史评估快照移入 `docs/strategy/archive/`
- README（中/英）首页改版：新增「从这里开始」导航、最短路径安装、与"玩具语言"
  的对比表、在线文档站入口、多语言教程进度与 Star 引导
- README（中/英）安装节改版：crates.io 一键安装（`cargo install rzc`）提为推荐
  方式并加版本徽章，源码编译降为方式二（开发者）
- 演示素材：`docs/demo/` 三语言 [vhs](https://github.com/charmbracelet/vhs) 录屏
  脚本（`demo-{zh,ja,ru}.tape`）与演示样例（实机验证：命中 E0384 教学报错、
  eject 导出成功），附 GIF 制作指南与规格（15 秒三幕挂载 README 首屏）
- 教程与文档过时内容修正：zh 教程入口文件名全量统一为 `main.zh`（对齐
  `rzc init` 产物，含 `.zh-demo` 重命名）；ja/ru/en 第一章补 `.vsix` 离线
  安装指引、ja 项目命令修正为 `rzc init --lang ja`、rustc 示例版本更新为
  1.98；教程第一章能力表编号、附录 D 命令示例与附录 E 迁移对照表修正；
  engine README 示例 API 修正；strategy 文档同步 crates.io 0.8.2 发布状态
- README（中/英）「从这里开始」页内锚点修复：GitHub 与 GitCode 的标题 id
  规则不同（GitHub 保留 emoji 后的前导连字符，GitCode 整体去除并 trim），
  统一为「emoji 紧贴标题文字 + 无连字符锚点」写法（`## 📖配套教程` ↔
  `#配套教程`），快速开始 / 配套教程 / 参与贡献 三个导航锚点双平台可用
- 中文 README 文档站入口补充国内访问提示：GitHub Pages 托管、国内网络
  可能无法稳定访问，替代路径为仓库内直接阅读 `tutorials/`
- 9 份翻译 README 全量重建：ja/ru/de/es/fr/pt/ko/ar/hi 由早期快照（约
  150 行）同步为与 zh/en 同构正文（9 章节 + 徽章 + 🪞 镜像说明 + 命令速查
  22 条 + 功能特性 6 小节 + 「从这里开始」5 链接导航含飞书入口）；错误示例
  按各语言包实机采集真实 E0384 输出，映射词逐语言核对
- 教程：恐慌输出示例四语言与产品实机输出对齐（zh / ja / ru 为本地化格式，
  en 标准格式；字节索引消息同步 rustc 1.98 实测）；第一章安装步骤重写为
  crates.io 优先并整理「第二步」结构；「网盘」字样清理、语言包计数
  （11 = 10 自然语言 + en 恒等包）校对
- 文档：translation-status 新增「第三方库映射（crates/ 覆盖差异）」节
  （zh 独有 6 表 / 9 翻译语言共享十表 / en 恒等包折叠的差异明示与补齐
  路径）；third-party-mapping §6 对应重写
- 分发渠道收敛：今后不再使用百度网盘——移除全部网盘分发措辞，离线包与
  `.vsix` 统一经 GitHub / GitCode Releases 分发（README 中/英及 9 翻译版
  「给发布者」、教程附录 D 中/en、`release-offline.sh` / `.ps1` 提示、
  release.yml 注释、`rzc doctor` 安装提示同步更新）

## [0.8.2] - 2026-09-21

### 新增
- zh 语言包新增三张 crate 表：`crates/电子表格.toml`（rust_xlsxwriter 写入 /
  calamine 读取 / umya_spreadsheet 读写）、`crates/对话框插件.toml`、
  `crates/自启插件.toml`（office-zb 实战：电子表格台账与 Tauri 插件化）
- zh 六表补条：mysql（事务选项 TxOpts / 迭代查询 / 列元数据）、tauri（插件注册 /
  事件广播 Emitter / 窗口事件）、密码学（new_from_slices / decrypt_padded_mut）、
  工具（chrono 时区专名与 from_timestamp）、序列化（serde_json Value 取值族）、
  数据库（rusqlite 只读打开 / ValueRef 取列引用）
- zh `stdlib.toml` 补条：路径类型 Path / 条目路径 / from_utf8_lossy / metadata /
  modified / output / to_vec / extend_from_slice / 是数字字符 / u8「字节」等；
  `module_paths.toml` 补「环境」→ env
- `mapping check` 新增 keywords.toml 可解析性与必需节完整性校验——缺节不报错但
  整类词条静默失效（缺 `["宏"]` 时宏调用不再补 `!`，产物非法却仍报转译成功），
  是最难排查的一类静默降级；11 语言 ui.toml 同步 `mc_missing_section` 词条
- VS Code 扩展：AI 关键设置（provider / baseUrl / apiKey）限定用户级——不可信
  仓库曾能把 baseUrl 指向第三方使明文密钥外发；激活时对历史工作区覆盖给出警告

### 修复
- cli：入口产物路径修复——词干为入口主函数名（`main` 或语言包主函数词，如 zh
  「主函数」）才聚合写入 `src/main.rs`；`build.zh` 等其余文件跟随自身名
  （`build.rs`），修复 build 脚本被静默覆盖为项目入口（weix 工具异常 #4）；
  母语主函数名词干同样聚合，修复 zh 教程 / 示例项目 `src/主函数.zh` 产物不落
  `src/main.rs` 致 cargo 报「no targets specified」
- cli：`resolve_rustdoc` 按 rustc sysroot 定位配套 rustdoc + cargo build 显式注入
  RUSTC——根治与 cargo 编译产物跨版本的链接 E0514
- cli：rust-analyzer 安装完整性校验不再降级——官方 digest 不可用（API 限流 /
  被阻断）时中止安装，阻断一次 API 请求即让任意二进制入装运行的路径随之关闭；
  ZIP 条目校验升级 safe_zip_entry_path（反斜杠归一化 + Windows 盘符前缀显式
  拒绝）；AI 响应体加 8 MiB 读取上限（timeout 不限字节数）
- engine：修复 `let 名: 类型` 类型标注含 `[`（切片 / 数组）时类型内别名被误收为
  值绑定、整文件豁免替换（weix-1 #15，E0425 假红根因）
- engine：缓存覆盖路径补置脏——磁盘副本不再残留旧产物；新增
  `combine_fingerprint`（`None` 与 `Some(0)` 不再因异或混同）
- lsp：hover 标题行仅为 `**` / `***` 时不再越界切片 panic（客户端源码文档注释
  可构造）；虚拟目录符号链接校验补「创建后复核」（覆盖创建前的 TOCTOU 窗口）
- vscode-extension：Windows 命令注入残留根治——按实际终端 shell（PowerShell /
  cmd / Git Bash 等）分派转义，此前按 os.platform 二分在 PowerShell 下失效；
  插件薄弱点加固（终端按 cwd 复用表 / 密钥迁移逐层容错 / 自动重启 10 分钟
  时间窗口 / 语言包探测共享模块，删除硬编码开发机路径）

### 性能
- engine 转译缓存写盘改为脏标记 + `flush` 批量（此前每次插入全量序列化写盘、
  N 文件 N 次写且发生在调用方持锁期间）；lsp 单文件声明收集按内容哈希缓存
  （每次按键仅重算内容变化的文件）；宏表 / 派生表改返回引用（消除逐次全表克隆）

### 工程
- cli 测试沙箱隔离（HOME / USERPROFILE / RZ_LANG_DIR 指向空目录——开发者机器
  的用户语言包与缓存不再干扰计数与输出断言）；mirror_check 补 9 例纯逻辑单测
- CI：基准回归对比超阈值失败附加工作流注解（明细匿名可读，区分 runner 负载
  波动与真实性能回归）
- 教程附录 D（zh/en）crate install 命令修正为 `--lang` 形式；README 语言包
  表述（11 内置包）与命令速查表同步

## [0.8.1] - 2026-09-18

### 新增
- zh 语言包新增 `crates/密码学.toml`：sha2 / md5 / hmac / pbkdf2 / aes / cbc / zeroize /
  ed25519_dalek / getrandom / hex 共 10 个 crate 的「模块路径 + 标识符」词条 40 余条
  （含 Aes256 / 分组解密器 / NoPadding / SigningKey / Signature / OsRng 等；weix-1
  全量中文化实战定稿）
- zh `工具 / 数据库 / 日志 / 命令行` 四表补条（rand_core / OsRng、SQLite 取值类型与
  绑定参数、tracing-appender、clap 补全等；weix-1 实测）

### 修复
- lsp：诊断与文档的关联升级为路径级（不再依赖 URI 字符串相等）——镜像检查的
  诊断发布 URI 复用缓存条目的客户端原样 URI（与编辑器打开的文档严格一致），
  教学注入与虚拟 URI 反查增加归一化路径兜底（容忍 Windows 盘符大小写 /
  分隔符 / verbatim 前缀差异）；修复 Windows 上因 URI 形式不一致导致诊断
  发布到编辑器不可见的 URI、教学提示缺失的问题
- lsp：教学诊断注入统一到三条诊断发布路径（RA 链 / 虚拟项目检查 / 镜像检查）——
  注入函数迁入 `response_map::diag_text`（`inject_teaching_diags`），由 entry 内容
  直接计算全角标点与教学 lint，不再依赖内置诊断缓存合并的时序：代理自跑检查
  （虚拟/镜像链）先于 rust-analyzer 首批发布时缓存为空，此前教学提示会随批次
  丢失（CI e2e 三平台失败根因）；e2e 全角断言锚点精确为「检测到全角标点」，
  与 rustc/RA 翻译文本（「混入了全角标点」）不再撞车；e2e 超时转储新增全部
  批次 URI 统计，CI 失败关键日志转为工作流注解（annotations API 匿名可读）
- zh `stdlib.toml`：`"线程数" → "available_parallelism"` 键修正为「可用并行数」——
  「线程数」与用户项目字段名高频撞车，且声明位豁免 / 访问位替换的单侧不一致会产出
  E0609（weix-1 实测）；新键与 fr 包 `parallelisme_disponible` 语义对齐
- engine：`crate::` 前缀（qualify）遮蔽豁免——模块名与文件内类型声明名 / use
  导入绑定名重名时，裸路径首段保持本地语义不加前缀（`结构体 项目配置` 与
  `模块 项目配置` 并存时，`项目配置::缺省()` 不再误产出
  `crate::项目配置::default()`——weix-1 E0425 假红根因）；含 glob 导入
  （`use …::*`）时项目项名集合一并豁免
- lsp：误报根治三步实施（weix-1 复现验证假红清零）——(1) 虚拟项目高保真化：
  同目录全部兄弟方言模块自动聚合（跨文件引用不再依赖逐个打开），
  `include_str!` / `include_bytes!` 引用资源按相对路径复制入虚拟项目，从源头
  消除 E0432/E0433 与 couldn't read 误报；(2) 权威诊断切换：打开 / 保存即
  执行「真实项目镜像」cargo check（项目树复制 + 方言转译产物覆盖 + 非 ASCII
  模块 `#[path]` 注解 + `--offline` 复用真实 target），rustc 口径诊断按方言
  坐标回译发布；镜像不可用时回退虚拟检查，RA 侧自跑 cargo check 关闭避免
  重复；(3) 编译级假红抑制：RA 链的 E 系列 error/hint（severity 1/4）不再
  转发（编译诊断以镜像为权威），教学 lint 与「rzc add」添加依赖提示完整保留
- lsp：诊断过滤链升级——E0433/E0432 覆盖 RA 新版消息格式（`cannot find X in
  crate` / `unresolved import crate::X` 及 severity=4 同伴 hint）、第三方依赖
  在虚拟项目缺失（对照用户 Cargo.toml 依赖表，`-`/`_` 归一化与紧致形式，
  另含 workspace/target 各节）、include 资源缺失三类误报过滤

### 工程
- lsp：多模块项目 e2e 语料测试「无假红」回归（E0583/E0754/E0432/E0433/
  couldn't read 五类历史误报 + 真错误锚点 E0425 不被误杀；rust-analyzer
  升级必跑）
- engine/cli：非 ASCII 模块名 `#[path]` 注解迁入引擎（`annotate_non_ascii_mods`），
  CLI 与 LSP 镜像共享同一实现

## [0.8.0] - 2026-09-17

### 新增
- VS Code 扩展新增「转译产物显示开关」：设置 `i18n-rust.hideGeneratedFiles`
  或命令「i18n: 显示/隐藏转译产物」一键把转译生成的 `.rs` 与 `.rs.bak`
  从资源管理器隐藏/恢复（写入工作区 `files.exclude`，关闭即移除条目），
  避免大量产物文件干扰视线；以 `.zh` 等方言文件为源码的项目适用
- engine/cli/lsp：教学 lint 第 4 条规则「易混方法名」——方法调用位未命中映射表的
  长中文串（≥2 字）存在编辑距离 ≤1 的相近词时提示（`.拉平()` → 建议「展平」；
  词表为关键字/宏/派生/模块路径/别名五表键并集）；CLI 与 IDE 诊断同步提示，
  `--no-lint` 可关闭（weix-1 #14）
- 10 个内置语言包（zh/ja/ko/ru/de/fr/es/pt/ar/hi）新增 `lint_confusable_method`
  UI 词条

### 修复
- engine：match 臂模式回溯越界修复——带块臂体（`=> { … }`）的下一臂在收集
  模式绑定时会穿过上一臂的块闭合 `}`（深度失衡后 `,`/`;` 不再终止），把上一臂
  语句里的类型标注（`让 x: T = …` 的 `T`）等裸标识符误收为值绑定 → 这些名字
  全文件豁免替换，`无符号机器整数`（= usize）实测不被翻译（E0425，weix-1 实测）；
  修复为识别「臂块体闭合」并即时截止——块体臂逗号可省略，该 `}` 即下一臂模式
  左边界；结构体模式 `点 { x }`、`点<T> { x }` 的 `}` 不受影响（weix-1 #27）
- engine：方法调用位让位——词法替换值是保留关键字的词（`枚举`→enum）在「前面是
  `.`」时保持原文，由别名阶段替换为有效词条：`列.迭代().枚举()` 产出
  `column.iter().enumerate()`（此前 `.枚举()` 直译为非法的 `.enum()` 编译失败）；
  同机制一并激活 7 个语言包共 12 条方法位死词条（zh 枚举/匹配/循环/函数、ja 親/待機、
  ru дождись/цикл、es esperar、fr attendre、ar لكل、hi मिलान 的别名词条）；替换值
  非保留字的词不受影响（`.文件()` 仍旧 `.file()`）（weix-1 #26.1）
- engine：模式绑定（match 臂模式 / let 解构 / for 模式）纳入声明收集，绑定位命中的
  映射词全文件豁免（与函数/闭包参数一致）：`匹配 有值(入参) { 有值(连接) => 连接 + 1 }`
  产出 `match Some(入参) { Some(连接) => 连接 + 1 }`（此前 `连接` 被误译为 `join`
  导致编译失败）；`for (键, 值) in …`、`for &长度 in …`、`if let Some(连接) = …`、
  `let (甲, 乙) = …` 同修复（weix-1 #21）
- engine：use 段内宏表词与标准库表冲突时让位——`使用 标准库::文件系统::文件 as 库文件;`
  产出 `use std::fs::File as 库文件;`（宏表无位置豁免，此前 `文件` 在 use 段被
  译成小写 `file` 导致编译失败）（weix-1 #16）
- engine：数字与中文类型黏连的拆分查表补齐标准库层——`0无符号机器整数` 产出
  `0usize`（此前仅 keywords 层词生效，如 `0无符号微整数` → `0u8`）（weix-1 #17）
- lsp：诊断消息 "found" 改为语境键——仅类型不匹配（`expected …, found …`）的
  `", found "` 译为「实际为」，E0599 的 `no method named … found for struct …`
  不再出现「实际为」误译
- tools：修复两处工具链临时目录泄漏（/tmp 累积）——教程验证脚本的工作目录
  （每次运行残留 `zrverify_*`）改由 atexit 进程退出统一清理；VS Code 扩展测试
  prompt-builder.test.ts 的临时语言包目录（每次运行残留 3 个 `i18n-rust-test-*`）
  改由 after 钩子在用例结束后删除

## [0.7.3] - 2026-09-16

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
- zh 语言包：keywords.toml 宏节补「测试」→`test`（原仅 stdlib 节有、属性位不生效），
支撑 `#[配置(测试)]` 模块与 `#[测试]` 函数写法；来自 xiaozs 更新检测单元测试实战验证
- 全部内置语言包 keywords.toml 宏节同步补齐「测试」母语词（テスト / 테스트 / тест /
Test / prueba / test / teste / اختبار / परीक्षण），跨语言完整性检查保持通过
- 教程：第十七章 17.13 增 use 花括号 FAQ——use 语句的路径段一律按「模块」解释
  （与模块同名的词无法表达类型含义），给出别名导入与英文原名两种绕法；
  附录E 增「`引用自我` 连写不被翻译」（中文键词须独立成词）与「clap 旗标名
  怎么控制」（裸 `长参数` 取字段名、英文旗标须显式赋值）两条 FAQ；
  均来自 weix-1 项目实测（问题记录 #6/#10/#11）
- zh 语言包：新增 tauri 2.11.5 中文映射（第一版 166 条 API + 系统托盘/菜单
  36 条补充，AI 生成 + 跨文件消歧 + 解释补齐；xiaozs 实战回馈）
- rzc mapping auto：`--target-version` 版本锁定（x.y.z 精确 / x.y / x 前缀锁定，
  映射文件头记录「基准版本」行；未锁定时提示结果不可复现）
- rzc mapping auto：AI 生成长映射自动分批调用（每批 50 条 + 进度提示），
  突破模型单次输出上限（tauri 等数百条 API 场景）
- rzc mapping auto：AI 生成后输出解释覆盖率提示（缺失条数可见，可重跑补齐）
- engine：内置映射清单改为编译期扫描动态生成（单一数据源，语言包增删免改代码）

### 修复
- engine：use 路径中 crate 名连字符规范化——Cargo 包名 `tracing-subscriber`
  在 Rust 路径中须写 `tracing_subscriber`，`使用 日志订阅 as 日志框架;` 不再
  转译出非法路径（weix-1 #1）
- engine/cli/lsp：声明豁免升级为项目级（跨文件）——A 文件 `pub 函数 新建()` 与
  B 文件 `模块::新建()` 调用侧一致，修复 E0599；`rzc check` 与语言服务器共用
  同一项目上下文（声明集合变化自动失效缓存）。豁免语义精确化：路径链内仅项目
  声明名豁免（未定义于项目的成员照常替换）、方法调用位 `表.长度()` 照常替换为
  `len`（字段访问位 `实例.长度` 仍豁免）；结构体字段名与枚举变体名纳入声明豁免
  （撞「值」=values、「保留」=retain 等键时保持原样）（weix-1 #4/#8/#9）
- engine：宏映射词在方法调用位（`.格式化(...)`）不再误补感叹号（weix-1 #5）
- engine：stdlib.toml 补模块路径词表达式位副本（输入输出/集合/网络/原子/核心/
  分配/进程/环境/转换/指针/数字/操作符/数组/泛型/迭代器）与属性词（允许/Unix系统）；
  命令行.toml 补 clap 派生属性词（简介/值名/动作/子命令字段）；日志.toml 补
  EnvFilter 系词条（weix-1 #2/#12）
- LSP：过滤虚拟项目固有的过程宏误报（cannot find attribute / cannot find derive
  macro——虚拟项目禁用过程宏且无第三方依赖所致），辅助属性 `#[arg(...)]` 与派生
  宏 `#[derive(Parser)]` 不再在 IDE 刷红；`unresolved import` 保留（「添加依赖」
  快速修复输入）（weix-1 #3）
- CLI：新增 `--no-lint` 关闭教学 lint 提示（项目开发场景；Unicode 混淆/全角标点
  告警不受影响），11 语言帮助文案同步（weix-1 #7）
- LSP：虚拟项目内容抹除文件式 `mod 名字;` 声明（连带属性/可见性/文档注释，
  1:1 空格替换保持行号列号）——方言 `模块 日志设置;` 不再触发 rust-analyzer
  的 E0583（找不到模块文件）与 E0754（非 ASCII 标识符）满屏红波浪线；
  转译/格式化仍基于未净化原文（新增 ra_content 双内容字段）
- LSP：聚合 main.rs 变更改用 `workspace/didChangeWatchedFiles` 通知
  rust-analyzer 重读（替代工作区移除+重加的重载机制），修复文件监听
  失效时模块文件被判 unlinked-file（“未包含在模块树中”）的误报
- LSP：诊断消息翻译补齐 10 个键（file not found for module / unlinked-file /
  E0754 / cannot find module 等）并修复短语表顺序（长键优先，避免 "found"
  先替换打碎 "file not found"）；过滤“引用未打开模块文件”的 E0433 误报
  （同目录存在同名方言文件时），拼写错误照常提示
- engine/cli/lsp：别名声明位保护——豁免集合补全（用户声明项/变量、函数与闭包
  参数）；「库特征实现块」内方法名照常替换（修复块内方法名不替换、参数名被
  误替换）
- engine/lsp：多文件诊断回译——子模块产物（如 `src/接口.rs`）解析回方言源文件
  （`接口.zh`），按各自列映射回译行列（修复误标入口文件与行列错位）
- engine：磁盘缓存新增引擎源码指纹——转译算法变更自动使缓存失效
- engine：诊断语义修正——构建实败不虚报「编译成功」，运行时失败不补打
  「编译错误」，lint 空行计入行号
- mapping：手动 rustdoc 注入 CARGO_PKG_* 环境变量（修复 tauri 等 proc macro
  展开期读取 CARGO_PKG_NAME 的提取 panic）
- mapping：AI 输出重复节头等不规范 TOML 时宽松解析抢救条目

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
