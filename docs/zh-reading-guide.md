# zh 分支代码阅读顺序与实操教程

> 面向三类读者：第一次读本仓库源码的贡献者、要改引擎/CLI/LSP 的开发者、要补语言包的译者。
> 目标：**先跑起来，再按依赖顺序读，最后用小练习把关键链路走通**。
> 配套入口：[项目地图](project-map.md)（改哪儿/怎么验）· [CONTRIBUTING](../CONTRIBUTING.md)（提交规范）· [zh 分支摩擦台账](zh-findings.md)（已知坑的实测记录）。

## 0. 先弄清分支模型

| 分支 | 内容 | 你能做什么 |
|---|---|---|
| `main` | **发布树**：所有 `.zh` 已 eject 为标准 Rust，`.rs` 全部入库 | 直接 `cargo build/test/clippy`、`make gate`、打发布包 |
| `zh`（本分支） | **方言源**：`crates/*/src`、tests/benches/examples 只保留 `.zh` | 读/写方言源码；构建与测试在 `.zh-build/` 隔离目录进行 |

`zh` **不能直接 merge 到 `main`**：改动经 `rzc eject` 生成 `release/eject-<版本>`
候选树，再通过 PR（`eject-verify` + 必需检查）合入。因此：

- 不要直接在 `zh` 上 `cargo build`——源码树没有 `.rs` 目标，cargo 会报
  `no targets specified in the manifest`；
- 所有编译、测试、门禁都在 `scripts/zh-build.sh` 生成的隔离副本里跑；
- 发布树的重门禁（教程四语、基准、覆盖率）在 `main` 上执行。

## 1. 10 分钟跑通 zh 分支

前置：Rust stable（`rust-toolchain.toml` 已锁定 1.88+）、Python 3、Git。

```bash
git clone https://github.com/liuqiTan80/i18n-rust
cd i18n-rust
git checkout zh

# 1) 生成隔离构建树并跑全量测试（首次会编译依赖）
make zh-build ARGS='test --workspace --locked'

# 2) 源码级门禁（可转译性 + 一词多译 + 绑定位 / 全中文棘轮）
make zh-artifacts
make zh-ascii
# 可转译性检查需要 rzc；若还没构建，先跑：
#   make zh-build ARGS='build --bin rzc'
# 没有 rzc 时 zh-artifacts 只跑两项审计，不判红。

# 3) 需要单独用 rzc 时（产物在 .zh-build/target/debug/rzc）
make zh-build ARGS='build --bin rzc'
.zh-build/target/debug/rzc --version        # 0.9.0
```

`scripts/zh-build.sh` 做了什么：

1. 定位 rzc（`RZC` 环境变量 → `target/{debug,release}/rzc` → `.zh-build/tool` 离线包
   → `release/rzc-*-linux-*.tar.gz`）；
2. 把工作树复制到 `.zh-build/src/`（`target/`、`.git/`、`.zh-build/` 排除）；
3. 对 `crates/*/src` 按目录、对 tests/benches/examples/build 按单文件执行
   `rzc eject -- …`（这一步就是“源码树只显示 `.zh`”的实现方式）；
4. `cargo fmt --all` 后执行你传入的 cargo 命令；构建缓存固定
   `CARGO_TARGET_DIR=.zh-build/target`，重建源码副本不丢缓存。

> 想只生成产物、不跑 cargo：`scripts/zh-build.sh`（无参数），构建树保留在
> `.zh-build/src/` 供你手动 `cd` 进去操作。

## 2. 目录地图（只列你要读的）

```text
crates/
├── engine/                 语言无关核心（最该先读）
│   ├── src/                *.zh 方言源：词法器、模块路径、映射管理器、缓存、诊断/…
│   └── lang-packs/<lang>/  11 个语言包（数据，不是代码）
├── cli/                    rzc 命令行
│   └── src/                main.zh（命令层）+ 安装 / 语言管理 / 映射生成 / 诊断输出…
├── lsp/                    语言服务器（rust-analyzer 代理）
│   └── src/                服务器、分析器、响应映射/、翻译缓存、镜像检查…
└── zh-pure/                纯函数方言测试样例（build.zh 把 tests-zh/*.zh 转译后执行）

scripts/zh-build.sh         zh 分支隔离构建
tools/                      门禁与验证脚本（教程、语言包、映射、ui 键索引…）
docs/zh-findings.md         方言化过程中撞出的真实坑与修法（强烈建议通读）
```

### 2.1 engine 模块（按职责分组）

