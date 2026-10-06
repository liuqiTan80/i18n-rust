# 新语言包贡献指南

本文介绍如何为 i18n-rust 新增一门语言包，并把它分享给其他用户使用。
如果你是**用母语写应用**的开发者，只是遇到个别词条缺失，请看
[missing-mapping-guide.md](./missing-mapping-guide.md)（应用开发者缺词条指引）。
第三方库映射（crates/）的工具链细节见 [third-party-mapping.md](./third-party-mapping.md)。

## 1. 语言包的组成

一个完整的语言包目录（以 `vi` 越南语为例）结构如下：

```
crates/engine/lang-packs/vi/
├── keywords.toml      # 关键字映射（fn/let/mut/... → 母语词）
├── stdlib.toml        # 标准库别名（String/Vec/println!/... → 母语词）
├── module_paths.toml  # use 路径段映射（std/collections/... → 母语词）
├── errors.toml        # 编译错误消息翻译（教学用）
├── lang_info.toml     # 语言元信息（名称、代码、作者等）
├── ui.toml            # CLI 界面文案翻译
└── crates/            # 第三方库映射（可选，可用工具自动生成）
    ├── salvo.toml
    └── ...
```

顶层 6 个文件目前以现有语言包（推荐参照 `zh`）为模板人工翻译；
`crates/` 目录可由脚手架工具自动生成骨架再翻译。

## 2. 本地翻译流程

### 2.1 核心文件

复制一份现成语言包作为起点，逐文件翻译键名：

```bash
cp -r crates/engine/lang-packs/zh crates/engine/lang-packs/vi
# 逐个编辑 keywords.toml / stdlib.toml / module_paths.toml / errors.toml / lang_info.toml / ui.toml
```

注意：键的英文值（等号右侧）不能改，只翻译左侧的母语键。

### 2.2 第三方库映射（crates/）

```bash
# 方式一：AI 自动翻译键名（需设置 DEEPSEEK_API_KEY）
rzc mapping scaffold zh vi --provider deepseek

# 方式二：生成 TODO 骨架，人工翻译键名
rzc mapping scaffold zh vi
```

`--provider deepseek` 会批量调用 AI 翻译全部键，并自动做冲突改名重试；
未翻译成功的键保留 TODO 标记——**重跑同一条命令即可自动补齐残留键**
（已存在的文件不会被重新生成，既有翻译不丢失），其余人工补齐。

### 2.3 校验

```bash
rzc mapping check crates/engine/lang-packs/vi     # crates 映射：重复键/关键字碰撞/跨文件冲突
rzc lang install crates/engine/lang-packs/vi      # 本地安装验证目录完整性
rzc lang list                       # 确认出现在列表中
```

端到端验证：写一段母语方言源码，`rzc eject` 应转出标准 Rust。

### 2.4 调试：“改了不生效”排查

语言包经 `build.rs` **编译期嵌入二进制**（`include_str!`），在项目外（如 /tmp）运行时
走的是内置数据——修改 `stdlib.toml` 等文件后若不重新编译，验证的仍是旧行为。

排查三步：

```bash
# 1. 确认数据来源（内置/项目/全局/显式目录，RZ_LOG=info 可见）
RZ_LOG=info rzc check 你的文件.zh
#    [信息] [映射加载] 映射数据来源: 内置语言包 zh（编译期嵌入，修改后需重新编译）

# 2. 重新编译使内置数据生效（仓库内修改后必做）
cargo build -p rzc

# 3. 想免编译验证：在主仓库内运行（走文件系统），或用 --lang-pack 指定目录
rzc check 你的文件.zh --lang-pack crates/engine/lang-packs/zh
```

加载优先级：`--lang-pack` 显式目录 > 项目内 `lang-packs/<码>`（主仓库为
`crates/engine/lang-packs/<码>`）> 全局 `~/.rz/lang-packs/<码>`（`RZ_LANG_DIR` 可改）> 内置。

### 2.5 陷阱：全局旧语言包遮蔽内置表

`~/.rz/lang-packs/` 下**含 `keywords.toml` 的目录都会被当作语言包吃掉**，语言 code 取
**目录名**。两个后果：

