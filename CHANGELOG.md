# 更新日志（Changelog）

所有显著变更记录于此。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

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
