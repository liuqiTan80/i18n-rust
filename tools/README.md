# tools/ —— 脚本索引

> 原则：**本地统一入口是 [`make`](../Makefile)**（`make help` 列全）。
> 本目录脚本是各 `make` 目标的实现细节，或维护者专项工具；除专项外无需直接调用。

## 一览

| 脚本 | 用途 | 常用调用 |
|---|---|---|
| `verify-tutorials.py` | 教程代码块可编译性验证（CI 硬门禁）：提取围栏代码块逐块编译；错误示例断言预期错误码；`// 预期输出:` 块断言 stdout | `make tutorials`（zh）· `make tutorials-all`（en/ja/ru） |
| `expected-failures.json` | 教程验证白名单：登记结构性噪音（跨块依赖等无法单独编译的块），键为 `文件:行号` | 随教程改动维护，须写明原因 |
| `verify-glossary.py` | 术语表 ↔ zh 语言包一致性（术语表引用的词须能在语言包中找到） | `make glossary` |
| `check-ui-keys.py` | ui.toml 键完备性（11 语言与 zh 对齐 + `{}` 占位符一致性） | `make ui-keys` |
| `check-mapping-warnings.py` | 映射告警数基线（只减不增）：运行 `rzc mapping check` 对照 `tools/mapping-baseline.json` | `make mapping-check` |
| `bench-check.sh` | 基准回归门禁：engine + lsp 双基准集与入库基线对比（阈值默认 30%） | `make bench-check` · `make bench-update`（刷新基线） |
| `build-site.py` | 文档站装配：从 `tutorials/`、`docs/` 原位组装四语言 mdBook 到 `_site/` | `make site` · `make site-serve` |
| `publish-feishu.py` | 教程发布到飞书知识库（上传素材 → md 导入 docx → 移入知识库；同名幂等跳过） | `make feishu`（需 FEISHU_* 凭证；先 `--dry-run` 预览） |
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

## 注意事项

- Python 脚本一律 `python3`；`__pycache__/` 已被 gitignore；
- **基准回归必须在同一台机器对照**：`bench-check.sh` 与入库基线比较，换机跑请先复核；
  `--update` 会重写基线文件（随提交走），支持 `--only engine|lsp` 与 `BENCH_REGRESSION_PCT` 调阈值；
- 注册中心两脚本仅在协议 / 发布流程变更时使用（见 [third-party-registry.md](../docs/third-party-registry.md)）；
- `dialect-convert.py` 产物只是初稿，必须过 `make tutorials-all` 门禁；
- 飞书发布凭证（`FEISHU_APP_SECRET`）仅经环境变量传入、严禁入库；平台侧
  一次性配置与发布后核验见[发布准备清单](../docs/strategy/发布准备清单.md)第 5 节。
