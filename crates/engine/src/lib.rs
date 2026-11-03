//! i18n-rust 核心引擎
//! 提供多语言 Rust 方言的词法处理、映射管理、诊断翻译、增量缓存、安全检测等功能

pub mod alias;
pub mod diagnostic;
pub mod eject_hints;
pub mod fullwidth;
pub mod logger;
pub mod module_path;
pub mod product_layout;
pub mod unicode_confusion;
#[path = "列位映射.rs"]
pub mod 列位映射;
#[path = "反译重生.rs"]
pub mod 反译重生;
#[path = "名字缓存.rs"]
pub mod 名字缓存;
#[path = "工具链.rs"]
pub mod 工具链;
#[path = "指南.rs"]
pub mod 指南;
#[path = "故障.rs"]
pub mod 故障;
#[path = "映射源.rs"]
pub mod 映射源;
#[path = "映射管理器.rs"]
pub mod 映射管理器;
#[path = "校验.rs"]
pub mod 校验;
#[path = "界面消息.rs"]
pub mod 界面消息;
#[path = "缓存.rs"]
pub mod 缓存;
#[path = "词法器.rs"]
pub mod 词法器;
#[path = "语言.rs"]
pub mod 语言;

use std::borrow::Cow;
use std::collections::HashSet;
use std::time::Instant;

/// 生产级翻译入口：完整转译管线（词法转译 → 模块路径替换 → 别名替换）
///
/// 流程：
/// 1. 查询增量缓存（内容哈希 + 语境指纹），命中直接复用翻译结果与源映射；
/// 2. 未命中时先执行 Unicode 混淆安全检查（零宽/双向/同形字符），再执行转译；
/// 3. 转译结果写入缓存供后续复用。
///
/// 与命令行工具的管线顺序保持一致；日志级别由 `logger::init()` 读取
/// `RZ_LOG` 环境变量（debug/info/warn/error）控制。
pub fn transpile_source(
    source: &str,
    manager: &映射管理器::映射管理器,
    cache: &mut 缓存::转译缓存,
) -> Result<String, 故障::转译故障> {
    Ok(transpile_source_with_map(source, manager, cache)?.output)
}

/// 同 [`transpile_source`]，同时返回源映射（被替换标识符的源偏移与翻译前后文本）
pub fn transpile_source_with_map(
    source: &str,
    manager: &映射管理器::映射管理器,
    cache: &mut 缓存::转译缓存,
) -> Result<缓存::TranspileOutput, 故障::转译故障> {
    transpile_source_with_project(source, manager, cache, None)
}

/// 同 [`transpile_source_with_map`]，附项目级声明上下文（跨文件声明豁免）
///
/// 缓存语境指纹并入项目上下文指纹（[`alias::ProjectContext::fingerprint`]）：
/// 项目声明集合变化时相关缓存自动失效，无需语言包版本变化触发。
///
/// 教学告警与缓存状态无关：命中缓存只复用**转译产物**，源码里的魔法数字、全角
/// 标点、Unicode 混淆字符还在原地，每次调用都照旧提示。旧实现把告警挂在转译
/// 闭包里（只有未命中才跑），实测同一份含魔法数字的 `main.zh` 连跑两次
/// `rzc check`：第一次出「[lint] 第 5 行第 12 列：魔法数字「5」」，第二次一条
/// 告警都没有——学习者会读成「提示消失＝问题已修」。
pub fn transpile_source_with_project(
    source: &str,
    manager: &映射管理器::映射管理器,
    cache: &mut 缓存::转译缓存,
    project: Option<&alias::ProjectContext>,
) -> Result<缓存::TranspileOutput, 故障::转译故障> {
    transpile_source_cached_inner(source, manager, cache, project, true)
}

/// 同 [`transpile_source_with_project`]，但不输出教学告警
///
/// 供调用方自行负责告警与文件归属的场景使用（CLI 多文件项目按文件名前缀输出，
/// 见 [`emit_teaching_warnings`]）：除告警外，产物与缓存行为逐字一致。
pub fn transpile_source_with_project_quiet(
    source: &str,
    manager: &映射管理器::映射管理器,
    cache: &mut 缓存::转译缓存,
    project: Option<&alias::ProjectContext>,
) -> Result<缓存::TranspileOutput, 故障::转译故障> {
    transpile_source_cached_inner(source, manager, cache, project, false)
}

/// 带缓存转译入口的唯一实现：`emit` 控制本入口是否输出教学告警
fn transpile_source_cached_inner(
    source: &str,
    manager: &映射管理器::映射管理器,
    cache: &mut 缓存::转译缓存,
    project: Option<&alias::ProjectContext>,
    emit: bool,
) -> Result<缓存::TranspileOutput, 故障::转译故障> {
    logger::init();
    let start = Instant::now();
    crate::log_info!(
        "transpile_engine",
        "{}",
        crate::语言::f("log_transpile_start", &[&source.len().to_string()])
    );

    let fingerprint = 缓存::转译缓存::combine_fingerprint(
        manager.取语境指纹(),
        project.map(alias::ProjectContext::fingerprint),
    );

    // 闭包内保持静默：把告警挂在闭包里，等于把它绑死在「缓存未命中」这个条件上
    let output = cache.get_or_transpile(source, fingerprint, || {
        Ok(transpile_pipeline_inner(
            source, manager, None, project, false,
        ))
    })?;
    emit_teaching_diagnostics(source, manager, emit);

    let elapsed = format!("{:?}", start.elapsed());
    crate::log_info!(
        "transpile_engine",
        "{}",
        crate::语言::f(
            "log_transpile_done",
            &[
                &source.len().to_string(),
                &output.output.len().to_string(),
                &elapsed,
            ]
        )
    );
    Ok(output)
}

/// 无缓存的完整转译管线：Unicode 混淆检查 → 词法转译 → 模块路径替换 → 别名替换
///
/// CLI / LSP 等短命进程可直接复用此管线，无需维护增量缓存；
/// Unicode 告警经日志（warn 级）输出，不阻断转译。
/// 返回转译结果与源映射（被替换标识符的源偏移与翻译前后文本）。
pub fn transpile_pipeline(
    source: &str,
    manager: &映射管理器::映射管理器,
) -> 缓存::TranspileOutput {
    transpile_pipeline_inner(source, manager, None, None, true)
}

/// 静默转译管线：不输出教学告警（Unicode 混淆/全角标点/lint）
///
/// 供调用方自行负责教学告警输出的场景使用：
/// - “事后重放”（如诊断列映射重建）：此前已对同一源码执行过完整管线
///   （写盘转译）并输出过教学告警，重放只为取 `pipeline_map`；
/// - 调用方需要带文件归属输出告警（如 CLI 多文件项目按文件名前缀输出）。
///
/// 除教学告警外，输出与 [`transpile_pipeline`] 完全一致。
pub fn transpile_pipeline_quiet(
    source: &str,
    manager: &映射管理器::映射管理器,
) -> 缓存::TranspileOutput {
    transpile_pipeline_inner(source, manager, None, None, false)
}

/// 同 [`transpile_pipeline_quiet`]，附项目级声明上下文（跨文件声明豁免）
///
/// CLI 多文件项目中项目内文件与诊断重放的转译入口：项目上下文由调用方
/// 预先收集（见 [`alias::ProjectContext::from_sources`]），与入口文件共享。
pub fn transpile_pipeline_quiet_with_project(
    source: &str,
    manager: &映射管理器::映射管理器,
    project: Option<&alias::ProjectContext>,
) -> 缓存::TranspileOutput {
    transpile_pipeline_inner(source, manager, None, project, false)
}

/// 同 [`transpile_pipeline`]，附项目级声明上下文（跨文件声明豁免）
///
/// 供无缓存场景的调用方使用；带缓存的入口见 [`transpile_source_with_project`]。
pub fn transpile_pipeline_with_project(
    source: &str,
    manager: &映射管理器::映射管理器,
    project: Option<&alias::ProjectContext>,
) -> 缓存::TranspileOutput {
    transpile_pipeline_inner(source, manager, None, project, true)
}

/// 同 [`transpile_pipeline`]，支持 LSP 虚拟项目的 `crate::` 前缀重写
///
/// `module_names` 为 Some 时，在模块路径替换后、别名替换前，为已知模块路径段
/// 添加 `crate::` 前缀（跨文件引用，见 [`module_path::qualify_module_paths_with_map`]）。
/// CLI 等非虚拟项目场景传 None 保持原有行为。
///
/// 返回的 `pipeline_map` 以母语源偏移升序记录全部替换（replacement 为最终输出文本），
/// 是列映射等消费方的唯一规则来源——消费方无需复刻任何转译判定。
pub fn transpile_pipeline_with_map(
    source: &str,
    manager: &映射管理器::映射管理器,
    module_names: Option<&HashSet<String>>,
) -> 缓存::TranspileOutput {
    transpile_pipeline_inner(source, manager, module_names, None, true)
}

/// 同 [`transpile_pipeline_with_map`]，附项目级声明上下文（LSP 虚拟项目）
///
/// LSP 跨文件场景与 CLI 同源：项目上下文（模块名与声明名）与 `crate::`
/// 前缀重写共同保证跨文件引用的成员在调用侧与声明侧行为一致。
pub fn transpile_pipeline_with_map_and_project(
    source: &str,
    manager: &映射管理器::映射管理器,
    module_names: Option<&HashSet<String>>,
    project: Option<&alias::ProjectContext>,
) -> 缓存::TranspileOutput {
    transpile_pipeline_inner(source, manager, module_names, project, true)
}

