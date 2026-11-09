# 架构与工程质量评审（2026-11）

> 评审基线：`zh@a0a016c`（方言源分支，58,154 行 `.zh`，1,804 个函数，927 个测试）。
> 评审范围：`crates/{engine,cli,lsp,zh-pure}`、`tools/`、`.github/`、构建与发布脚本。
> 方法：全量静态阅读 + 定向 grep/脚本度量 + 已有基准与 CI 配置交叉核对；未修改代码。
> 优先级：**P0** 安全/数据损坏/关键路径；**P1** 显著质量、性能或流程风险；**P2** 可维护性；
> **P3** 打磨项。难度：**S**（<1 天）、**M**（1–3 天）、**L**（>1 周）。

## 0. 结论摘要

**总体判断**：工程基底明显高于同类个人项目——零正则的 `rustc_lexer` 转译、927 个测试、
fmt/clippy/MSRV/覆盖率/审计/基准/四语教程门禁、三仓镜像、发布树校验都已具备。
当前风险集中在三处：**少数安全边界（路径/下载/LSP 信任模型）**、**LSP 全量镜像与逐键全扫的性能**、
**门禁“存在但不阻断”**。先把这三类止血，再做结构性拆分，收益最大。

### 最优先 12 项

| # | 优先级 | 事项 | 位置 | 预期收益 | 难度 |
|---|---|---|---|---|---|
| 1 | P0 | `rzc crate` 的 `--lang`/库名路径穿越，可写 `~/.cargo/config.toml` 提权 | `crates/cli/src/包仓库.zh:281-292`、`:331-339`、`:526-533` | 消除任意文件写/删与 RCE 链 | S |
| 2 | P0 | LSP 打开即 `cargo check`，会执行不受信项目的 `build.rs`/`.cargo/config` | `crates/lsp/src/服务器.zh:819-821`、`crates/lsp/src/镜像检查.zh:39,244-263,524-533` | 打开恶意仓库不再自动执行命令 | M |
| 3 | P0 | LSP `Content-Length` 无上限分配，单条消息可 OOM | `crates/lsp/src/分析器.zh:305` | 消除 DoS/崩溃 | S |
| 4 | P0 | 托管产物无条件写盘，mtime 全变导致每次 run/check 重编译 | `crates/cli/src/main.zh:2492-2502` | 二次 run/check 省整包重编译 | S |
| 5 | P0 | LSP 每次保存全量删镜像→复制整树→全项目重译 | `crates/lsp/src/镜像检查.zh:99-157`、`服务器.zh:821` | 大项目保存延迟降数百 ms–数秒 | L |
| 6 | P0 | 镜像检查声明收集无缓存，每次对全项目重跑词法 | `crates/lsp/src/镜像检查.zh:349-353` | 百文件项目每次保存省数十–数百 ms | M |
| 7 | P0 | `main` 必需检查缺 coverage/bench/lsp-e2e | `.github/rulesets/protect-main-release.json:30-34` | 低覆盖/性能回归/三平台破坏不再合入 | S |
| 8 | P0 | `zh` 分支零编译门禁；eject 破坏要等 main PR 才暴露 | `.github/workflows/zh-gates.yml:35-39` | 反馈环从天级缩到分钟级 | M |
| 9 | P0 | `check-prod-panics.py` 只扫 `.rs`，在 zh 直接 exit 1（已复现） | `tools/check-prod-panics.py:87`、`:36` | 恢复“生产零 panic”自动保障 | S |
| 10 | P0 | Release 实际只上传 `.vsix`，三平台二进制与离线包缺失 | `.github/workflows/release.yml:187-226,260` | Release 下载与 README 承诺一致 | M |
| 11 | P1 | 语言包/社区映射下载无 https 强制、超时/大小/摘要校验 | `crates/cli/src/语言管理.zh:65-76,546-583,735-772` | 防劫持转译语义与压缩炸弹 | M |
| 12 | P1 | `--force` 重装不覆盖旧二进制却更新版本号 | `crates/cli/src/安装.zh:214-220,432-443,262-264` | 被篡改的工具链可真正修复 | S |

### P0 修复状态（2026-11-08）

