# 应用开发者缺词条指引

你用 i18n-rust（`rzc`）写应用时，发现某个库 API 没有对应的母语词条、
或想给项目加自己的译名，按本文处理。
本文面向**写应用的新手**；新增整门语言、翻译内核词表等维护者话题见
[contributing-lang-pack.md](./contributing-lang-pack.md) 与
[third-party-mapping.md](./third-party-mapping.md)。

约定：下文用 `zh` 代表语言代码（中文包），换成你使用的语言即可。

## 1. 先分清「缺词条」和「写错了」

| 现象 | 大概率原因 |
|---|---|
| `rzc check` 报 `E0599 类型上找不到该方法`，方法名还是中文 | 缺词条，本文要解决的 |
| 很常见的词也报同样的错 | 写法与词表不一致（先查词表） |
| 报错明显指向英文 API 或语法 | 代码本身的问题，按错误提示正常排错 |

查词表（速查表收录关键字、模块路径、标准库与第三方别名词，共一千多条）：

```bash
rzc cheat zh | grep 长度        # 搜「长度」有没有词条
rzc cheat zh --markdown         # 导出 Markdown 表格，可存进自己的笔记
```

真实例子——写 `列表.调整大小(10, 0);`：

```
错误[E0599]: 类型上找不到该方法
  --> main.zh:6:8
   |     列表.调整大小(10, 0);
```

`调整大小` 不在词表里（标准库没有 Vec 的 `resize` 词条），所以原样保留，
Rust 侧自然没有这个方法——补一条 `"调整大小" = "resize"` 即可（见第 3 节）。

## 2. 先认清：项目用的是哪一份语言包

映射数据有四级来源，**按优先级整体选用，不是合并**：

| 优先级 | 来源 | 位置 | 说明 |
|---|---|---|---|
| 1 | `--lang-pack` 指定 | 命令行参数 | 不动任何文件，临时试用 |
| 2 | 项目语言包 | `<项目根>/lang-packs/<码>/` | 随项目走、团队共享（推荐） |
| 3 | 全局语言包 | `~/.rz/lang-packs/<码>/`（`RZ_LANG_DIR` 可改） | 本机跨项目 |
| 4 | 内置语言包 | 编译进 rzc 二进制 | 零配置默认 |

确认实际来源：

```bash
RZ_LOG=info rzc check src/main.zh
# [信息] [映射加载] 映射数据来源: 项目语言包 /path/lang-packs/zh（覆盖全局与内置）
```

两个关键事实：

- **项目语言包要放就放完整包**：目录里连 `keywords.toml` 都没有会直接报错
  「加载本地语言包失败: 关键字文件不存在」——不能只丢一个定制文件进去；
- 项目语言包**整体覆盖**全局与内置：想"改词"也用它，项目内同名键会取代内置的旧词。

（项目根 = 从源文件所在目录向上找到的第一个含 `Cargo.toml` 的目录。）

## 3. 处理缺词条

### 3.1 先查社区有没有现成的

```bash
rzc crate search rand              # 关键词可省：列出注册中心全部已发布映射
rzc crate install rand --lang zh   # 只装 rand 这一个 crate 的映射到全局语言包
```

### 3.2 项目内加词条（推荐）

```bash
# 1) 拿一份完整语言包
rzc lang install zh                              # 从远程装到全局 ~/.rz/lang-packs/zh
cp -r ~/.rz/lang-packs/zh 你的项目/lang-packs/zh   # 复制进项目
```

2) 在 `crates/` 下新建定制文件（文件名任意，也按 crate 分文件）：

```toml
# 你的项目/lang-packs/zh/crates/项目定制.toml
["模块路径"]
"MySQL" = "mysql"          # use 语句里的路径段

["标识符"]
"调整大小" = "resize"      # 类型 / 函数 / 方法名
```

3) 校验并编译验证：

```bash
rzc mapping check lang-packs/zh    # 质量校验：重复键 / 关键字碰撞 / 跨文件冲突
rzc check src/main.zh              # 编译验证
```

命名三条（花十秒检查，省得回头改名）：

