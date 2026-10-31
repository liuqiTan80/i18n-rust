# tools/ —— 脚本索引

> 原则：**本地统一入口是 [`make`](../Makefile)**（`make help` 列全）。
> 本目录脚本是各 `make` 目标的实现细节，或维护者专项工具；除专项外无需直接调用。

## 一览

| 脚本 | 用途 | 常用调用 |
|---|---|---|
| `verify-tutorials.py` | 教程代码块可编译性验证（CI 硬门禁）：提取围栏代码块逐块编译；错误示例断言预期错误码；`// 预期输出:` 块断言 stdout；相邻的 `// 文件:` 块合成一个多文件项目（见下）；`--ctx` 把白名单块补上本章前文再验一遍（见下）；`--outputs` 复验裸围栏「运行结果」是否等于程序实际 stdout（见下） | `make tutorials`（zh）· `make tutorials-all`（en/ja/ru） |
| `expected-failures.json` | 教程验证白名单：登记结构性噪音（跨块依赖等无法单独编译的块），键为 `文件:行号` | 随教程改动维护，须写明原因 |
| `ctx-baseline.json` | 上下文复验基线：白名单块补上前文后「前文补全通过」的**下限**（按语言分记，只增不减） | `--ctx --ctx-update` 上调；低于下限判红 |
| `output-baseline.json` | 输出展示复验基线：各语言「对齐对数（输出一致 + 顺序无关一致）」的**下限**（只增不减） | `--outputs --outputs-update` 上调；低于下限或出现任何「输出不符」判红 |
| `verify-glossary.py` | 术语表 ↔ zh 语言包一致性（术语表引用的词须能在语言包中找到） | `make glossary` |
| `check-glossary-consistency.py` | 术语三方一致性门禁（总术语表↔附录B↔各章「本章术语表」）：六项 HARD——两表词条重复、见引用断链/自环、见链与目标章号不一致、两表同词章号冲突、总表/附录B 词条须字面出现在被引章正文、章表词必在总表（覆盖度，跳过错误码 `^E\d{4}$` 与「常见问题」）；豁免登记 `glossary-phantoms.json`；`--report` 打印两表差集 | `make glossary-consistency` |
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
# 可选：--json report.json 输出机器可读报告 · --parallel N 并行度（默认 10）
#       --serialize 附加章节串联验证 · --ctx 白名单块上下文复验（门禁默认开启）
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

### 上下文复验（`--ctx`）

白名单条目（`tools/expected-failures.json`）过去相当于「这些块不验证」。`--ctx` 把它改成
「按读者的输入顺序验证」：对白名单里 category 为 `上下文依赖` / `教学演示` 的块，把**同一文档内
到此块为止**的前文按 `--serialize` 的拼接规则合成一个临时项目再跑一次——读者照着书敲到这一步，
手里正是这份前文。每块归入三态之一：

| 归属 | 含义 | 处置 |
| --- | --- | --- |
| `前文补全通过` | 只是缺前文，补上即编译通过 | 计入基线下限（`tools/ctx-baseline.json`，只增不减） |
| `本块致错` | 去掉本块前文能编译，加上本块就报错 | 逐条排查：多半是正文真墙（如 15.2 的 `让 颜色` 少写 `可变`，`弹出` 即报 E0596） |
| `前文噪音` | 前文自身还编不过（它自己也在白名单里），无法归因 | 修好前面的块，噪音自然转移成前两态 |

基线下限按语言记，并随白名单**收缩等量放宽**：条目被删（多半因为该块已能单块
编译，不再需要上下文）后它就不在复验集合里了，通过数会跟着少一个——这是进步不是
回退，所以判定用的下限是「基线 − 复验块数的跌幅」，跌幅超过收缩量才判红。

`故意报错` / `练习答案` 两类不复验：拼上前文也不该编译通过（前者是反例，后者把答案留给读者），
复验它们只会制造假红。临时项目目录名带上「块序 + 阶段」，否则同章多块并行会写同一个
`src` 互相覆盖（实测让通过数在两跑之间跳动）。

```bash
# 记录 / 上调基线（改完教程后跑；非 zh 附加 --dir tutorials/en --lang en）
python3 tools/verify-tutorials.py --allowlist tools/expected-failures.json --ctx --ctx-update
```

`make tutorials` / `make tutorials-all`，以及 CI（`.github/workflows/ci.yml`）与发版链
（`release.yml`）的教程步骤都已带 `--ctx`：「前文补全通过」低于基线即判红，
高于基线只提示可上调，`本块致错` 逐条打印（含第一条真错误行，lint 教学提示行已剥离）。

### 输出展示复验（`--outputs`）

书里给读者看的「运行结果」有两种载体：代码块内的 `// 预期输出:` 注释（单块验证已断言
stdout），和紧随代码块之后的**裸围栏**（` ``` ` 无语言标注）。后者历史上不受任何校验——
正文一改，展示块跟着改，改漏就一直错着（实测：并发会话把 9.3 的 `推入('，')` 改成
`推入('！')`，展示块那一处就漏了；四语 224 对里，en/ja/ru 各有数条与程序实际输出不符）。

**配套规矩：裸围栏只放程序打印出来的 stdout。** 命令行记录（`$ cargo test`）、编译器
诊断、恐慌信息改用 ` ```text ` / ` ```bash `；zh 已按此清理 41 处。

验证器把每对「程序块 + 紧随其后的展示块」拿去 `rzc run`，展示块必须与实际 stdout 逐行
严格相等。配对条件（宁漏不错）：① 展示块是裸围栏；② 上一个围栏是 ` ```rust `；③ 两者
没跨过 `<details>` 边界（否则练习题的「题目要求」块会被当成运行结果）；④ 中间有引导语
（正文以「输出：/出力：/Output:/Вывод:」收尾，或最近标题是「运行结果」）。