- 早期 `rzc lang install` 留下的旧快照会**遮蔽内置表**：补了新词条、重编译了 release，
  跑出来仍是旧译文。
- 给旧目录改名（如 `zh` → `zh.bak`）**不算移出**——`zh.bak` 仍被识别为 code 为 `zh.bak`
  的语言包并接管 `.zh` 扩展名。

处置：把陈旧目录**移出 `lang-packs/` 整个父目录**（如 `~/.rz/stale-<码>-pack`），
再用 `RZ_LOG=info rzc check 文件.zh` 确认「映射数据来源」已回到内置表。

上述两类情况 `rzc doctor` 会直接报出来（含版本对比与解除命令），无需靠猜：

```bash
rzc doctor
# 语言包: /home/<user>/.rz/lang-packs
#   ⚠ 全局包 zh（v0.3）遮蔽内置语言包（v1.0），当前 .zh 源码走全局副本
#     → 升级 rzc 后诊断文案仍是旧的，多半是此因：rzc lang remove <码>，或把该目录移出 lang-packs/
#   ⚠ 备份目录 zh.stale-test.bak 仍被当作语言包吃掉（语言 code 取目录名）：请移出 …/.rz/lang-packs
```

未报警只表示「无同名遮蔽 / 无备份遗留 / 无目录名与声明扩展名不符」；第三方自研包
（目录名与扩展名一致，如 `vi`）属正常，不报警。

### 2.6 陷阱：诊断译文只有真跑才暴露缺口

诊断消息按「精确匹配 → 最长通配段 → 最长前缀 → 最长后缀」查表（通配段键见 2.7）。
因为有 `consider `、`try `
这类**前缀兜底键**，表残缺不会回落成整句英文，而是输出半截译文半截英文：

```text
💡 修复建议：考虑 making this binding mutable      ← 命中 `consider `，后半句未译
```

这类缺口静态检查（TOML 解析、键数齐平、覆盖度对比）**一律看不见**，只能实跑采样。
补键方法：

```bash
# 1. 取 help / note 子消息原文（JSON 不会因首个错误提前中止）
rustc --edition 2021 --emit=metadata --error-format=json 反例.rs 2>&1 \
  | python3 -c "import sys,json;[print(d['level'],':',d['message']) for l in sys.stdin if l.startswith('{') for d in [json.loads(l)]+json.loads(l).get('children',[]) if d['level'] in ('help','note')]" | sort -u

# 2. 免重编译试键：写进临时语言包目录，用 --lang-pack 指过去
echo '["消息翻译"."consider cloning the value if the performance cost is acceptable"]' >> /tmp/pack/errors.toml
rzc check 反例.zh --lang-pack /tmp/pack
```

两点注意：兜底前缀键会**吞掉整条消息**（如 `the trait \`` → 只译开头、后半句连同关键
类型名一起丢失），故只在译文能覆盖完整语义时才加前缀键；前缀匹配**优先于**后缀匹配，
两种键形态不要对同一句式并存，否则后缀键永不生效。

### 2.7 通配段键：两端动态的消息

动态名不止一处时（`the trait \`X\` is not implemented for \`Y\``、E0106 的
`...borrowed from \`a\` or \`b\``），单一前缀/后缀键只能取到头段或尾段。这类消息改用
**通配段键**：键里用 `?` 标记每段动态内容，第 n 个 `?` 依次对应模板里的 `{q0}`/`{q1}`/`{q2}`。
匹配顺序为「精确 → 最长通配段 → 最长前缀 → 最长后缀」，即通配段键**优于**前缀/后缀键；
消息必须同时以首段字面量开头、以尾段字面量结尾，否则不命中；未被模板引用的 `?`
直接丢弃（不会粘回原文）。

```toml
["消息翻译"."the trait `?` is not implemented for `?`"]
"消息模板" = "特征「{q0}」未对「{q1}」实现"
```

两个易踩的约束：

- **锚点要全局唯一**。`?` 按「上一段之后首次出现」定位，若消息前半句里也含有你的
  锚点子串（如 `...a borrowed value, but the signature does not say ... borrowed from`
  里 `borrowed from ` 出现两次），首个 `?` 会把整段前缀当成参数名抓走。键请从消息
  开头写全，不要只剪句中片段。
