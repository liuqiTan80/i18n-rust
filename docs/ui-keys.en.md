# `ui.toml` message key index

<!-- 由 tools/ui-keys-index.py 生成，请勿手工编辑：先改生成器再 `python3 tools/ui-keys-index.py --write`。 -->

Every entry under `["界面消息"]` in a language pack is listed below with the
place in the code that renders it, how many `{}` placeholders it carries, and the
actual argument expressions passed at those call sites. Use it to judge the tone
of a message (error, hint, help text) and what each placeholder stands for
before you translate it.

Key count: **622** · statically referenced: 563 ·
resolved at runtime by prefix: 59 · unused: 0

| Prefix | Section |
|---|---|
| `ext_*` | VS Code extension (contributes labels, commands, settings) (44) |
| `cmd_*` | CLI: subcommand descriptions (clap help) (34) |
| `arg_*` | CLI: argument descriptions (clap help) (33) |
| `cli_*` | CLI: general output (2) |
| `mg_*` | CLI: `mapping` generation internals (35) |
| `mc_*` | CLI: `mapping check` findings (49) |
| `mapping_*` | CLI: `mapping` user-facing flow (38) |
| `lang_*` | CLI: `lang` subcommands (14) |
| `lc_*` | CLI: language-pack download/verification (10) |
| `lsp_*` | Language server (LSP) messages (98) |
| `diag_*` | CLI/engine: diagnostic rendering (15) |
| `unicode_name_*` | Engine: Unicode character names (dynamic, by codepoint) (60) |
| `guide_*` | CLI: guided first-run tutorial (14) |
| `crate_*` | CLI: crate registry (`rzc crate`) (20) |
| `load_*` | Engine: language-pack loading (19) |
| `err_*` | Engine: lexer/loader errors (11) |
| `run_*` | CLI: `rzc run` output (2) |
| `transpile_*` | Engine/CLI: transpile messages (2) |
| _其余_ | Other UI messages (122) |

Placeholder arguments are shown as they appear in source, e.g. `&[&created.to_string(),
&path.display().to_string()]` → `count / path-ish expression`. A key with no `{}` needs
no substitution at all.

## VS Code extension (contributes labels, commands, settings)

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `ext_add_dep_no_workspace` | 0 | `tools/vscode-extension/src/extension.ts:1388` | — | 未打开工作区，无法添加依赖 |
| `ext_cfg_ai_api_key` | 0 | `tools/vscode-extension/package.json` | — | AI API 密钥（Ollama 本地服务可留空）。已弃用：扩展会自动将此值迁移到加密的 SecretStorage 并清空该设置。仅用户级配置，工作区不可覆盖。 |
| `ext_cfg_ai_api_key_deprecation` | 0 | `tools/vscode-extension/package.json` | — | 密钥现由扩展安全存储（SecretStorage）管理；此处填写的值会在下次使用时自动迁移并清空。 |
| `ext_cfg_ai_base_url` | 0 | `tools/vscode-extension/package.json` | — | AI API 基础地址（留空时使用所选服务商的默认地址，如 DeepSeek: https://api.deepseek.com/v1；仅用户级配置，工作区不可覆盖） |
| `ext_cfg_ai_max_tokens` | 0 | `tools/vscode-extension/package.json` | — | AI 最大生成 token 数（默认 2048） |
| `ext_cfg_ai_model` | 0 | `tools/vscode-extension/package.json` | — | AI 模型名称（留空时使用所选服务商的默认模型） |
| `ext_cfg_ai_provider` | 0 | `tools/vscode-extension/package.json` | — | AI 提供商标识：openai / deepseek / qwen / glm / ollama / anthropic / gemini / custom（仅用户级配置，工作区不可覆盖） |
| `ext_cfg_ai_system_prompt` | 0 | `tools/vscode-extension/package.json` | — | 自定义系统提示词。留空时按当前语言包自动生成。若要在自定义的同时随语言包切换自动注入方言词表，请在文本中放置占位符 `{{dialect_tables}}`（运行时会被替换为当前语言包的方言词表提示）；若不含该占位符，则完全覆盖内置提示词… |
| `ext_cfg_ai_temperature` | 0 | `tools/vscode-extension/package.json` | — | AI 采样温度（0~2，默认 0.1，教学场景偏好稳定输出） |
| `ext_cfg_ai_timeout` | 0 | `tools/vscode-extension/package.json` | — | AI 请求超时时间（秒，默认 60） |
| `ext_cfg_auto_convert_fullwidth` | 0 | `tools/vscode-extension/package.json` | — | 在代码区（非字符串、非注释）输入全角符号时，自动转换为半角符号 |
| `ext_cfg_check_updates` | 0 | `tools/vscode-extension/package.json` | — | 启动后检查 GitHub Releases 上的新版本（每天最多一次），发现新版本时弹出提示 |
| `ext_cfg_hide_generated` | 0 | `tools/vscode-extension/package.json` | — | 隐藏转译产物：将 `**/*.rs` 与 `**/*.rs.bak` 写入工作区 `files.exclude`（资源管理器不再显示，关闭时移除条目恢复显示）。适用于以 `.zh` 等方言文件为源码、`.rs` 为转译产物的项目；工作区本… |
| `ext_cfg_language_pack` | 0 | `tools/vscode-extension/package.json` | — | 当前使用的语言包 |
| `ext_cfg_language_pack_path` | 0 | `tools/vscode-extension/package.json` | — | 语言包目录路径（留空则使用默认） |
| `ext_cfg_rzc_path` | 0 | `tools/vscode-extension/package.json` | — | rzc 命令行工具路径（可为绝对路径、工作区相对路径或 PATH 中的命令名） |
| `ext_cfg_server_path` | 0 | `tools/vscode-extension/package.json` | — | i18n-rust-lsp 可执行文件路径（可为绝对路径、工作区相对路径或 PATH 中的命令名） |
| `ext_cfg_trace_server` | 0 | `tools/vscode-extension/package.json` | — | 跟踪 LSP 服务器通信 |
| `ext_cmd_ai_chat` | 0 | `tools/vscode-extension/package.json` | — | AI 对话 |
| `ext_cmd_ai_generate` | 0 | `tools/vscode-extension/package.json` | — | AI 生成方言代码（自动 rzc check 校验并修正） |
| `ext_cmd_ai_list_models` | 0 | `tools/vscode-extension/package.json` | — | 获取 AI 模型列表 |
| `ext_cmd_ai_select_provider` | 0 | `tools/vscode-extension/package.json` | — | 选择 AI 提供商 |
| `ext_cmd_cargo_add` | 0 | `tools/vscode-extension/package.json` | — | 添加依赖 (cargo add) |
| `ext_cmd_check` | 0 | `tools/vscode-extension/package.json` | — | 类型检查 |
| `ext_cmd_eject` | 0 | `tools/vscode-extension/package.json` | — | 导出为 Rust 源码 |
| `ext_cmd_explain_diagnostic` | 0 | `tools/vscode-extension/package.json` | — | AI 讲解当前诊断 |
| `ext_cmd_lang_install` | 0 | `tools/vscode-extension/package.json` | — | 安装语言包 (lang install) |
| `ext_cmd_mapping_check` | 0 | `tools/vscode-extension/package.json` | — | 校验第三方库映射 (mapping check) |
| `ext_cmd_mapping_scaffold` | 0 | `tools/vscode-extension/package.json` | — | 生成新语言翻译脚手架 (mapping scaffold) |
| `ext_cmd_restart_server` | 0 | `tools/vscode-extension/package.json` | — | 重启语言服务器 |
| `ext_cmd_run` | 0 | `tools/vscode-extension/package.json` | — | 运行 |
| `ext_cmd_select_lang_pack` | 0 | `tools/vscode-extension/package.json` | — | 选择语言包 |
| `ext_cmd_toggle_generated_files` | 0 | `tools/vscode-extension/package.json` | — | 显示/隐藏转译产物 (.rs / .rs.bak) |
| `ext_cmd_transpile_preview` | 0 | `tools/vscode-extension/package.json` | — | 转译预览（并排对比） |
| `ext_explain_need_file` | 0 | `tools/vscode-extension/src/extension.ts:983` | — | 请先打开一个文件，再使用诊断讲解 |
| `ext_explain_no_diag` | 0 | `tools/vscode-extension/src/extension.ts:988` | — | 当前文件没有错误或警告，无需讲解 |
| `ext_explain_no_diag_nearby` | 0 | `tools/vscode-extension/src/extension.ts:994` | — | 未找到光标附近的诊断 |
| `ext_lang_alias_ja_1` | 0 | `tools/vscode-extension/package.json` | — | Rust (日语) |
| `ext_lang_alias_zh_1` | 0 | `tools/vscode-extension/package.json` | — | Rust (中文) |
| `ext_lang_alias_zh_2` | 0 | `tools/vscode-extension/package.json` | — | 中文 Rust |
| `ext_lsp_binary_missing` | 0 | `tools/vscode-extension/src/extension.ts:1130` | — | 未找到 i18n-rust-lsp 可执行文件。请运行 `rzc install lsp` 安装语言服务器（环境自查请运行 `rzc doctor`），或在设置 i18n-rust.serverPath 中指定二进制路径（高亮/补全/诊断… |
| `ext_lsp_restarted` | 0 | `tools/vscode-extension/src/extension.ts:659` | — | 语言服务器已重启 |
| `ext_lsp_start_failed` | 1 | `tools/vscode-extension/src/extension.ts:1231` | — | i18n-rust 语言服务器启动失败：{}（详见输出面板「i18n-rust LSP」，环境自查请运行 `rzc doctor`） |
| `ext_need_dialect_file` | 0 | `tools/vscode-extension/src/extension.ts:597`<br>`tools/vscode-extension/src/extension.ts:611`<br>`tools/vscode-extension/src/extension.ts:625` | — | 请打开一个方言源码文件（.zh/.ja 等） |

## CLI: subcommand descriptions (clap help)

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `cmd_add_about` | 0 | `crates/cli/src/main.rs:2960` | — | 添加第三方库依赖（封装 cargo add） |
| `cmd_cheat_about` | 0 | `crates/cli/src/main.rs:3010` | — | 母语 ↔ Rust 映射速查表（关键字/模块路径/别名/派生特征） |
| `cmd_check_about` | 0 | `crates/cli/src/main.rs:2943` | — | 检查方言源文件（编译并翻译错误消息） |
| `cmd_crate_about` | 0 | `crates/cli/src/main.rs:3130` | — | 第三方库共享注册中心（search / install / list / remove / update / publish） |
| `cmd_crate_install_about` | 0 | `crates/cli/src/main.rs:3140` | — | 安装单个第三方库映射：从注册中心复制到全局语言包 |
| `cmd_crate_list_about` | 0 | `crates/cli/src/main.rs:3152` | — | 列出已安装的社区映射 |
| `cmd_crate_publish_about` | 0 | `crates/cli/src/main.rs:3169` | — | 发布本地映射到注册中心（先经质量门禁） |
| `cmd_crate_remove_about` | 0 | `crates/cli/src/main.rs:3156` | — | 删除已安装的社区映射（从全局语言包与清单移除） |
| `cmd_crate_search_about` | 0 | `crates/cli/src/main.rs:3133` | — | 检索注册中心已发布的第三方库映射（可选关键词过滤） |
| `cmd_crate_update_about` | 0 | `crates/cli/src/main.rs:3165` | — | 按已安装清单重新拉取所有映射（获取他人更新） |
| `cmd_eject_about` | 0 | `crates/cli/src/main.rs:2950` | — | 把方言源文件或目录导出为标准 Rust 代码 |
| `cmd_gaps_about` | 0 | `crates/cli/src/main.rs:2997` | — | 查看/导出未翻译消息清单（英文残留），补译后可清零 |
| `cmd_guide_about` | 0 | `crates/cli/src/main.rs:3020` | — | 母语编程指南（全量词条表 + 书写规则，供学习者与 AI 助手使用） |
| `cmd_init_about` | 0 | `crates/cli/src/main.rs:2932` | — | 创建新项目（生成对应语言的主文件模板） |
| `cmd_init_ai_help` | 0 | `crates/cli/src/main.rs:2937` | — | 同时在项目内生成 AGENTS.md（AI 编程指南，由当前语言包渲染） |
| `cmd_install_about` | 0 | `crates/cli/src/main.rs:2967` | — | 安装配套组件（语言服务器 i18n-rust-lsp） |
| `cmd_install_lsp_about` | 0 | `crates/cli/src/main.rs:2970` | — | 安装语言服务器 i18n-rust-lsp（VS Code 扩展的补全/诊断后端） |
| `cmd_lang_about` | 0 | `crates/cli/src/main.rs:3043` | — | 语言包管理（list / install / remove） |
| `cmd_lang_install_about` | 0 | `crates/cli/src/main.rs:3049` | — | 安装语言包：本地目录路径直接复制；语言代码从远程仓库下载 |
| `cmd_lang_list_about` | 0 | `crates/cli/src/main.rs:3045` | — | 列出所有已安装的语言包（内置 + 用户安装） |
| `cmd_lang_remove_about` | 0 | `crates/cli/src/main.rs:3066` | — | 删除用户安装的语言包（内置语言包不可删除） |
| `cmd_lang_search_about` | 0 | `crates/cli/src/main.rs:3059` | — | 浏览远程语言包市场（可选关键词过滤） |
| `cmd_mapping_about` | 0 | `crates/cli/src/main.rs:3074` | — | 自动生成第三方库映射 |
| `cmd_mapping_auto_about` | 0 | `crates/cli/src/main.rs:3077` | — | 自动生成第三方库映射文件：提取 crate 公开 API，AI 或规则生成母语名与解释 |
| `cmd_mapping_check_about` | 0 | `crates/cli/src/main.rs:3099` | — | 校验第三方库映射质量：重复键/关键字避让/跨文件冲突/条目数一致性 |
| `cmd_mapping_check_target_help` | 0 | `crates/cli/src/main.rs:3101` | — | 内置语言代码（如 zh）或语言包目录路径；省略时校验全部内置语言 |
| `cmd_mapping_coverage_about` | 0 | `crates/cli/src/main.rs:3122` | — | 语料覆盖矩阵：用后端真实源码检验语言包关键字/API 覆盖度，自动列出缺失的母语映射 |
| `cmd_mapping_scaffold_about` | 0 | `crates/cli/src/main.rs:3106` | — | 从源语言 crates 映射生成目标语言的翻译骨架 |
| `cmd_mapping_scaffold_provider_help` | 0 | `crates/cli/src/main.rs:3117` | — | 翻译方式：rule（默认，生成 TODO 骨架待人工翻译）或 deepseek / openai / compatible（AI 翻译键名，端点、密钥、模型由 RZ_AI_* 环境变量指定） |
| `cmd_mapping_scaffold_source_help` | 0 | `crates/cli/src/main.rs:3108` | — | 源语言代码（内置语言，如 zh） |
| `cmd_mapping_scaffold_target_help` | 0 | `crates/cli/src/main.rs:3111` | — | 目标语言代码（新语言包目录名，如 vi） |
| `cmd_relocalize_about` | 0 | `crates/cli/src/main.rs:3030` | — | 跨母语阅读：把方言 / 标准 Rust 工程反译重生为目标方言（逐文件正向转译到标准 Rust 再反译，产物结构忠实、可再次 rzc run / rzc check） |
| `cmd_run_about` | 0 | `crates/cli/src/main.rs:2940` | — | 运行方言源文件 |
| `cmd_transpile_about` | 0 | `crates/cli/src/main.rs:2956` | — | 把方言源文件转译为标准 Rust 代码并输出到标准输出（不写文件） |