| 分组 | 文件 |
|---|---|
| 词法/转译 | `词法器.zh`、`别名.zh`、`模块路径.zh`、`产物布局.zh`、`列位映射.zh` |
| 映射数据 | `映射源.zh`、`映射管理器.zh`、`名字缓存.zh`、`缓存.zh` |
| 诊断 | `诊断/mod.zh`、`诊断模型.zh`、`消息翻译.zh`、`诊断翻译.zh`、`教学诊断记录.zh`、`输出格式.zh`、`待译清单.zh` |
| 质量/安全 | `校验.zh`（教学 lint）、`全角标点.zh`、`混淆检测.zh`、`导出冲突提示.zh` |
| 语言/界面 | `语言.zh`（内置语言包路由）、`界面消息.zh`（ui.toml 解析与 `{}` 替换） |
| 其他 | `故障.zh`（错误类型）、`日志.zh`、`指南.zh`（`rzc guide`）、`工具链.zh`、`反译重生.zh`（`rzc relocalize`） |

### 2.2 cli 模块

| 文件 | 职责 |
|---|---|
| `main.zh` | clap 命令定义与分发、run/check/eject/transpile 主管线；测试模块 `母语测试` |
| `界面.zh` | 按文件/语言加载界面消息表 |
| `内置语言.zh` | 编译期内嵌语言包（`定义内置语言!` / `获取内置数据` / `内置语言清单`） |
| `语言管理.zh` | `rzc lang list/install/remove/search` |
| `安装.zh` | `rzc install lsp/toolchain` |
| `包仓库.zh` | `rzc crate …` 社区注册中心 |
| `映射检查.zh` / `映射覆盖.zh` / `映射生成/` | `rzc mapping check/coverage/scaffold/auto` |
| `诊断输出.zh` | 诊断打印、行列回译、教学提示渲染 |
| `翻译缺口.zh` | `rzc gaps` 待译清单 |
| `临时守卫.zh` | 临时目录 RAII 清理 |

### 2.3 lsp 模块

| 文件 | 职责 |
|---|---|
| `服务器.zh` | LSP 主循环、初始化握手、请求分发 |
| `分析器.zh` | rust-analyzer 子进程生命周期与双向转发 |
| `翻译缓存.zh` | 方言文档 → 虚拟 `.rs` 的缓存与行列映射 |
| `镜像检查.zh` | 真实项目镜像 `cargo check`（权威诊断） |
| `响应映射/` | `响应.zh`、`诊断文本.zh`（噪声过滤）、`测试.zh`、`mod.zh` |
| `界面消息.zh` | LSP 侧界面文案 |

### 2.4 语言包结构（`crates/engine/lang-packs/zh/`）

```text
keywords.toml       关键字/宏/声明词/控制流（词法层替换，任意位置生效）
module_paths.toml   模块路径（std::collections::HashMap 等）
stdlib.toml         标准库类型/方法/字段词形
errors.toml         错误码桩 + 整句消息翻译
ui.toml             界面消息（CLI/LSP 共用；11 语言键必须齐平）
lang_info.toml      语言元信息（显示名、扩展名、版本）
crates/*.toml       第三方库映射（serde/tokio/clap…）
ai-examples.md      AI 助手用的示例语料
```

## 3. 推荐阅读顺序

**原则：先看数据，再看管线，最后看入口。** 不要从 `main.zh` 第一行开始读。

### 路线 A：转译内核（1–2 天）

1. `crates/engine/lang-packs/zh/keywords.toml` + `stdlib.toml` + `module_paths.toml`
   ——先建立“母语词 ↔ 英文词”的直觉；
2. `crates/engine/src/lib.zh` ——公开入口（`转译管线`、`转译源文本`），知道一次转译
   返回什么（`转译输出`：`输出` / `源映射` / `流程映射`）；
3. `词法器.zh` ——逐 token 扫描、替换、宏与声明名处理；
4. `模块路径.zh` + `别名.zh` ——路径段替换、声明冻结、跨文件上下文；
5. `映射管理器.zh` + `映射源.zh` ——四张表怎么装配、合并序与冲突检测；
6. `缓存.zh` + `名字缓存.zh` ——内容哈希/语境指纹、何时失效；
7. `产物布局.zh` ——`.rz/gen/` 托管产物与 `#[path]` 注解；
8. `列位映射.zh` ——转译前后列偏移回放（诊断回译的基础）。

### 路线 B：诊断链路（半天）

1. `诊断/诊断模型.zh` ——rustc JSON → 内部诊断模型；
2. `诊断/消息翻译.zh` ——错误码桩、整句消息表、`消息表胜出` 判据；
3. `诊断/诊断翻译.zh` ——类型本地化、所有权叙事、引用层数提示；
4. `诊断/教学诊断记录.zh` + `输出格式.zh` ——教学提示的收集与渲染；
5. `诊断/待译清单.zh` + CLI 的 `翻译缺口.zh` ——`rzc gaps` 的数据来源；
6. 回到 `lib.zh` 看 `输出教学告警` 如何把三者串起来。