/// 同 [`transpile_pipeline_with_map_and_project`]，项目上下文由调用方在词法
/// 阶段后补全（单趟：本文件声明不再单独预扫）
///
/// LSP 每次按键都要重建项目上下文：本文件内容已变，声明名按内容哈希缓存
/// 必然失效，而 [`transpile_pipeline_with_map_and_project`] 要求先有完整上下文
/// 才能转译——调用方只能先跑一整趟 `alias::collect_source_declarations`
///（内部含完整词法转译）取本文件声明，随后管线里的词法阶段再跑一遍。
///
/// 本入口把词法阶段提到回调之前执行，本文件声明从词法产物顺带收集，回调
/// 只需合并其他文件的缓存声明。输出与「预扫声明 → 建上下文 → 完整管线」
/// 逐字节一致（项目上下文按构造相同），另返回本文件声明与所用上下文，
/// 供调用方缓存与变更判定。
pub fn transpile_pipeline_with_deferred_project<F>(
    source: &str,
    manager: &映射管理器::映射管理器,
    module_names: Option<&HashSet<String>>,
    build_project: F,
) -> (
    缓存::TranspileOutput,
    alias::DeclaredNames,
    alias::ProjectContext,
)
where
    F: FnOnce(&alias::DeclaredNames) -> alias::ProjectContext,
{
    deferred_project_inner(source, manager, module_names, build_project, true)
}

/// 同 [`transpile_pipeline_with_deferred_project`]，但不输出教学告警
///
/// 供调用方自行负责教学告警输出的场景使用（LSP 把教学提示作为诊断发布，
/// 转译时的日志告警只是重复刷屏）：除告警外输出完全一致。
pub fn transpile_pipeline_with_deferred_project_quiet<F>(
    source: &str,
    manager: &映射管理器::映射管理器,
    module_names: Option<&HashSet<String>>,
    build_project: F,
) -> (
    缓存::TranspileOutput,
    alias::DeclaredNames,
    alias::ProjectContext,
)
where
    F: FnOnce(&alias::DeclaredNames) -> alias::ProjectContext,
{
    deferred_project_inner(source, manager, module_names, build_project, false)
}

/// [`transpile_pipeline_with_deferred_project`] 的唯一实现
fn deferred_project_inner<F>(
    source: &str,
    manager: &映射管理器::映射管理器,
    module_names: Option<&HashSet<String>>,
    build_project: F,
    emit_teaching_warnings: bool,
) -> (
    缓存::TranspileOutput,
    alias::DeclaredNames,
    alias::ProjectContext,
)
where
    F: FnOnce(&alias::DeclaredNames) -> alias::ProjectContext,
{
    emit_teaching_diagnostics(source, manager, emit_teaching_warnings);
    let (lex, input_tokens) = transpile_lex_stage(source, manager);
    // 词法产物要分词一次才收得了声明（建项目上下文的前置）；use 路径阶段
    // 恰好也在这份文本上跑，于是这份 token 流一路交给后续阶段复用。
    // 它不必重新分词：词法阶段自己那份输入 token 流按本阶段编辑表推进就是它
    let lex_tokens = derive_lex_tokens(&input_tokens, &lex);
    // 词法产物上收集本文件声明：与 `alias::collect_source_declarations` 同源
    // （后者正是「词法转译 + 收集」），仅少一次重复的转译
    let declared = alias::collect_declared_names_in(&lex.output, manager.取别名表(), &lex_tokens);
    let project = build_project(&declared);
    let output = transpile_after_lex(
        source,
        manager,
        module_names,
        Some(&project),
        Some(LexReuse {
            declared: Some(&declared),
            tokens: &lex_tokens,
        }),
        lex,
    );
    (output, declared, project)
}

/// 同 [`transpile_pipeline_with_map_and_project`]，但不输出教学告警
///
/// 与 [`transpile_pipeline_quiet`] 同口径：除教学告警外输出完全一致。
pub fn transpile_pipeline_quiet_with_map_and_project(
    source: &str,
    manager: &映射管理器::映射管理器,
    module_names: Option<&HashSet<String>>,
    project: Option<&alias::ProjectContext>,
) -> 缓存::TranspileOutput {
    transpile_pipeline_inner(source, manager, module_names, project, false)
}

/// 转译管线的唯一实现：`emit_teaching_warnings` 控制教学告警日志的开关
fn transpile_pipeline_inner(
    source: &str,
    manager: &映射管理器::映射管理器,
    module_names: Option<&HashSet<String>>,
    project: Option<&alias::ProjectContext>,
    emit_teaching_warnings: bool,
) -> 缓存::TranspileOutput {
    emit_teaching_diagnostics(source, manager, emit_teaching_warnings);
    let (lex, input_tokens) = transpile_lex_stage(source, manager);
    // 词法产物的 token 流由词法阶段自己那份派生（见 [`derive_lex_tokens`]），
    // 交给 use 路径与别名两个阶段共用：预计算入口（CLI 整棵编译）同样只付
    // 一整趟分词。声明名不在本入口预算（调用方已带项目上下文），传 `None`。
    let lex_tokens = derive_lex_tokens(&input_tokens, &lex);
    transpile_after_lex(
        source,
        manager,
        module_names,
        project,
        Some(LexReuse {
            declared: None,
            tokens: &lex_tokens,
        }),
        lex,
    )
}

/// 教学告警（Unicode 混淆 / 全角标点 / 教学 lint）：仅告警不阻断
///
/// `label` 为 Some 时每条告警带文件归属前缀（`标签：正文`）：多文件项目里裸
/// 行列定位不到具体文件，按文件输出的调用方（CLI）传自己的显示名；None 是管线
/// 内部出口（入口文件语境，裸行列即命令传入的那个文件）。
///
/// 本函数是教学扫描集合的**唯一**事实来源：调用方不要再各自拼一遍扫描器。历史
/// 上 CLI 侧那份少调了英文回落与保留字撞名两类，入口文件会提示而兄弟文件不会，
/// 同一份代码换条路径就漏报（见 `lint_teaching_all_chars`）。
pub fn emit_teaching_warnings(
    source: &str, manager: &映射管理器::映射管理器, label: Option<&str>
) {
    // 归属前缀只在有标签时拼一次：None 走原样，不额外分配
    let owned = |text: String| match label {
        Some(name) => format!("{name}：{text}"),
        None => text,
    };
    // 词法处理前的 Unicode 混淆安全检查（零宽/双向/同形字符，仅告警）
    for warning in unicode_confusion::check_unicode_confusion(source) {
        crate::log_warn!("unicode_confusion", "{}", owned(warning.format()));
    }
    // 全角标点教学检查（中文输入法常见错误：代码位置的全角标点，仅告警不阻断）；
    // 字符串/注释内的全角标点合法，由扫描器自动跳过
    //
    // 以下四个扫描器共用同一份字符切片：源码只物化一次，避免各扫一遍
    let chars: Vec<char> = source.chars().collect();
    for warning in fullwidth::find_fullwidth_punct_chars(&chars) {
        crate::log_warn!("fullwidth", "{}", owned(warning.format()));
    }
    // 教学 lint（初学者代码风格提示，仅告警不阻断）：未标注类型/魔法数字/嵌套过深/易混方法名。
    // CLI `--no-lint` 可关闭（校验::set_teaching_lint_enabled），
    // 供项目开发（非教学）场景静默刷屏提示
    if 校验::teaching_lint_enabled() {
        // 一把算全部教学提示：共用同一份字符切片，英文回落（母语词条却写成
        // `fn`/`Vec`/`.collect()` 等）与保留字撞名（`函数 匹配` → `fn match`）
        // 在同一次遍历里判定，见 `校验::lint_teaching_all`；易混词表的分桶索引
        // 由 manager 缓存（每语言包一次，不是每文件一次）
        for warning in 校验::lint_teaching_all_chars(
            &chars,
            校验::WordIndexSource::Shared(manager.取校验分桶()),
            manager.取歧义构造器(),
            manager.取英文回落(),
            manager.取关键词表(),
        ) {
            crate::log_warn!("lint", "{}", owned(warning.format()));
        }
    }
}

/// 管线内部出口：`emit` 控制是否输出（不带文件归属）
fn emit_teaching_diagnostics(source: &str, manager: &映射管理器::映射管理器, emit: bool) {
    if emit {
        emit_teaching_warnings(source, manager, None);
    }
}

/// 词法阶段（关键字/宏/派生/让位词映射）
///
/// 另返回本阶段自己分好的输入 token 流（它本来就要分一次）：词法产物的
/// token 流可据此派生，管线不必再对输出整份重新分词。
fn transpile_lex_stage(
    source: &str,
    manager: &映射管理器::映射管理器,
) -> (词法器::TranspileResult, Vec<rustc_lexer::Token>) {
    词法器::transpile_with_map_tokens(
        source,
        manager.取关键词表(),
        manager.取宏表(),
        manager.取派生特征表(),
        manager.取让位词(),
        manager.取方法让位词(),
        manager.取别名表(),
    )
}

/// 词法产物的分词结果（各阶段共用一份 token 流时的载体）
///
/// 分词口径与各阶段自己收集的完全一致（`with_capacity(len/4+8)` + `tokenize`）。
fn collect_tokens(source: &str) -> Vec<rustc_lexer::Token> {
    let mut tokens: Vec<rustc_lexer::Token> = Vec::with_capacity(source.len() / 4 + 8);
    tokens.extend(rustc_lexer::tokenize(source));
    tokens
}