## CLI: argument descriptions (clap help)

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `arg_add_crates_help` | 0 | `crates/cli/src/main.rs:2962` | — | 要添加的 crate 名，可用 @version 指定版本（如 serde_json、tokio@1） |
| `arg_cheat_lang_help` | 0 | `crates/cli/src/main.rs:3012` | — | 内置语言代码（如 zh）或语言包目录路径；省略时按系统语言检测 |
| `arg_check_fix_help` | 0 | `crates/cli/src/main.rs:2945` | — | 自动修复全角标点（写入源文件，仅转换有半角对应的字符） |
| `arg_coverage_lang_help` | 0 | `crates/cli/src/main.rs:3124` | — | 内置语言代码（如 zh）；省略时检测全部内置语言 |
| `arg_crate_author_help` | 0 | `crates/cli/src/main.rs:3180` | — | 译者署名；缺省取 git user.name |
| `arg_crate_file_help` | 0 | `crates/cli/src/main.rs:3177` | — | 映射文件路径；缺省按全局/项目语言包或当前目录查找 |
| `arg_crate_keyword_help` | 0 | `crates/cli/src/main.rs:3135` | — | 关键词（匹配 crate 名 / 语言 / 作者）；省略时列出全部 |
| `arg_crate_name_help` | 0 | `crates/cli/src/main.rs:3158`<br>`crates/cli/src/main.rs:3171` | — | crate 名 |
| `arg_crate_name_normalize_help` | 0 | `crates/cli/src/main.rs:3142` | — | crate 名（连字符归一为下划线） |
| `arg_eject_files_help` | 0 | `crates/cli/src/main.rs:2952` | — | 要导出的方言源文件或目录；可一次传多个（批量导出，失败项单独汇总） |
| `arg_force_help` | 0 | `crates/cli/src/main.rs:2972`<br>`crates/cli/src/main.rs:2988`<br>`crates/cli/src/main.rs:3054` | — | 已存在时强制覆盖 |
| `arg_gaps_clear_help` | 0 | `crates/cli/src/main.rs:3005` | — | 清空清单（补完消息表后使用） |
| `arg_gaps_export_help` | 0 | `crates/cli/src/main.rs:3002` | — | 把清单导出为 TSV（列：LEVEL/CODE/消息，制表符分隔）到指定路径 |
| `arg_gaps_lang_help` | 0 | `crates/cli/src/main.rs:2999` | — | 语言代码（缺省时按当前界面语言） |
| `arg_guide_lang_help` | 0 | `crates/cli/src/main.rs:3022` | — | 内置语言代码（如 zh）；省略时按系统语言自动检测 |
| `arg_guide_out_help` | 0 | `crates/cli/src/main.rs:3025` | — | 把指南写入指定文件（如 AGENTS.md） |
| `arg_install_help` | 0 | `crates/cli/src/main.rs:3094` | — | 生成映射后自动把对应 crate 加入当前项目依赖 |
| `arg_lang_code_help` | 0 | `crates/cli/src/main.rs:3068` | — | 语言代码（语言包目录名） |
| `arg_lang_help` | 0 | `crates/cli/src/main.rs:2934`<br>`crates/cli/src/main.rs:3082`<br>`crates/cli/src/main.rs:3145` | — | 语言代码（语言包目录名，如 zh、ru；默认按系统语言检测） |
| `arg_lang_search_keyword_help` | 0 | `crates/cli/src/main.rs:3061` | — | 关键词（匹配语言代码或显示名称）；省略时列出全部远程语言包 |
| `arg_lang_source_help` | 0 | `crates/cli/src/main.rs:3051` | — | 本地语言包目录路径，或远程语言代码 |
| `arg_mapping_auto_crate_help` | 0 | `crates/cli/src/main.rs:3079` | — | 目标 crate 名（需已安装或可从 crates.io 获取） |
| `arg_markdown_help` | 0 | `crates/cli/src/main.rs:3015` | — | 以 Markdown 表格输出，便于嵌入文档 |
| `arg_no_lint_help` | 0 | `crates/cli/src/main.rs:2928` | — | 不输出教学 lint 提示（初学者代码风格警告；Unicode 混淆/全角标点告警不受影响） |
| `arg_output_help` | 0 | `crates/cli/src/main.rs:3091`<br>`crates/cli/src/main.rs:3114` | — | 输出文件路径（默认 lang-packs/<语言>/crates/<crate名>.toml） |
| `arg_provider_help` | 0 | `crates/cli/src/main.rs:3085` | — | AI 服务商：deepseek / openai / compatible（默认 deepseek；端点、密钥、模型由 RZ_AI_BASE_URL / RZ_AI_API_KEY / RZ_AI_MODEL 指定）或 rule（离线规则… |
| `arg_relocalize_from_help` | 0 | `crates/cli/src/main.rs:3035` | — | 源方言语言代码；省略时按各文件扩展名自动检测 |
| `arg_relocalize_out_dir_help` | 0 | `crates/cli/src/main.rs:3038` | — | 输出目录（镜像源结构、扩展名换为目标语言）；默认为源工程根旁的 relocalized.<to> |
| `arg_relocalize_to_help` | 0 | `crates/cli/src/main.rs:3032` | — | 目标方言语言代码（必填，如 ja）；决定输出工程所用语言 |
| `arg_target_version_help` | 0 | `crates/cli/src/main.rs:3088` | — | 锁定目标 crate 版本（如 2.11.5 精确锁定；2.11 / 2 为该线最新）；省略时用最新版（不可复现） |
| `arg_tc_ra_only_help` | 0 | `crates/cli/src/main.rs:2985` | — | 仅升级 rust-analyzer（跳过 rustc/cargo 的 300MB 重下） |
| `arg_tc_ra_tag_help` | 0 | `crates/cli/src/main.rs:2982` | — | rust-analyzer 官方 Release tag（默认锁定版本） |
| `arg_tc_version_help` | 0 | `crates/cli/src/main.rs:2979` | — | 工具链版本（默认与 rzc 锁定版本一致，如 1.98.0） |

## CLI: general output

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `cli_about` | 0 | `crates/cli/src/main.rs:2926`<br>`crates/cli/src/界面.rs:249`<br>`crates/cli/src/界面.rs:250` | — | 多语言 Rust 教学方言编译器 |
| `cli_error_prefix` | 1 | `crates/cli/src/main.rs:408` | &format!("{错误值:#}") | 错误：{} |

## CLI: `mapping` generation internals

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `mg_ai_batch_progress` | 2 | `crates/cli/src/映射生成/智能.rs:278` | &(批序号 + 1).to_string() / &批们.len().to_string() | AI 生成映射中...（第 {}/{} 批） |
| `mg_ai_endpoint` | 3 | `crates/cli/src/映射生成/智能.rs:167` | &地址 / &端点.模型 / 端点.密钥变量名 | AI 端点：{}（模型 {}，密钥取自 {}） |
| `mg_cmd_build_deps` | 0 | `crates/cli/src/映射生成/文档结构.rs:209` | — | 编译依赖 |
| `mg_cmd_gen_doc` | 0 | `crates/cli/src/映射生成/文档结构.rs:582` | — | 生成文档 JSON |
| `mg_cmd_parse_deps` | 0 | `crates/cli/src/映射生成/文档结构.rs:110` | — | 解析依赖 |
| `mg_err_ai_no_content` | 0 | `crates/cli/src/映射生成/智能.rs:233` | — | AI 响应缺少 choices[0].message.content |
| `mg_err_ai_no_toml` | 0 | `crates/cli/src/映射生成/智能.rs:306` | — | AI 返回内容中没有 TOML 节 |
| `mg_err_ai_parse` | 1 | `crates/cli/src/映射生成/智能.rs:223` | &错误值.to_string() | 解析 AI 响应失败: {} |
| `mg_err_ai_read` | 1 | `crates/cli/src/映射生成/智能.rs:217` | &错误值.to_string() | 读取 AI 响应失败: {} |
| `mg_err_ai_request` | 1 | `crates/cli/src/映射生成/智能.rs:196` | &错误值.to_string() | AI 请求失败: {} |
| `mg_err_ai_status` | 1 | `crates/cli/src/映射生成/智能.rs:202` | &响应.status().to_string() | AI 服务返回状态码 {} |
| `mg_err_ai_toml_parse` | 1 | `crates/cli/src/映射生成/智能.rs:322` | &严格错误.to_string() | AI 返回的 TOML 无法解析: {} |
| `mg_err_api_key_empty` | 1 | `crates/cli/src/映射生成/智能.rs:103` | 变量名 | 环境变量 {} 为空 |
| `mg_err_build_failed` | 1 | `crates/cli/src/映射生成/文档结构.rs:306` | 包名 | crate '{}' 编译失败或没有产生任何库产物 |
| `mg_err_crate_not_found` | 1 | `crates/cli/src/映射生成/文档结构.rs:147` | 包名 | 未找到 crate '{}'（检查名称拼写，或确认该 crate 可在 crates.io 获取） |
| `mg_err_failed` | 2 | `crates/cli/src/映射生成/文档结构.rs:605` | 说明 / &摘要 | {}失败： {} |
| `mg_err_mkdir` | 1 | `crates/cli/src/映射生成/mod.rs:388` | &父路径.display().to_string() | 创建输出目录失败: {} |
| `mg_err_no_api_key` | 1 | `crates/cli/src/映射生成/智能.rs:105` | 链[0] | 未设置环境变量 {}（DeepSeek 端点也可用 DEEPSEEK_API_KEY，自定义端点也可用 OPENAI_API_KEY） |
| `mg_err_no_doc_json` | 1 | `crates/cli/src/映射生成/文档结构.rs:586` | &文档路径.display().to_string() | rustdoc 未生成 JSON 文档: {} |
| `mg_err_no_index` | 0 | `crates/cli/src/映射生成/文档抽取.rs:32` | — | rustdoc JSON 缺少 index 字段 |
| `mg_err_no_lib` | 1 | `crates/cli/src/映射生成/文档结构.rs:480` | 包名 | crate '{}' 没有 src/lib.rs（无库目标，无法提取 API） |
| `mg_err_no_manifest` | 1 | `crates/cli/src/映射生成/文档结构.rs:473` | 包名 | crate '{}' 缺少 manifest 路径 |
| `mg_err_no_src_dir` | 1 | `crates/cli/src/映射生成/文档结构.rs:477` | 包名 | 无法确定 crate '{}' 的源码目录 |
| `mg_err_parse_manifest` | 2 | `crates/cli/src/映射生成/文档结构.rs:504` | &清单路径.display().to_string() / &错误值.to_string() | 解析 {} 失败: {} |
| `mg_err_parse_meta` | 1 | `crates/cli/src/映射生成/文档结构.rs:115` | &错误值.to_string() | 解析 cargo metadata 输出失败: {} |
| `mg_err_parse_rustdoc` | 1 | `crates/cli/src/映射生成/文档抽取.rs:26` | &错误值.to_string() | 解析 rustdoc JSON 失败: {} |
| `mg_err_read_manifest` | 1 | `crates/cli/src/映射生成/文档结构.rs:498` | &清单路径.display().to_string() | 读取 {} 失败 |
| `mg_err_run_failed` | 1 | `crates/cli/src/映射生成/文档结构.rs:594` | 说明 | 执行 {} 失败（请确认 cargo/rustdoc 已安装） |
| `mg_err_tempdir` | 1 | `crates/cli/src/映射生成/文档结构.rs:40` | &错误值.to_string() | 创建临时目录失败: {} |
| `mg_err_write` | 1 | `crates/cli/src/映射生成/mod.rs:392` | &输出路径.display().to_string() | 写入映射文件失败: {} |
| `mg_header_crate` | 1 | `crates/cli/src/映射生成/映射输出.rs:82` | 包名 | # crate: {} |
| `mg_header_generated_at` | 1 | `crates/cli/src/映射生成/映射输出.rs:93` | &当前时间戳() | # 生成时间: {} |
| `mg_header_target_version` | 1 | `crates/cli/src/映射生成/映射输出.rs:88` | 版本文 | # 基准版本: {} |
| `mg_header_title` | 0 | `crates/cli/src/映射生成/映射输出.rs:79` | — | # 自动生成的第三方库映射 |
| `mg_warn_ai_explain_long` | 2 | `crates/cli/src/映射生成/智能.rs:358` | &解释.chars().count().to_string() / 中文名 | 警告: AI 解释超过 40 字（{} 字）：{} |

