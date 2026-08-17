# 第三方库映射维护指南

本文档面向语言包维护者，说明第三方库（crates）映射的格式、工具链与校验规则。

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

每个 crate 一个文件：`lang-packs/<语言>/crates/<文件>.toml`，三个可选节：

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

提取目标 crate 公开 API 生成映射骨架，默认写入项目根
`lang-packs/<lang>/crates/<crate>.toml`（已存在时先打印覆盖警告）。
生成完成后**自动对所在语言包运行一次冲突检测**，便于立即发现新文件
引入的键冲突。

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

存在 error 时退出码非零（已接入 CI 门禁）。

### 3.3 翻译脚手架：`rzc mapping scaffold`

```bash
rzc mapping scaffold zh vi                              # 默认：生成 TODO 骨架待人工翻译
rzc mapping scaffold zh vi --provider deepseek          # AI 自动翻译键名（需 DEEPSEEK_API_KEY）
rzc mapping scaffold zh vi --output 自定义目录           # 指定输出目录
```

将源语言全部 crates 文件复制到 `lang-packs/<目标>/crates/`（可用
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

## 5. 注册发布与双副本同步

语言包存在两份副本，必须保持一致：

- `lang-packs/`：根目录副本，供 LSP/离线发布/远程安装消费；
- `crates/engine/lang-packs/`：编译期内嵌副本（`include_str!`），随 rzc 二进制分发。

修改后执行：

```bash
rsync -a --delete lang-packs/ crates/engine/lang-packs/
cargo build --workspace   # 内嵌数据需重新编译才生效
```

CI 已内置双副本一致性门禁与映射质量门禁（`rzc mapping check`）。

## 6. 关于 en（英语）包

英语包的母语键即英文本身，第三方映射为恒等替换，**无需 crates/ 目录**；
`rzc mapping check` 的跨语言条目数一致性对比会自动跳过无 crates 文件的语言。

## 7. 验证清单

新增或修改映射后依次执行：

1. `cargo build --workspace`（内嵌数据生效）；
2. `rzc mapping check`（全部内置语言 + 一致性）；
3. 编写使用新映射词的方言源码，`rzc eject` 检查转译结果；
4. `cargo test --workspace`（含全内置语言通过校验的回归测试）；
5. `rsync -a --delete lang-packs/ crates/engine/lang-packs/` 同步双副本。