| 事项 | 状态 | 落点 |
|---|---|---|
| 第 1 项：`--lang`/库名路径穿越 | 已修复 | `包仓库.zh` 入口统一校验语言代码与库名，安装/删除前再做规范路径子树检查；补单测 |
| 第 2 项：LSP 镜像执行不受信仓库命令 | 部分修复 | 镜像排除 `.cargo` 与 `rust-toolchain*`，编译检查清空 `RUSTC_WRAPPER`/`RUSTFLAGS` 等钩子并强制离线；`build.rs` 执行的完整信任模型仍待设计 |
| 第 3 项：`Content-Length` 无上限 | 已修复 | `分析器.zh` 设 16 MiB 硬上限，超限拒绝该帧；补常量与日志 |
| 第 4 项：托管产物无条件写盘 | 已修复 | `main.zh` 写前比较内容，相同即跳过；入口产物同样保留修改时间 |
| 第 5 项：LSP 每次保存全量重建镜像 | 已修复 | 镜像增量同步（大小 + 修改时间跳过未改动文件、清理失效条目）；整文件转译按（内容哈希 + 语境指纹）复用，声明集合变化时自动失效并重译 |
| 第 6 项：镜像声明收集无缓存 | 已修复 | `转译缓存::从内容集构建项目上下文` 复用按内容哈希的单文件声明缓存；补同步/清理单测 |
| 第 7–10 项：必需检查、zh 编译门禁、panic 门禁、Release 三平台产物 | 已修复 | `.github/rulesets/*`、`zh-gates.yml`、`check-prod-panics.py`、`release.yml` |

## 1. 安全性

| 优先级 | 位置 | 问题 | 改进方向 | 预期收益 | 难度 |
|---|---|---|---|---|---|
| P0 | `crates/cli/src/包仓库.zh:281-292,331-339,526-533,573-580` | `--lang`/库名未校验即 `连接` 到全局语言包目录；`../` 可写/删任意 `.toml`（如 `~/.cargo/config.toml` → `rustc-wrapper` 提权） | 统一调用 `校验语言代码`；库名限 `[A-Za-z0-9_-]+`；`规范路径()` 后必须仍在 `全局语言目录()` 子树内；拒绝空名/`.`/`..` | 消除任意文件写删与 RCE 链 | S |
| P0 | `crates/lsp/src/服务器.zh:819-821`、`镜像检查.zh:39,244-263,524-533` | 打开文档即镜像整个项目并 `cargo check`；镜像未排除 `.cargo`，会执行 `build.rs` 与项目 cargo 配置 | 默认改为保存时/手动触发，或仅对受信目录启用；镜像排除 `.cargo`；用独立 `CARGO_HOME` 并清空 `RUSTC_WRAPPER` 等 | 打开不受信仓库不再执行其命令 | M |
| P0 | `crates/lsp/src/分析器.zh:296-309` | `向量![0u8; 正文字节数]` 按对端声明长度分配，无上限；依赖 `lsp-server` 同样无上限 | 读取器设硬上限（如 16 MiB），超限回协议错误；封装客户端侧限长读取 | 消除单帧 OOM/abort | S |
| P0 | `crates/cli/src/main.zh:2492-2530` | 托管产物写入未检查父目录/目标符号链接；恶意仓库可借 `.rz/gen` 覆盖任意文件 | 写入前逐级 `symlink_metadata` 检查；Unix 用 `O_NOFOLLOW`；托管根为链接时拒绝 | 防止恶意仓库覆盖用户文件 | S |
| P1 | `crates/cli/src/语言管理.zh:65-76,546-583,735-772` | `curl -L` 无 `--proto`/`--max-time`/`--max-filesize`；`RZ_LANG_REPO` 可指向 http；仅“指纹变化告警” | 强制 https/ssh；curl 加协议/超时/大小限制；语言包附 `SHA256SUMS` 或签名；首次/变化时确认 | 防中间人篡改关键字/别名映射 | M |
| P1 | `crates/cli/src/安装.zh:214-220,432-443,262-264` | `--force` 目标已存在则 `继续`，但 `version.txt` 仍更新；被篡改的 rustc/cargo/RA 不会被覆盖 | 强制删除/截断后复制，逐文件摘要校验，全部成功后才写版本文件，并跑 `--version` 校验 | 工具链修复可信 | S |
| P1 | `crates/cli/src/语言管理.zh:777-829`、`安装.zh:331-337,386-398` | ZIP/GZ 解压与下载全量读入内存，无条目/总字节上限 | 流式解压/下载；限制单条目、总字节、条目数；超限中止并清理 | 防压缩炸弹与内存耗尽 | M |
| P1 | `crates/cli/src/映射生成/智能.zh:67-81,189-193` | 自定义 AI 端点允许 `http://`，Bearer key 明文外发 | 仅允许 https（本地回环显式放行）；非 https 直接报错 | 防密钥泄露 | S |
| P1 | `crates/cli/src/语言管理.zh:310-342,405-410`、`main.zh:2167-2206` | 第三方语言包可声明扩展名 `rs`，致 rzc 把用户 `.rs` 当方言源并覆盖 | 校验扩展名：非空、字母数字、不得为 `rs/toml/json` 等保留后缀 | 防源码被误转译覆盖 | S |
| P1 | `crates/cli/src/main.zh:1163-1166,1306-1313` | `rzc add` 库名原样作为 cargo 参数，`-` 开头可注入 `--git/--path` | 校验 crate 名并拒绝前导 `-`；必要时用 `--` 分隔 | 防依赖注入执行 | S |
| P1 | `crates/engine/src/缓存.zh:384-408`、`crates/cli/src/语言管理.zh:913-914` | `~/.rz` 目录/缓存默认 0755/0644，缓存含转译源码 | 目录 0700、文件 0600；已有文件加载后校正权限 | 防同机用户读取源码/配置 | S |
| P1 | `.github/workflows/release.yml:36-47,54-57,298-299,417` | 作业无顶层最小权限；第三方 action 用可变 tag；PAT 作业级可见；`cargo login` 落盘 token | 顶层 `contents: read`，发布步骤提权；action 固定 SHA；PAT 仅注入发布步骤；用 `CARGO_REGISTRY_TOKEN` | 降低供应链/密钥暴露风险 | S |
| P2 | `crates/lsp/src/翻译缓存.zh:1566-1588` | `include_str!/include_bytes!` 复制的源路径未限制在项目根内 | 源路径也要求落在项目根内，拒绝 `..`/绝对路径 | 防项目外文件被读入虚拟项目 | S |
| P2 | `crates/lsp/src/服务器.zh:1573-1579` | 用 `sh -c "kill -0 <pid>"` 探活，扩大 PATH 依赖 | Unix 用 `libc::kill(pid, 0)`/`rustix` | 去 shell 依赖 | S |
| P2 | `crates/lsp/src/服务器.zh:662,694-701,1247-1298` | 未知 LSP 请求/通知全部转发给 rust-analyzer | 维护方法白名单，未知方法回 `MethodNotFound` | 缩小代理攻击面 | M |
| P2 | `crates/cli/src/语言管理.zh:1028-1041` | 目录递归复制跟随目录符号链接，可能循环/越界 | `symlink_metadata` 判定并跳过目录链接，限深度/条目数 | 防本地安装逃逸 | S |