- **含撇号的键不能用单引号字面量串**。TOML 的 `'...'` 不支持转义，`doesn't`/`can't`
  里的撇号会把字符串提前截断，整包解析失败；此类键（含值）必须用双引号串并把内部的
  `"` 转义。提交前先逐包回读一遍：

```bash
python3 -c "import tomllib,pathlib;[print(d.name, len(tomllib.loads((d/'errors.toml').read_text(encoding='utf-8')).get('消息翻译', {}))) for d in sorted(pathlib.Path('crates/engine/lang-packs').iterdir()) if (d/'errors.toml').is_file()]"
```

## 3. 分享给他人：两条路线

### 路线 A：合入主仓库（推荐，所有人默认内置）

1. Fork 本仓库，把语言包放入 `crates/engine/lang-packs/vi/`
   （项目为单一数据源架构：编译期内嵌与文件系统消费共用这一份，
   无需任何同步步骤）
2. 若希望**编译期内置**（无需安装即可用），还需在
   `crates/cli/src/内置语言.rs` 中：
   - 用 `定义内置语言!` 宏添加 vi 的静态数据（含 crates/ 文件名列表）
   - 在 `获取内置数据` 与 `拥有内置语言` 增加分支
   - 更新 `内置语言代码` 与相关测试断言

   也可以只合入数据不内置——用户通过
   `rzc lang install vi` 从主仓库远程安装（安装器已兼容
   `crates/engine/lang-packs/<语言>` 目录结构）。
3. 提交 PR。CI 会跑全量测试与 `rzc mapping check` 质量门禁。

### 路线 B：自建语言包仓库（无需 PR，即发即用）

把语言包推到自己的 git 仓库，支持两种目录结构：

```
你的仓库/
├── vi/              # 结构一：仓库根直接放语言目录
└── lang-packs/vi/   # 结构二：嵌套一层 lang-packs/（二选一即可）
```

其他用户一条命令安装（git clone 优先，失败自动回退 curl 下载 ZIP）：

```bash
RZ_LANG_REPO=https://gitcode.com/你的账号/你的语言包仓库 rzc lang install vi
```

说明：

- `RZ_LANG_REPO` 指向你的仓库地址即可，无需发布到任何注册表
- GitCode 仓库默认按 `master` 分支打包，GitHub 按 `main` 分支
- 更新语言包后用户重装加 `--force`：`rzc lang install vi --force`
- 删除：`rzc lang remove vi`（不影响内置语言包）

## 4. 验证清单（提交前自查）

- [ ] 顶层 6 个 toml 齐全，键的英文值未被改动
- [ ] `rzc mapping check crates/engine/lang-packs/<码>` 无错误
- [ ] `rzc lang install crates/engine/lang-packs/<码>` 安装成功
- [ ] 母语方言源码 `rzc eject` 转出标准 Rust 且可编译
- [ ] `make lang-packs`（`tools/check-lang-packs.py`）全绿：逐包 TOML 真回读、消息表/错误码表/关键词/stdlib 键数结构齐平、版本号与内置表一致、无短前缀吞整句告警（进 CI 门禁，无需靠手工）
- [ ] 反例语料无残留：`cargo test -p i18n-rust-engine --test diag_corpus_gate` 全过（语料来自 `tools/diag-corpus/collect.py` 对 `rustc --error-format json` 的实跑采样，签入 `crates/engine/tests/data/diag-corpus.tsv`；新增高频反例后重跑 collect.py 刷新语料即自动拦住中英混排）
- [ ] `~/.rz/lang-packs/` 下无陈旧/改名遗留目录（`rzc doctor` 可直接查出，见 2.5）
- [ ] 高频反例实跑 `rzc check`，逐条看「修复建议」有无中英混排（静态检查查不出，见 2.6）
- [ ] 两端动态的消息用通配段键（见 2.7），且逐包跑过上面的 TOML 回读命令（撇号会炸掉整包）
- [ ] （路线 A）若内置，`cargo test --workspace` 全过
