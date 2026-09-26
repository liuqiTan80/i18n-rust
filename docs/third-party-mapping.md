# 第三方库映射维护指南

本文档面向语言包维护者，说明第三方库（crates）映射的格式、工具链与校验规则。
写应用时只是补几条缺的词条，请看应用开发者视角的
[missing-mapping-guide.md](./missing-mapping-guide.md)，不必从本文起步。

## 1. 背景与机制

方言源码转译为标准 Rust 时按以下顺序替换：

1. **关键字替换**（`keywords.toml`）：词法层面，最先执行；
2. **模块路径替换**：`use` 语句内的路径段，查「模块路径」表；
3. **别名替换**（标识符表）：token 级别，含 `stdlib.toml` 与 `crates/*.toml` 合并结果。

因此：

- crates 键与关键字同键时，**关键字先替换，crates 条目永不生效**；
- `stdlib.toml` 最后加载，与 crates 同键不同值时 **stdlib 优先，crates 条目失效**；
- crates 目录内多个文件的标识符节合并为一个全局表，**同键不同值会产生不确定的覆盖**。

## 2. 映射文件格式

每个 crate 一个文件：`crates/engine/lang-packs/<语言>/crates/<文件>.toml`，三个可选节：

```toml
# ["模块路径"]：use 路径段替换（use 方言名::子模块 → use 英文路径）
["模块路径"]
"序列化" = "serde"

# ["标识符"]：token 级别名替换（库内类型、函数等）
["标识符"]
"服务器" = "Server"

# ["解释"]：教学用说明（可选，仅 rzc 展示，不参与转译）
["解释"]
"服务器" = "HTTP 服务器主体。"
```

硬性约束：

- 键必须带双引号（非 ASCII 键的 TOML 要求）；
- **同一文件内禁止重复键**（TOML 解析直接失败）；
- 「标识符」节的键在所有 crates 文件 + stdlib + keywords 范围内应唯一或同值。

## 3. 工具链

### 3.1 自动生成：`rzc mapping auto`

```bash
rzc mapping auto salvo --lang zh --provider rule    # 离线规则模式
rzc mapping auto salvo --lang zh                    # AI 模式（需 DEEPSEEK_API_KEY）
```

提取目标 crate 公开 API 生成映射骨架，默认写入项目语言包根
`<项目语言包根>/<lang>/crates/<crate>.toml`（主仓库内为 crates/engine/lang-packs/，
用户项目为 lang-packs/；已存在时先打印覆盖警告）。
生成完成后**自动对所在语言包运行一次冲突检测**，便于立即发现新文件
引入的键冲突。

**版本锁定（生产映射必选）**：

```bash
rzc mapping auto tauri --target-version 2.11.5    # 精确锁定 =2.11.5
rzc mapping auto tauri --target-version 2.11      # 该线最新（2.11.*）
rzc mapping auto tauri --target-version v2.11.5   # 兼容前导 v / =
```

不带 `--target-version` 时依赖解析为**当时最新版**（结果不可复现，命令会打印
提示）：上游发布破坏性大版本后重跑会静默改变生成基准。生产映射务必锁定——
锁定的版本写入临时项目依赖（`=x.y.z` 精确 / `x.y.*` 前缀），提取时实际解析到
的版本记录在映射文件头（`# 基准版本: x.y.z`），配合 git 历史即可追溯每版映射
的生成依据；`--install` 也按同一版本需求添加项目依赖（`crate@=x.y.z`），保证
应用依赖与映射基准一致。

**AI 模式分批**：公开 API 数量多（数百条）时按每批 50 条自动分批调用
AI（单次调用输出会超模型上限），逐批打印进度；任一批失败则整体回退
规则模式（名称保留规则结果、解释留空），可重跑补全。

### 3.2 质量校验：`rzc mapping check`

```bash
rzc mapping check zh        # 校验单个内置语言
rzc mapping check 某目录/xx  # 校验外部语言包目录
rzc mapping check           # 校验全部内置语言 + 跨语言条目数一致性
```

检查项：

| 级别 | 规则 | 说明 |
|---|---|---|
| error | TOML 解析失败 | 含重复键 |
| error | 关键字避让 | crates 键与 keywords 键相撞且**值不同**（同值视为安全冗余） |
| error | 跨文件同键不同值 | crates 文件之间标识符键冲突，合并非确定 |
| warning | stdlib 覆盖 | crates 键与 stdlib 标识符同键不同值，crates 条目失效 |

