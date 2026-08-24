# Changelog

## 0.6.1

### 修复
- **单文件直调判断修正**：有第三方依赖的项目不再误启用 rustc 直调（依赖行识别修正）；单文件判定范围扩展到项目根；临时 exe 失败时清理。
- **工具链下载安全**：`rzc install toolchain` 校验官方 SHA-256（防篡改）；`rzc doctor` 重构（状态行纯函数 + 版本升级指引）。
- **发布工作流健壮性**：修复 release.yml 结构损坏（缓存步骤缩进）；CI 工具链下载跨 run 缓存（300MB 复用）；LSP 的 PATH 扫描统一到 engine 工具链定位（删除重复实现）。
- **生产代码 unwrap 清零**：cargo run 管道 expect 改为错误传播。

### 文档
- README 与附录D 补充 `rzc install toolchain`/`rzc doctor`；离线发布包内置完整工具链（即开即用，无需 rustup）。

## 0.6.0

### 新增
- **内置官方工具链（脱离 rustup）**：`rzc install toolchain` 一键下载官方 standalone rustc/cargo/rust-analyzer 到 `~/.rz/toolchain`；`rzc doctor` 环境诊断；rzc 与 LSP 自动优先使用内置工具链——无需 rustup、无需 PATH 配置、无组件管理、无网络索引卡死。
- **单文件直调 rustc**：单文件项目（仅 main.zh、无依赖）`rzc run/check` 直接调用 rustc 绕开 cargo（无索引开销、无 Cargo.lock 依赖），诊断翻译完全复用；多文件/有依赖自动回退 cargo。

### 修复
- **语义着色同标识符跨行颜色一致**：rust-analyzer 的 token 修饰符（声明处 declaration、宏参数内 macro 等）在 VS Code 有独立颜色，代理侧强制清零——同一类型全局同色，便于按颜色查找标识符。

## 0.5.11

### 修复
- **LSP 诊断全中文**：编辑器波浪线诊断接入 errors.toml 消息表（与 CLI 同源，含教学提示与动态占位符填充），多行消息逐行翻译；新增 E0004 系列（匹配不完整/缺少匹配分支/类型说明/defined here）与 dead_code 复数变体（`变体 A 和 B 从未被构造`）翻译键。
- **语法高亮中文失效根因**：TextMate 的 `\b` 是 ASCII 词边界（中文永不成立），改为 Unicode 属性前后置断言——`匹配`/`让`/`函数` 等关键字在纯语法高亮下即可着色。
- **双占位符填充 bug**：`{q0}`/`{q1}` 取值修复（前者取第一个引号对、后者取最后一个）。

## 0.5.10

### 新增
- **派生特征名支持中文**：`#[派生(克隆, 调试)]` 可直接写中文特征名（克隆→Clone、调试→Debug、复制→Copy 等 9 个），方法调用 `值.克隆()` 不受影响；教程派生写法全面改为中文。
- **扩展启动诊断**：日志输出 LSP 二进制路径、版本与语言包路径，便于排查二进制新旧与语言包加载问题。

### 修复
- **中文提示补全**：新增 `匹配` 分支相关翻译（arm body without braces、expected a pattern、replace `;` with `,` 等 5 键+教学提示）；cargo 编译摘要 `due to N previous errors` 本地化为「（此前已有 N 个错误）」。

## 0.5.9

### 修复
- **语义着色错乱**：`pub fn main` 仅写入磁盘虚拟文件，发送给 rust-analyzer 的内存文档保持用户原文——变量/函数等颜色不再错位。
- **诊断悬停中文化**：relatedInformation 子消息（help/note）同步翻译，波浪线悬停不再出现英文。
- **补全体验**：关键字补全自动补空格（`让 可变` 不再粘连成 `让可变`）；方法/函数补全自动带括号（`长度()`）与光标占位；右键菜单移除未声明的 `editor.action.formatDocument`（消除 VS Code 校验警告）。
- **关键字提示**：用关键字做标识符时给出中文教学提示（"该词是关键字，不能用作标识符"）。

### 文档
- 教程 32 篇英文关键字批量中文化（`fn`/`let`/`mut`/`String`/`&mut` 等替换为方言写法）。

## 0.5.8

### 修复
- **LSP 稳定性**：修复 rust-analyzer 工作区重载竞态导致的随机崩溃（loaded_sysroot SendError panic）——虚拟项目工作区改在首次打开文档时以纯 added 添加，消除与初始化加载的并发；扩展自动重启上限放宽至 20 次，崩溃后自动恢复。
- **中文消息补全**：新增 40 个高频翻译键（语法错误 expected 兜底、help 建议 consider/try 兜底、所有权/借用 label、类型错误、note 消息）；`could not compile` 错误摘要译为「无法编译」。

## 0.5.7

