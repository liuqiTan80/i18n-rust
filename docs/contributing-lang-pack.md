# 新语言包贡献指南

本文介绍如何为 zrRust 新增一门语言包，并把它分享给其他用户使用。
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

## 3. 分享给他人：两条路线

### 路线 A：合入主仓库（推荐，所有人默认内置）

1. Fork 本仓库，把语言包放入 `crates/engine/lang-packs/vi/`
   （项目为单一数据源架构：编译期内嵌与文件系统消费共用这一份，
   无需任何同步步骤）
2. 若希望**编译期内置**（无需安装即可用），还需在
   `crates/cli/src/builtin_lang.rs` 中：
   - 用 `define_builtin_lang!` 宏添加 vi 的静态数据（含 crates/ 文件名列表）
   - 在 `get_builtin_data` 与 `has_builtin_lang` 增加分支
   - 更新 `builtin_lang_codes` 与相关测试断言

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
- [ ] （路线 A）若内置，`cargo test --workspace` 全过