### 路线 C：CLI（半天）

1. `crates/cli/src/main.zh` 的命令枚举（`命令组`）与 `主函数` 分发；
2. `run` / `check` / `eject` / `transpile` 四条主管线（文件后半段）；
3. `诊断输出.zh` 如何把 engine 诊断打印成母语文本；
4. `界面.zh` + `内置语言.zh` 如何选语言包；
5. 最后再看 `安装.zh` / `语言管理.zh` / `包仓库.zh` / `映射生成/` 这些“周边命令”。

### 路线 D：LSP（1 天）

1. `crates/lsp/src/服务器.zh` 的初始化与请求分发；
2. `翻译缓存.zh`（文档同步 + 行列映射）；
3. `响应映射/响应.zh`（各请求类型的反向翻译）；
4. `响应映射/诊断文本.zh`（RA 噪声过滤，正例/反例双向断言）；
5. `镜像检查.zh`（真实项目 `cargo check` 通路）；
6. `分析器.zh`（子进程与消息通道）。

### 路线 E：测试与门禁（随用随查）

- `crates/engine/tests/*.zh`：差分、模糊、语料门禁、反译、别名复用；
- `crates/lsp/tests/e2e.zh`：真实 rust-analyzer 端到端；
- `crates/zh-pure/tests-zh/纯函数.zh`：最小方言单测写法；
- `tools/`：教程验证、语言包检查、映射告警基线、ui 键索引。

## 4. 实操教程（每个练习都可直接跑）

### 练习 1：跟踪一个宏从词表到产物

```bash
make zh-build ARGS='build --bin rzc'
RZC=.zh-build/target/debug/rzc
printf '函数 主函数() {\n    打印行!("你好");\n}\n' > /tmp/宏.zh
$RZC transpile /tmp/宏.zh
```

观察输出里的 `println!`。然后在 `keywords.toml` 的 `["宏"]` 节找到 `打印行 = println`。
结论：**宏在词法层按词形替换**，不依赖上下文；这也是“绑定位不能用关键字表词形”的根因。

### 练习 2：补一个标准库词形（并保持 11 语言齐平）

1. 在 `crates/engine/lang-packs/zh/stdlib.toml` 增加一条（追加到节末尾，保持主词条在前）；
2. 至少同步 `en` 的恒等键，并让 `python3 tools/check-lang-packs.py --strict` 通过；
3. 重新生成隔离构建树并验证：
   ```bash
   make zh-build ARGS='build --bin rzc'
   echo '函数 主函数() { 让 v = 向量::新建(); 打印行!("{}", v.长度()); }' > /tmp/词条.zh
   .zh-build/target/debug/rzc transpile /tmp/词条.zh
   ```
4. 注意：改语言包后 rzc 的**编译期内嵌数据**需要重新构建才生效；转译缓存会按
   语言包指纹自动失效，通常不用手动清。

### 练习 3：走一遍 E0308 诊断翻译

```bash
mkdir -p /tmp/诊断练习 && cd /tmp/诊断练习
cat > Cargo.toml <<'EOF'
[package]
name = "诊断练习"
version = "0.1.0"
edition = "2024"
EOF
cat > main.zh <<'EOF'
函数 主函数() {
    让 数: 整数 = "文本";
}
EOF
/home/oop/code/i18n-rust/.zh-build/target/debug/rzc check main.zh --no-lint
```

输出应为 `错误[E0308]: 类型不匹配：期望 整数，实际得到 字符串引用` 并附 💡 提示。
对照阅读顺序：`诊断模型.zh`（取 code/spans）→ `消息翻译.zh`（E0308 桩）→
`诊断翻译.zh`（类型本地化）→ `诊断输出.zh`（渲染）。

### 练习 4：给 LSP 加一个响应映射

以悬停（hover）为例：在 `crates/lsp/src/响应映射/响应.zh` 找到对应匹配臂，把
rust-analyzer 返回的 `MarkupContent` 文本里残留的英文标识符按 `翻译缓存` 的映射
回译；再在 `响应映射/测试.zh` 加一条“正例 + 反例”单测。验证：

```bash
make zh-build ARGS='test -p i18n-rust-lsp --lib'
```

### 练习 5：写一个方言单测并对拍 eject

1. 在 `crates/zh-pure/tests-zh/纯函数.zh` 增加一个 `#[测试]` 函数（写法参考同文件）；
2. 跑：
   ```bash
   make zh-build ARGS='test -p zh-pure'
   ```
3. 把同一段逻辑写在临时 `.zh` 文件里，执行 `rzc eject`，再 `cargo run` 对比输出
   ——这就是“转译产物与手写 Rust 等价”的最小验证。

