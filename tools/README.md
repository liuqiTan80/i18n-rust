# tools/ —— 脚本索引

> 原则：**本地统一入口是 [`make`](../Makefile)**（`make help` 列全）。
> 本目录脚本是各 `make` 目标的实现细节，或维护者专项工具；除专项外无需直接调用。

## 一览

| 脚本 | 用途 | 常用调用 |
|---|---|---|
| `verify-tutorials.py` | 教程代码块可编译性验证（CI 硬门禁）：提取围栏代码块逐块编译；错误示例断言预期错误码；`// 预期输出:` 块断言 stdout；相邻的 `// 文件:` 块合成一个多文件项目（见下） | `make tutorials`（zh）· `make tutorials-all`（en/ja/ru） |
| `expected-failures.json` | 教程验证白名单：登记结构性噪音（跨块依赖等无法单独编译的块），键为 `文件:行号` | 随教程改动维护，须写明原因 |
| `verify-glossary.py` | 术语表 ↔ zh 语言包一致性（术语表引用的词须能在语言包中找到） | `make glossary` |
| `check-ui-keys.py` | ui.toml 键完备性（11 语言与 zh 对齐 + `{}` 占位符一致性） | `make ui-keys` |
| `check-extension-ui-keys.py` | 扩展文案三方一致：`extension.ts` 消费的 `ext_` 键 = `gen-ui-strings.mjs` 白名单 = `src/generated/ui-strings.ts`，并校验生成物未陈落于 ui.toml；纯 Python，免 npm/node_modules | `make ext-ui-keys` |
| `ui-keys-index.py` | 生成/校验 `docs/ui-keys.en.md`（界面消息键 → 消费点 + `{}` 实参表达式 + zh 原文）：索引陈旧或新增无调用点的死键即失败；动态取键前缀需在此显式登记 | `make ui-index` / `make ui-index-write` |
| `check-mapping-warnings.py` | 映射告警数基线（只减不增）：运行 `rzc mapping check` 对照 `tools/mapping-baseline.json` | `make mapping-check` |
| `bench-check.sh` | 基准回归门禁：engine + lsp 双基准集与入库基线对比（阈值默认 30%） | `make bench-check` · `make bench-update`（刷新基线） |
| `build-site.py` | 文档站装配：从 `tutorials/`、`docs/` 原位组装四语言 mdBook 到 `_site/` | `make site` · `make site-serve` |
| `publish-feishu.py` | 教程同步到飞书知识库（上传素材 → md 导入 docx → 移入知识库）；按 `docs/strategy/feishu-publish-state.json` 的正文指纹增量：新章节导入、已变更章节 `--update` 原地替换正文 | `make feishu-check`（离线看落后清单）· `make feishu`（需 FEISHU_* 凭证） |
| `dialect-convert.py` | 标准 Rust 代码块 → 指定语言方言（写译本初稿用；字符串 / 注释不转换） | `python3 tools/dialect-convert.py --lang ja --src en.md --out ja.md` |
| `init-registry.sh` | 初始化"第三方映射共享注册中心"空仓库骨架（本地目录，含 git init） | `./tools/init-registry.sh [目标目录]` |
| `test-crate-registry.sh` | 注册中心端到端冒烟（publish / search / install / list / remove，全离线） | `RZC=target/debug/rzc bash tools/test-crate-registry.sh` |

## 排查单章时直接调用验证器

```bash
cargo build --bin rzc                                  # 先产出 target/debug/rzc
python3 tools/verify-tutorials.py \
  --dir tutorials/en --lang en \                       # 教程目录与语言包
  --rzc target/debug/rzc \                             # rzc 路径
  --allowlist tools/expected-failures.json             # 白名单
# 可选：--json report.json 输出机器可读报告 · --parallel N 并行度（默认 10）· --serialize 附加章节串联验证
```

### 多文件示例协议（`// 文件:`）

讲"一个模块一个文件"的示例（第 17 章）单独编译必然报 E0583/E0601，所以验证器支持把
相邻代码块合成一个临时项目整体检查：

- 块首行写 `// 文件: 餐具库.zh`，该块就被写进临时项目的 `src/餐具库.zh`；
- **同一文档内相邻**的此类块自动组成一个项目（≥ 2 块），中间插入别的代码块会拆组；
- 组内必须有入口文件：文件名词干为 `main`，或该语言方言里等价于 `main` 的母语词
  （zh `主函数` / ja `主関数` / ru `главная`）；没有入口直接判红；
- 组内文件名不得重复（重复会静默覆盖前一块，判红）；
- 只有一块带 `// 文件:` 声明也判红——作者多半漏了配对的这一块，不静默当普通块放过；
- `// 预期错误:` / `// 预期输出:` 写在组内任一块、整组继承；两者都不写时默认断言编译通过；
- 标记一律用中文，不随教程语言变化；`--serialize` 章节串联会跳过这些块。

现成用法见 `tutorials/第十七章：模块系统.md` 的 17.6 与练习三、
`tutorials/en/Chapter-17-Modules.md` 的 17.6 与 Exercise 3。读者不必在自己代码里写这行，
教程正文已就近说明它是门禁标注。

## 注意事项

- Python 脚本一律 `python3`；`__pycache__/` 已被 gitignore；
- **基准回归必须在同一台机器对照**：`bench-check.sh` 与入库基线比较，换机跑请先复核；
  `--update` 会重写基线文件（随提交走），支持 `--only engine|lsp` 与 `BENCH_REGRESSION_PCT` 调阈值；
- 注册中心两脚本仅在协议 / 发布流程变更时使用（见 [third-party-registry.md](../docs/third-party-registry.md)）；
- `dialect-convert.py` 产物只是初稿，必须过 `make tutorials-all` 门禁；
- 飞书发布凭证（`FEISHU_APP_SECRET`）仅经环境变量传入、严禁入库；平台侧
  一次性配置与发布后核验见[发布准备清单](../docs/strategy/发布准备清单.md)第 5 节。