/// 按某阶段的编辑表把 token 流推进到该阶段输出的坐标
///
/// 编辑都是**整段覆盖若干相邻 token** 的替换（词法阶段改标识符 token、use 路径
/// 改 `模块词` 整个 token、数字后缀黏连时一条编辑跨两个 token），`kind` 序列
/// 只取决于替换文本。因此「把被覆盖的 token 换成替换值自己的 token」就得到
/// 该阶段输出的 token 流，不必把整份文件重新分词（359 / 2000 行样例分别
/// 实测 189 / 421 µs）。替换值本身仍要分词（它可能含 `::`，如 `标准集合` →
/// `std::collections`，也可能含补出来的 `!`，如 `打印行` → `println!`），但每处
/// 只有几个 token，且替换值原样占据原文置，前后邻接关系不变，所以单独分词与
/// 整份分词的结果同构（该假设由单测、语料差分测试与 debug 构建下的自检守住）。
///
/// 编辑起点不在 token 边界、终点落进 token 内部、或编辑表没走完时返回 `None`：
/// 调用方退回现场分词，宁慢不错。`源码映射条目` 按匹配位置升序发出，故与
/// token 同序遍历一次即可。
fn rebase_tokens(
    base: &[rustc_lexer::Token],
    edits: &[缓存::源码映射条目],
) -> Option<Vec<rustc_lexer::Token>> {
    let mut out: Vec<rustc_lexer::Token> = Vec::with_capacity(base.len());
    let mut index = 0usize;
    let mut offset = 0usize;
    let mut next_edit = 0usize;
    while index < base.len() {
        let token = &base[index];
        // 编辑起点落在本 token 内部（如数字字面量的后缀被单独改写：`1024整数`
        // 是一个 Literal token，后缀 `整数` 是其尾部字节）：整段对齐前提已破，
        // 立刻放弃派生，不要跑完全流才发现。
        if let Some(edit) = edits.get(next_edit)
            && edit.源偏移 < offset
        {
            return None;
        }
        if let Some(edit) = edits.get(next_edit)
            && edit.源偏移 == offset
        {
            // 同一起点的编辑必然从本 token 起整段覆盖：往后吃满 `length`
            let end = offset + edit.length;
            let mut covered = index;
            let mut cursor = offset;
            while cursor < end {
                let spanned = base.get(covered)?;
                cursor += spanned.len;
                covered += 1;
            }
            if cursor != end {
                return None;
            }
            let replacement = edit.替换文字.as_str();
            let before = out.len();
            for part in rustc_lexer::tokenize(replacement) {
                out.push(rustc_lexer::Token {
                    kind: part.kind,
                    len: part.len,
                });
            }
            // 空替换（把 token 抹成空白）无从对齐，退回现场分词
            if out.len() == before {
                return None;
            }
            next_edit += 1;
            index = covered;
            offset = cursor;
        } else {
            out.push(rustc_lexer::Token {
                kind: token.kind,
                len: token.len,
            });
            offset += token.len;
            index += 1;
        }
    }
    (next_edit == edits.len()).then_some(out)
}

/// 由词法阶段的输入 token 流派生 `lex.output` 的 token 流
///
/// 派生只在编辑与 token 边界对齐时成立（见 [`rebase_tokens`]）；对不齐时退回
/// 现场分词。debug 构建下额外把派生结果与整份重新分词逐 token 对比，不一致
/// 即崩——那意味着某个替换值与相邻字符合并成了不同的 token 序列（宁可开发期
/// 吵，也不让错位的偏移表静默流入产物）。
fn derive_lex_tokens(
    input_tokens: &[rustc_lexer::Token],
    lex: &词法器::TranspileResult,
) -> Vec<rustc_lexer::Token> {
    let derived = rebase_tokens(input_tokens, &lex.final_edits);
    #[cfg(debug_assertions)]
    if let Some(tokens) = &derived {
        let fresh = collect_tokens(&lex.output);
        debug_assert!(
            tokens
                .iter()
                .zip(&fresh)
                .all(|(a, b)| a.kind == b.kind && a.len == b.len)
                && tokens.len() == fresh.len(),
            "词法 token 派生与重新分词不一致：派生 {} token / 重分 {} token",
            tokens.len(),
            fresh.len()
        );
    }
    derived.unwrap_or_else(|| collect_tokens(&lex.output))
}

/// 延迟入口与预计算入口在词法产物上已经算好的成果，交给后续阶段复用
///
/// 两者都以 `lex.output` 为输入：那份分词流（use 路径阶段直接借用，别名阶段
/// 借用由它再派生的一份）与声明名（只有延迟入口会先收集，它必须先把声明
/// 汇总成项目上下文才能继续；预计算入口的上下文由调用方备好，声明名由别名
/// 阶段自己现场收集）。缺哪个就由哪个阶段自己补上。
struct LexReuse<'a> {
    declared: Option<&'a alias::DeclaredNames>,
    tokens: &'a [rustc_lexer::Token],
}