## 5. 方言书写规则（读代码前必须知道）

| 规则 | 说明 | 相关记录 |
|---|---|---|
| `如果让` 不能写 `如果 有值(x) = y` | 后者会产出非法的 `if Some(x) = y`，且 transpile/lint 零提示 | F22 |
| 声明名/绑定名避开关键字表词形 | 词法层无位置保护，`让 无 = x` 会变 `let None = x`；同目录声明还会冻结整目录的替换 | F12/F21/F57 |
| 排除一词多译 | 一个中文词只能对应一个英文值；歧义词进代码区会被 `check-zh-artifacts` 判红 | F17 |
| 模块文件名 `mod.zh` 的子模块同目录 | `#[path]` 不能带 `mod/` 前缀（0.9.0 已修） | `注解导出模块` |
| 中文模块要手写 `#[path]` | rustc 不为非 ASCII 模块名推断文件名（E0754） | F1 |
| 新增/删除同目录 `.zh` 后重转整目录 | 声明冻结是“按目录聚簇”的，单文件重转可能漏掉连带影响 | F21 |
| 改词表要 11 语言齐平 | `check-lang-packs --strict` 会逐包比对键与占位符 | 语言包门禁 |

## 6. 调试与取证

```bash
RZC=.zh-build/target/debug/rzc

# 看映射加载来源（内置/项目/全局/显式目录）
RZ_LOG=info $RZC check 文件.zh

# 只看转译、不编译（单文件）
$RZC transpile 文件.zh

# 批量导出（可传多个文件/目录，失败项单独汇总）
$RZC eject src/main.zh src/工具.zh

# 查看未翻译消息
$RZC gaps --export /tmp/gaps.tsv

# 语言包覆盖度（在仓库根）
$RZC mapping coverage --lang zh

# 诊断语料抽样（需要采集器）
python3 tools/diag-corpus/collect.py --help
```

常见现象与定位：

| 现象 | 先看哪里 |
|---|---|
| 中文词没被替换 | 语言包是否收录该词形；是否被同目录声明冻结；是否是一词多译被禁用 |
| 产物编译报 E0308/E0277 | `诊断/消息翻译.zh` 的码表桩；`诊断/诊断翻译.zh` 的类型本地化 |
| 行号对不上 | `列位映射.zh`；`#[path]` 注解行是否参与行号换算 |
| LSP 诊断不更新 | `翻译缓存.zh` 的失效键；`镜像检查.zh` 的 `check_running/pending` |
| 改了语言包不生效 | 是否重新构建 rzc（内置数据）；转译缓存指纹 |
| 门禁说“产物漂移” | 0.9.0 起不再比对入库产物；`zh-artifacts` 只验可转译性与审计 |

## 7. 提交清单

zh 分支：

```bash
make zh-build ARGS='test --workspace --locked'
make zh-artifacts
make zh-ascii
```

eject 到 main 后（main 分支）：

```bash
make gate
make tutorials-all        # 改教程/转译规则时
make bench-check          # 改热路径时
```

提交信息用约定式提交（`fix(engine): …` / `feat(cli): …` / `docs: …`），
一次提交只做一件事；用户可见行为变更同步 `CHANGELOG.md` 的 `[Unreleased]`。

## 8. 中英名称对照（旧英文名 → 现中文名）

| engine | cli | lsp |
|---|---|---|
| alias → `别名` | builtin_lang → `内置语言` | analyzer → `分析器` |
| cache → `缓存` | crate_registry → `包仓库` | mirror_check → `镜像检查` |
| column_map → `列位映射` | diagnostics → `诊断输出` | response_map → `响应映射` |
| diagnostic → `诊断` | install → `安装` | server → `服务器` |
| eject_hints → `导出冲突提示` | lang_manager → `语言管理` | translation_cache → `翻译缓存` |
| error → `故障` | mapping_check → `映射检查` | ui → `界面消息` |
| fullwidth → `全角标点` | mapping_coverage → `映射覆盖` | diag_text → `响应映射/诊断文本` |
| guide → `指南` | mapping_gen → `映射生成` | |
| lexer → `词法器` | temp_guard → `临时守卫` | |
| lint → `校验` | translation_gaps → `翻译缺口` | |
| logger → `日志` | ui → `界面` | |
| mapping_manager → `映射管理器` | | |
| mapping_source → `映射源` | | |
| module_path → `模块路径` | | |
| name_cache → `名字缓存` | | |
| product_layout → `产物布局` | | |
| relocalize → `反译重生` | | |
| toolchain → `工具链` | | |
| ui → `界面消息` | | |
| unicode_confusion → `混淆检测` | | |
| language → `语言` | | |