## 2. 性能

> 基准参照：`lexer_stage_2000` ≈ 845µs、`pipeline_no_cache_2000` ≈ 3.12ms（engine baseline）。
> 以下收益为基于路径的估算，落地后需用新增基准复测。

| 优先级 | 位置 | 问题 | 改进方向 | 预期收益 | 难度 |
|---|---|---|---|---|---|
| P0 | `crates/cli/src/main.zh:2492-2502` | 托管产物无条件 `写入`，不比较既有内容；cargo 以 mtime 判定改动 | 写前读旧内容比较，相同则跳过；复用 `写入转译产物` 的判断 | 二次 run/check 省本项目整包重编译 | S |
| P0 | `crates/lsp/src/镜像检查.zh:99-157`、`服务器.zh:821` | 每次打开/保存全量删镜像、复制整树、全项目重译、`cargo check` | 增量镜像（mtime+size 只复制变化）；按内容摘要复用；检查结果按源哈希缓存 | 大项目保存延迟降数百 ms–数秒 | L |
| P0 | `crates/lsp/src/镜像检查.zh:349-353` | 每次检查重建 `项目上下文`，全项目逐文件词法收集声明，无缓存 | 复用 `翻译缓存` 的声明名缓存（按内容哈希），只算改动文件 | 百文件项目省数十–数百 ms/次 | M |
| P1 | `crates/lsp/src/镜像检查.zh:41-44,524-567` | 200ms 轮询、600s 超时；stdout/stderr 全量入内存；target-dir 与用户构建争锁 | 流式解析 + 阻塞等待；镜像独立 target-dir；超时分级 | 去轮询延迟、避免 600s 互锁 | M |
| P1 | `crates/cli/src/main.zh:455,461,2249-2274,2283-2338` | 项目上下文与转译各跑一遍词法；入口文件读两次 | 单趟入口：词法顺带产出本文件声明，复用入口内容 | 百文件项目省数十–数百 ms/命令 | M |
| P1 | `crates/lsp/src/翻译缓存.zh:283,436-521` | 每次按键 `同步兄弟模块`，递归 `read_dir`+`metadata` 全项目 | 文件监听或节流；按键只处理已打开文件 | 千文件项目每按键省 1–5ms | M |
| P1 | `crates/lsp/src/翻译缓存.zh:318-319,706-737` | 文档代号每次 update 递增 → 用户词汇缓存必失效；重扫全部条目且未过滤未打开文件 | 词汇集按条目增量维护；只扫已打开条目；缓存键改内容哈希集合 | 补全首帧从 O(全项目) 降到 O(已打开) | M |
| P1 | `crates/lsp/src/翻译缓存.zh:899-910,956-989` | 每次按键遍历全部条目建集合/汇总声明 | 增量维护模块名集合与声明汇总 | 每按键省 2 次 O(条目数) 遍历 | S |
| P1 | `crates/engine/src/别名.zh:154-165`、`lib.zh:125-128`、`翻译缓存.zh:335,344` | 每次带缓存转译都重算项目指纹（3 集合转 Vec/排序/哈希） | 构造时算一次或 `OnceLock`；LSP 用声明集合版本号 | 大项目每按键省数十 µs | S |
| P1 | `crates/lsp/src/服务器.zh:1301`、`响应映射/响应.zh:164,173,379,485,502,817` | 每请求/响应克隆整棵 JSON；语义令牌等大数组最贵 | 取得所有权后就地改；只读字段用 `&Value` | 大响应映射耗时降 30–50% | M |
| P1 | `crates/lsp/src/服务器.zh:473-496`、`分析器.zh:49` | RA 读取用无界通道；单转发线程串行映射，无背压/优先级 | 轻量响应直通；限通道容量，积压时降级 | 诊断批量时补全/悬停尾延迟下降 | M |
| P1 | `tools/bench-check.sh:38`、`.github/workflows/ci.yml:419-420`、`protect-main-release.json:30-34` | bench 阈值本地 30%/CI 40%，且 bench 不在必需检查 | bench 纳入必需检查或 nightly 阻断；按基准项分级阈值 | 性能回归合并前被挡 | S |
| P2 | `crates/cli/src/main.zh:752-765,775-779`、`诊断输出.zh:769` | check 全量缓冲 stdout/stderr 再拼一份全文；诊断会被 serde 解析两次 | 流式逐行解析；首次解析复用 `Value` | 峰值内存降 50–60%，省一次 serde | M |
| P2 | `crates/cli/src/映射检查.zh:530-543,724-779` | 11 个语言包各构造 3 次视图，33 次 TOML 解析 + 多份全文复制 | 一次构造 11 个视图复用；视图持 `&'static str` | 命令耗时降 60–70% | M |
| P2 | `crates/engine/src/词法器.zh:191,200,276-280,452-476` | 编辑表未预分配；替换点先 `格式化!` 拼中间串；源映射整表克隆 | 预分配；直接写输出缓冲；源映射共享条目/下标 | 词法阶段省 2–5% | M |
| P2 | `crates/lsp/benches/hot_paths.zh:93-206` | 只覆盖 `update_document`/`reverse_transpile`；缺镜像、语义令牌、词汇、指纹、增量变更基准 | 增补 4 类基准；接入合并前门禁 | 让 P0/P1 优化可度量、可防回退 | M |
| P2 | `Cargo.toml:30-33`、`crates/zh-pure/Cargo.toml` | release 未开 `panic=abort`；zh-pure 把 engine 作 build-dependency 多编译一份 | 评估 `panic=abort`；zh-pure build.rs 只在 `tests-zh` 变更时重跑；体积预算门禁 | 体积再降 5–10%，clean 构建省一份 engine | M |
| P2 | `.github/workflows/ci.yml:435-451` | 构建矩阵仅 linux-x64/windows-x64/macos-arm64 | 补 linux-arm64、linux-musl、macos-x64 | 覆盖 ARM 服务器/Intel Mac/容器 | M |