## CLI: `mapping check` findings

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `mc_cov_all_ok` | 0 | `crates/cli/src/映射覆盖.rs:547` | — | 🏁 全部语言关键字覆盖通过 |
| `mc_cov_corpus_intro` | 0 | `crates/cli/src/映射覆盖.rs:525` | — | 📖 语料：后端真实源码（crates/engine + cli + lsp 的 src/*.rs） |
| `mc_cov_crate_missing` | 3 | `crates/cli/src/映射覆盖.rs:469` | 条目名 / &次数.to_string() / 示例路径 | ⚠️ 第三方 crate 缺母语映射: {}（{} 次，例: {}） |
| `mc_cov_header` | 1 | `crates/cli/src/映射覆盖.rs:416` | &报告值.语言代码 | 🔬 语料覆盖矩阵: {} |
| `mc_cov_kw_missing` | 3 | `crates/cli/src/映射覆盖.rs:440` | 条目名 / &次数.to_string() / 示例路径 | ❌ 关键字缺母语映射: {}（{} 次，例: {}） |
| `mc_cov_kw_ok` | 0 | `crates/cli/src/映射覆盖.rs:498` | — | ✅ 关键字全覆盖 |
| `mc_cov_more` | 1 | `crates/cli/src/映射覆盖.rs:446`<br>`crates/cli/src/映射覆盖.rs:460`<br>`crates/cli/src/映射覆盖.rs:477` | &(关键字总数 - 30).to_string() / &(标准库总数 - 25).to_string() / &(第三方总数 - 15).to_string() / &(类型总… | … 其余 {} 条省略（用 rzc mapping coverage --lang <代码> 查看全部） |
| `mc_cov_no_repo` | 0 | `crates/cli/src/映射覆盖.rs:515`<br>`crates/cli/src/映射覆盖.rs:518` | — | ❌ 语料覆盖检测需要源码仓库：请在 zrRust 仓库根目录运行此命令 |
| `mc_cov_stats` | 4 | `crates/cli/src/映射覆盖.rs:426` | &报告值.已覆盖数.to_string() / &报告值.总名称数.to_string() / &百分比.to_string() / &源码数.to_string() | 覆盖: {} / {}（{}%）· 语料 {} 个源文件 |
| `mc_cov_std_missing` | 3 | `crates/cli/src/映射覆盖.rs:454` | 条目名 / &次数.to_string() / 示例路径 | ⚠️ std 模块/类型缺母语映射: {}（{} 次，例: {}） |
| `mc_cov_type_missing` | 3 | `crates/cli/src/映射覆盖.rs:486` | 条目名 / &次数.to_string() / 示例路径 | ⚠️ 类型名缺母语映射（多为编译器内部实现，可按需忽略）: {}（{} 次，例: {}） |
| `mc_cross_conflict` | 5 | `crates/cli/src/映射检查.rs:364`<br>`crates/cli/src/映射检查.rs:1092`<br>`crates/cli/src/映射检查.rs:1525` | — | ❌ 键 "{}" 跨文件冲突: {} = "{}" 与 {} = "{}" |
| `mc_cross_conflict_mp` | 5 | `crates/cli/src/映射检查.rs:362`<br>`crates/cli/src/映射检查.rs:1097`<br>`crates/cli/src/映射检查.rs:1641` | — | ❌ 模块路径键 "{}" 跨文件冲突: {} = "{}" 与 {} = "{}" |
| `mc_cross_derive_missing` | 3 | `crates/cli/src/映射检查.rs:1877`<br>`crates/cli/src/映射检查.rs:671` | — | ❌ {} 缺少 {} 个派生特征（派生属性参数不可用）: {} |
| `mc_cross_err_missing` | 3 | `crates/cli/src/映射检查.rs:1928`<br>`crates/cli/src/映射检查.rs:686` | — | ❌ {} 缺少 {} 个错误码节（无母语教学提示）: {} |
| `mc_cross_integrity_header` | 0 | `crates/cli/src/映射检查.rs:721` | — | 🔬 跨语言完整性检查（基准：zh）: |
| `mc_cross_integrity_none` | 0 | `crates/cli/src/映射检查.rs:757` | — | （无可比较的语言包） |
| `mc_cross_integrity_ok` | 0 | `crates/cli/src/映射检查.rs:744` | — | ✅ 关键字/派生特征/错误码全覆盖 |
| `mc_cross_integrity_warn` | 1 | `crates/cli/src/映射检查.rs:750` | &报告值.警告列表.len().to_string() | ✅ 覆盖完整（{} 条消息翻译缺失警告） |
| `mc_cross_kw_missing` | 3 | `crates/cli/src/映射检查.rs:1907`<br>`crates/cli/src/映射检查.rs:656` | — | ❌ {} 缺少 {} 个关键字值（方言词不可用）: {} |
| `mc_cross_lang_consistent` | 0 | `crates/cli/src/映射检查.rs:791` | — | ✅ 各语言条目数一致 |
| `mc_cross_lang_header` | 0 | `crates/cli/src/映射检查.rs:776` | — | 📊 跨语言标识符条目数: |
| `mc_cross_lang_inconsistent` | 0 | `crates/cli/src/映射检查.rs:789` | — | ⚠️ 各语言条目数不一致，请人工同步（可用 rzc mapping scaffold 生成骨架） |
| `mc_cross_msg_missing` | 3 | `crates/cli/src/映射检查.rs:1948`<br>`crates/cli/src/映射检查.rs:701` | — | ⚠️ {} 缺少 {} 个消息翻译键（回退英文原文）: {} 等 |
| `mc_danger_invalid` | 3 | dynamic | crates/cli/src/映射检查.rs render_issue() | ❌ {} 危险等级 "{}" 的值 "{}" 无法识别（只允许 panic / rewrite / safe，多风险用 + 连接） |
| `mc_danger_orphan` | 2 | dynamic | crates/cli/src/映射检查.rs render_issue() | ❌ {} 危险元数据 "{}" 没有对应词条（可能已改名，元数据已失效） |
| `mc_danger_understated` | 3 | dynamic | crates/cli/src/映射检查.rs render_issue() | ❌ {} 词条 "{}" 映射到会 panic 的 "{}"，却未标注 panic（不得隐式 panic） |
| `mc_danger_understated_rewrite` | 2 | dynamic | crates/cli/src/映射检查.rs render_issue() | ❌ {} 词条 "{}" 是关键字/宏（任意位置替换），却未标注 rewrite（不得误标为纯安全） |
| `mc_failed` | 2 | `crates/cli/src/映射检查.rs:501` | &报告值.错误列表.len().to_string() / &报告值.警告列表.len().to_string() | ❌ 校验未通过: {} 个错误 / {} 条警告 |
| `mc_header` | 1 | `crates/cli/src/映射检查.rs:470` | 语言代码 | 🔍 映射校验: {} |
| `mc_keyword_collision` | 5 | `crates/cli/src/映射检查.rs:1089`<br>`crates/cli/src/映射检查.rs:1508`<br>`crates/cli/src/映射检查.rs:374` | — | ❌ {}[{}] 键 "{}" 与关键字冲突（crates 值 "{}" 永不生效，关键字值为 "{}"） |
| `mc_missing_section` | 2 | `crates/cli/src/映射检查.rs:1455`<br>`crates/cli/src/映射检查.rs:1468`<br>`crates/cli/src/映射检查.rs:278` | — | ❌ {} keywords.toml 缺少必需节 ["{}"]（整类词条静默失效，转译仍报成功） |
| `mc_no_crates` | 1 | `crates/cli/src/映射检查.rs:830` | 源语言 | 源语言 {} 没有 crates 映射，无法生成脚手架 |
| `mc_ok` | 0 | `crates/cli/src/映射检查.rs:492`<br>`crates/cli/src/映射检查.rs:946`<br>`crates/cli/src/映射检查.rs:974` | — | ✅ 校验通过 |
| `mc_ok_with_warnings` | 1 | `crates/cli/src/映射检查.rs:494`<br>`crates/cli/src/映射检查.rs:941`<br>`crates/cli/src/映射检查.rs:2029` | &报告值.警告列表.len().to_string() | ✅ 校验通过（{} 条警告） |
| `mc_parse_failed` | 2 | `crates/cli/src/映射检查.rs:1577`<br>`crates/cli/src/映射检查.rs:284`<br>`crates/cli/src/映射检查.rs:342` | — | ❌ {} TOML 解析失败: {} |
| `mc_scaffold_ai_batch` | 2 | `crates/cli/src/映射检查.rs:906` | &(批序 + 1).to_string() / &总批数.to_string() | 🤖 AI 正在翻译键（第 {}/{} 批）... |
| `mc_scaffold_ai_done` | 2 | `crates/cli/src/映射检查.rs:918` | &替换数.to_string() / &项列表.len().to_string() | ✅ AI 翻译完成：已替换 {}/{} 个键 |
| `mc_scaffold_ai_partial` | 1 | `crates/cli/src/映射检查.rs:926` | &(项列表.len() - 替换数).to_string() | ⚠️ {} 个键未被 AI 翻译，保留 TODO 标记，需人工补齐 |
| `mc_scaffold_ai_retry` | 3 | `crates/cli/src/映射检查.rs:953` | &冲突项.len().to_string() / &轮次.to_string() / &智能最大重试轮数.to_string() | 🔄 检出 {} 个冲突键，AI 改名中（第 {}/{} 轮）... |
| `mc_scaffold_ai_retry_left` | 1 | `crates/cli/src/映射检查.rs:971` | 目标语言 | ⚠️ 改名后仍有冲突残留，请运行 rzc mapping check {} 人工处理 |
| `mc_scaffold_generated` | 2 | `crates/cli/src/映射检查.rs:847` | &已创建数.to_string() / &输出目录.display().to_string() | ✅ 已生成 {} 个脚手架文件 → {} |
| `mc_scaffold_hint` | 1 | `crates/cli/src/映射检查.rs:861` | 目标语言 | 下一步: 翻译标记 TODO({}) 的键，英文值保持不变 |
| `mc_scaffold_skip_existing` | 1 | `crates/cli/src/映射检查.rs:855` | &已跳过数.to_string() | ⚠️ 已存在 {} 个脚手架文件，跳过生成（保留既有翻译） |
| `mc_stats` | 3 | `crates/cli/src/映射检查.rs:474` | &报告值.统计.包文件数.to_string() / &报告值.统计.模块路径条目数.to_string() / &报告值.统计.标识符条目数.to_string() | 统计: {} 个文件 / 模块路径 {} 条 / 标识符 {} 条 |
| `mc_stdlib_parse_failed` | 1 | `crates/cli/src/映射检查.rs:1599`<br>`crates/cli/src/映射检查.rs:1623`<br>`crates/cli/src/映射检查.rs:294` | — | ❌ stdlib.toml 解析失败: {}（标准库映射整体失效） |
| `mc_stdlib_shadow` | 4 | `crates/cli/src/映射检查.rs:1560`<br>`crates/cli/src/映射检查.rs:396`<br>`crates/cli/src/映射检查.rs:2011` | — | ⚠️ {} 键 "{}"（值 "{}"）被 stdlib 覆盖（stdlib 值 "{}"），crates 条目失效 |
| `mc_unknown_source` | 1 | `crates/cli/src/映射检查.rs:810` | 源语言 | 未知源语言: {}（不是内置语言） |
| `mc_unknown_target` | 1 | `crates/cli/src/映射检查.rs:554`<br>`crates/cli/src/映射覆盖.rs:533` | 目标值 / 语言代码 | 未知校验目标: {}（不是内置语言代码，也不是已存在的目录） |

## CLI: `mapping` user-facing flow

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `mapping_ai_coverage` | 2 | `crates/cli/src/映射生成/mod.rs:321` | &覆盖数.to_string() / &总数.to_string() | 解释覆盖：{}/{} 条 |
| `mapping_ai_coverage_missing` | 3 | `crates/cli/src/映射生成/mod.rs:309` | &覆盖数.to_string() / &总数.to_string() / &(总数 - 覆盖数).to_string() | ⚠️ 解释覆盖：{}/{} 条（{} 条缺失，可重跑生成或手动补充） |
| `mapping_ai_fallback` | 2 | `crates/cli/src/映射生成/mod.rs:331` | "RZ_AI_API_KEY / DEEPSEEK_API_KEY" / &错误值.to_string() | AI 调用失败（请检查环境变量 {} 与网络）：{}；已回退规则模式。 |
| `mapping_ai_success` | 1 | `crates/cli/src/映射生成/mod.rs:302` | 服务商 | ✅ AI（{}）生成母语名与解释成功。 |
| `mapping_auto_install_failed` | 2 | `crates/cli/src/main.rs:1329`<br>`crates/cli/src/main.rs:1337` | 包名 / &进程状态值.to_string() / &错误值.to_string() | 自动添加依赖 {} 失败：{} |
| `mapping_auto_install_no_project` | 0 | `crates/cli/src/main.rs:1309` | — | 未找到 Cargo 项目，跳过自动安装依赖 |
| `mapping_auto_installed` | 1 | `crates/cli/src/main.rs:1324` | 包名 | ✅ 依赖 {} 已自动加入当前项目 |
| `mapping_cat_keywords` | 0 | `crates/engine/src/映射源.rs:519`<br>`crates/engine/src/语言.rs:301` | — | 关键字 |
| `mapping_cat_stdlib` | 0 | `crates/engine/src/映射源.rs:520` | — | 标准库 |
| `mapping_cat_third_party` | 0 | `crates/engine/src/映射源.rs:521` | — | 第三方库 |
| `mapping_crate_empty` | 0 | `crates/cli/src/映射生成/mod.rs:179` | — | crate 名不能为空 |
| `mapping_crate_invalid` | 1 | `crates/cli/src/映射生成/mod.rs:185` | 包名 | crate 名只能包含字母、数字、连字符和下划线，实际输入：{} |
| `mapping_crate_name_line` | 3 | `crates/cli/src/映射生成/mod.rs:400` | &包中文名 / 包名 | crate 母语名: {}（模块路径映射 {} → {}） |
| `mapping_disclaimer` | 0 | `crates/cli/src/映射生成/mod.rs:46` | — | 本文件中的解释由 AI 根据 API 名称和签名自动生成，不是原 crate 的官方文档翻译，仅供学习参考。原 crate 的版权归其作者所有。 |
| `mapping_extracted` | 2 | `crates/cli/src/映射生成/mod.rs:255` | &条目们.len().to_string() / &统计文本 | ✅ 提取到 {} 个公开 API（{}） |
| `mapping_extracting` | 1 | `crates/cli/src/映射生成/mod.rs:210`<br>`crates/cli/src/映射生成/mod.rs:218` | 包名 / 文档包名 | 正在提取 crate '{}' 的公开 API（首次运行需编译依赖，可能耗时）… |
| `mapping_generated` | 1 | `crates/cli/src/映射生成/mod.rs:396` | &输出路径.display().to_string() | ✅ 已生成映射: {} |
| `mapping_keyword_conflict` | 4 | `crates/cli/src/映射生成/mod.rs:373` | &中文名 / &关键字英文 / &本项英文 | 警告: 母语名 '{}' 与关键字映射冲突（keywords.toml: {} → {}，本映射: → {}），\n 该条目生成后不会生效，请手动改名或补充。 |
| `mapping_kind_const` | 0 | `crates/cli/src/映射生成/mod.rs:78` | — | 常量 |
| `mapping_kind_enum` | 0 | `crates/cli/src/映射生成/mod.rs:74` | — | 枚举 |
| `mapping_kind_function` | 0 | `crates/cli/src/映射生成/mod.rs:72` | — | 函数 |
| `mapping_kind_macro` | 0 | `crates/cli/src/映射生成/mod.rs:77` | — | 宏 |
| `mapping_kind_struct` | 0 | `crates/cli/src/映射生成/mod.rs:73` | — | 结构体 |
| `mapping_kind_trait` | 0 | `crates/cli/src/映射生成/mod.rs:75` | — | 特征 |
| `mapping_kind_type_alias` | 0 | `crates/cli/src/映射生成/mod.rs:76` | — | 类型别名 |
| `mapping_name_conflict` | 2 | `crates/cli/src/映射生成/mod.rs:270` | &中文名 / &条目值.英文名 | 警告: 母语名 '{}' 与 '{}' 冲突，已跳过（可手动补充）。 |
| `mapping_no_api` | 1 | `crates/cli/src/映射生成/mod.rs:226` | 包名 | 未从 crate '{}' 提取到任何公开 API（该 crate 可能没有可文档化的库目标） |
| `mapping_no_macro` | 0 | `crates/cli/src/映射生成/mod.rs:259` | — | 提示: rustdoc JSON 格式暂不包含 macro_rules! 宏定义，宏映射需手动补充。 |
| `mapping_overwrite_warn` | 1 | `crates/cli/src/映射生成/mod.rs:383` | &输出路径.display().to_string() | ⚠️ 警告：文件已存在，将覆盖: {} |
| `mapping_rule_mode` | 0 | `crates/cli/src/映射生成/mod.rs:339` | — | 未配置 AI，无法生成 API 解释（规则模式，解释留空）。 |
| `mapping_source_builtin` | 1 | `crates/cli/src/main.rs:2878` | &语言码 | 映射数据来源: 内置语言包 {}（编译期嵌入，修改语言包后需重新编译；自定义请用 rzc lang install） |
| `mapping_source_explicit` | 1 | `crates/cli/src/main.rs:2794` | &路径值.display().to_string() | 映射数据来源: --lang-pack 显式目录 {}（优先于项目/全局/内置） |
| `mapping_source_global` | 1 | `crates/cli/src/main.rs:2858` | &全局路径.display().to_string() | 映射数据来源: 全局用户语言包 {}（覆盖内置；RZ_LANG_DIR 可改目录） |
| `mapping_source_project` | 1 | `crates/cli/src/main.rs:2842` | &本地路径.display().to_string() | 映射数据来源: 项目语言包 {}（覆盖全局与内置） |
| `mapping_unknown_provider` | 1 | `crates/cli/src/映射检查.rs:825`<br>`crates/cli/src/映射生成/mod.rs:206` | 服务商 | 未知 AI 服务商 '{}'，可选：deepseek / openai / compatible / rule |
| `mapping_usage_hint` | 1 | `crates/cli/src/映射生成/mod.rs:402` | 语言代码 | 使用方式: 在项目中创建 lang-packs/{}/ 目录后放入此文件，或用 --lang-pack 指定。 |
| `mapping_version_invalid` | 1 | `crates/cli/src/映射生成/mod.rs:193` | 版本文 | 版本号无效: {}（需为 x、x.y 或 x.y.z，可带预发布/构建后缀，如 3.0.0-alpha.0） |
| `mapping_version_unlocked` | 0 | `crates/cli/src/映射生成/mod.rs:197` | — | 提示: 未指定 --target-version，依赖将解析为当前最新版本（生成结果不可复现）；生产映射建议锁定，如 --target-version 2.11.5 |

## CLI: `lang` subcommands

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `lang_already_installed` | 3 | `crates/cli/src/语言管理.rs:911` | 语言码 / &目标目录.display().to_string() | 语言包 '{}' 已安装（{}）。如需覆盖，请使用 `rzc lang install {} --force`。 |
| `lang_builtin_not_removable` | 1 | `crates/cli/src/语言管理.rs:460` | 语言码 | 语言包 '{}' 是内置语言包，不可删除。 |
| `lang_install_hint` | 0 | `crates/cli/src/语言管理.rs:931` | — | 现在可通过 `rzc run/check 文件.<扩展名>` 直接使用。 |
| `lang_installed` | 2 | `crates/cli/src/语言管理.rs:929` | 语言码 / &目标目录.display().to_string() | ✅ 语言包 '{}' 安装成功：{} |
| `lang_list_display` | 5 | `crates/cli/src/main.rs:1096` | &标签 / &显示文本 / &扩展文本 / &版本文本 | [{}] {}（扩展名：{}，版本：{}）{} |
| `lang_not_builtin` | 2 | `crates/cli/src/main.rs:2871` | &语言码 | 语言包 '{}' 未内置，请通过 `rzc lang install {}` 下载安装。 |
| `lang_not_found` | 2 | `crates/cli/src/语言管理.rs:466` | 语言码 / &目标目录.display().to_string() | 语言包 '{}' 不存在（{}）。可用 `rzc lang list` 查看已安装的语言包。 |
| `lang_removed` | 1 | `crates/cli/src/语言管理.rs:453` | 语言码 | ✅ 语言包 '{}' 已删除。 |
| `lang_search_empty` | 1 | `crates/cli/src/main.rs:1123` | 关键词值 | 未找到匹配的语言包（关键词：{}） |
| `lang_search_empty_all` | 0 | `crates/cli/src/main.rs:1125`<br>`crates/cli/src/main.rs:1128` | — | 远程仓库中没有可用语言包 |
| `lang_search_failed` | 2 | `crates/cli/src/语言管理.rs:518` | 关键词.unwrap_or("") / &源列表.len().to_string() / &错误明细.join("\n") | 浏览远程语言包市场失败：已依次尝试 {} 个源： {} |
| `lang_search_header` | 1 | `crates/cli/src/main.rs:1114` | &界面表.取消息("lang_search_source") | 远程语言包市场（来源：{}）： |
| `lang_search_hint` | 0 | `crates/cli/src/main.rs:1138` | — | 安装：rzc lang install <语言代码> |
| `lang_search_source` | 0 | `crates/cli/src/main.rs:1115` | — | GitCode/GitHub，可用 RZ_LANG_REPO 覆盖 |

## CLI: language-pack download/verification

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `lc_err_curl_download` | 2 | `crates/cli/src/语言管理.rs:579`<br>`crates/cli/src/语言管理.rs:770` | &源项.压缩包地址 / 明细.trim() / 压缩包地址 | curl 下载失败（{}）：{} |
| `lc_err_curl_run` | 1 | `crates/cli/src/语言管理.rs:571`<br>`crates/cli/src/语言管理.rs:762` | &错误值.to_string() | 执行 curl 失败：{}（需要安装 git 或 curl 之一） |
| `lc_err_git_curl_both` | 2 | `crates/cli/src/语言管理.rs:693` | &克隆错误 / &下载错误.to_string() | git clone 错误：{} curl 下载错误：{} |
| `lc_err_git_run` | 1 | `crates/cli/src/语言管理.rs:708` | &错误值.to_string() | 执行 git 失败：{} |
| `lc_err_lang_code_from_path` | 1 | `crates/cli/src/语言管理.rs:431` | &源路径.display().to_string() | 无法从路径确定语言代码：{} |
| `lc_err_open_zip` | 2 | `crates/cli/src/语言管理.rs:791` | &压缩包路径.display().to_string() / &错误值.to_string() | 无法打开压缩包 {}：{} |
| `lc_err_read_zip` | 1 | `crates/cli/src/语言管理.rs:801` | &错误值.to_string() | 读取压缩包失败：{} |
| `lc_err_read_zip_entry` | 1 | `crates/cli/src/语言管理.rs:809` | &错误值.to_string() | 读取压缩包条目失败：{} |
| `lc_err_source_detail` | 3 | `crates/cli/src/语言管理.rs:509`<br>`crates/cli/src/语言管理.rs:646` | &(序号 + 1).to_string() / &源项.仓库地址 / &错误值.to_string() | 源{}（{}）：{} |
| `lc_err_write_file` | 2 | `crates/cli/src/语言管理.rs:833` | &目标路径.display().to_string() / &错误值.to_string() | 写入 {} 失败：{} |

## Language server (LSP) messages

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `lsp_about` | 0 | `crates/lsp/src/main.rs:106`<br>`crates/lsp/src/界面消息.rs:172`<br>`crates/lsp/src/界面消息.rs:178` | — | i18n-rust LSP 代理服务器 |
| `lsp_action_add_dependency` | 1 | `crates/lsp/src/响应映射/响应.rs:924` | &显示名 | 添加依赖 {}（cargo add） |
| `lsp_action_fix_fullwidth` | 2 | `crates/lsp/src/响应映射/响应.rs:972` | &字符值 / &替换值 | 将「{}」替换为「{}」 |
| `lsp_action_ignore_lint` | 0 | `crates/lsp/src/响应映射/响应.rs:999` | — | 忽略此行教学提示（// 教学忽略） |
| `lsp_err_cache_lock` | 0 | `crates/lsp/src/翻译缓存.rs:298`<br>`crates/lsp/src/翻译缓存.rs:584`<br>`crates/lsp/src/翻译缓存.rs:624` | — | 翻译缓存写锁获取失败 |
| `lsp_err_entry_missing` | 1 | `crates/lsp/src/翻译缓存.rs:357`<br>`crates/lsp/src/翻译缓存.rs:391` | 资源标识 | 翻译条目不存在: {} |
| `lsp_err_expect_initialize` | 1 | `crates/lsp/src/服务器.rs:250` | &请求值.method | 期望 initialize 请求，收到: {} |
| `lsp_err_expect_request` | 0 | `crates/lsp/src/服务器.rs:256` | — | 期望 Request，收到其他类型消息 |
| `lsp_err_extensions` | 0 | `crates/lsp/src/main.rs:71` | — | 错误: --extensions 需要指定扩展名列表（逗号分隔） |
| `lsp_err_handle_notification` | 1 | `crates/lsp/src/服务器.rs:580` | &错误值.to_string() | 处理客户端通知失败: {} |
| `lsp_err_handle_request` | 1 | `crates/lsp/src/服务器.rs:571` | &错误值.to_string() | 处理客户端请求失败: {} |
| `lsp_err_lang_pack` | 0 | `crates/lsp/src/main.rs:61` | — | 错误: --language-pack 需要指定路径 |
| `lsp_err_load_keywords` | 1 | `crates/lsp/src/服务器.rs:1938` | &错误值.to_string() | 加载关键字失败: {} |
| `lsp_err_lock` | 0 | `crates/lsp/src/服务器.rs:1244` | — | 待映射请求锁获取失败 |
| `lsp_err_ra_init_timeout` | 0 | `crates/lsp/src/服务器.rs:393` | — | 等待 rust-analyzer 初始化响应超时，中止启动 |
| `lsp_err_ra_not_found` | 0 | `crates/lsp/src/分析器.rs:403` | — | 未找到 rust-analyzer。请安装（`rustup component add rust-analyzer`）或设置 RUST_ANALYZER_PATH 环境变量 |
| `lsp_err_ra_not_running` | 0 | `crates/lsp/src/分析器.rs:177`<br>`crates/lsp/src/分析器.rs:265`<br>`crates/lsp/src/分析器.rs:585` | — | rust-analyzer 进程不可用（未启动或正在重启） |
| `lsp_err_ra_restart` | 1 | `crates/lsp/src/服务器.rs:523`<br>`crates/lsp/src/服务器.rs:530` | &错误值.to_string() | rust-analyzer 自动重启失败：{} |
| `lsp_err_ra_start` | 1 | `crates/lsp/src/分析器.rs:100` | &错误值.to_string() | 启动 rust-analyzer 失败: {} |
| `lsp_err_ra_stdin` | 0 | `crates/lsp/src/分析器.rs:105` | — | 无法获取 rust-analyzer stdin |
| `lsp_err_ra_stdout` | 0 | `crates/lsp/src/分析器.rs:110` | — | 无法获取 rust-analyzer stdout |
| `lsp_err_read_body` | 0 | `crates/lsp/src/分析器.rs:333` | — | 读取 rust-analyzer 消息体失败 |
| `lsp_err_read_output` | 1 | `crates/lsp/src/分析器.rs:309` | &错误值.to_string() | 读取 rust-analyzer 输出失败: {} |
| `lsp_err_recv_initialize` | 1 | `crates/lsp/src/服务器.rs:236` | &错误值.to_string() | 接收 initialize 请求失败: {} |
| `lsp_err_send_format` | 1 | `crates/lsp/src/服务器.rs:1221` | &错误值.to_string() | 发送格式化响应失败: {} |
| `lsp_err_send_initialize` | 1 | `crates/lsp/src/服务器.rs:453` | &错误值.to_string() | 发送 initialize 响应失败: {} |
| `lsp_err_temp_symlink` | 1 | `crates/lsp/src/服务器.rs:1442`<br>`crates/lsp/src/服务器.rs:1456` | &目录值.display().to_string() | 临时目录是符号链接，出于安全拒绝使用: {} |
| `lsp_err_write_lock` | 0 | `crates/lsp/src/分析器.rs:119`<br>`crates/lsp/src/分析器.rs:125`<br>`crates/lsp/src/分析器.rs:131` | — | 写入端锁获取失败 |
| `lsp_extensions_opt` | 0 | `crates/lsp/src/main.rs:112` | — | --extensions <列表> 方言扩展名列表（默认: .zh,.de,.ja） |
| `lsp_fail_notify` | 1 | `crates/lsp/src/服务器.rs:210` | 错误原因 | i18n-rust 语言服务器启动失败：{}（详见输出面板「i18n-rust LSP」） |
| `lsp_gen_lib_comment` | 0 | `crates/lsp/src/翻译缓存.rs:1349` | — | // 由 i18n-rust LSP 代理自动生成，请勿手动修改。 |
| `lsp_gen_module_fallback` | 0 | `crates/lsp/src/翻译缓存.rs:1365` | — | 模块 |
| `lsp_help_opt` | 0 | `crates/lsp/src/main.rs:114` | — | --help, -h 显示帮助信息 |
| `lsp_hint_add_dependency` | 1 | `crates/lsp/src/响应映射/诊断文本.rs:233` | 包名 | 💡 教学提示：若这是未声明的第三方库，运行 `rzc add {}` 添加依赖。 |
| `lsp_hint_cannot_find` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:227` | — | 💡 教学提示：请检查名称拼写是否正确，以及是否已通过 use 导入。 |
| `lsp_hint_mismatched_types` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:225` | — | 💡 教学提示：Rust 是强类型语言，请确保赋值和函数参数的类型一致。 |
| `lsp_hint_unused` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:229` | — | 💡 教学提示：未使用的变量可以用下划线 _ 前缀标记，如 _变量名。 |
| `lsp_install_already` | 1 | `crates/cli/src/安装.rs:59` | &目标路径.display().to_string() | ✅ 语言服务器 i18n-rust-lsp 已安装：{}（如需重装请加 --force） |
| `lsp_install_cargo` | 1 | `crates/cli/src/安装.rs:132` | 版本值 | 正在通过 cargo 从 crates.io 安装 i18n-rust-lsp {} ... |
| `lsp_install_done` | 1 | `crates/cli/src/安装.rs:152` | 版本值 | ✅ 语言服务器 i18n-rust-lsp {} 安装完成。 |
| `lsp_install_failed` | 1 | `crates/cli/src/安装.rs:107`<br>`crates/cli/src/安装.rs:113`<br>`crates/cli/src/安装.rs:119` | "无有效安装目录" / &错误值.to_string() / &状态值.to_string() | cargo install 失败（{}）。请检查网络连接，或从发布页手动下载 i18n-rust-lsp。 |
| `lsp_install_hint` | 1 | `crates/cli/src/安装.rs:155` | &目标路径.display().to_string() | 重启 VS Code 或执行「i18n-rust: 重启服务器」即可生效（安装位置：{}） |
| `lsp_install_local` | 1 | `crates/cli/src/安装.rs:124` | &目标路径.display().to_string() | ✅ 已从离线包安装语言服务器：{} |
| `lsp_install_no_cargo` | 1 | `crates/cli/src/安装.rs:142` | &错误值.to_string() | 执行 cargo 失败：{}。请先安装 Rust 工具链（https://rustup.rs），或从发布页手动下载 i18n-rust-lsp。 |
| `lsp_install_version_mismatch` | 2 | `crates/cli/src/安装.rs:66` | &已装版本 / env!("CARGO_PKG_VERSION") | 已安装的语言服务器版本为 {}，与当前 rzc 版本 {} 不一致。请执行 `rzc install lsp --force` 重新安装以保持一致。 |
| `lsp_install_version_unknown` | 1 | `crates/cli/src/安装.rs:75` | &目标路径.display().to_string() | 无法确定已安装语言服务器 {} 的版本；如需确保与 rzc 一致，请执行 `rzc install lsp --force`。 |
| `lsp_internal_error` | 0 | `crates/lsp/src/服务器.rs:1768` | — | 内部错误 |
| `lsp_lang_pack_opt` | 0 | `crates/lsp/src/main.rs:111` | — | --language-pack, -l <路径> 语言包目录路径（默认: lang-packs/zh） |
| `lsp_log_cache_removed` | 1 | `crates/lsp/src/翻译缓存.rs:566`<br>`crates/lsp/src/翻译缓存.rs:600` | &资源标识 / 资源标识 | 翻译缓存已移除: {} |
| `lsp_log_cache_updated` | 2 | `crates/lsp/src/翻译缓存.rs:402` | 资源标识 / &内容.lines().count().to_string() | 翻译缓存已更新: {} ({} 行) |
| `lsp_log_client_disconnected` | 0 | `crates/lsp/src/服务器.rs:548` | — | 客户端连接已断开 |
| `lsp_log_client_init` | 0 | `crates/lsp/src/服务器.rs:161` | — | 客户端初始化完成 |
| `lsp_log_client_notification` | 1 | `crates/lsp/src/服务器.rs:674` | &通知值.method | 客户端通知: {} |
| `lsp_log_client_request` | 2 | `crates/lsp/src/服务器.rs:647` | &请求值.method / &format!("{:?}", 请求值.id) | 客户端请求: {} id={} |
| `lsp_log_doc_opened` | 2 | `crates/lsp/src/服务器.rs:811` | 资源标识 / &文档版本.to_string() | 文档已打开并翻译: {} ({}) |
| `lsp_log_exit` | 0 | `crates/lsp/src/main.rs:219` | — | i18n-rust LSP 代理服务器已退出 |
| `lsp_log_lang_pack` | 1 | `crates/lsp/src/main.rs:207` | &参数.语言包路径.display().to_string() | 语言包路径: {} |
| `lsp_log_loaded_mappings` | 2 | `crates/lsp/src/服务器.rs:108` | &管理器.关键词表.len().to_string() / &管理器.取宏表().len().to_string() | 已加载 {} 个关键字映射，{} 个宏名称 |
| `lsp_log_mappings_fallback` | 1 | `crates/lsp/src/服务器.rs:1924` | &错误值.to_string() | 从映射表加载失败: {}, 使用备用 |
| `lsp_log_ra_crashed` | 0 | `crates/lsp/src/服务器.rs:597` | — | rust-analyzer 进程异常退出，正在自动重启并重新同步… |
| `lsp_log_ra_init_done` | 0 | `crates/lsp/src/服务器.rs:361` | — | rust-analyzer 初始化完成 |
| `lsp_log_ra_output_ended` | 0 | `crates/lsp/src/分析器.rs:149` | — | rust-analyzer 输出已结束 |
| `lsp_log_ra_restarted` | 0 | `crates/lsp/src/服务器.rs:637` | — | rust-analyzer 已自动重启，文档已重新同步 |
| `lsp_log_ra_starting` | 1 | `crates/lsp/src/分析器.rs:82` | &分析器路径.display().to_string() | 启动 rust-analyzer: {} |
| `lsp_log_ra_stopping` | 0 | `crates/lsp/src/分析器.rs:237` | — | 正在停止 rust-analyzer... |
| `lsp_log_ra_thread_exit` | 0 | `crates/lsp/src/服务器.rs:495` | — | rust-analyzer 转发线程已退出 |
| `lsp_log_shutdown` | 0 | `crates/lsp/src/服务器.rs:557` | — | 收到 shutdown 请求 |
| `lsp_log_start` | 0 | `crates/lsp/src/main.rs:203` | — | i18n-rust LSP 代理服务器启动 |
| `lsp_options` | 0 | `crates/lsp/src/main.rs:110` | — | 选项: |
| `lsp_phrase_aborting_due_to` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:180` | — | 中止，原因： |
| `lsp_phrase_borrowed_immutable` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:154` | — | 被不可变借用 |
| `lsp_phrase_borrowed_mutable` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:158` | — | 被可变借用 |
| `lsp_phrase_cannot_borrow` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:151` | — | 无法借用 |
| `lsp_phrase_cannot_find_function` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:129` | — | 找不到函数 |
| `lsp_phrase_cannot_find_module` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:133` | — | 找不到模块 |
| `lsp_phrase_cannot_find_type` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:125` | — | 找不到类型 |
| `lsp_phrase_cannot_find_value` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:121` | — | 找不到变量 |
| `lsp_phrase_does_not_implement` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:168` | — | 未实现 |
| `lsp_phrase_expected` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:140` | — | 期望 |
| `lsp_phrase_field` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:165` | — | 字段 |
| `lsp_phrase_file_not_found` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:179` | — | 文件未找到 |
| `lsp_phrase_found` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:147` | — | 实际为 |
| `lsp_phrase_is_not_satisfied` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:173` | — | 未被满足 |
| `lsp_phrase_method_not_found` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:163` | — | 方法未找到 |
| `lsp_phrase_mismatched_types` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:137` | — | 类型不匹配 |
| `lsp_phrase_no_method_named` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:160` | — | 没有名为 |
| `lsp_phrase_previous_error` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:181` | — | 前一个错误 |
| `lsp_phrase_the_trait` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:170` | — | 特征 |
| `lsp_phrase_type_mismatch` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:139` | — | 类型不匹配 |
| `lsp_phrase_unresolved_import` | 0 | `crates/lsp/src/服务器.rs:1368`<br>`crates/lsp/src/响应映射/诊断文本.rs:177` | — | 未解析的导入 |
| `lsp_phrase_unused_import` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:150` | — | 未使用的导入 |
| `lsp_phrase_unused_variable` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:149` | — | 未使用的变量 |
| `lsp_unknown_arg` | 1 | `crates/lsp/src/main.rs:91` | &参数表[下标] | 未知参数: {} |
| `lsp_usage` | 0 | `crates/lsp/src/main.rs:108` | — | 用法: i18n-rust-lsp [选项] |
| `lsp_use_help` | 0 | `crates/lsp/src/main.rs:92` | — | 使用 --help 查看帮助 |
| `lsp_version_opt` | 0 | `crates/lsp/src/main.rs:113` | — | --version, -V 输出版本号 |
| `lsp_warn_builtin_fallback` | 0 | `crates/lsp/src/服务器.rs:1946` | — | 未找到语言包文件，使用内置关键字映射 |
| `lsp_warn_request_timeout` | 0 | `crates/lsp/src/服务器.rs:1598` | — | 转发请求超时：rust-analyzer 未响应 |

## CLI/engine: diagnostic rendering

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `diag_fix_suggestion` | 1 | `crates/engine/src/诊断/诊断翻译.rs:556` | &提示 | 修复建议：{} |
| `diag_kind_error` | 0 | `crates/engine/src/语言.rs:301`<br>`crates/engine/src/诊断/教学诊断记录.rs:47` | — | 错误 |
| `diag_kind_help` | 0 | `crates/engine/src/诊断/教学诊断记录.rs:50` | — | 帮助 |
| `diag_kind_ice` | 0 | `crates/engine/src/诊断/教学诊断记录.rs:51` | — | 内部编译器错误 |
| `diag_kind_note` | 0 | `crates/engine/src/诊断/教学诊断记录.rs:49` | — | 注释 |
| `diag_kind_warning` | 0 | `crates/engine/src/诊断/教学诊断记录.rs:48` | — | 警告 |
| `diag_ownership_borrow_conflict` | 3 | `crates/engine/src/诊断/教学诊断记录.rs:171`<br>`crates/engine/src/诊断/教学诊断记录.rs:331` | 变量 / &冲突.可变方.起始行号.to_string() / &冲突.不可变方.起始行号.to_string() | 变量 `{}` 的可变借用（第 {} 行）与不可变借用（第 {} 行）重叠：两者不能同时存在。 |
| `diag_ownership_borrowed` | 2 | `crates/engine/src/诊断/教学诊断记录.rs:201` | 变量 / &借用处.起始行号.to_string() | 变量 `{}` 在第 {} 行被借用。 |
| `diag_ownership_borrowed_in_use` | 3 | `crates/engine/src/诊断/教学诊断记录.rs:189` | 变量 / &借用处.起始行号.to_string() / &复用处.起始行号.to_string() | 变量 `{}` 在第 {} 行被借用，第 {} 行仍在被使用。 |
| `diag_ownership_conflict` | 1 | `crates/engine/src/诊断/教学诊断记录.rs:204` | 变量 | 变量 `{}` 存在所有权冲突。 |
| `diag_ownership_moved` | 2 | `crates/engine/src/诊断/教学诊断记录.rs:197` | 变量 / &移动处.起始行号.to_string() | 变量 `{}` 在第 {} 行被移动。 |
| `diag_ownership_moved_reused` | 3 | `crates/engine/src/诊断/教学诊断记录.rs:181` | 变量 / &移动处.起始行号.to_string() / &复用处.起始行号.to_string() | 变量 `{}` 在第 {} 行被移动，第 {} 行尝试再次使用。 |
| `diag_ref_depth_hint` | 2 | `crates/engine/src/诊断/诊断翻译.rs:576` | &本地化引用类型(&预期类型, &self.类型映射) / &本地化引用类型(&实际类型, &self.类型映射) | 引用层数不匹配：期望 `{}`，实际是 `{}`。检查 `&` 的数量：实参前补 `&`，或让参数类型去掉多余的 `&`。 |
| `diag_rustc_float` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:109`<br>`crates/lsp/src/响应映射/诊断文本.rs:112`<br>`crates/engine/src/诊断/诊断翻译.rs:662` | — | 浮点数 |
| `diag_rustc_integer` | 0 | `crates/lsp/src/响应映射/诊断文本.rs:108`<br>`crates/lsp/src/响应映射/诊断文本.rs:114`<br>`crates/engine/src/诊断/诊断翻译.rs:661` | — | 整数 |

## Engine: Unicode character names (dynamic, by codepoint)

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `unicode_name_00AD` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 软连字符 |
| `unicode_name_0391` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Α |
| `unicode_name_0392` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Β |
| `unicode_name_0395` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Ε |
| `unicode_name_0397` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Η |
| `unicode_name_0399` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Ι |
| `unicode_name_039A` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Κ |
| `unicode_name_039C` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Μ |
| `unicode_name_039D` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Ν |
| `unicode_name_039F` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Ο |
| `unicode_name_03A1` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Ρ |
| `unicode_name_03A4` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Τ |
| `unicode_name_03A5` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Υ |
| `unicode_name_03A7` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊大写字母 Χ |
| `unicode_name_03B1` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 α |
| `unicode_name_03B5` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 ε |
| `unicode_name_03B9` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 ι |
| `unicode_name_03BA` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 κ |
| `unicode_name_03BC` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 μ |
| `unicode_name_03BD` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 ν |
| `unicode_name_03BF` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 ο |
| `unicode_name_03C1` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 ρ |
| `unicode_name_03C4` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 τ |
| `unicode_name_03C5` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 υ |
| `unicode_name_03C7` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 χ |
| `unicode_name_03C9` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 希腊小写字母 ω |
| `unicode_name_0410` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔大写字母 А |
| `unicode_name_0412` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔大写字母 В |
| `unicode_name_0415` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔大写字母 Е |
| `unicode_name_041A` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔大写字母 К |
| `unicode_name_041D` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔大写字母 Н |
| `unicode_name_041E` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔大写字母 О |
| `unicode_name_0420` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔大写字母 Р |
| `unicode_name_0421` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔大写字母 С |
| `unicode_name_0422` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔大写字母 Т |
| `unicode_name_0423` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔大写字母 У |
| `unicode_name_0425` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔大写字母 Х |
| `unicode_name_0430` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔小写字母 а |
| `unicode_name_0435` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔小写字母 е |
| `unicode_name_043E` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔小写字母 о |
| `unicode_name_0440` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔小写字母 р |
| `unicode_name_0441` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔小写字母 с |
| `unicode_name_0445` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 西里尔小写字母 х |
| `unicode_name_180E` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 蒙古语元音分隔符 |
| `unicode_name_200B` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 零宽空格 |
| `unicode_name_200C` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 零宽非连接符 |
| `unicode_name_200D` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 零宽连接符 |
| `unicode_name_200E` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 从左到右标记 |
| `unicode_name_200F` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 从右到左标记 |
| `unicode_name_202A` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 从左到右嵌入 |
| `unicode_name_202B` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 从右到左嵌入 |
| `unicode_name_202C` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 弹出方向格式化 |
| `unicode_name_202D` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 从左到右覆盖 |
| `unicode_name_202E` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 从右到左覆盖 |
| `unicode_name_2060` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 单词连接符 |
| `unicode_name_2066` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 从左到右隔离 |
| `unicode_name_2067` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 从右到左隔离 |
| `unicode_name_2068` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 首项强隔离 |
| `unicode_name_2069` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 弹出方向隔离 |
| `unicode_name_FEFF` | 0 | dynamic | crates/engine/src/混淆检测.rs (format! by codepoint) | 零宽不换行空格 |

## CLI: guided first-run tutorial

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `guide_example_head` | 0 | `crates/engine/src/指南.rs:263` | — | ## 示例：入口文件的正确写法 |
| `guide_footer` | 0 | `crates/engine/src/指南.rs:273` | — | 由 `rzc guide` 生成；语言包更新后重跑以刷新。 |
| `guide_intro` | 1 | `crates/engine/src/指南.rs:241` | 语言码 | 本项目用{}编写 Rust 代码：语言结构（关键字/宏）与标准库调用都使用母语词条，rzc 会自动转译为标准 Rust 交给官方工具链编译。生成或修改本项目代码时，请遵循下方书写规则与全量词条表。 |
| `guide_rule_danger` | 0 | `crates/engine/src/指南.rs:249` | — | - 词条表「危险等级」列含义：`纯安全` 只在受保护位置替换；`会改写标识符` 在任意位置替换（同名自定义标识符会被改写）；`会 panic` 运行时可能 panic，仅显式选用，优先用安全同义词条；多风险以 `+` 连接并在「备注」列说… |
| `guide_rule_gaps` | 0 | `crates/engine/src/指南.rs:261` | — | - 若输出中出现未翻译的英文消息，`rzc gaps` 查看待译清单（不影响运行）；补齐词条表是社区贡献的一部分。 |
| `guide_rule_ident` | 0 | `crates/engine/src/指南.rs:246` | — | - 自定义函数名、变量名、类型名可用母语或英文，转译保留原样；注意危险等级为「会改写标识符」的词条会连带改写同名自定义标识符，命名时避开这类词。 |
| `guide_rule_keywords` | 0 | `crates/engine/src/指南.rs:245` | — | - 语言关键字、宏、标准库类型与方法一律使用下方词条表中的母语词条（如写 `函数` 而非 `fn`、`打印行!` 而非 `println!`）：英文写法虽能编译，但会触发「英文回落」教学提示。 |
| `guide_rule_pitfalls` | 0 | `crates/engine/src/指南.rs:260` | — | - 两个实测坑：标准 Rust 没有 `"""` 三引号多行字符串，写出即报「期望 . ; ? }」——跨行文本用原始字符串 `r#"…"#`（内部 `"` 免转义）或逐行拼接 `向量![行1, 行2].连接("\n")`（每行普通字符串… |
| `guide_rule_punct` | 0 | `crates/engine/src/指南.rs:248` | — | - 代码位置使用半角标点（逗号、分号、括号、引号等）；全角标点只允许出现在字符串与注释内，否则 rzc 会给出全角标点告警。 |
| `guide_rule_raw_ident` | 0 | `crates/engine/src/指南.rs:247` | — | - 自定义名字与 Rust 保留字或宏同名时（如 `匹配`、`测试`），用 `r#` 前缀转义：`函数 r#匹配` → `fn r#match`，调用处同样写 `r#匹配`；本文件内声明的同名函数优先于宏，跨文件同名调用请用 `r#` 区… |
| `guide_rule_workflow` | 1 | `crates/engine/src/指南.rs:256` | 语言码 | - 工作方式：`rzc run src/main.{}` 运行项目，`rzc check <文件>` 只做检查；诊断的行列与源码位置自动回译到母语源文件。IDE 中安装 i18n-rust VS Code 扩展获得实时诊断与母语补全。 |
| `guide_rules_head` | 0 | `crates/engine/src/指南.rs:242` | — | ## 书写规则 |
| `guide_table_head` | 1 | `crates/engine/src/指南.rs:269` | &总条数.to_string() | ## 全量词条表（{} 条） |
| `guide_title` | 1 | `crates/engine/src/指南.rs:240` | 语言码 | # rzc 母语编程指南 —— {} ↔ Rust |

## CLI: crate registry (`rzc crate`)

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `crate_already_installed` | 2 | `crates/cli/src/包仓库.rs:278` | 包名 / 语言代码 | 映射 {}（语言 {}）已安装，使用 --force 覆盖 |
| `crate_file_missing` | 1 | `crates/cli/src/包仓库.rs:261`<br>`crates/cli/src/包仓库.rs:267` | &条目值.映射文件 | 索引指向的映射文件不存在：{} |
| `crate_index_parse_failed` | 1 | `crates/cli/src/包仓库.rs:148` | &错误值.to_string() | 解析 index.json 失败：{} |
| `crate_installed` | 3 | `crates/cli/src/包仓库.rs:288` | 包名 / 语言代码 / &目标路径.display().to_string() | ✅ 映射 {}（语言 {}）安装成功：{} |
| `crate_list_empty` | 0 | `crates/cli/src/包仓库.rs:300`<br>`crates/cli/src/包仓库.rs:342` | — | 尚未安装任何社区映射（用 rzc crate install 获取）。 |
| `crate_list_header` | 1 | `crates/cli/src/包仓库.rs:305` | &已装清单.len().to_string() | 已安装社区映射（共 {} 个）： |
| `crate_not_found` | 2 | `crates/cli/src/包仓库.rs:255` | 包名 / 语言代码 | 注册中心未找到映射：{}（语言 {}） |
| `crate_publish_bad_section` | 1 | `crates/cli/src/包仓库.rs:470` | 节名.as_str() | 映射文件含不允许的节：{}（仅允许模块路径 / 标识符 / 解释） |
| `crate_publish_clone_failed` | 1 | `crates/cli/src/包仓库.rs:546` | 地址 | 克隆注册中心仓库失败：{} |
| `crate_publish_file_missing` | 1 | `crates/cli/src/包仓库.rs:423` | &文件值.display().to_string() | 指定的映射文件不存在：{} |
| `crate_publish_invalid_toml` | 1 | `crates/cli/src/包仓库.rs:459` | &错误值.to_string() | 映射文件不是合法 TOML：{} |
| `crate_publish_not_found` | 2 | `crates/cli/src/包仓库.rs:449` | 包名 / 语言代码 | 未找到待发布的映射文件（{} / {}）：请用 --file 指定路径 |
| `crate_publish_not_table` | 0 | `crates/cli/src/包仓库.rs:464` | — | 映射文件根必须是 TOML 表 |
| `crate_publish_push_failed` | 1 | `crates/cli/src/包仓库.rs:574`<br>`crates/cli/src/包仓库.rs:580` | 错误文本.trim() / &错误值.to_string() | 推送注册中心失败：{} |
| `crate_publish_wrote_local` | 1 | `crates/cli/src/包仓库.rs:516` | &仓库目录.display().to_string() | 已写入本地注册中心：{} |
| `crate_published` | 0 | `crates/cli/src/包仓库.rs:404` | 包名 / 语言代码 | ✅ 映射已发布到注册中心。 |
| `crate_removed` | 2 | `crates/cli/src/包仓库.rs:333` | 包名 / 语言代码 | ✅ 映射 {}（语言 {}）已移除。 |
| `crate_search_empty` | 0 | `crates/cli/src/包仓库.rs:217` | — | 注册中心暂无可匹配的映射。 |
| `crate_search_hint` | 0 | `crates/cli/src/包仓库.rs:227` | — | 安装：rzc crate install <crate> --lang <语言> |
| `crate_updated` | 2 | `crates/cli/src/包仓库.rs:371` | &计数.to_string() / &已装清单.len().to_string() | ✅ 已更新 {} / {} 个社区映射。 |

## Engine: language-pack loading

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `load_builtin_lang_pack_failed` | 1 | `crates/cli/src/main.rs:2889` | &错误值.to_string() | 加载内置语言包失败: {} |
| `load_error_msg_failed` | 1 | `crates/cli/src/诊断输出.rs:595` | &错误值.to_string() | 加载错误消息失败: {} |
| `load_global_lang_pack_failed` | 1 | `crates/cli/src/main.rs:2863` | &错误值.to_string() | 加载全局语言包失败: {} |
| `load_keywords_missing` | 1 | `crates/engine/src/故障.rs:222` | — | 关键字文件不存在: {} |
| `load_lang_pack_failed` | 1 | `crates/cli/src/main.rs:2799` | &错误值.to_string() | 加载语言包失败: {} |
| `load_local_lang_pack_failed` | 1 | `crates/cli/src/main.rs:2847` | &错误值.to_string() | 加载本地语言包失败: {} |
| `load_map_file_missing` | 1 | `crates/engine/src/故障.rs:226`<br>`crates/engine/src/故障.rs:234`<br>`crates/engine/src/故障.rs:239` | — | 映射表文件不存在: {} |
| `load_parse_builtin_keywords_failed` | 1 | `crates/engine/src/故障.rs:256` | — | 解析内置关键字 TOML 失败: {} |
| `load_parse_builtin_paths_failed` | 1 | `crates/engine/src/故障.rs:265` | — | 解析内置模块路径 TOML 失败: {} |
| `load_parse_builtin_stdlib_failed` | 1 | `crates/engine/src/故障.rs:272` | — | 解析内置标准库 TOML 失败: {} |
| `load_parse_keywords_failed` | 1 | `crates/engine/src/故障.rs:224` | — | 解析关键字 TOML 失败: {} |
| `load_parse_map_failed` | 1 | `crates/engine/src/故障.rs:247` | — | 解析映射表失败: {} |
| `load_parse_map_path_failed` | 2 | `crates/engine/src/故障.rs:237`<br>`crates/engine/src/故障.rs:242` | — | 解析映射表 {} 失败: {} |
| `load_parse_module_paths_failed` | 1 | `crates/engine/src/故障.rs:231` | — | 解析模块路径 TOML 失败: {} |
| `load_read_dir_failed` | 1 | `crates/engine/src/故障.rs:343` | — | 读取第三方库目录失败: {} |
| `load_read_keywords_failed` | 1 | `crates/engine/src/故障.rs:223` | — | 无法读取关键字文件: {} |
| `load_read_map_failed` | 1 | `crates/engine/src/故障.rs:246` | — | 读取映射表失败: {} |
| `load_read_map_path_failed` | 2 | `crates/engine/src/故障.rs:235`<br>`crates/engine/src/故障.rs:240`<br>`crates/engine/src/故障.rs:253` | — | 读取映射表 {} 失败: {} |
| `load_read_module_paths_failed` | 1 | `crates/engine/src/故障.rs:228` | — | 无法读取模块路径文件: {} |

## Engine: lexer/loader errors

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `err_cache_unavailable` | 1 | `crates/engine/src/故障.rs:156` | 原因 | 翻译缓存不可用：{} |
| `err_confusion_char` | 3 | `crates/engine/src/故障.rs:153` | &发生位置.描述() / &码位文字 / 详情 | 检测到可疑 Unicode 字符（{}）：U+{}（{}） |
| `err_input_invalid` | 1 | `crates/engine/src/故障.rs:136` | 原因 | 输入无效：{} |
| `err_lex_error` | 2 | `crates/engine/src/故障.rs:139` | &发生位置.描述() / 详情 | 词法错误（{}）：{} |
| `err_line_col` | 2 | `crates/cli/src/界面.rs:245`<br>`crates/engine/src/故障.rs:58`<br>`crates/engine/src/界面消息.rs:290` | "3" / "7" / &self.行号.to_string() / &self.列号.to_string() | 第 {} 行第 {} 列 |
| `err_mapping_missing` | 1 | `crates/engine/src/故障.rs:144` | 名称 | 映射缺失：找不到 `{}` 的翻译映射 |
| `err_mapping_missing_at` | 2 | `crates/engine/src/故障.rs:143` | &点.描述() / 名称 | 映射缺失（{}）：找不到 `{}` 的翻译映射 |
| `err_parse_error_messages` | 1 | `crates/engine/src/故障.rs:279` | — | 解析错误消息 TOML 失败: {} |
| `err_read_error_messages` | 2 | `crates/engine/src/故障.rs:277` | — | 无法读取错误消息文件 {}: {} |
| `err_unsupported` | 1 | `crates/engine/src/故障.rs:162` | 构造 | 暂不支持的语法构造：`{}` |
| `err_unsupported_at` | 2 | `crates/engine/src/故障.rs:161` | &点.描述() / 构造 | 暂不支持的语法构造（{}）：`{}` |

## CLI: `rzc run` output

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `run_lib_no_bin` | 2 | `crates/cli/src/main.rs:480` | &路径文本 | {} 是库入口（产物是 Cargo 的 src/lib.rs）：库里没有可执行的主程序，`rzc run` 运行不了它。校验代码用 `rzc check {}`；要让程序跑起来，请另写一个方言主程序入口来调用这个库，或用 `cargo t… |
| `run_not_default_entry` | 2 | `crates/cli/src/main.rs:486` | &源文件值.display().to_string() / &源路径.display().to_string() | {} 不是项目默认入口：经 cargo 运行时 `rzc run` 只运行 src/main（当前产物 {}）。请改用 `cargo run --bin <名称>` / `cargo run --example <名称>` 运行已生成的产… |

## Engine/CLI: transpile messages

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `transpile_backup` | 2 | `crates/cli/src/main.rs:2562` | &路径值.display().to_string() / &备份.display().to_string() | 已将 {} 备份为 {}（原内容被新的转译结果覆盖） |
| `transpile_file_failed` | 2 | `crates/cli/src/main.rs:2409` | &路径值.display().to_string() / &错误值.to_string() | 转译 {} 失败: {} |

## Other UI messages

| Key | `{}` | Rendered at | Arguments | zh source text |
|---|---|---|---|---|
| `add_cargo_failed` | 1 | `crates/cli/src/main.rs:1170` | &错误值.to_string() | 执行 cargo add 失败：{} |
| `add_mapping_missing` | 2 | `crates/cli/src/main.rs:1203` | &包名 | ✅ 已添加 {}。该 crate 暂无母语映射，源码中请使用英文名 {}（可用 rzc mapping auto 生成映射） |
| `add_mapping_ready` | 2 | `crates/cli/src/main.rs:1199` | &包名 / &别名 | ✅ 已添加 {}。该 crate 有母语别名「{}」，母语源码中可直接使用 |
| `add_no_project` | 0 | `crates/cli/src/main.rs:1161` | — | 未找到 Cargo 项目（当前目录及上级均无 Cargo.toml），请在项目内运行 |
| `add_no_targets_hint` | 2 | `crates/cli/src/main.rs:1181` | &入口名 | 这个项目还没有 Cargo 能读取的构建入口：`Cargo.toml` 未声明 [lib]/[[bin]]，`src/` 下也没有 {} 对应的 .rs 文件。先运行 `rzc check {}` 生成标准 Rust 入口，再重试本命令 |
| `agents_generated` | 1 | `crates/cli/src/main.rs:898`<br>`crates/cli/src/main.rs:2773` | &输出路径.display().to_string() / "AGENTS.md" | AI 编程指南已生成：{}（语言包更新后重跑 rzc guide 可刷新） |
| `cargo_check_failed` | 2 | `crates/cli/src/main.rs:770` | &项目根.display().to_string() / &错误值.to_string() | 执行 cargo check 失败，项目根: {}，错误: {} |
| `cargo_progress_checking` | 1 | `crates/cli/src/诊断输出.rs:253` | — | 正在检查 {} |
| `cargo_progress_compiling` | 1 | `crates/cli/src/诊断输出.rs:252` | — | 正在编译 {} |
| `cargo_progress_could_not_compile` | 1 | `crates/cli/src/诊断输出.rs:272`<br>`crates/cli/src/诊断输出.rs:276` | 主段 / 片段裁剪 | 无法编译 {} |
| `cargo_progress_due_to` | 1 | `crates/cli/src/诊断输出.rs:273` | 数量 | （此前已有 {} 个错误） |
| `cargo_progress_error` | 1 | `crates/cli/src/诊断输出.rs:256`<br>`crates/cli/src/诊断输出.rs:261`<br>`crates/cli/src/诊断输出.rs:545` | — | 错误：{} |
| `cargo_progress_finished` | 1 | `crates/cli/src/诊断输出.rs:254` | — | 编译完成 {} |
| `cargo_progress_running` | 1 | `crates/cli/src/诊断输出.rs:255` | — | 正在运行 {} |
| `cargo_run_failed` | 2 | `crates/cli/src/main.rs:563`<br>`crates/cli/src/main.rs:610`<br>`crates/cli/src/main.rs:629` | &项目根.display().to_string() / &错误值.to_string() | 执行 cargo run 失败，项目根: {}，错误: {} |
| `compile_error` | 1 | `crates/cli/src/诊断输出.rs:828`<br>`crates/cli/src/诊断输出.rs:861` | &编译错误文本() | 编译错误： {} |
| `dir_exists` | 1 | `crates/cli/src/main.rs:2714` | 项目名 | 目录 {} 已存在 |
| `eject_args_duplicate` | 1 | `crates/cli/src/main.rs:1860` | &重复项.display().to_string() | 导出目标重复：{}（同一目标只需传一次） |
| `eject_failed` | 2 | `crates/cli/src/main.rs:1892` | &单个文件.display().to_string() / &format!("{错误值:#}") | 导出失败：{}（{}） |
| `eject_hint_declared_name` | 4 | `crates/engine/src/导出冲突提示.rs:59` | &行号文本 / &列号文本 / &self.词条 / &self.目标 | 第 {} 行第 {} 列：声明的名字「{}」命中词条，转译后会被改写成「{}」，请换个名字（教学提示） |
| `eject_hint_field_shadowed` | 4 | `crates/engine/src/导出冲突提示.rs:55` | &行号文本 / &列号文本 / &self.词条 / &self.目标 | 第 {} 行第 {} 列：字段名「{}」命中词条，转译后字段名会变成「{}」，serde 键随之改名（教学提示） |
| `eject_hint_panic_as_text` | 4 | `crates/engine/src/导出冲突提示.rs:63` | &行号文本 / &列号文本 / &self.词条 / &self.目标 | 第 {} 行第 {} 列：「{}」会 panic，却被当文本使用（→「{}」），建议改用安全同义条目并显式处理「无」（教学提示） |
| `eject_no_source` | 1 | `crates/cli/src/main.rs:1941`<br>`crates/cli/src/main.rs:2046` | &源文件值.display().to_string() / &扫描根.display().to_string() | 目录 {} 下没有可导出的方言源文件 |
| `eject_summary_ok` | 1 | `crates/cli/src/main.rs:1902` | &成功数.to_string() | 共导出 {} 个目标 |
| `eject_summary_partial` | 2 | `crates/cli/src/main.rs:1909` | &成功数.to_string() / &失败数.to_string() | 导出结束：成功 {} 个，失败 {} 个 |
| `exported_to` | 1 | `crates/cli/src/main.rs:1868`<br>`crates/cli/src/main.rs:1883` | &输出路径.display().to_string() | 已导出到 {} |
| `first_run_hello` | 0 | `crates/cli/src/main.rs:1033` | — | 欢迎使用 rzc——用母语学习 Rust 的编译工具！ |
| `first_run_next_steps` | 0 | `crates/cli/src/main.rs:1045` | — | 下一步：`rzc doctor` 检查环境 · `rzc init` 创建项目 · `rzc install lsp` 为 VS Code 提供补全诊断 |
| `first_run_rustc_missing` | 0 | `crates/cli/src/main.rs:1043` | — | 未检测到 rustc 编译器。请安装 Rust 工具链（访问 https://rustup.rs），或执行 `rzc install toolchain` 使用内置工具链。 |
| `fullwidth_fix_done` | 2 | `crates/cli/src/main.rs:689` | &修正数.to_string() / &源文件值.display().to_string() | 已自动修复 {} 处全角标点：{} |
| `fullwidth_fix_remaining` | 1 | `crates/cli/src/main.rs:698` | &剩余数.to_string() | 仍有 {} 处仅提示的全角字符（顿号/全角空格等），请手动修改 |
| `fullwidth_hint_only` | 0 | `crates/engine/src/全角标点.rs:79` | — | 无对应半角字符，请手动修改 |
| `fullwidth_to_half` | 1 | `crates/engine/src/全角标点.rs:77` | &半角.to_string() | 应改为半角「{}」 |
| `fullwidth_warning_at` | 4 | `crates/engine/src/全角标点.rs:82` | &self.行号.to_string() / &self.列序.to_string() / &self.涉事字符.to_string() / &指引 | 第 {} 行第 {} 列：检测到全角标点「{}」，{} |
| `gaps_cleared` | 2 | `crates/cli/src/翻译缺口.rs:32` | &语言代码 / &清单文件.display().to_string() | 已清空 {} 语言的待译清单：{} |
| `gaps_none` | 1 | `crates/cli/src/翻译缺口.rs:40`<br>`crates/cli/src/翻译缺口.rs:49` | &清单文件.display().to_string() | 没有待译消息：{} |
| `gaps_written` | 2 | `crates/cli/src/翻译缺口.rs:60` | &导出路径.display().to_string() / &清单.len().to_string() | 待译清单已导出：{}（{} 条） |
| `global_lang_dir` | 1 | `crates/cli/src/main.rs:1103` | &全局语言目录().display().to_string() | 全局语言包目录：{} |
| `hint_add_dependency` | 2 | `crates/cli/src/诊断输出.rs:1211` | 名称 | 提示: "{}" 疑似未声明的第三方库，运行 `rzc add {}` 添加依赖 |
| `hint_own_crate_lib` | 3 | `crates/cli/src/诊断输出.rs:1223` | 名称 / 路径段 | "{}" 是本项目的包名，不是第三方库——Cargo 不允许一个包依赖自己，`rzc add {}` 必然失败。要用本项目自己的库代码，先运行 `rzc check {}` 生成 Cargo 的 src/lib.rs，然后重新校验 |
| `hint_own_crate_no_lib` | 4 | `crates/cli/src/诊断输出.rs:1228` | 名称 / 路径段 | "{}" 是本项目的包名，不是第三方库——Cargo 不允许一个包依赖自己，`rzc add {}` 必然失败。本项目还没有库入口 {}：`use 包名::…` 引用的就是这个文件编译出的库，把可复用的代码写进去，再运行 `rzc che… |
| `init_lang_fallback` | 2 | `crates/cli/src/main.rs:2636` | &区域原文 / 初始化缺省语言 | 未识别系统语言 '{}'，已回退到 '{}'。可用 `rzc init <项目名> --lang <语言代码>` 指定语言。 |
| `init_lang_menu_invalid` | 0 | `crates/cli/src/main.rs:2704` | — | 无效选择，请重新输入。 |
| `init_lang_menu_prompt` | 1 | `crates/cli/src/main.rs:2691` | &缺省提示 | 输入序号或语言代码后回车（直接回车使用默认 {}）： |
| `init_lang_menu_title` | 0 | `crates/cli/src/main.rs:2678` | — | 当前系统语言没有对应的语言包，请选择项目语言： |
| `init_unknown_lang` | 2 | `crates/cli/src/main.rs:2623` | &语言值 / &可用语言代码文本() | 未知语言 '{}'。可用语言：{}。安装第三方语言包请用 `rzc lang install <语言代码>`。 |
| `input_is_directory` | 1 | `crates/cli/src/main.rs:1781` | &源文件值.display().to_string() | {} 是目录：`rzc run` / `rzc check` / `rzc transpile` 只接受单个源文件；批量导出目录请用 `rzc eject <目录>` |
| `input_missing` | 1 | `crates/cli/src/main.rs:1775`<br>`crates/cli/src/main.rs:1820`<br>`crates/cli/src/main.rs:2021` | &源文件值.display().to_string() / &锚点.display().to_string() | {} 不存在：请检查路径（文件或目录） |
| `installed_langs_count` | 1 | `crates/cli/src/main.rs:1067` | &清单.len().to_string() | 已安装的语言包（共 {} 个）： |
| `invalid_lang_code` | 1 | `crates/cli/src/语言管理.rs:448`<br>`crates/cli/src/语言管理.rs:474` | 语言码 | 无效的语言代码 '{}'：应为仓库内目录名。 |
| `invalid_lang_dir` | 1 | `crates/cli/src/语言管理.rs:1025` | &路径值.display().to_string() | 无效的语言包目录：{} 下缺少 keywords.toml。 |
| `legacy_product_moved` | 2 | `crates/cli/src/main.rs:1455` | &旧产物.display().to_string() / &备份.display().to_string() | 旧布局生成文件 {} 已迁到 .rz/gen/（原内容备份为 {}） |
| `lint_confusable_method` | 4 | `crates/engine/src/校验.rs:101` | &self.行号.to_string() / &self.列号.to_string() / &self.相关文本 / &self.附加文本 | 第 {} 行第 {} 列：方法名「{}」未收录（将原样保留），是否想写「{}」？（教学提示） |
| `lint_deep_indent` | 2 | `crates/engine/src/校验.rs:97` | &self.行号.to_string() / &self.相关文本 | 第 {} 行：嵌套过深（{} 个空格缩进），建议拆分函数（教学提示） |
| `lint_english_fallback` | 4 | `crates/engine/src/校验.rs:110` | &self.行号.to_string() / &self.列号.to_string() / &self.相关文本 / &self.附加文本 | 第 {} 行第 {} 列：`{}` 有母语词条 `{}`，建议使用母语写法（教学提示） |
| `lint_magic_number` | 3 | `crates/engine/src/校验.rs:89` | &self.行号.to_string() / &self.列号.to_string() / &self.相关文本 | 第 {} 行第 {} 列：魔法数字「{}」，建议命名常量（教学提示） |
| `lint_reserved_name_collision` | 4 | `crates/engine/src/校验.rs:119` | &self.行号.to_string() / &self.列号.to_string() / &self.相关文本 / &self.附加文本 | 第 {} 行第 {} 列：声明的名字「{}」转译后是 Rust 保留字「{}」，产物非法或语义被静默改写；请改名，或用 r# 原始标识符转义（教学提示） |
| `lint_untyped_let` | 2 | `crates/engine/src/校验.rs:85` | &self.行号.to_string() / &self.列号.to_string() | 第 {} 行第 {} 列：`让` 未标注类型，试试显式写出类型（教学提示） |
| `log_cache_disk_discard` | 0 | `crates/engine/src/缓存.rs:369` | — | 磁盘缓存版本不匹配或已损坏，已丢弃 |
| `log_cache_disk_loaded` | 1 | `crates/engine/src/缓存.rs:452` | &self.条目.len().to_string() | 已从磁盘加载缓存（{} 条） |
| `log_cache_disk_save_failed` | 0 | `crates/engine/src/缓存.rs:425` | — | 磁盘缓存保存失败 |
| `log_cache_evict` | 1 | `crates/engine/src/缓存.rs:688` | &头部哈希值.to_string() | 容量已满，淘汰最旧条目（哈希 {}） |
| `log_cache_hit` | 2 | `crates/engine/src/缓存.rs:589` | &内容.len().to_string() / &语境指纹.to_string() | 命中缓存（{} 字节，指纹 {}） |
| `log_cache_miss` | 2 | `crates/engine/src/缓存.rs:600` | &内容.len().to_string() / &语境指纹.to_string() | 缓存未命中（{} 字节，指纹 {}），执行翻译 |
| `log_level_debug` | 0 | `crates/engine/src/日志.rs:42` | — | 调试 |
| `log_level_error` | 0 | `crates/engine/src/日志.rs:45` | — | 错误 |
| `log_level_info` | 0 | `crates/engine/src/日志.rs:43` | — | 信息 |
| `log_level_warn` | 0 | `crates/engine/src/日志.rs:44` | — | 警告 |
| `log_transpile_cache_fallback` | 0 | `crates/cli/src/main.rs:2143` | — | 缓存转译失败，回退无缓存管线 |
| `log_transpile_done` | 3 | `crates/engine/src/lib.rs:139` | &源文本.len().to_string() / &输出产物.输出.len().to_string() / &已用 | 翻译完成（{} 字节 → {} 字节，耗时 {}） |
| `log_transpile_start` | 1 | `crates/engine/src/lib.rs:122` | &源文本.len().to_string() | 开始翻译（{} 字节） |
| `manifest_no_target` | 0 | `crates/cli/src/main.rs:1602` | — | Cargo.toml 没有声明编译目标，项目中也找不到 src/main.rs 或 src/lib.rs。请先在 src/main.rs 编写主程序，或在 Cargo.toml 中声明 [[bin]]/lib。 |
| `manifest_parse_error` | 2 | `crates/cli/src/main.rs:1596` | &路径值.display().to_string() / &错误值.to_string() | Cargo.toml 解析失败：{}（{}） |
| `no_available_ext` | 0 | `crates/cli/src/main.rs:2811` | — | 无 |
| `no_lang_installed` | 0 | `crates/cli/src/main.rs:1062` | — | 没有已安装的语言包。 |
| `no_project_root` | 1 | `crates/cli/src/main.rs:1364` | &源文件值.display().to_string() | 未找到项目根目录（向上未找到 Cargo.toml）。源文件: {}。请先使用 `rzc init` 创建项目。 |
| `not_removable` | 0 | `crates/cli/src/main.rs:1084` | — | （不可删除） |
| `panic_header` | 2 | `crates/cli/src/诊断输出.rs:397` | &显示名 / 定位文本 | 线程 '{}' 恐慌于 {}: |
| `panic_msg_borrow` | 0 | `crates/cli/src/诊断输出.rs:404` | — | 引用单元格已被借用 |
| `panic_msg_borrow_mut` | 0 | `crates/cli/src/诊断输出.rs:407` | — | 引用单元格已被可变借用 |
| `panic_msg_byte_end_oob` | 2 | `crates/cli/src/诊断输出.rs:434` | — | 切片终点字节索引 {} 超出字符串长度 {} |
| `panic_msg_byte_start_oob` | 2 | `crates/cli/src/诊断输出.rs:432` | — | 切片起点字节索引 {} 超出字符串长度 {} |
| `panic_msg_char_boundary` | 4 | `crates/cli/src/诊断输出.rs:443` | 索引段.trim() / 字符值 / 起点段.trim() / 终点段.trim() | 字节索引 {} 不是字符边界；它位于 '{}'（字节 {}..{}）内部 |
| `panic_msg_index_oob` | 2 | `crates/cli/src/诊断输出.rs:411` | 长度段.trim() / 索引段.trim() | 下标越界：长度是 {}，但下标是 {} |
| `panic_note` | 0 | `crates/cli/src/诊断输出.rs:350` | — | 提示：设置 RUST_BACKTRACE=1 环境变量可显示回溯 |
| `panic_thread_main` | 0 | `crates/cli/src/诊断输出.rs:393` | — | 主函数 |
| `project_created` | 1 | `crates/cli/src/main.rs:2775` | 项目名 | ✅ 项目 '{}' 创建成功！ |
| `project_created_hint` | 1 | `crates/cli/src/main.rs:2776` | 语言码 | 语言包已内置，无需手动复制。如需自定义，可创建 lang-packs/{}/ 目录覆盖。 |
| `project_run_hint` | 1 | `crates/cli/src/main.rs:2777` | 语言码 | 使用 `rzc run src/main.{}` 运行。 |
| `readme_template` | 2 | `crates/cli/src/main.rs:2758` | 项目名 / 语言码 | # {} 使用 `rzc run src/main.{}` 运行。 |
| `relocalize_done` | 2 | `crates/cli/src/main.rs:2118` | &计数.to_string() / &输出根.display().to_string() | 已反译重生 {} 个文件到：{} |
| `relocalize_no_module_entry` | 0 | `crates/cli/src/main.rs:2112` | — | 未在输出工程中发现入口文件（main / 主函数词），产物可能无法直接 rzc run / rzc check |
| `remote_install_failed` | 3 | `crates/cli/src/语言管理.rs:655` | 语言码 / &源列表.len().to_string() / &错误明细.join("\n") | 远程安装语言包 '{}' 失败，已依次尝试 {} 个源： {} 请检查网络连接；或通过环境变量 RZ_LANG_REPO 指定自定义仓库地址。 |
| `remote_lang_not_found` | 1 | `crates/cli/src/语言管理.rs:735`<br>`crates/cli/src/语言管理.rs:777` | 语言码 | 远程仓库中不存在语言包 '{}'。请检查语言代码，或使用 `rzc lang list` 查看已安装的语言包。 |
| `stale_products_pruned` | 1 | `crates/cli/src/main.rs:2503` | &已清理.len().to_string() | 已清理 {} 个失效生成文件（对应母语源文件已删除或改名） |
| `success_compile` | 0 | `crates/cli/src/诊断输出.rs:822`<br>`crates/cli/src/诊断输出.rs:858` | — | ✅ 编译成功，没有错误。 |
| `tag_builtin` | 0 | `crates/cli/src/main.rs:1071` | — | 内置 |
| `tag_user` | 0 | `crates/cli/src/main.rs:1072` | — | 用户安装 |
| `tc_doctor_help` | 0 | `crates/cli/src/main.rs:2993` | — | 诊断工具链环境：内置工具链 / PATH / 版本对比 |
| `tc_download_rustc` | 1 | `crates/cli/src/安装.rs:252` | &地址 | 正在下载 rustc/cargo（官方 standalone，体积较大，请耐心等待）... {} |
| `tc_extracting` | 0 | `crates/cli/src/安装.rs:274` | — | 正在解压并安装... |
| `tc_install_already` | 1 | `crates/cli/src/安装.rs:242` | &程序目录.display().to_string() | 内置工具链已安装（{}）；如需更新版本请使用 --force 重新安装 |
| `tc_install_done` | 1 | `crates/cli/src/安装.rs:297` | &程序目录.display().to_string() | 内置工具链安装完成：{} rzc 与 LSP 将优先使用内置工具链，不再依赖 rustup 与 PATH 配置 |
| `tc_install_help` | 0 | `crates/cli/src/main.rs:2977` | — | 一键安装内置工具链（standalone rustc/cargo/rust-analyzer，脱离 rustup） |
| `tc_ra_installed` | 1 | `crates/cli/src/安装.rs:284`<br>`crates/cli/src/安装.rs:313` | 分析器标签 | rust-analyzer {} 已安装 |
| `tc_ra_skipped` | 1 | `crates/cli/src/安装.rs:287` | &错误值.to_string() | rust-analyzer 下载失败（{}）；可稍后执行 rzc install lsp 或手动安装 |
| `template_main` | 0 | `crates/cli/src/main.rs:2754`<br>`crates/engine/src/指南.rs:265` | — | 函数 主函数() { 打印行!("你好，世界！"); } |
| `tool_prefix_caused_by` | 1 | `crates/cli/src/诊断输出.rs:549` | — | 原因：{} |
| `tool_prefix_help` | 1 | `crates/cli/src/诊断输出.rs:548` | — | 提示：{} |
| `tool_prefix_note` | 1 | `crates/cli/src/诊断输出.rs:547` | — | 说明：{} |
| `tool_prefix_warning` | 1 | `crates/cli/src/诊断输出.rs:546` | — | 警告：{} |
| `translation_gap_notice` | 2 | `crates/cli/src/诊断输出.rs:534` | &已记录.len().to_string() / &目录.display().to_string() | 提示：{} 条消息暂无译文，已按原文显示并记入待译清单：{}（可用 `rzc gaps` 查看） |
| `unicode_bidi_hint` | 1 | `crates/engine/src/混淆检测.rs:114` | &本地化字符名(码元, 名字) | {}。双向控制符可篡改代码的显示顺序（bidi 攻击），可能隐藏恶意代码 |
| `unicode_cat_bidi` | 0 | `crates/engine/src/混淆检测.rs:22` | — | 双向文本控制符 |
| `unicode_cat_homoglyph` | 0 | `crates/engine/src/混淆检测.rs:23` | — | 同形异义字符 |
| `unicode_cat_zero_width` | 0 | `crates/engine/src/混淆检测.rs:21` | — | 零宽字符 |
| `unicode_homoglyph_hint` | 2 | `crates/engine/src/混淆检测.rs:132` | &形似.to_string() / &本地化字符名(码元, 名字) | 形似拉丁字母 '{}'（{}）。可能被用于同形异义攻击，伪装标识符 |
| `unicode_warning_at` | 5 | `crates/engine/src/混淆检测.rs:51` | &self.行号.to_string() / &self.列号.to_string() / &self.类别.显示文字() / &码位 | 第 {} 行第 {} 列：检测到{} U+{}（{}） |
| `unicode_zero_width_hint` | 1 | `crates/engine/src/混淆检测.rs:100` | &本地化字符名(码元, 名字) | {}。此类字符肉眼不可见，可能被用于隐藏代码或绕过检测 |
| `unknown` | 0 | `crates/cli/src/main.rs:1078`<br>`crates/cli/src/main.rs:1081`<br>`crates/cli/src/映射检查.rs:62` | — | 未知 |
| `unknown_extension` | 2 | `crates/cli/src/main.rs:2821` | 扩展值 / &可用文本 | 无法识别源文件扩展名 '{}'。当前可用扩展名：{}。或通过 --lang-pack 指定语言包目录。 |
| `warn_builtin_errors_failed` | 1 | `crates/cli/src/诊断输出.rs:608` | &错误值.to_string() | 警告: 加载内置错误消息失败: {} |

## Keys with no static call site

These are resolved at runtime (prefix registered in the generator), so a plain
text search will not find them — they still must be translated.

| Key | Why static search misses it |
|---|---|
| `unicode_name_00AD` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0391` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0392` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0395` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0397` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0399` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_039A` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_039C` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_039D` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_039F` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03A1` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03A4` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03A5` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03A7` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03B1` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03B5` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03B9` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03BA` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03BC` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03BD` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03BF` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03C1` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03C4` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03C5` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03C7` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_03C9` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0410` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0412` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0415` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_041A` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_041D` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_041E` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0420` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0421` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0422` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0423` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0425` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0430` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0435` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_043E` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0440` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0441` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_0445` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_180E` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_200C` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_200D` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_200E` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_200F` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_202A` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_202B` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_202C` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_202D` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_202E` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_2060` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_2066` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_2067` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_2068` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_2069` | crates/engine/src/混淆检测.rs (format! by codepoint) |
| `unicode_name_FEFF` | crates/engine/src/混淆检测.rs (format! by codepoint) |

## Unused keys

No code path renders these. `tools/ui-keys-index.py --check` fails if the set
grows, so a newly added key has to be wired up in the same change.

```text
(none)
```
