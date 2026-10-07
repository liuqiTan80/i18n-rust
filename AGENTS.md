# AI 编码指令（本分支：中文专属）

> 本文件是**跨工具入口**：Qoder 读 `.qoder/rules/`，Claude Code / Codex / Cursor / Copilot
> 等代理读 `AGENTS.md`。二者指向同一份权威规范，勿在此复制全文（防口径漂移）。
> **单一真源**：`.qoder/rules/中文编码规范.md`（简明规范）+ `docs/zh-selfhost-guide.md`（详细判据与历史坑，§3 三层判据）。

## 一句话背景

本分支 `exp/zh-selfhost` 是 i18n-rust（中文方言 Rust 编译器）的**自举源真相**：`.zh` 是中文源码，
`.rs` 是 `rzc eject` 生成的英文产物。**目标是在 `.zh` 里全中文写代码，同时不破坏 eject 往返与门禁。**

## 不可协商核心（任何代理动手前先遵守）

1. **改 `.zh`，绝不手改 `.rs`**。`.rs` 是产物、与 `.zh` 1:1 配对且都入库。改完 `.zh` 跑 `make zh-regen`
   再生 `.rs`，`zh-verify` 守字节一致。把 `.rs` 里的英文"顺手翻中"会破坏 eject 往返。
2. **`.zh` 里的标识符、注释、`///` 文档、面向用户字符串一律中文**。但"全中文 ≠ 零英文"：
   两类英文**必须保留**（强行翻中会破坏 eject 或让门禁失明）——
   - 结构必需：`fn main`、`use std::…`、外部 crate 路径、serde 字段名、LSP 协议键、`.rs` 产物位的关键字英文形态；
   - 门禁契约数据：`help-check` 英文负样本夹具、`demo-check` 的 `MUST_HAVE` 预期改写串、clap `Usage:`/`Print help`、
     测试里喂给别名替换/文档提取/LSP 翻译的英文输入样本。
   - 判定顺序：命中在 `"…"`/`r#"…"#` 串内或 `//`/`///` 注释后的英文 → 留；行首真定义位且非 `fn main` → 才是真漏网须翻中。
3. **注释/文档引用的符号名、文件名、intra-doc 链接 [`…`] 必须与 `.zh` 现状一致**，改名/翻转后勿滞后残留旧英文。
   但 `.zh` 自洽链接在 eject 的 `.rs` 文档层会报 unresolved（双语言伪报），**这类绝不能改**——判据见 guide §3。
4. **命名禁区**：勿用与词表关键词同名的裸中文标识符（`测试`/`匹配`/`让`/`函数`…），eject 会还原成英文破坏语法；
   改用复合中文名（`替换结果`/`源码映射项`…）。新增词条前先查是否与 `crates/engine/lang-packs/zh/*.toml` 键冲突。
5. **术语以 `tutorials/总术语表.md` 为准**，勿自造译名。

## 提交前自检（一条命令兜底）

```bash
make gate
```

含 fmt/clippy/test/prod-panics/ui-keys/lang-packs/mapping-check/tutorials-all/glossary 与
zh 专属四门禁 `zh-verify` / `zh-demo-check` / `zh-help-check` / `zh-doc-check`。任一不过即失败。

⚠️ **门禁有边界，靠你自律**：`make gate` 全绿 **≠** 绝无英文。门禁只管有编译器/预言机可
客观判定的维度（注释引用失效、产物字节一致、用户面本地化、改写往返）；**你在 `.zh` 代码位
新写的英文标识符不会被抓**（它是合法 Rust、能 eject、字节一致）。故**动手时就按本规范用中文
命名**，勿指望门禁兜底。详见 `docs/zh-selfhost-guide.md` §3「自动化的边界」。

## Git 纪律

本分支**永不合并进 `main`**；除非修复问题，**不动 `main`**。