### 新增
- **所有权可视化打通**：编辑器保存时代理自跑 cargo check（离线模式），E0382/E0502/E0507 等借用错误现在会显示为移动（黄）/再次使用（红）/生命周期（绿）高亮；修复虚拟项目 cargo check 入口（E0601/E0603）与 Cargo.lock 预生成（避免 crates.io 索引卡死）。
- **语义着色支持**：透传 rust-analyzer 的 semanticTokensProvider 能力，变量/参数/关键字等语义颜色生效（token 坐标经 delta 还原→列映射→重编码回方言文件）。

### 修复
- LSP 语言包缺失时的内置回退改为物化 engine 内嵌完整中文包（原硬编码旧表仅 54 词且无宏表，导致 `打印行`/`让` 不翻译、误报 cannot find macro 与语法错误）。

## 0.5.6

### 新增
- `rzc install lsp` 版本校验：已安装的语言服务器与 rzc 版本不一致或无法确认时，提示执行 `--force` 重装；`i18n-rust-lsp` 新增 `--version` 输出（10 语言提示文案）。

### 变更
- 移除英文语言包：Rust 本身以英文书写，恒等映射无教学价值，内置语言由 11 种调整为 10 种（`.en` 方言文件不再支持）。

### 内部
- cargo fmt 全量修复 + clippy 清零（CI 的 fmt 检查长期失效已修复）；CI Node 20→22。

## 0.5.5

### 修复
- `rzc init` 生成的 `rust-toolchain.toml` 不再硬编码 1.85：改为动态探测本机当前生效工具链版本（components 含 rust-analyzer/rust-src），避免 rust-analyzer 报"工具链过于陈旧"导致方言文件语义着色（变量颜色）失效。
- 测试环境变量竞态修复：依赖/修改 `RZ_LANG_DIR`、`LANG` 等环境变量的测试统一持进程级互斥锁，消除并行测试互相污染。

### 文档
- 重构 README：新增功能特性、命令速查、项目结构与工作原理等章节，补充 CI/crates.io/许可证徽章。
- 新增 `docs/dialect-framework-blueprint.md`（方言编程框架生成蓝图）。

## 0.5.3

### 修复
- LSP 在 Windows 上找不到 rust-analyzer 导致启动即退出、反复重启：查找逻辑跨平台化（PATH 扫描替代 Unix 专属 which，支持 PATHEXT/.exe；主目录定位回退 USERPROFILE），并补充 ~/.cargo/bin 候选。

## 0.5.2

### 新增
- `rzc install lsp` 一键安装语言服务器（VS Code 扩展的补全/诊断后端）：优先从离线包同目录免网络复制，否则从 crates.io 安装与 rzc 版本严格一致的版本。

### 改进
- LSP 缺失时的错误提示改为引导安装：`rzc install lsp`（或 `cargo install i18n-rust-lsp`）。

## 0.4.0

### 新增
- 对齐引擎全部 11 种方言语言：中文、English、日本語、Deutsch、Español、Français、Português、Русский、한국어、हिन्दी、العربية（语法高亮文件由 `scripts/gen-grammars.mjs` 从语言包自动生成）。
- 新增配置 `i18n-rust.rzcPath`：指定 rzc 命令行工具路径。
- AI 对话支持进度通知中取消；新会话自动中止上一个会话。
- 扩展日志输出通道「i18n-rust 日志」。
- 新增 `node --test` 单元测试（shell 引用、插入位置计算、TOML 解析、提示词生成）。

### 修复
- AI 系统提示词语言包定位失效（目录英文化后仍按显示名拼路径，导致永远回退英文提示词）：现按语言代码目录读取，并从 `lang_info.toml` 读取显示名。
- 全角符号自动转换在粘贴含换行的多行文本时替换位置跨行错位：改为逐字符换行感知的位置计算。
- 终端命令拼接存在引号/反引号注入风险：所有 `sendText` 参数改为平台感知安全引用；`eject` 改用无 shell 的 `execFile`。
- LSP 二进制查找仅支持 Unix `which` 且只看 `target/debug`：改为跨平台 PATH 扫描（Windows 支持 PATHEXT/.exe）并补充 `target/release`；找不到二进制时给出明确错误与设置入口。
- Run/Check 每次新建终端导致终端刷屏：改为复用单一 `i18n-rust` 终端。

### 变更
- AI 密钥从明文设置迁移到 VS Code SecretStorage（激活时自动一次性迁移，原设置项标记弃用）。
- 使用 esbuild 打包扩展（vsix 不再捆绑 node_modules，体积显著减小）。
- 语言包选择器列出全部 11 种语言。

## 0.3.1

- 右键菜单覆盖 rust-zh/rust-en/rust-de 三种方言并补充格式化命令。
- AI 提供商抽象层（OpenAI/DeepSeek/通义千问/智谱 GLM/Ollama/自定义）。
- 所有权错误可视化装饰器与全角符号自动转换。