## 3. 架构与耦合

| 优先级 | 位置 | 问题 | 改进方向 | 预期收益 | 难度 |
|---|---|---|---|---|---|
| P1 | `crates/engine/src/lib.zh:5-45` | 21 个模块全部 `公开 模块`；CLI/LSP 直接依赖 10+ 内部模块（32 处） | 收敛为 4 个门面：`转译`/`诊断`/`语言包`/`工具链`；其余 `公开(包)` | 降低跨 crate 改名连锁与认知负担 | M |
| P1 | `crates/engine/src/lib.zh:60-366` | 15 个公开转译入口，多为组合包装 | 保留 `转译源文本`/`带项目转译源文本`/`静默…` 三类，其余用配置结构体 | 缩小 API 面、减少重复测试矩阵 | M |
| P1 | `crates/cli/src/main.zh:413` | `运行` 单函数 600 行、深度 7，混合分发/上下文/写产物/子进程/诊断 | 拆 `运行_单文件`/`运行_多文件`/`运行_诊断流`，主函数只编排 | 可读性/可测性显著提升 | L |
| P1 | `crates/cli/src/main.zh:441` | 37 臂子命令分派 + 5 个枚举，新增命令需改多处 | 命令元数据表（名称/帮助键/handler），枚举只留参数 | 新增命令成本减半 | M |
| P1 | `crates/cli/src/诊断输出.zh:706`、`crates/lsp/src/镜像检查.zh:603` | rustc JSON 诊断管线两套（解析/跨度/翻译） | engine 暴露 `诊断流管线`，CLI/LSP 只做输出通道 | 消除术语/回译漂移 | L |
| P1 | `crates/lsp/src/响应映射/诊断文本.zh:219,598`、`crates/engine/src/诊断/诊断翻译.zh:182` | LSP 重实现整词替换与所有权详情 | JSON→`编译器诊断` 后复用 engine；LSP 只做适配 | 单一事实来源 | L |
| P1 | `crates/engine/src/语言.zh:34,64`、`界面消息.zh:93`、`内置语言.zh:120`、`诊断输出.zh:831` | 全局可变语言码 + 4 处默认 `"zh"` | 显式 `语言上下文` 贯穿；至少统一 `默认语言码` 常量 | 并发/多工作区不再依赖全局副作用 | M |
| P2 | `crates/lsp/src/服务器.zh:46-88` | 6+ 个 `Arc<Mutex<…>>` 散落在服务器字段，锁顺序不显式 | 收进 `共享状态` 结构，文档化锁顺序 | 降低死锁/竞态风险 | M |
| P2 | `crates/lsp/src/翻译缓存.zh`（2,942 行/96 函数） | 状态机/查询/持久化/测试混在一个模块 | 拆状态、查询、持久化三块，测试外置 | 可维护性 | L |
| P2 | `crates/engine/src/映射管理器.zh:107` | 63 函数/1,311 行，加载器与查询面揉在一起 | 拆 `加载器` 与 `查询面` 两个类型 | 降低认知负担 | M |
| 保持 | `crates/cli/Cargo.toml:16`、`lsp/Cargo.toml:31`、`engine/Cargo.toml:18` | 依赖方向单向、无环 | 加 `cargo deny` 防未来反向依赖 | 防架构退化 | S |