存在 error 时退出码非零（已接入 CI 门禁）。另有**告警数基线**：
`make mapping-check`（CI 同款）经 `tools/check-mapping-warnings.py` 对照
`tools/mapping-baseline.json` 校验告警数只减不增；当前基线 2 条，均为已明示的
结构性差异（跨语言条目数、en 消息翻译节缺省）。

### 3.3 翻译脚手架：`rzc mapping scaffold`

```bash
rzc mapping scaffold zh vi                              # 默认：生成 TODO 骨架待人工翻译
rzc mapping scaffold zh vi --provider deepseek          # AI 自动翻译键名（需 DEEPSEEK_API_KEY）
rzc mapping scaffold zh vi --output 自定义目录           # 指定输出目录
```

将源语言全部 crates 文件复制到 `<项目语言包根>/<目标>/crates/`（可用
`--output` 指定其他目录），英文值保持不变，每个键值行追加
`# TODO(<目标>): 将键从 <源> 翻译` 注释。

两种翻译方式（`--provider`）：

- **`rule`（默认）**：仅生成骨架，键需人工翻译；
- **`deepseek`**：按 60 键/批调用 AI 翻译键名（附带英文值作上下文），
  译完自动跑 check 回环——检出冲突的键连同已占用键清单再送 AI 改名
  （最多 2 轮），仍未解决的列出留给人工。未被 AI 翻译的键保留 TODO 标记；
  已存在的文件不会重新生成（既有翻译不丢失），重跑只处理残留 TODO，幂等安全。

翻译时只改键、不改英文值，完成后用 `rzc mapping check <目标目录>` 校验。
新语言的完整工作流见 [contributing-lang-pack.md](./contributing-lang-pack.md)。

## 4. 命名避坑规则

新增/翻译键时遵循（参考 salvo 翻译实践）：

1. **关键字避让**：与 `keywords.toml` 键相同的词必须改用多字词
   （如"错误"→"错误中止"、"空"→"空处理器"）；
2. **跨文件唯一**：同一母语词在多个 crate 含义不同时，各自用多字词区分
   （如"连接"在异步库=join、数据库=连接对象）；
3. **多字词整体验证**：确认 lexer 将多字词作为整体 token 分词
   （参考 stdlib 既有用法"错误种类""非空指针"）；
4. **同名复用**：不同 crate 中同一英文 API 语义一致时可用同键同值
   （如 salvo 与网络库共用 `HTTP请求 = Request`），同键同值安全。

## 5. 单一数据源与生效方式

语言包全项目只有一份：`crates/engine/lang-packs/`（编译期内嵌与
文件系统消费共用同一数据，无需任何同步步骤）：

- rzc/LSP 的文件系统加载（`--lang-pack`、项目内覆盖、`rzc lang install`）直接读这份；
- 编译期内嵌（`include_str!` 由 build.rs 扫描生成）也来自这份，随 rzc 二进制分发。

修改后只需重新编译即可让内嵌数据生效：

```bash
cargo build --workspace   # 内嵌数据需重新编译才生效
```

CI 已内置映射质量门禁（`rzc mapping check`）。

## 6. 关于 en（英语）包

en 语言包为**恒等映射**（母语键即英文本身，第三方映射为恒等替换）：英语即
Rust 的原语言，包内词条不改变转译结果，其作用是让 rzc/LSP 识别 `.en`
项目（8 月曾一度删除致 en 教程门禁静默失效 168 例，随后恢复，见
[translation-status.md](./translation-status.md) 2026-09-08 条目）。

两处**结构性差异**属预期，不强行同步（已登记告警基线）：

- crates 表与 9 个翻译语言同为 10 张，但标识符为 373 条（少 3 条）：
  `spawn`/`filter`/`route` 的第二个语义语境在英语中与第一个同词，TOML
  同键唯一（重复键会解析失败）而自然折叠；
- 未建 `["消息翻译"]` 节：诊断消息本就为英文，回退原文即最终效果。

覆盖差异总览（zh 独有 6 表等）见 [translation-status.md](./translation-status.md)。

## 7. 验证清单

新增或修改映射后依次执行：

1. `cargo build --workspace`（内嵌数据生效）；
2. `make mapping-check`（全部内置语言 + 一致性 + 告警数基线；单跑
   `rzc mapping check` 不校验基线）；
3. 编写使用新映射词的方言源码，`rzc eject` 检查转译结果；
4. `cargo test --workspace`（含全内置语言通过校验的回归测试）；
5. 语言包目录 `crates/engine/lang-packs/` 为唯一事实源（build.rs 自动内嵌），
   无需维护第二副本。