1. **避开关键字**：`错误` `空` `文本` 这类词已被关键字体系占用，碰到就加字
   （`错误类型`）——与关键字同键且值不同会直接报错；
2. **一词一义**：同一中文词在不同库想对应不同 API 时，用多字词区分
   （如 `阻塞`→`阻塞调用`、`启动`→`发射`）；跨文件同键不同值是硬错误；
3. **能同值就同值**：不同 crate 里同一个 API（如 `Request`）共用同一中文词
   是安全冗余，直接从别处抄来即可。

### 3.3 批量生成骨架：rzc mapping auto

```bash
rzc add rand@0.8.5                                       # 添加依赖
rzc mapping auto rand --lang zh --target-version 0.8.5   # AI 生成（默认 deepseek，需 DEEPSEEK_API_KEY）
rzc mapping auto rand --lang zh --provider rule          # 离线规则模式：只翻能推断的词，其余保留英文
```

- 默认输出到 `lang-packs/<码>/crates/<crate>.toml`（`--output` 可改路径），生成后人工润色键名；
- 生成完**自动跑一次冲突校验**，与关键字撞车的条目（如 `错误`→`Error`）会当场报出来，
  手动改名（`错误类型`）即可；
- `--install` 可顺带把依赖写进 `Cargo.toml`。

### 3.4 全局生效（本机跨项目）

直接编辑 `~/.rz/lang-packs/zh/crates/` 下的 toml（没有就先 `rzc lang install zh`）。
注意 `rzc lang install zh --force` 重装会覆盖你的改动；团队共享与长期维护仍建议项目包。

## 4. 「改了不生效」排查

先看数据来源，再对照下表：

```bash
RZ_LOG=info rzc check 文件.zh
```

| 你改的是 | 生效方式 |
|---|---|
| 项目语言包 | 保存即生效 |
| 全局语言包 | 保存即生效（注意 `--force` 重装会覆盖） |
| 内置语言包（zrRust 仓库内） | `cargo build` 重新编译后生效 |
| 只是验证想法 | `rzc check 文件.zh --lang-pack <语言包目录>`，用指定目录，不动任何文件 |

翻译缓存按语言包指纹自动失效，一般无需手动清缓存。

## 5. 新手常见坑

**变量名被替换了？**
词表里有两类词，行为不同：

- 关键字词（`文本` `错误` `盒子` 等）：词法层替换，出现在哪里都换，
  `let 文本 = ...` 会变成 `let str = ...`；
- 别名词（`长度` `容量` `路径` 等）：在 `let` 声明、函数名、参数、字段名等
  位置有保护，但**模式绑定没有保护**——`如果让 有值(路径) = ...` 里的 `路径`
  会被替换（若映射值是大写名如 `Path`，还会触发命名警告）。

建议自定义标识符避开词表词，先用 `rzc cheat zh | grep 词` 查一下。

**多字词会被拆开吗？**
不会。替换按整词（token）匹配，`参数` 和 `单参数` 可以共存、互不干扰。

**`use` 里能翻，函数体里不行？**
模块路径条目**只在 `use` 路径中生效**；函数体内写作 `库名::函数()` 时（如
`serde_json::to_string_pretty`），要在「标识符」节补一条同键同值的副本。

**看到 `"English" = "English"` 这样的条目？**
规则模式生成的恒等占位，替换等于没换；把键名改成中文即生效。

**项目语言包会不会滞后于内置更新？**
会。项目包是完整快照，内置词表升级后项目里不会自动跟进；升级后重新复制一份
新包、把定制文件 diff 合并进去即可。

## 6. 想让所有人都用上你的词

- 发布到社区注册中心：`rzc crate publish <crate> --lang zh`，
  其他用户 `rzc crate install` 即可获得（详见
  [third-party-registry.md](./third-party-registry.md)）；
- 回流内置词表：向上游仓库提 PR（详见
  [contributing-lang-pack.md](./contributing-lang-pack.md)）。

还有疑问时，先跑 `rzc mapping check` 和 `RZ_LOG=info rzc check`，
多数问题这两条命令就能定位。