## 4. 代码质量与可维护性

| 优先级 | 位置 | 问题 | 改进方向 | 预期收益 | 难度 |
|---|---|---|---|---|---|
| P0 | `tools/check-prod-panics.py:87,36` | 只扫 `.rs`；zh 上直接 exit 1，白名单仍写 `内置语言.rs` | 同时扫 `.zh`/`.rs`；白名单改 `.zh`；CI 固定执行 | 恢复“生产零 panic”保障 | S |
| P1 | `crates/cli/src/main.zh:441` | 37 臂 `匹配` 与 5 个命令枚举强耦合 | 表驱动分派（见架构） | 降低新增命令成本 | M |
| P1 | `crates/cli/src/映射生成/文档结构.zh:96`、`映射生成/mod.zh:166` | 247/246 行函数，深度 6–7 | 按“解析/渲染/写盘”拆段 | 可读性/可测性 | M |
| P1 | `crates/engine/src/故障.zh:195` | 28 臂“目标 × 失败种类”硬编码 | 二维常量表/按目标分组的消息方案 | 新增目标少改 28 行 | M |
| P2 | `crates/engine/src/全角标点.zh:236` 与 `校验.zh:979` | `是原始前缀` 逐字重复 | 抽到 `词法器`/`字符判定` | 去重、单一行为 | S |
| P2 | `crates/cli/src/安装.zh:415` 与 `语言管理.zh:974` | `文件摘要`（SHA256 流程）逐字重复 | 抽 `工具文件`/`工具链` 公共函数 | 去重 | S |
| P2 | `crates/lsp/src/服务器.zh:1477` 与 `镜像检查.zh:235` | `是符号链接` 重复 | 放 `lsp/src/文件工具.zh` | 去重 | S |
| P2 | `crates/engine/src/界面消息.zh:65,70`、`语言.zh:219,229`、`cli/界面.zh:106,115`、`lsp/界面消息.zh:118,123` | 同一“取消息/取带参消息”四套包装 | 定义 `消息查询` 特征，回退链只实现一次 | 去重、统一回退语义 | M |
| P2 | `crates/cli/src/main.zh:1554` 与 `crates/lsp/src/镜像检查.zh:186` | 两套“查找项目根”，存在性判定不一致 | 抽 `项目根` 模块，统一语义 | 消除边界差异 | S |
| P2 | `crates/cli/src/诊断输出.zh:81,181` | `#[允许(检查工具::太多参数)]`（10+ 参数） | 参数收进 `编译上下文` 结构体 | 去 allow、提高可测性 | M |
| P2 | `crates/engine/src/lib.zh:5-45` | 21 个公开模块无模块级 `///` | 每模块补“职责 + 不负责什么” | 新人上手更快 | S |
| P2 | `crates/lsp/src/响应映射/响应.zh:1-2` | 1,503 行/39 函数仅 2 行文件头 | 补映射契约、坐标还原规则、不映射清单 | 降低维护风险 | S |
| P3 | `crates/cli/src/内置语言.zh:60` | 函数名 `内置文件或恐慌` 把 panic 写进标识符 | 改为 `内置文件或默认`/返回 `结果` | 命名与错误策略一致 | S |
| P3 | `crates/engine/src/别名.zh:834`、`全角标点.zh:236` | 句断式命名（`闭花括号结束臂体`、`是原始前缀`） | 改为“做什么”的命名 | 可读性 | S |
| P3 | 测试命名 | `测_` 前缀与完整描述句混用 | 统一为描述句 | 一致性 | S |