| 归属 | 含义 | 处置 |
| --- | --- | --- |
| `输出一致` | 展示块逐行等于实际 stdout | 计入对齐下限 |
| `输出不符` | 展示块与程序真实输出不同 | **任何一条直接判红，不受基线保护** |
| `顺序无关一致` | 白名单登记过 `输出顺序不定`，本次行序不同、内容逐行一致 | 计入对齐下限；少了照样判红 |
| `无法复验` | 程序单独跑不通（上下文依赖）、省略块、运行非零退出 | 逐条打印，不判红 |
| `非stdout展示` | 形态是诊断/命令行/构建记录 | 提示改 ` ```text `，不判红 |

`rzc run` 会把引擎警告追加到 **stdout**（`警告[...]`、`  -->`、`   |`、`💡`、`---`，
俄文还有 `Предупреждение[...]` 与语言包缺口的「сохранён как」提示行）。比对前
`strip_engine_noise` 从第一条噪音行处截断——展示块只该印程序自己打印的内容。

固有不确定只有哈希容器（`哈希集合`/`哈希映射`）的遍历顺序：同一块连跑三次可得三种行序。
这类块在白名单登记 `输出顺序不定`，验证器**只放宽顺序、不放宽内容**（按行排序后逐行
核对，少一行、多一行、改一行都判红）。教学上需要读者每次复现的场合（第二十五章综合
实战）直接改用 `有序映射`，而不是登记豁免。

基线 `tools/output-baseline.json` 按语言记「对齐对数 = 输出一致 + 顺序无关一致」的下限，
只增不减；配对数比基线每少一对，下限等量放宽（那对多半被改成 ` ```text ` 或挪进答案区）。

```bash
# 记录 / 上调基线（改完教程后跑；非 zh 附加 --dir tutorials/en --lang en）
python3 tools/verify-tutorials.py --allowlist tools/expected-failures.json --outputs --outputs-update
```

`make tutorials` / `make tutorials-all`、CI 与发版链的教程步骤都带 `--outputs`。
## 白名单维护

- **答案折叠区归 `练习答案`**：`<details> 查看答案`（含 `参考答案` / `Reference answer` / `🔍 Ответ`）
  里的片段是「读者不必抄」的，身份就是 `练习答案`——它既不参与上下文复验，也不进后续块的
  前文。挂在 `上下文依赖` 下只会抬高 `本块致错`（实测四语 21 条如此）。
- **过期即删**：门禁会列出「已不失败，建议删除」的条目。把块改成自足（补上缺失的
  `使用`、把依赖的数据写进块内）之后，条目要删掉，否则它会一直替这个块遮丑。
- **行号会漂**：条目以 `文件:起始行号` 定位，正文增删行就要同步。按「第 k 个代码块」
  重算（新旧文件的块序号一一对应，再用块内容等值校验），比按肉眼数行可靠——直接按
  「新文件里是否存在这个行号」判定会撞上别的块的头，实测漏不动。
- **省略标记只看代码**：`...` / `……` / `省略` 出现在代码里，或作为注释的第一个 token
  （`// ... 这里略过实现`）时，该块才被判为「省略块」跳过。写在注释中间的词不算——
  `让 全部 = &分数[..]; // 两头都省略 = 整段` 讲的是范围语法，`} // ❌ 还可能……` 讲的是
  穷尽匹配，两者都是要验证的正文（旧规则把它们整块跳过：前者连累下一块在 `--ctx` 里
  被误判成本块致错，后者同小节的两个反例块则从来没进过验证）。

## 注意事项

- Python 脚本一律 `python3`；`__pycache__/` 已被 gitignore；
- **基准回归必须在同一台机器对照**：`bench-check.sh` 与入库基线比较，换机跑请先复核；
  `--update` 会重写基线文件（随提交走），支持 `--only engine|lsp` 与 `BENCH_REGRESSION_PCT` 调阈值；
- 注册中心两脚本仅在协议 / 发布流程变更时使用（见 [third-party-registry.md](../docs/third-party-registry.md)）；
- `dialect-convert.py` 产物只是初稿，必须过 `make tutorials-all` 门禁；
- **验证器的方言登记只有一处**：`verify-tutorials.py` 的 `MAIN_FUNCS` 同时喂「完整程序」识别
  与主函数体拆分。新方言只往这张表填就行；再写出 `if lang == "zh" … elif …` 这种硬编码分支
  就会重演 ru 的故障——`функция главная` 在那头认得到、在这头认不到，含主函数的块拆不出
  函数体，拼出的前文多一个孤立 `}`，整章永远归不进「前文补全通过」；
- 飞书发布凭证（`FEISHU_APP_SECRET`）仅经环境变量传入、严禁入库；平台侧
  一次性配置与发布后核验见[发布准备清单](../docs/strategy/发布准备清单.md)第 5 节。