/// 词法之后的管线：use 路径替换 → `crate::` 前缀 → 别名替换 → 编辑表合并
///
/// `reuse` 为词法产物上已算好的声明名与 token 流（延迟项目上下文入口必然
/// 先算过）：传入时 use 路径阶段与别名阶段都跳过重复的分词/状态机；`None`
/// 时由各阶段自行完成。两条路径输出逐字节一致（见 `tests/alias_declared_
/// reuse.rs` 与 `module_path` 的差分测试）。
fn transpile_after_lex(
    source: &str,
    manager: &映射管理器::映射管理器,
    module_names: Option<&HashSet<String>>,
    project: Option<&alias::ProjectContext>,
    reuse: Option<LexReuse<'_>>,
    lex: 词法器::TranspileResult,
) -> 缓存::TranspileOutput {
    // 阶段 2：use 语句路径替换
    //
    // 各阶段输出用 Cow 串起来：阶段未启用时直接借用上一阶段输出，不再整份
    // clone——早先三个阶段各 clone 一次全文（2000 行文件即三次全文拷贝）
    let lex_output = lex.output;
    // 调用方已分好词时直接借用那份 token 流
    let reuse_tokens = reuse.as_ref().map(|r| r.tokens);
    let (mp_out, mp_edits): (Cow<'_, str>, Vec<缓存::源码映射条目>) = if manager
        .模块路径表
        .is_empty()
    {
        (Cow::Borrowed(lex_output.as_str()), Vec::new())
    } else {
        let replaced = match reuse_tokens {
            Some(tokens) => module_path::replace_module_paths_in_tokens(
                &lex_output,
                manager.取模块路径表(),
                tokens,
            ),
            None => module_path::replace_module_paths_with_map(&lex_output, manager.取模块路径表()),
        };
        (Cow::Owned(replaced.output), replaced.edits)
    };
    // 阶段 3（可选）：`crate::` 前缀重写（LSP 虚拟项目跨文件引用）；
    // 项目项名集合供 glob 导入场景的遮蔽豁免（无项目上下文时传空集）
    let (qual_out, qual_edits): (Cow<'_, str>, Vec<缓存::源码映射条目>) = if let Some(names) =
        module_names
        && !names.is_empty()
    {
        let empty_items = HashSet::new();
        let item_names = project.map(|p| &p.items).unwrap_or(&empty_items);
        let qualified =
            module_path::qualify_module_paths_with_map(mp_out.as_ref(), names, item_names);
        (Cow::Owned(qualified.output), qualified.edits)
    } else {
        (Cow::Borrowed(mp_out.as_ref()), Vec::new())
    };
    // 阶段 4 的输入是 `qual_out`：它的 token 流可由词法产物那份派生——use 路径
    // 替换只改标识符 token 的长度，`crate::` 前缀阶段真插入过文本时无从派生
    //（token 数量变了），退回现场分词。
    let rebased_tokens;
    let alias_tokens: Option<&[rustc_lexer::Token]> = match reuse_tokens {
        Some(base) if qual_edits.is_empty() && mp_edits.is_empty() => Some(base),
        Some(base) if qual_edits.is_empty() => {
            rebased_tokens = rebase_tokens(base, &mp_edits);
            rebased_tokens.as_deref()
        }
        _ => None,
    };
    // 阶段 4：标识符别名替换（声明位保护 + 项目级声明上下文）；
    // 附模块路径表：use 之外的路径根位模块词（`异步运行时::睡眠`、
    // `#[异步运行时::主函数]`）在别名阶段回退模块路径表完成转译
    let (al_out, al_edits): (Cow<'_, str>, Vec<缓存::源码映射条目>) =
        if manager.标识符别名.is_empty() && manager.模块路径表.is_empty() {
            (Cow::Borrowed(qual_out.as_ref()), Vec::new())
        } else {
            let aliased = alias::replace_aliases_reusing(
                qual_out.as_ref(),
                manager.取别名表(),
                manager.取模块路径表(),
                project,
                reuse.as_ref().and_then(|r| r.declared),
                alias_tokens,
            );
            (Cow::Owned(aliased.output), aliased.edits)
        };

    // 组合各阶段编辑表为母语源坐标的全管线地图（replacement 取最终输出文本）
    // 词法阶段编辑按所有权传入：它们已是母语源坐标，无需夹边界时可原样移动
    let later: Vec<&[缓存::源码映射条目]> = vec![&mp_edits, &qual_edits, &al_edits];
    let pipeline_map = compose_pipeline_map(source, lex.final_edits, &later);

    缓存::TranspileOutput::with_full_map(al_out.into_owned(), lex.source_map, pipeline_map)
}

/// 合并各阶段编辑表为母语源坐标的最终编辑地图
///
/// `lex_edits`（词法阶段）的编辑已直接以母语源偏移记录，且按所有权传入；
/// `later` 中的各阶段编辑以各自输入文本偏移记录，需经逆映射链换算回母语源
/// 偏移。`original/length` 一律以母语源文本为准（后续阶段替换的 token 未被
/// 前置阶段改写，长度一致）。
/// 同一源偏移出现多条编辑时保留最后阶段（后写覆盖先写，按阶段顺序稳定排序）。
fn compose_pipeline_map(
    source: &str,
    lex_edits: Vec<缓存::源码映射条目>,
    later: &[&[缓存::源码映射条目]],
) -> Vec<缓存::源码映射条目> {
    // 先为后续阶段算好母语源偏移（逆映射要读前面的全部阶段，含词法阶段；
    // 算完后词法编辑才可被移动消费）
    //
    // 换算走 [`ReverseCursor`] 而非逐条从头扫描：某阶段的编辑按输入偏移升序，
    // 而逆映射单调（替换区间内一律返回 token 起点），故一条查询的答案不会小于
    // 前一条——游标只前进不回退，整段换算从 O(该阶段编辑数 × 前置阶段编辑数)
    // 降到 O(两者之和)。逐条重扫在大文件上是实测的平方项：2246 行、别名密集
    //（词法 2528 条 + 别名 1683 条）的语料冷转译 37.4 ms，同规模只改关键字的
    // 语料 3.26 ms；行数翻倍时间 ×3.9。与旧实现逐偏移等价
    //（见 `test_reverse_cursor_matches_naive`）。
    let mut later_offsets: Vec<Vec<usize>> = Vec::with_capacity(later.len());
    for (stage_idx, edits) in later.iter().enumerate() {
        let mut offsets = Vec::with_capacity(edits.len());
        if edits.is_empty() {
            later_offsets.push(offsets);
            continue;
        }
        // 换算链：先夹更后的阶段（后写覆盖先写，逆序回溯），最后夹词法阶段
        let mut chain: Vec<ReverseCursor<'_>> = later[..stage_idx]
            .iter()
            .rev()
            .copied()
            .map(ReverseCursor::new)
            .collect();
        let mut lex_cursor = ReverseCursor::new(&lex_edits);
        for e in edits.iter() {
            let mut off = e.源偏移;
            for cursor in &mut chain {
                off = cursor.map(off);
            }
            offsets.push(lex_cursor.map(off));
        }
        later_offsets.push(offsets);
    }

    let total = lex_edits.len() + later.iter().map(|e| e.len()).sum::<usize>();
    let mut merged: Vec<缓存::源码映射条目> = Vec::with_capacity(total);
    // 词法阶段：坐标已就位，边界无需调整时直接移动条目（每条省下两次文本
    // 拷贝——359 行样例 456 条编辑，这张表是管线里最大的一份）
    for e in lex_edits {
        let end = e.源偏移 + e.length;
        let 边界无需调整 = end <= source.len()
            && source.is_char_boundary(e.源偏移)
            && source.is_char_boundary(end)
            && &source[e.源偏移..end] == e.original.as_str();
        if 边界无需调整 {
            merged.push(e);
            continue;
        }
        let (start, end) = clamp_to_char_boundaries(source, e.源偏移, e.length);
        merged.push(缓存::源码映射条目::new(
            e.源偏移,
            e.length,
            &source[start..end],
            e.替换文字.as_str(),
        ));
    }

    for (stage_idx, edits) in later.iter().enumerate() {
        for (e, &zh_offset) in edits.iter().zip(later_offsets[stage_idx].iter()) {
            // original/length 以母语源文本为准。
            // 注意：后续阶段的 e.length 处于该阶段输入坐标，未必等于母语源坐标长度；
            // 这里把起止偏移都夹到母语源文本的合法字符边界，避免多字节字符（如日本語の
            // メソッド名 `から`）切片越界引发 panic。transpile 输出本身不受影响。
            let (start, end) = clamp_to_char_boundaries(source, zh_offset, e.length);
            merged.push(缓存::源码映射条目::new(
                zh_offset,
                e.length,
                &source[start..end],
                e.替换文字.as_str(),
            ));
        }
    }
    // 稳定排序：同偏移多条编辑时，阶段靠后的保持在后（最终消费取后者）
    merged.sort_by_key(|e| e.源偏移);
    // 同偏移去重：保留最后一条（阶段靠后的替换即为最终输出文本）
    let mut deduped: Vec<缓存::源码映射条目> = Vec::with_capacity(merged.len());
    for e in merged {
        if let Some(last) = deduped.last_mut()
            && last.源偏移 == e.源偏移
        {
            *last = e;
        } else {
            deduped.push(e);
        }
    }
    deduped
}

/// 把编辑区间夹到母语源文本的合法字符边界
///
/// 返回 `(start, end)`：`start` 向后推进到字符边界、`end` 向前回退到字符边界
/// （不越过 `start`），两者都不超文本长度。后续阶段的 `length` 处于该阶段坐标，
/// 未必对齐母语源的字符边界（如日本語のメソッド名 `から`），直接切片会 panic。
fn clamp_to_char_boundaries(source: &str, offset: usize, length: usize) -> (usize, usize) {
    let src_len = source.len();
    let mut start = offset.min(src_len);
    while start < src_len && !source.is_char_boundary(start) {
        start += 1;
    }
    let mut end = (offset + length).min(src_len);
    while end > start && !source.is_char_boundary(end) {
        end -= 1;
    }
    (start, end)
}

/// 逆映射：把"上一阶段输出坐标"的偏移映射回"上一阶段输入坐标"
///
/// edits 按 input_offset 升序、互不重叠（每个被替换 token 一条）。
/// 落在替换区间内的偏移返回 token 起点（token 级边界近似——列映射在
/// token 起点记录分段点，区间内位置近似映射到起点即可）。
///
/// **仅供测试参照**：生产路径按升序批量查询，用 [`ReverseCursor`] 保住游标，
/// 不必每次从表头重扫（后者是实测的平方项）。保留这份朴素实现就是为了
/// 能对着一模一样的语义做差分（`test_reverse_cursor_matches_naive`）。
#[cfg(test)]
fn inverse_map_offset(edits: &[缓存::源码映射条目], out_offset: usize) -> usize {
    let mut in_cursor = 0usize;
    let mut out_cursor = 0usize;
    for e in edits {
        let gap = e.源偏移 - in_cursor; // 输入侧未替换区间
        if out_offset <= out_cursor + gap {
            return in_cursor + (out_offset - out_cursor);
        }
        out_cursor += gap;
        if out_offset < out_cursor + e.替换文字.len() {
            return e.源偏移; // 替换区间内：token 起点近似
        }
        out_cursor += e.替换文字.len();
        in_cursor = e.源偏移 + e.length;
    }
    in_cursor + (out_offset - out_cursor)
}

/// 逆映射游标：对**非递减**的查询偏移序列，只前进一趟就把全部答案换算完
///
/// 与 [`inverse_map_offset`]（逐条从头扫描）逐偏移等价，但省掉重复扫描：整段
/// 查询 O(编辑数 + 查询数) 而不是 O(编辑数 × 查询数）。前提由调用方保证：
/// 各阶段编辑表按输入偏移升序，且映射本身单调（区间内一律返回 token 起点，
/// 不会把后面的查询映到比前面的答案更小的偏移）。debug 构建下断言序列确实
/// 非递减——前提破了就是错乱偏移流进产物，宁可在开发期吵。
pub(crate) struct ReverseCursor<'a> {
    /// 前置阶段的编辑表（按输入偏移升序、互不重叠）
    edits: &'a [缓存::源码映射条目],
    /// 下一个尚未跨过的编辑
    index: usize,
    /// `edits[index]` 起点两侧的偏移：输入侧与输出侧
    in_offset: usize,
    out_offset: usize,
    /// 上一次查询的偏移（非递减前提的自检）
    last: usize,
}

impl<'a> ReverseCursor<'a> {
    /// 新游标：停在文本起点，尚未消费任何编辑
    pub(crate) fn new(edits: &'a [缓存::源码映射条目]) -> Self {
        Self {
            edits,
            index: 0,
            in_offset: 0,
            out_offset: 0,
            last: 0,
        }
    }

    /// 查询一个输出侧偏移对应的输入侧偏移（查询序列必须非递减）
    pub(crate) fn map(&mut self, out: usize) -> usize {
        debug_assert!(
            out >= self.last,
            "逆映射游标的查询偏移必须非递减：本次 {out} < 上次 {}",
            self.last
        );
        self.last = out;
        while self.index < self.edits.len() {
            let e = &self.edits[self.index];
            // 未替换区间 [in_offset, e.源偏移)：输出侧等长、逐字节对应
            let gap_end = self.out_offset + (e.源偏移 - self.in_offset);
            if out <= gap_end {
                return self.in_offset + (out - self.out_offset);
            }
            // 替换区间内：一律映到 token 起点（与旧实现同一近似）；
            // 游标不前进——后续查询可能还落在同一条替换里
            if out < gap_end + e.替换文字.len() {
                return e.源偏移;
            }
            // 整条跨过，游标推进到下一条编辑之前
            self.out_offset = gap_end + e.替换文字.len();
            self.in_offset = e.源偏移 + e.length;
            self.index += 1;
        }
        // 表已走完：尾部未替换区间逐字节对应
        self.in_offset + (out - self.out_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_manager() -> 映射管理器::映射管理器 {
        let keywords_toml = r#"
["声明"]
"函数" = "fn"
"让" = "let"
"枚举" = "enum"
["类型"]
"整数" = "i32"
["宏"]
"打印行" = "println"
"文件" = "file"
["标准库成员"]
"迭代" = "iter"
"#;
        let module_paths_toml = r#"
["模块路径"]
"标准集合" = "std::collections"
"#;
        let stdlib_toml = r#"
["模块路径"]
"线程" = "std::thread"
["标识符"]
"字符串" = "String"
"枚举" = "enumerate"
"#;
        let third_party_data = [(
            "测试库.toml",
            r#"
["模块路径"]
"网络库" = "netlib"
["标识符"]
"服务器" = "Server"
"#,
        )];
        映射管理器::映射管理器::从内置载入(
            keywords_toml,
            module_paths_toml,
            stdlib_toml,
            &third_party_data,
        )
        .expect("创建测试管理器失败")
    }

    /// `rebase_tokens` 派生出的 token 流必须与对阶段输出重新分词的结果逐项相同
    ///
    /// 派生只改被替换 token 的长度（编辑整 token 对齐、替换值是单个标识符）。
    /// 这里用真实模块路径替换产生编辑，再把派生结果与 `tokenize(输出)` 对拍：
    /// `kind` 与 `len` 任一不同都说明同构假设不成立。
    #[test]
    fn test_rebase_tokens_matches_fresh_tokenize() {
        let manager = create_manager();
        let path_map = manager.取模块路径表();
        for src in [
            "使用 标准集合::哈希映射;\n",
            "使用 标准集合 :: 哈希映射;\n",
            "使用 线程::生成;\n使用 标准集合::双向队列;\n",
            "使用 网络库 as 网络;\nfn f() { 标准集合::新建(); }\n",
            "// 使用 标准集合::哈希映射;\nfn f() {}\n",
            "fn f() { let x = 1; }\n",
            "",
        ] {
            let base = collect_tokens(src);
            let stage = module_path::replace_module_paths_with_map(src, path_map);
            let Some(rebased) = rebase_tokens(&base, &stage.edits) else {
                panic!("{src:?}: 单个标识符替换应能派生 token 流");
            };
            let fresh = collect_tokens(&stage.output);
            assert_eq!(rebased.len(), fresh.len(), "{src:?}: token 数不一致");
            for (i, (a, b)) in rebased.iter().zip(&fresh).enumerate() {
                assert_eq!(a.kind, b.kind, "{src:?}: 第 {i} 个 token 的 kind");
                assert_eq!(a.len, b.len, "{src:?}: 第 {i} 个 token 的长度");
            }
        }
    }

    /// 替换值含 `::` 时派生会把替换值再分词，得到的仍是同构的 token 流
    #[test]
    fn test_rebase_tokens_splits_multi_token_replacement() {
        let src = "标准集合::哈希映射;";
        let base = collect_tokens(src);
        let edits = vec![缓存::源码映射条目::new(
            0,
            "标准集合".len(),
            "标准集合",
            "std::collections",
        )];
        let rebased = rebase_tokens(&base, &edits).expect("应能派生");
        let fresh = collect_tokens("std::collections::哈希映射;");
        assert_eq!(rebased.len(), fresh.len(), "token 数应与重新分词一致");
        for (i, (a, b)) in rebased.iter().zip(&fresh).enumerate() {
            assert_eq!(a.kind, b.kind, "第 {i} 个 token 的 kind");
            assert_eq!(a.len, b.len, "第 {i} 个 token 的长度");
        }
    }

    /// 编辑未落在 token 边界时派生失败（不能默默错位）
    #[test]
    fn test_rebase_tokens_rejects_misaligned_edit() {
        let src = "标准集合::哈希映射;";
        let base = collect_tokens(src);
        // token 起点：0 / 12 / 13 / 14 / 26；其余偏移都落在某个 token 内部
        for offset in [1usize, 3, 15, 20, 25] {
            let edits = vec![缓存::源码映射条目::new(offset, 1, "x", "y")];
            assert!(
                rebase_tokens(&base, &edits).is_none(),
                "偏移 {offset} 不在 token 边界，应拒绝"
            );
        }
    }

    /// 一条编辑跨过多个 token 时同样可派生（被覆盖的 token 整段消失）
    ///
    /// 词法阶段的数字后缀黏连（`1024整数`）就是这么一条编辑；这里用
    /// `标准集合::` → `std::`（标识符 + 两个冒号）固定该形态。
    #[test]
    fn test_rebase_tokens_spans_multi_token_edit() {
        let src = "标准集合::哈希映射;";
        let base = collect_tokens(src);
        let edits = vec![缓存::源码映射条目::new(
            0,
            "标准集合::".len(),
            "标准集合::",
            "std::",
        )];
        let rebased = rebase_tokens(&base, &edits).expect("跨 token 编辑应能派生");
        let fresh = collect_tokens("std::哈希映射;");
        assert_eq!(rebased.len(), fresh.len(), "token 数应与重新分词一致");
        for (i, (a, b)) in rebased.iter().zip(&fresh).enumerate() {
            assert_eq!(a.kind, b.kind, "第 {i} 个 token 的 kind");
            assert_eq!(a.len, b.len, "第 {i} 个 token 的长度");
        }
    }

    /// 词法产物的 token 流可由「词法阶段输入 token + 词法编辑表」派生
    ///
    /// 管线里最贵的一趟重复分词就在这一句上（2000 行样例实测 421 µs），所以
    /// 同构假设必须直接锁住：逐形态比 `derive_lex_tokens` 与 `tokenize(输出)`。
    /// 形态涵盖宏补 `!`、`!` 邻接、数字后缀黏连、生命周期、字面量、跨行注释、
    /// use 路径与表达式里的模块词、空文件。
    #[test]
    fn test_derive_lex_tokens_matches_fresh_tokenize() {
        let manager = create_manager();
        for src in [
            "打印行!(\"x\");\n",
            "打印行(\"x\");\n",
            "如果 标志 != 打印行(1) {}\n",
            "让 a = 文件!(1); 让 b = 文件(2);\n",
            "函数 f<'a>(x: &'a 整数) -> 整数 { *x }\n",
            "让 c = '中'; 让 b = b'x'; 让 s = \"含 打印行 的串\"; 让 r = r#\"原始\"#;\n",
            "/* 块\n注释 */ 让 x = 打印行(1);\n",
            "使用 标准集合::哈希映射;\n让 m = 标准集合::新建();\n",
            "标准集合::哈希映射::新建();\n",
            "函数 主函数() { 让 可变 数量: 整数 = 5; }\n",
            "枚举 E { A, B }\n",
            "让 x = 1..=2; 让 y = -1;\n",
            "让 r#函数 = 1;\n",
            "// 只有注释\n",
            "",
        ] {
            let (lex, input_tokens) = transpile_lex_stage(src, &manager);
            // 派生必须真的成立（否则本测试只是「退回现场分词」的同义反复）
            assert!(
                rebase_tokens(&input_tokens, &lex.final_edits).is_some(),
                "{src:?}: 未能由输入 token 派生"
            );
            let derived = derive_lex_tokens(&input_tokens, &lex);
            let fresh = collect_tokens(&lex.output);
            assert_eq!(derived.len(), fresh.len(), "{src:?}: token 数不一致");
            for (i, (a, b)) in derived.iter().zip(&fresh).enumerate() {
                assert_eq!(a.kind, b.kind, "{src:?}: 第 {i} 个 token 的 kind");
                assert_eq!(a.len, b.len, "{src:?}: 第 {i} 个 token 的长度");
            }
        }
    }

    /// 真 zh 语言包（而非上面那份小映射表）：替换点越密，派生越容易暴露问题
    fn zh_manager() -> 映射管理器::映射管理器 {
        use std::path::Path;

        映射管理器::映射管理器::从目录载入(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh"),
        )
        .expect("加载 zh 语言包失败")
    }

    /// 后缀与数字黏连时不派生（编辑落在 token 内部），但退回路径仍须同构
    #[test]
    fn test_derive_lex_tokens_falls_back_on_partial_edit() {
        let manager = zh_manager();
        for src in [
            "让 n = 1024整数;\n",
            "让 n = 1024无符号机器整数;\n",
            "让 x = 3整数;\n",
        ] {
            let (lex, input_tokens) = transpile_lex_stage(src, &manager);
            assert!(
                rebase_tokens(&input_tokens, &lex.final_edits).is_none(),
                "{src:?}: 编辑落在 token 内部，应拒绝派生"
            );
            let derived = derive_lex_tokens(&input_tokens, &lex);
            let fresh = collect_tokens(&lex.output);
            assert_eq!(derived.len(), fresh.len(), "{src:?}: 退回路径 token 数");
            for (i, (a, b)) in derived.iter().zip(&fresh).enumerate() {
                assert_eq!(a.kind, b.kind, "{src:?}: 退回路径第 {i} 个 token 的 kind");
                assert_eq!(a.len, b.len, "{src:?}: 退回路径第 {i} 个 token 的长度");
            }
        }
    }

    /// 真实语料上派生与重新分词同构（zh 教程全部代码块）
    ///
    /// 单测的十几份形态只能算抽样；这一条把仓库里的方言语料全部跑一遍：
    /// 任何教程块里出现「派生与重新分词不同构」都会在这里反出来（debug 构建
    /// 下的管线自检也会反，但只在真的跑过那条管线时）。派生不成立（退回
    /// 现场分词）不算错，只统计不崩。
    #[test]
    fn test_derive_lex_tokens_matches_on_tutorial_corpus() {
        use std::fs;
        use std::path::Path;

        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tutorials");
        // 语料目录缺席分两种，必须区分对待：旧写法不分青红皂白 `return`，而这一返回
        // 正好在 `checked > 400` 的语料下限断言之前（实测把目录指向不存在的名字：
        // 用例 0.00 s 绿色通过，真实跑是 0.12 s）——也就是说本用例唯一的名值守门人
        // 被它自己绕过了。仓库布局的判据是 workspace 根清单在不在：在，则教程目录
        // 缺席只能是路径写错或检出不全，必须判红；不在（发布包/独立 crate）才是真的跳过。
        if !dir.is_dir() {
            let workspace_manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
            assert!(
                !workspace_manifest.is_file(),
                "教程语料目录缺失：{}（workspace 布局下它必须存在，否则本用例等于没跑）",
                dir.display()
            );
            return; // 非仓库布局（如发布包）里没教程目录，不阻断测试
        }
        let entries = fs::read_dir(&dir).expect("教程目录存在但读不了");
        let manager = zh_manager();
        let mut checked = 0usize;
        let mut rejected = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            for block in markdown_code_blocks(&text) {
                let (lex, input_tokens) = transpile_lex_stage(&block, &manager);
                checked += 1;
                if rebase_tokens(&input_tokens, &lex.final_edits).is_none() {
                    rejected += 1;
                }
                let derived = derive_lex_tokens(&input_tokens, &lex);
                let fresh = collect_tokens(&lex.output);
                assert_eq!(
                    derived.len(),
                    fresh.len(),
                    "{:?} 的块：token 数不一致",
                    path.file_name()
                );
                for (i, (a, b)) in derived.iter().zip(&fresh).enumerate() {
                    assert_eq!(
                        a.kind,
                        b.kind,
                        "{:?} 的块：第 {i} 个 token 的 kind",
                        path.file_name()
                    );
                    assert_eq!(
                        a.len,
                        b.len,
                        "{:?} 的块：第 {i} 个 token 的长度",
                        path.file_name()
                    );
                }
            }
        }
        assert!(
            checked > 400,
            "语料过少（{checked} 块），检查教程目录/解析是否失效"
        );
        // 后缀黏连等形态只能退回现场分词；占比不应过半，否则本优化失效
        assert!(rejected * 2 < checked, "派生失败过多：{rejected}/{checked}");
    }

    /// 提取 Markdown 围栏代码块（```rust / ```zh / 裸 ```）
    fn markdown_code_blocks(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut current: Option<String> = None;
        for line in text.lines() {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("```") {
                match current.take() {
                    None => {
                        let lang = rest.trim();
                        if matches!(lang, "" | "rust" | "zh" | "rust,zh" | "zh,rust") {
                            current = Some(String::new());
                        } else {
                            // 非代码块（bash/text/…）：置 None 但仍需闭合，
                            // 用一个永不出发的占位块标记跳过
                            current = Some("\u{0}".to_string());
                        }
                    }
                    Some(buf) => {
                        if !buf.starts_with('\u{0}') {
                            out.push(buf);
                        }
                        current = None;
                    }
                }
            } else if let Some(buf) = current.as_mut()
                && !buf.starts_with('\u{0}')
            {
                buf.push_str(line);
                buf.push('\n');
            }
        }
        out
    }

    fn new_cache() -> 缓存::转译缓存 {
        // 静音走一次性写入且持级别锁：旧写法每次建缓存都裸写全局级别，等于给
        // 所有「断言级别」的用例埋了个不持锁的竞争者（撑开窗口实测 12/12 轮红）。
        logger::quiet_for_tests();
        缓存::转译缓存::with_default_capacity()
    }

    #[test]
    fn test_full_pipeline_and_incremental_cache() {
        let manager = create_manager();
        let mut cache = new_cache();
        let source = "函数 主函数() { 让 数量: 整数 = 5; 打印行(\"你好\") }";

        let first = transpile_source(source, &manager, &mut cache).expect("翻译失败");
        let second = transpile_source(source, &manager, &mut cache).expect("翻译失败");

        // 输出一致且包含各阶段替换结果（词法 + 模块路径 + 别名）
        assert_eq!(first, second);
        assert!(
            first.contains("fn 主函数() { let 数量: i32 = 5; println!(\"你好\") }"),
            "实际输出：{}",
            first
        );

        // 第二次调用命中缓存：条数 1、未命中 1 次、命中 1 次
        assert_eq!(cache.current_count(), 1);
        assert_eq!(cache.miss_count(), 1);
        assert_eq!(cache.hit_count(), 1);
        assert_eq!(cache.hit_rate(), 0.5);
    }

    #[test]
    fn test_content_change_triggers_retranslate() {
        let manager = create_manager();
        let mut cache = new_cache();

        transpile_source("函数 主函数() { 让 x = 1; }", &manager, &mut cache).expect("翻译失败");
        transpile_source("函数 主函数() { 让 x = 2; }", &manager, &mut cache).expect("翻译失败");

        assert_eq!(cache.current_count(), 2);
        assert_eq!(cache.miss_count(), 2);
    }

    #[test]
    fn test_pipeline_method_position_defers_reserved_word() {
        // 方法位让位（`枚举` 词法值为保留字 enum、别名表为 enumerate）：
        // 完整管线产出 `.iter().enumerate()`（此前为 `.iter().enum()` 编译失败）
        let manager = create_manager();
        let mut cache = new_cache();
        let source = "函数 主函数() { 让 n = 甲.迭代().枚举(); }";
        let out = transpile_source(source, &manager, &mut cache).expect("翻译失败");
        assert!(out.contains(".iter().enumerate()"), "实际输出：{}", out);

        // 替换值非保留字的词不受让位影响：`.文件()` 照旧 `.file()`（宏节词表）
        let mut cache = new_cache();
        let out = transpile_source("函数 主函数() { 让 f = 甲.文件(); }", &manager, &mut cache)
            .expect("翻译失败");
        assert!(out.contains(".file()"), "实际输出：{}", out);
    }

    #[test]
    fn test_transpile_with_map_records_keyword_replacements() {
        let manager = create_manager();
        let mut cache = new_cache();
        let source = "函数 主函数() { 让 x = 1; }";

        let output = transpile_source_with_map(source, &manager, &mut cache).expect("翻译失败");

        // 函数 与 让 被替换，主函数 保持原样不产生映射
        let fn_map = output
            .source_map
            .iter()
            .find(|m| m.original == "函数")
            .expect("应有 函数 映射");
        assert_eq!(fn_map.替换文字, "fn");
        assert_eq!(
            &source[fn_map.源偏移..fn_map.源偏移 + fn_map.length],
            "函数"
        );

        let let_map = output
            .source_map
            .iter()
            .find(|m| m.original == "让")
            .expect("应有 让 映射");
        assert_eq!(let_map.替换文字, "let");
        assert!(!output.source_map.iter().any(|m| m.original == "主函数"));
    }

    #[test]
    fn test_language_pack_update_invalidates_cache() {
        let manager = create_manager();
        let mut cache = new_cache();
        let source = "函数 主函数() { 让 x = 1; }";

        transpile_source(source, &manager, &mut cache).expect("翻译失败");
        // 语言包变化（新增 可变→mut 映射）→ 语境指纹变化 → 缓存失效重新翻译
        let new_manager = 映射管理器::映射管理器::从内置载入(
            r#"
["声明"]
"函数" = "fn"
"让" = "let"
"可变" = "mut"
"#,
            "[\"模块路径\"]\n",
            "[\"模块路径\"]\n[\"标识符\"]\n",
            &[],
        )
        .expect("创建新管理器失败");
        transpile_source(source, &new_manager, &mut cache).expect("翻译失败");

        // 同内容哈希 → 覆盖原条目（条数不变）；语境变化 → 未命中计数增加
        assert_eq!(cache.current_count(), 1);
        assert_eq!(cache.miss_count(), 2);
        // 新映射生效：可变→mut
        let result = transpile_source("函数 主函数() { 让 可变 x = 1; }", &new_manager, &mut cache)
            .expect("翻译失败");
        assert!(result.contains("let mut x"));
    }

    #[test]
    fn test_zero_width_char_warns_but_does_not_block() {
        let manager = create_manager();
        let mut cache = new_cache();
        // 源码含零宽空格（token 之间，Rust 视为空白），翻译应正常完成
        let source = "函数 主函数() {\u{200B} 让 x = 1; }";
        let result = transpile_source(source, &manager, &mut cache);
        assert!(result.is_ok(), "零宽字符不应阻断翻译：{:?}", result);
        assert!(result.unwrap().contains("fn 主函数()"));
    }

    // ===== 全管线编辑地图（pipeline_map）测试 =====

    /// 把编辑地图应用到源文本（按 源偏移 拼接 replacement），
    /// 是"地图与真实输出一致"的权威校验——任何阶段改规则却不同步地图都会使此测试失败。
    fn apply_pipeline_map(source: &str, map: &[缓存::源码映射条目]) -> String {
        let mut result = String::new();
        let mut pos = 0usize;
        for e in map {
            result.push_str(&source[pos..e.源偏移]);
            result.push_str(&e.替换文字);
            pos = e.源偏移 + e.length;
        }
        result.push_str(&source[pos..]);
        result
    }

    /// 完整管线（词法 + use 路径 + crate:: 前缀 + 别名）下，
    /// 地图回放结果必须与真实输出逐字符一致
    #[test]
    fn test_pipeline_map_replays_to_output() {
        let manager = create_manager();
        let module_names = HashSet::from(["辅助".to_string()]);
        let source = "使用 标准集合::哈希映射;\n\
                      函数 主函数() {\n\
                      \x20   辅助::辅助函数();\n\
                      \x20   让 数量: 整数 = 5;\n\
                      \x20   打印行(\"你好\");\n\
                      }";
        let output = transpile_pipeline_with_map(source, &manager, Some(&module_names));
        assert!(output.output.contains("crate::辅助::辅助函数()"));
        let rebuilt = apply_pipeline_map(source, &output.pipeline_map);
        assert_eq!(
            rebuilt, output.output,
            "pipeline_map 回放必须与真实输出一致\n实际输出：{}\n回放输出：{}",
            output.output, rebuilt
        );
    }

    /// 不带 module_names（CLI 场景）时地图同样与输出一致
    #[test]
    fn test_pipeline_map_replays_without_qualify() {
        let manager = create_manager();
        let source = "函数 主函数() { 让 数量: 整数 = 5; 打印行(\"你好\") }";
        let output = transpile_pipeline_with_map(source, &manager, None);
        let rebuilt = apply_pipeline_map(source, &output.pipeline_map);
        assert_eq!(rebuilt, output.output);
    }

    /// 静默重放与常规管线输出逐项一致：仅跳过教学告警，不得改变任何转译结果
    ///
    /// 诊断列映射重建走 `transpile_pipeline_quiet` 重放，若重放规则与写盘
    /// 转译（`transpile_pipeline`）有任何差异，列号回译就会错位。
    #[test]
    fn test_pipeline_quiet_matches_normal() {
        let manager = create_manager();
        let source = "函数 主函数() {\n    让 数量: 整数 = 42;\n    打印行(\"你好\");\n}";
        let normal = transpile_pipeline(source, &manager);
        let quiet = transpile_pipeline_quiet(source, &manager);
        assert_eq!(normal.output, quiet.output, "静默重放的转译输出必须一致");
        assert_eq!(
            normal.pipeline_map, quiet.pipeline_map,
            "静默重放的管线地图必须一致"
        );
    }

    /// 游标版逆映射与朴素版逐偏移等价：多种表形态 × 全量偏移查询 × 阶段链
    ///
    /// 生产路径按升序批量查询（见 `compose_pipeline_map`），游标只前进不回退；
    /// 朴素版每次从表头重扫。两者必须逐偏移给出同一答案——偏移换算错了，
    /// 全管线地图就错位，列映射回译跟着错。形态覆盖：空表、首条从 0 起、
    /// 替换变长/变短/等长/清空、相邻无缝隙，以及表尾之后的越界查询。
    #[test]
    fn test_reverse_cursor_matches_naive() {
        fn 表(条目: &[(usize, usize, &str, &str)]) -> Vec<缓存::源码映射条目> {
            条目
                .iter()
                .map(|(偏移, 长度, 原文, 替换)| {
                    缓存::源码映射条目::new(*偏移, *长度, 原文, 替换)
                })
                .collect()
        }
        let 形态: Vec<Vec<缓存::源码映射条目>> = vec![
            表(&[]),
            表(&[(0, 6, "函数", "fn")]),
            表(&[
                (3, 6, "函数", "fn"),
                (20, 3, "让", "let"),
                (40, 9, "整数", "i32"),
            ]),
            表(&[
                (0, 6, "打印行", ""),
                (10, 3, "甲", "乙丙丁"),
                (14, 2, "等", "等"),
            ]),
            表(&[(5, 2, "aa", "bb"), (7, 2, "cc", "dd")]),
        ];
        for 编辑表 in &形态 {
            // 查到表尾之后一大截：尾部未替换区间的换算也要对齐
            let 上界 = 编辑表
                .last()
                .map(|e| e.源偏移 + e.替换文字.len() + 32)
                .unwrap_or(64);
            let 朴素: Vec<usize> = (0..=上界)
                .map(|偏移| inverse_map_offset(编辑表, 偏移))
                .collect();
            let mut 游标 = ReverseCursor::new(编辑表);
            for (偏移, 期望) in 朴素.iter().enumerate() {
                assert_eq!(
                    *期望,
                    游标.map(偏移),
                    "偏移 {偏移} 的换算不一致（表：{编辑表:?}）"
                );
            }
        }

        // 随机表形状：固定种子 LCG 造 200 张表，条数、间隔（含 0，即相邻无缝隙）、
        // 原文长度（含 0，即同偏移并列）与替换长度（含 0，即整段删除）全部随机。
        // 上面手写的形态容易漏边角，随机铺开才能把差分覆盖面拉开；不变量与
        // 生产一致：按输入偏移升序且互不重叠（下一条偏移 ≥ 上一条偏移 + 上一条长度）。
        let mut 种子 = 0x9E37_79B9_7F4A_7C15u64;
        let mut 随机 = move || -> usize {
            种子 = 种子
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (种子 >> 33) as usize
        };
        for 轮 in 0..200 {
            let mut 编辑表 = Vec::new();
            let mut 偏移 = 随机() % 4;
            for _ in 0..(随机() % 8 + 1) {
                let 原文长度 = 随机() % 4;
                let 替换长度 = 随机() % 4;
                编辑表.push(缓存::源码映射条目::new(
                    偏移,
                    原文长度,
                    // ASCII：让 length 字段就是字节长度，与 `原文长度` 自洽
                    &"x".repeat(原文长度),
                    &"y".repeat(替换长度),
                ));
                偏移 += 原文长度 + 随机() % 5;
            }
            // 查询上界：整表走完得到的输出侧末尾再往后一截（尾部未替换区间）
            let 上界 = 偏移 + 16;
            let 朴素: Vec<usize> = (0..=上界)
                .map(|偏移| inverse_map_offset(&编辑表, 偏移))
                .collect();
            let mut 游标 = ReverseCursor::new(&编辑表);
            for (偏移, 期望) in 朴素.iter().enumerate() {
                assert_eq!(
                    *期望,
                    游标.map(偏移),
                    "第 {轮} 轮随机表：偏移 {偏移} 的换算不一致（表：{编辑表:?}）"
                );
            }
        }

        // 阶段链：上一环的答案作为下一环的查询（对应 `compose_pipeline_map` 里
        // 「先夹更后阶段、再夹词法阶段」的串联），逐环同样必须等价
        let 甲 = 表(&[(0, 6, "使用", "use"), (12, 6, "标准库", "std")]);
        let 乙 = 表(&[(2, 3, "向量", "Vec"), (9, 2, "映射", "Map")]);
        let mut 游标甲 = ReverseCursor::new(&甲);
        let mut 游标乙 = ReverseCursor::new(&乙);
        for 偏移 in 0..=120usize {
            let 期望 = inverse_map_offset(&甲, inverse_map_offset(&乙, 偏移));
            let 中间 = 游标乙.map(偏移);
            assert_eq!(
                期望,
                游标甲.map(中间),
                "阶段链偏移 {偏移} 的换算不一致（乙给出 {中间}）"
            );
        }
    }

    /// 真实语言包 + 别名密集多阶段源：全管线地图回放仍须逐字等于输出
    ///
    /// 手工小表测不到「词法上千条 × 别名/`crate::` 前缀阶段数百条」的换算链，
    /// 而这条链正是 `compose_pipeline_map` 里游标推进的地方；前提破了（游标
    /// 乱序、单调性假设不成立）就直接反映为回放错位，不靠人工比对也能接住。
    #[test]
    fn test_pipeline_map_replays_on_alias_heavy_real_pack() {
        let manager = zh_manager();
        let mut source =
            String::from("使用 标准库::向量::向量;\n使用 标准库::映射::映射;\n\n模块 辅助;\n\n");
        for i in 0..120 {
            source.push_str(&format!(
                "函数 函数_{i}(项: 向量<字符串>, 表: 映射<字符串, 整数>) -> 整数 {{\n\
                 \x20   让 个数 = 项.长度();\n\
                 \x20   项.推送(\"新项\");\n\
                 \x20   让 合计 = 项.迭代().计数();\n\
                 \x20   表.插入(\"键{i}\", 合计);\n\
                 \x20   返回 辅助::查询(表, 个数) + 项.长度();\n\
                 }}\n\n"
            ));
        }
        let module_names: HashSet<String> = ["辅助".to_string()].into_iter().collect();
        // 静默入口：本用例只关心偏移换算，不要被上千条教学告警埋掉失败输出
        let output = transpile_pipeline_quiet_with_map_and_project(
            &source,
            &manager,
            Some(&module_names),
            None,
        );
        // 三道阶段的编辑都进了地图：条目数远超小表，才有资格作为游标的回归样本
        assert!(
            output.pipeline_map.len() > 1000,
            "别名密集语料的地图条目应上千：{}",
            output.pipeline_map.len()
        );
        assert!(
            output.output.contains("crate::辅助::query"),
            "语料应确实走通 `crate::` 前缀阶段：{}",
            &output.output[..output.output.len().min(400)]
        );
        let rebuilt = apply_pipeline_map(&source, &output.pipeline_map);
        assert_eq!(
            rebuilt, output.output,
            "地图回放必须逐字等于输出（偏移换算错位即在此暴露）"
        );
    }

    /// 宏自动补的 `!` 计入 replacement；模块路径段条目为 `crate::辅助`
    #[test]
    fn test_pipeline_map_records_macro_bang_and_qualify() {
        let manager = create_manager();
        let module_names = HashSet::from(["辅助".to_string()]);
        let source = "函数 主函数() {\n    辅助::辅助函数();\n    打印行(\"你好\");\n}";
        let output = transpile_pipeline_with_map(source, &manager, Some(&module_names));

        let macro_entry = output
            .pipeline_map
            .iter()
            .find(|e| e.original == "打印行")
            .expect("应有宏条目");
        assert_eq!(macro_entry.替换文字, "println!");
        let qual_entry = output
            .pipeline_map
            .iter()
            .find(|e| e.original == "辅助")
            .expect("应有模块路径条目");
        assert_eq!(qual_entry.替换文字, "crate::辅助");
    }

    /// use 语句路径段经模块路径阶段替换、末段经别名阶段替换（LSP 之前缺失的环节）
    #[test]
    fn test_pipeline_map_use_stmt_module_path_and_alias() {
        let manager = 映射管理器::映射管理器::从内置载入(
            r#"
["声明"]
"函数" = "fn"
"让" = "let"
"使用" = "use"
["宏"]
"打印行" = "println"
"#,
            r#"
["模块路径"]
"标准集合" = "std::collections"
"#,
            r#"
["标识符"]
"哈希映射" = "HashMap"
"字符串" = "String"
"#,
            &[],
        )
        .expect("创建管理器失败");
        let source = "使用 标准集合::哈希映射;\n函数 主函数() {\n    让 s: 字符串 = 哈希映射::新建();\n    打印行(\"你好\");\n}";
        let output = transpile_pipeline_with_map(source, &manager, None);
        assert!(
            output.output.contains("use std::collections::HashMap;"),
            "use 语句路径与末段都应被替换：{}",
            output.output
        );
        assert_eq!(
            apply_pipeline_map(source, &output.pipeline_map),
            output.output
        );
    }

    /// 属性测试：正向转译 → 反向转译应精确还原母语源（回译不变量）
    ///
    /// 约束：测试源避开模块路径（反向转译无模块路径表，`网络库::服务器` 之类
    /// 无法还原）；宏调用写带感叹号形式（正向自动补的 `!` 反向原样保留，
    /// 源写 `打印行(` 会还原成 `打印行!(` 造成差异）。
    #[test]
    fn test_property_roundtrip_preserves_source() {
        let manager = create_manager();
        let source = "函数 主函数() {\n    让 数量: 整数 = 5;\n    让 文本: 字符串 = \"世界\";\n    打印行!(\"你好\");\n}";
        let output = transpile_pipeline(source, &manager);
        assert!(
            output.output.contains("fn 主函数()") && output.output.contains("println!"),
            "正向转译应产出英文：{}",
            output.output
        );

        // 合并关键字 + 宏 + 别名三类映射并反转（英文 → 中文），与正向管线互为逆
        let mut reverse_map = std::collections::HashMap::new();
        for (zh, en) in manager.取关键词表() {
            reverse_map.insert(en.clone(), zh.clone());
        }
        for (zh, en) in manager.取宏表() {
            reverse_map.insert(en.clone(), zh.clone());
        }
        for (zh, en) in manager.取别名表() {
            reverse_map.insert(en.clone(), zh.clone());
        }

        let restored = 词法器::反译(
            &output.output,
            &reverse_map,
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(restored, source, "回译应精确还原母语源");
    }

    /// 属性测试：英文输出再经转译管线应保持不变（幂等不变量）
    ///
    /// 若映射表中混入英文键、或宏感叹号自动补逻辑对英文输出不幂等，
    /// 连续转译会持续改变文本，本测试守护管线的收敛性。
    #[test]
    fn test_property_retranspile_idempotent() {
        let manager = create_manager();
        let source = "函数 主函数() {\n    让 数量: 整数 = 5;\n    打印行!(\"你好\");\n}";
        let en = transpile_pipeline(source, &manager).output;
        let again = transpile_pipeline(&en, &manager).output;
        assert_eq!(en, again, "对英文输出再转译应保持不变");
    }

    /// 属性测试：全部内置语言包的映射表不得含循环引用
    ///
    /// 循环（A→B 且 B→A）会使正向/反向转译来回抖动，属语言包配置错误；
    /// 本测试加载全部内置语言包，守护新增条目时不会引入互指。
    #[test]
    fn test_all_builtin_lang_packs_no_mapping_cycles() {
        let codes = 语言::builtin_language_codes();
        assert!(codes.len() >= 9, "应覆盖全部内置语言：{codes:?}");
        for code in codes {
            let files = 语言::builtin_lang_files(code);
            let get = |name: &str| {
                files
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, c)| *c)
                    .unwrap_or("")
            };
            let keywords = get("keywords.toml");
            assert!(!keywords.is_empty(), "{code} 语言包缺少 keywords.toml");
            let third_party: Vec<(&str, &str)> = files
                .iter()
                .filter(|(n, _)| n.starts_with("crates/"))
                .map(|(n, c)| (*n, *c))
                .collect();
            let manager = 映射管理器::映射管理器::从内置载入(
                keywords,
                get("module_paths.toml"),
                get("stdlib.toml"),
                &third_party,
            )
            .expect("语言包应能解析");
            let cycles = manager.查循环();
            assert!(cycles.is_empty(), "{code} 语言包存在循环映射：{cycles:?}");
        }
    }

    /// 门禁：内置语言包的「方法位让位」词在完整管线中必须落到别名层词条
    ///
    /// 词法替换值为保留关键字的词（`枚举`→enum）在方法位直译非法
    /// （`.枚举()` → `.enum()`）；让位机制应将其保留给别名阶段
    /// （`.enumerate()`）。本测试对每个语言包在每个让位词上验证管线闭环，
    /// 不硬编码具体词汇——新增语言包自动纳入守护。
    #[test]
    fn test_builtin_lang_packs_method_defer_words_reach_alias_stage() {
        for code in 语言::builtin_language_codes() {
            let files = 语言::builtin_lang_files(code);
            let get = |name: &str| {
                files
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, c)| *c)
                    .unwrap_or("")
            };
            let third_party: Vec<(&str, &str)> = files
                .iter()
                .filter(|(n, _)| n.starts_with("crates/"))
                .map(|(n, c)| (*n, *c))
                .collect();
            let manager = 映射管理器::映射管理器::从内置载入(
                get("keywords.toml"),
                get("module_paths.toml"),
                get("stdlib.toml"),
                &third_party,
            )
            .expect("语言包应能解析");

            let mut cache = new_cache();
            for word in manager.取方法让位词() {
                let target = manager.取别名表().get(word).expect("让位词应有别名词条");
                let source = format!("函数 主函数() {{ 让 x = 甲.{}(); }}", word);
                let out = transpile_source(&source, &manager, &mut cache).expect("翻译失败");
                assert!(
                    out.contains(&format!(".{}()", target)),
                    "{code}: `.{}()` 应经让位落到别名词条 `.{}()`，实际输出：{}",
                    word,
                    target,
                    out
                );
            }
        }
    }

    /// 门禁：附录C《常见错误信息字典》收录的错误码，在全部内置语言包中
    /// 都必须有教学提示（errors.toml 的 "教学提示" 字段）
    ///
    /// 附录C 是教程侧的高频错误清单，与教程章节同步维护；
    /// 教学提示缺失会让新手在常见错误上得不到母语指引，
    /// 因此该清单与语言包的覆盖关系由测试守护（附录C 新增错误码时同步更新本清单）。
    #[test]
    fn test_appendix_c_error_codes_have_teaching_hints_in_all_packs() {
        // 与 tutorials/附录C：常见错误信息字典.md 的 `### E0xxx` 标题同步维护
        let appendix_c_codes: &[&str] = &[
            "E0004", "E0063", "E0072", "E0106", "E0204", "E0261", "E0277", "E0308", "E0382",
            "E0405", "E0425", "E0432", "E0502", "E0507", "E0508", "E0531", "E0573", "E0596",
            "E0597", "E0599", "E0603",
        ];
        let codes = 语言::builtin_language_codes();
        assert!(codes.len() >= 9, "应覆盖全部内置语言：{codes:?}");
        for code in codes {
            let files = 语言::builtin_lang_files(code);
            let errors_toml = files
                .iter()
                .find(|(n, _)| *n == "errors.toml")
                .map(|(_, c)| *c)
                .expect("{code} 语言包缺少 errors.toml");
            let manager = diagnostic::ErrorTranslationManager::load_from_string(errors_toml)
                .expect("errors.toml 应能解析");
            for error_code in appendix_c_codes {
                let entry = manager
                    .translation_table
                    .get(*error_code)
                    .unwrap_or_else(|| {
                        panic!("{code} 语言包缺少附录C 错误码 [{error_code}] 的教学提示")
                    });
                assert!(
                    entry.teaching_hint.is_some(),
                    "{code} 语言包的 [{error_code}] 缺少教学提示（附录C 已收录）"
                );
            }
        }
    }
}