## 5. 工程化、CI/CD 与发布

| 优先级 | 位置 | 问题 | 改进方向 | 预期收益 | 难度 |
|---|---|---|---|---|---|
| P0 | `protect-main-release.json:30-34` | 必需检查只有 test/windows-gates/msrv/eject-verify；coverage/bench/lsp-e2e/audit 不阻断 | 至少加 coverage、bench、lsp-e2e；按 paths 过滤 build/vsix | 门禁从“有记录”变“有约束” | S |
| P0 | `protect-zh-dev.json:6-11` | zh 无任何 required_status_checks | 给 zh 加 zh-gates + 编译检查 | 失败不再直接合入 | S |
| P0 | `.github/workflows/zh-gates.yml:35-39` | zh 只跑两个 Python 检查，完全不编译 | 加 `scripts/zh-build.sh test -p i18n-rust-engine -p rzc` 与 eject 自检 | 反馈环从天级到分钟级 | M |
| P0 | `tools/check-prod-panics.py:87` | zh 上扫不到 `.rs` 直接失败且无人执行 | 扫 `.zh`；纳入 zh-gates | 恢复 panic 门禁 | S |
| P0 | `release.yml:187-226,260` | Release 附件实际只有 `.vsix` + SHA256SUMS，没有三平台二进制/离线包 | CI 内构建三平台产物（或跨 workflow artifact），统一上传 | Release 与 README 承诺一致 | M |
| P1 | `release.yml:354-358` | `crates` 与 vsix 并行；vsix 失败时 crates.io 已发布不可撤回 | `crates` 改 `needs:[check, vsix]` | 消除不可回滚状态 | S |
| P1 | `ci.yml:322-342` | audit 只在 main push/PR，无 schedule、无 `cargo-deny`/SBOM | weekly audit + cargo-deny + SBOM 随 Release | CVE/许可证/来源问题可见 | M |
| P1 | `.github/workflows/*.yml`（`uses: @v4/@v2`） | 第三方 action 未固定 SHA | 固定 SHA，dependabot 维护 | 降低供应链投毒风险 | S |
| P1 | 仓库根（无 `.github/dependabot.yml`） | cargo/npm/actions 依赖靠手工升级 | 加 dependabot（weekly，分组） | 更新节奏化、减少滞后 | S |
| P1 | `release.yml:17-18,191-192`、`release-offline.sh` | 离线包由发布者本机打包后手工上传 | cargo-dist/cargo-packager 在 CI 生成三平台包 + 校验和并自动上传 | 发布可重复、无漏传 | L |
| P1 | `zh-gates.yml` / `ci.yml:31-35` | 全部重门禁只在 main；zh 是实际开发分支 | zh 跑编译/单测/clippy/bench 快速档 | 开发期即发现回归 | M |
| P2 | `ci.yml:17` vs `:297,308` | 头部注释写 79%，实际 `--fail-under-lines 85` | 统一为 85 并抽到 `env` | 避免误读阈值 | S |
| P2 | `ci.yml:267-320` | 覆盖率单阈值、无增量/模块阈值、不上传 lcov | 分模块阈值 + PR 覆盖率变化 | 新低覆盖模块不再被均值掩盖 | M |
| P2 | `ci.yml:247-264` | MSRV 只 build，不编译 tests/benches/examples | 改 `cargo +1.88 test --no-run --all-targets` | MSRV 与全部目标一致 | S |
| P2 | `ci.yml:347-392` | lsp-e2e 无 `timeout-minutes`；每次重下 RA | 加超时；缓存 `~/.rz/toolchain` | 防挂死、省 3–4 次下载 | S |
| P2 | `tools/*.py`（20+ 脚本） | 门禁脚本自身无单测（历史上已出现“注入坏数据仍 EXIT=0”） | 关键脚本加 pytest 正/负样本 | 防门禁安静失效 | M |
| P2 | `Makefile:38`、`scripts/zh-build.sh:82-89` | `make gate` 与 CI 步骤不完全一致；zh 无单一 `zh-gate`；每次复制整棵 39MB 树 | 抽共享步骤清单；加 `zh-gate`；只复制必要目录 | 本地/CI 判定一致，构建更快 | M |

## 6. 可扩展性

| 优先级 | 位置 | 问题 | 改进方向 | 预期收益 | 难度 |
|---|---|---|---|---|---|
| P1 | `crates/cli/src/内置语言.zh:65-95,107-121` | 11 个语言宏调用 + 12 臂语言码 `match`，新增语言改三处（注释自认） | 构建脚本从 `lang-packs/` 生成宏与 match，或 PHF 静态表 | 新增语言零代码改动 | L |
| P1 | `crates/cli/src/main.zh:73,213,240,296,352` | 命令枚举与 37 处 `命令属性` 硬编码，新增命令需同步多处 | 命令元数据表 + handler 注册 | 扩展命令成本减半 | M |
| P1 | `crates/engine/src/语言.zh:34`、`界面消息.zh:93`、`内置语言.zh:120`、`诊断输出.zh:831` | 默认语言 `zh` 散落 4 处 | 唯一常量或从 `lang_info.toml` 读取 | 改默认语言只改一处 | S |
| P2 | `crates/engine/src/工具链.zh:103`、三个 `Cargo.toml:3`、`安装.zh:502` | 版本号多处硬编码（锁定工具链/三 crate/升级提示） | workspace 注入 `CARGO_PKG_VERSION`，工具链版本单独配置 | 发版不再漏改 | S |
| P2 | `crates/cli/src/语言管理.zh:28-29`、`安装.zh:226,303,350` | GitHub/GitCode/rust-lang/rust-analyzer URL 硬编码 | 集中到 `下载源` 常量/配置，支持环境变量覆盖 | 换源/镜像零散改 | S |
| P2 | `crates/engine/src/产物布局.zh:19`、`工具链.zh:25`、`待译清单.zh:84`、`诊断输出.zh:912-913` | `.rz/gen`/`.rz/gaps` 等路径字符串散落 | 统一从 `产物布局` 导出常量 | 改布局只改一处 | S |
| P2 | `crates/cli/src/内置语言.zh:48-52`、`诊断输出.zh:563`、`映射检查.zh:1278-1292` | 语言包五个文件名清单硬编码多处 | 定义 `语言包文件` 常量集/构建生成 | 新增文件不再漏改 | S |
| P2 | `crates/cli/src/main.zh:2791-2811`、`lsp/服务器.zh:1948-1985` | 项目内 `lang-packs/<code>` 自动采用，仓库可静默改变转译语义 | 默认仅全局/内置；项目语言包需显式开关 | 降低不受信仓库影响 | S |

## 7. 用户体验（UX）

| 优先级 | 位置 | 问题 | 改进方向 | 预期收益 | 难度 |
|---|---|---|---|---|---|
| P1 | crates.io / README | crates.io 仍为 0.8.4，README 描述 0.9.0；用户 `cargo install` 得到旧版 | 尽快发布 0.9.0；或在 `rzc --version` 加“有新版本”提示；README 已加版本注记 | 减少“文档与行为不一致”投诉 | S |
| P1 | `crates/lsp/src/服务器.zh:819-821` | 打开文件即触发全量 `cargo check`，大项目首屏诊断慢且无进度/取消 | 默认保存时/手动触发；发 `$/progress`；支持取消；提供“立即检查”命令 | 首屏更快、可预期 | M |
| P1 | `crates/lsp/src/响应映射/诊断文本.zh` | 5 个 RA 噪声过滤器可能误杀真错，用户无“查看原始诊断”入口 | 增加设置/命令显示未过滤诊断；记录过滤计数 | 降低“错误被吞”的不信任 | M |
| P2 | `crates/lsp/src/服务器.zh`、`翻译缓存.zh` | 键入/保存后诊断更新无状态提示，长检查期间用户不知道是否在工作 | 状态栏/进度通知；区分“已翻译/正在编译” | 降低等待焦虑 | S |
| P2 | 语言包 | 运行时仍可能出现“暂无译文”消息（gaps） | 用 `rzc gaps --export` 清零；发布前门禁要求 0 新增 | 减少中英混排 | S |
| P2 | `.github/workflows/ci.yml:435-451` | 无 linux-arm64/musl/macos-x64 产物 | 补矩阵/交叉编译 | ARM 服务器/Intel Mac/容器可用 | M |
| P2 | `rzc eject` | 无 `--dry-run`/差异预览，批量导出前无法确认影响 | 加 `--dry-run` 输出“将写哪些文件/冲突” | 降低误覆盖风险 | S |
| P2 | CLI 输出 | 未发现 `NO_COLOR`/非 TTY 宽度适配（需补） | 支持 `NO_COLOR`、按终端宽度换行 | 日志/CI/窄终端可读 | S |
| P2 | 11 份 README | 翻译与中文版事实漂移（版本/命令/进度）靠手工同步 | 单源生成或扩展 parity 门禁到“版本/命令/数字”事实行 | 减少翻译过时 | M |
| P3 | 阿拉伯语等 RTL | 界面已本地化，但终端 RTL 排版/双向文本未处理 | 至少文档说明；终端输出避免混排 | 提升 RTL 可用性 | M |
| P3 | `rzc init` | 无交互式语言选择/预览，默认按系统语言 | `--lang` 已有；可加交互向导与模板预览 | 首次体验更友好 | S |

## 8. 已经做得好的部分（不要回退）

- **转译核心**：零正则、`rustc_lexer` 逐 token；`名字缓存` 256 槽零分配；`映射文本` ≤22 字节内联；
  LRU + 代际缓存；语境指纹 `OnceLock`；benchmark 覆盖冷/热/别名密集/教学检查。
- **质量门禁**：fmt、clippy `-D warnings`、debug+release 全测、MSRV、覆盖率 85%、audit、
  bench、四语教程逐块编译、全中文棘轮、一词多译/绑定位审计、发布树 `eject-verify`。
- **安全已有防线**：rustc 包 SHA256 校验、tar 条目路径清洗、注册中心 `file` 字段子树校验、
  不向 RA 透传客户端 `rootUri`、临时目录创建前后查符号链接、生产 panic 门禁（main 上有效）。
- **发布与镜像**：三仓镜像、tag 保护、release 版本一致性、Gitee/GitCode Release、离线包。

## 9. 实施路线图

### 第 1 周（止血，全部 S/M）
1. H1 路径校验（`包仓库` 安装/移除/发布）+ M1 符号链接写保护；
2. H4 `Content-Length` 上限；H5 `--force` 真覆盖；
3. A7 产物写前比较；B2/B3 必需检查补齐；check-prod-panics 支持 `.zh`；
4. Release 附件补三平台二进制，`crates` 改 `needs:[check, vsix]`；
5. 修复 ci.yml 79/85 注释漂移。

### 第 2–4 周（关键路径）
1. H3 LSP 信任模型 + 镜像排除 `.cargo`/独立 `CARGO_HOME`；
2. A14/A15 增量镜像 + 声明缓存；A25 补镜像/语义令牌/词汇基准；
3. zh-gates 加编译与 eject 自检；A27 bench 纳入阻断；
4. A17–A19 每按键增量；A6 单趟项目上下文；
5. H2/H5 下载与安装校验；M3/M4 流式+上限。

### 第 2 个月（结构性）
1. 拆 `main.zh`（运行/分派/项目处理）；合并 rustc JSON 诊断管线；
2. LSP 复用 engine 诊断；engine 门面收敛；
3. 语言清单去硬编码（构建生成）；默认语言/版本/路径常量统一；
4. 依赖 dependabot、action SHA、cargo-deny/SBOM、门禁脚本自测；
5. UX：LSP 进度/取消/原始诊断开关、`eject --dry-run`、`NO_COLOR`。

### 季度（长期）
1. LSP 全链路增量（镜像、词汇、缓存、通道背压）；
2. 离线包 CI 化（cargo-dist）、可复现构建与签名；
3. 三平台矩阵补齐（linux-arm64/musl/macos-x64）；
4. 翻译单源生成/事实门禁；RTL 与窄终端适配。

## 10. 附录：复核命令

```bash
# 基线
git -C /home/oop/code/i18n-rust log -1 --oneline
# 规模
find crates -name '*.zh' -type f -exec wc -l {} + | sort -rn | head
# 门禁（zh）
python3 tools/check-zh-ascii.py
python3 tools/check-zh-artifacts.py
python3 tools/check-prod-panics.py        # 当前在 zh 会失败：只扫 .rs
# 构建/测试（隔离目录）
scripts/zh-build.sh test --workspace --locked
# 基准
scripts/zh-build.sh bench -p i18n-rust-engine --bench transpile
scripts/zh-build.sh bench -p i18n-rust-lsp --bench hot_paths
# 发布树校验
scripts/eject-verify.sh --rzc .zh-build/target/debug/rzc --zh . --candidate <main 工作树>
```

> 说明：本报告只做诊断，不含修复；P0 项建议单独开分支，按“一项一提交”落地。
