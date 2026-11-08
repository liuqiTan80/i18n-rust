//! 诊断消息翻译与所有权详情提取（CLI 同源消息表）。
//!
//! 由服务器启动时初始化错误消息翻译器（语言包 errors.toml，缺失时
//! 回退引擎内嵌 zh），为诊断发布提供消息中文化与
//! 所有权错误的叙事化详情。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::SystemTime;

use i18n_rust_engine::诊断::{借用冲突, 所有权详情, 诊断位置};
use serde_json::Value;

use crate::翻译缓存::{翻译条目, 英文列转母语列};

/// 诊断翻译器（errors.toml 消息表 + 错误码表 + 类型映射）：与 CLI 完全同源，
/// 走引擎 `渲染主消息` 统一口径，覆盖 rustc/rust-analyzer 的常见消息。
/// 由服务器启动时初始化（语言包目录 errors.toml + 映射管理器，缺失时回退内置
/// zh）；未初始化时为 无，翻译退化为下方轻量短语替换。
static 诊断翻译器槽: OnceLock<Option<i18n_rust_engine::诊断::诊断翻译器>> = OnceLock::new();

/// 初始化诊断翻译器（errors.toml 消息表 + 由映射管理器构建的类型映射；
/// 失败/缺失时回退内置 zh），使 LSP 主消息与 CLI 对同一错误码与消息得到同一译文。
pub fn 初始化诊断翻译器(
    语言包路径: &std::path::Path,
    管理器: &i18n_rust_engine::映射管理器::映射管理器,
) {
    let 类型映射 = i18n_rust_engine::诊断::构建类型映射(管理器);
    let 翻译管理器 = i18n_rust_engine::诊断::错误消息管理器::从文件载入(
        &语言包路径.join("errors.toml"),
    )
    .ok()
    .or_else(内置中文错误翻译器);
    let 翻译器 = 翻译管理器
        .map(|管理器值| i18n_rust_engine::诊断::诊断翻译器::新建(管理器值, 类型映射));
    let _ = 诊断翻译器槽.set(翻译器);
}

/// 物化引擎内嵌的中文 errors.toml 并加载翻译器（语言包目录缺失时兜底）
fn 内置中文错误翻译器() -> Option<i18n_rust_engine::诊断::错误消息管理器> {
    let 目录 = tempfile::tempdir().ok()?;
    let 内容 = i18n_rust_engine::语言::内置语言包文件("zh")
        .iter()
        .find(|(file_name, _)| *file_name == "errors.toml")?
        .1;
    std::fs::write(目录.path().join("errors.toml"), 内容).ok()?;
    i18n_rust_engine::诊断::错误消息管理器::从文件载入(
        &目录.path().join("errors.toml"),
    )
    .ok()
}

/// 翻译诊断主消息为当前界面语言（与 CLI 同口径）
///
/// 错误码参数（若有）供错误码表优先命中；主标签参数为 rustc 主
/// 跨度标签（镜像检查从真实 rustc JSON 可得，供提取期望/实际类型）。
/// 优先走引擎 `渲染主消息`（错误码表→消息表→占位符回填→类型中文化）；
/// 未命中时退化为轻量短语替换。多行消息按行逐条翻译后拼接（仅首行适用
/// 错误码/标签与教学提示）。仅反引号外文本参与兜底替换，避免误伤标识符。
///
/// 公开(包)：镜像检查（真实项目 rustc 诊断）与诊断发布共用同一渲染。
pub(crate) fn 翻译诊断消息(
    错误码: Option<&str>, 消息: &str, 主标签: Option<&str>
) -> String {
    // 多行消息（rust-analyzer 的 E0004 等）逐行翻译；错误码/标签只属于首行
    if 消息.contains('\n') {
        let mut 是首行 = true;
        let 行列表: Vec<String> = 消息
            .split('\n')
            .map(|行文本| {
                let 已翻译 = if 是首行 {
                    翻译诊断消息单行(错误码, 行文本, 主标签, true)
                } else {
                    翻译诊断消息单行(None, 行文本, None, false)
                };
                是首行 = false;
                已翻译
            })
            .collect();
        return 行列表.join("\n");
    }
    翻译诊断消息单行(错误码, 消息, 主标签, true)
}

/// 从 rust-analyzer 诊断里归集含期望与实际字样的候选标签文本。
///
/// rust-analyzer 把类型不匹配的期望/实际放在关联信息而非主
/// 跨度标签（CLI 的 rustc JSON 则带在主跨度标签）。本函数从关联
/// 消息中取首个命中者，作为 `渲染主消息` 的主标签，使分析器
/// 直连路径也能回填 `{期望}`/`{实际}`，与 CLI/镜像检查同一完整译文。
/// 无命中时返回 无（渲染器自动回退消息表，不劣于旧行为）。
pub(crate) fn 查找期望实得标签(诊断值: &Value) -> Option<String> {
    诊断值
        .get("relatedInformation")
        .and_then(|值| 值.as_array())?
        .iter()
        .filter_map(|项| 项.get("message").and_then(|值| 值.as_str()))
        .find(|消息| 消息.contains("expected ") && 消息.contains(", found "))
        .map(|文本串| 文本串.to_string())
}

/// 轻量短语替换表（诊断翻译兜底）：界面全局语言固定，首次构建后缓存。
/// 诊断每次按键都会发布，若每次翻译都重新构建约 30 对字符串是纯浪费。
static 诊断短语替换表: LazyLock<Vec<(String, String)>> = LazyLock::new(|| {
    let 界面 = crate::界面消息::全局界面();
    let mut 替换项: Vec<(String, String)> = vec![
        ("{integer}".to_string(), 界面.取消息("diag_rustc_integer")),
        ("{float}".to_string(), 界面.取消息("diag_rustc_float")),
        (
            "floating-point number".to_string(),
            界面.取消息("diag_rustc_float"),
        ),
        ("integer".to_string(), 界面.取消息("diag_rustc_integer")),
    ];

    // 常见错误模式翻译
    let 替换表 = [
        (
            "cannot find value",
            界面.取消息("lsp_phrase_cannot_find_value"),
        ),
        (
            "cannot find type",
            界面.取消息("lsp_phrase_cannot_find_type"),
        ),
        (
            "cannot find function",
            界面.取消息("lsp_phrase_cannot_find_function"),
        ),
        (
            "cannot find module",
            界面.取消息("lsp_phrase_cannot_find_module"),
        ),
        (
            "mismatched types",
            界面.取消息("lsp_phrase_mismatched_types"),
        ),
        ("type mismatch", 界面.取消息("lsp_phrase_type_mismatch")),
        ("expected", 界面.取消息("lsp_phrase_expected")),
        // “实得”仅在类型不匹配场景译为
        // 「实际为」：以带逗号的长键限定语境——裸词“实得”会把 E0599 的
        // 未找到方法的消息误译为「实际为」
        //（“找到”义），也会与“方法未找到”变体相互干扰
        (
            ", found ",
            format!("，{} ", 界面.取消息("lsp_phrase_found")),
        ),
        ("unused variable", 界面.取消息("lsp_phrase_unused_variable")),
        ("unused import", 界面.取消息("lsp_phrase_unused_import")),
        ("cannot borrow", 界面.取消息("lsp_phrase_cannot_borrow")),
        (
            "borrowed as immutable",
            界面.取消息("lsp_phrase_borrowed_immutable"),
        ),
        (
            "borrowed as mutable",
            界面.取消息("lsp_phrase_borrowed_mutable"),
        ),
        ("no method named", 界面.取消息("lsp_phrase_no_method_named")),
        (
            "method not found",
            界面.取消息("lsp_phrase_method_not_found"),
        ),
        ("field", 界面.取消息("lsp_phrase_field")),
        (
            "does not implement",
            界面.取消息("lsp_phrase_does_not_implement"),
        ),
        ("the trait", 界面.取消息("lsp_phrase_the_trait")),
        (
            "is not satisfied",
            界面.取消息("lsp_phrase_is_not_satisfied"),
        ),
        (
            "unresolved import",
            界面.取消息("lsp_phrase_unresolved_import"),
        ),
        ("file not found", 界面.取消息("lsp_phrase_file_not_found")),
        ("aborting due to", 界面.取消息("lsp_phrase_aborting_due_to")),
        ("previous error", 界面.取消息("lsp_phrase_previous_error")),
    ];

    for (英文词, 本地词) in 替换表 {
        替换项.push((英文词.to_string(), 本地词));
    }
    // 长键优先：按键长降序应用，避免短串先替换打碎长短语
    //（如短键先于“文件未找到”会把后者拆坏）。
    // 语境冲突已从源头消解（裸词“实得”改为带逗号的语境键），
    // 排序仍作为一般性防御保留。
    替换项.sort_by_key(|项| 项.0.len());
    替换项.reverse();
    替换项
});

/// 单行诊断主消息渲染：引擎同口径优先，轻量短语表兜底
fn 翻译诊断消息单行(
    错误码: Option<&str>,
    消息: &str,
    主标签: Option<&str>,
    带提示: bool,
) -> String {
    let 界面 = crate::界面消息::全局界面();

    // 1. 引擎同口径渲染（与 CLI 完全同源）：错误码表优先→消息表（精确/前缀/
    //    后缀/通配段）→ 占位符回填，回填进来的值走类型映射中的类型中文化
    //（模板字面里作者手写的入口名或类型名不参与改写）。命中任一表即返回。
    if let Some(翻译器) = 诊断翻译器槽.get().and_then(|选项值| 选项值.as_ref())
        && let Some(已渲染) = 翻译器.渲染主消息(错误码, 消息, 主标签)
    {
        let mut 文本串 = 已渲染.文字;
        if 带提示 && let Some(提示) = &已渲染.提示 {
            文本串.push('\n');
            文本串.push_str(提示);
        }
        return 文本串;
    }

    // 2. 轻量短语替换（兜底）：静态表首次构建后缓存，避免每次分配
    let 替换项 = &*诊断短语替换表;
    let mut 结果值 = 反引号外替换(消息, 替换项);

    // 添加教学提示
    if 消息.contains("mismatched types") || 消息.contains("type mismatch") {
        结果值.push_str(&界面.取消息("lsp_hint_mismatched_types"));
    } else if 消息.contains("cannot find") {
        结果值.push_str(&界面.取消息("lsp_hint_cannot_find"));
    } else if 消息.contains("unused") {
        结果值.push_str(&界面.取消息("lsp_hint_unused"));
    } else if i18n_rust_engine::诊断::是否未解析导入消息(消息) {
        // 未解析导入：提示通过 `rzc add <crate>` 添加缺失依赖
        if let Some(包名) = i18n_rust_engine::诊断::未解析包候选(消息).first() {
            结果值.push_str(&界面.取带参消息("lsp_hint_add_dependency", &[包名]));
        }
    }

    结果值
}

/// 整词替换：仅当目标前后字符均非 ASCII 标识符字符时替换，
/// 避免裸词模式误伤标识符子串（如英文单词内部的相同子串）
fn 整词替换(文本串: &str, 原词: &str, 新词: &str) -> String {
    let 是标识符字符 = |字符值: char| 字符值.is_ascii_alphanumeric() || 字符值 == '_';
    let mut 结果值 = String::with_capacity(文本串.len());
    let mut 剩余 = 文本串;
    while let Some(位置) = 剩余.find(原词) {
        let 结束位置 = 位置 + 原词.len();
        let 前侧可替换 = 剩余[..位置]
            .chars()
            .next_back()
            .is_none_or(|字符值| !是标识符字符(字符值));
        let 后侧可替换 = 剩余[结束位置..]
            .chars()
            .next()
            .is_none_or(|字符值| !是标识符字符(字符值));
        结果值.push_str(&剩余[..位置]);
        结果值.push_str(if 前侧可替换 && 后侧可替换 {
            新词
        } else {
            原词
        });
        剩余 = &剩余[结束位置..];
    }
    结果值.push_str(剩余);
    结果值
}

/// 按顺序对反引号包裹之外的文本应用替换，反引号内的内容（标识符/
/// 类型名引用）保持原样；未成对的反引号后文本仍参与替换
fn 反引号外替换(输入: &str, 替换项: &[(String, String)]) -> String {
    let 应用 = |片段: &str| -> String {
        let mut 文本串 = 片段.to_string();
        for (原词, 新词) in 替换项 {
            // 单词模式（如 integer/expected）用整词匹配，避免误伤
            // to_integer/unexpected 等标识符子串；短语模式保持子串替换
            文本串 = if 原词
                .chars()
                .all(|字符值| 字符值.is_ascii_alphanumeric() || 字符值 == '_')
            {
                整词替换(&文本串, 原词, 新词)
            } else {
                文本串.replace(原词.as_str(), 新词.as_str())
            };
        }
        文本串
    };

    let mut 结果值 = String::with_capacity(输入.len());
    let mut 剩余 = 输入;
    while let Some(起始位置) = 剩余.find('`') {
        结果值.push_str(&应用(&剩余[..起始位置]));
        let 之后 = &剩余[起始位置 + 1..];
        match 之后.find('`') {
            Some(结束位置) => {
                // 成对反引号：内部内容原样保留
                结果值.push('`');
                结果值.push_str(&之后[..结束位置]);
                结果值.push('`');
                剩余 = &之后[结束位置 + 1..];
            }
            None => {
                // 未成对：剩余文本照常替换
                结果值.push('`');
                结果值.push_str(&应用(之后));
                剩余 = "";
                break;
            }
        }
    }
    结果值.push_str(&应用(剩余));
    结果值
}

/// 判断一条诊断是否为虚拟项目主程序文件中关于主函数的提示级提示
///
/// 虚拟项目将用户代码作为模块聚合到 main.rs 中，`fn main()` 在模块内
/// 并非真正的程序入口，rust-analyzer 会发出“此处是名为主函数的函数”
/// 等教学无关的提示。此函数识别并过滤这类诊断。
pub(super) fn 是主函数提示(诊断值: &Value, _虚拟资源标识: &str) -> bool {
    // 仅过滤提示级别（严重级为 4）
    let 严重级 = 诊断值
        .get("severity")
        .and_then(|值| 值.as_u64())
        .unwrap_or(0);
    if 严重级 != 4 {
        return false;
    }
    let 消息 = 诊断值
        .get("message")
        .and_then(|值| 值.as_str())
        .unwrap_or("");
    // 过滤“此处是名为主函数的函数”类提示
    消息.contains("here is a function named `main`")
        || (消息.contains("function `main`") && 消息.contains("never used"))
}

/// 过滤虚拟项目固有的过程宏误报（#3 症状二）
///
/// 虚拟项目禁用了过程宏（`procMacro.enable=false`）且不含第三方依赖，
/// `#[派生(解析器)]`→`#[derive(Parser)]`（命令行解析库等）无法展开，其辅助属性与
/// 派生宏的名字解析必然失败：
/// - 找不到属性名（辅助属性，如属性的长参数写法）
/// - 找不到派生宏
///
/// 这两类诊断在虚拟项目语境下恒为误报（用户项目中 `rzc check` 正常通过），
/// 直接过滤；`unresolved import` 不过滤——它是“添加依赖”快速修复的输入，
/// 且用户项目真实缺依赖时同样出现。
pub(super) fn 是缺依赖噪音(诊断值: &Value) -> bool {
    let 消息 = 诊断值
        .get("message")
        .and_then(|值| 值.as_str())
        .unwrap_or("");
    消息.contains("cannot find attribute") || 消息.contains("cannot find derive macro")
}

/// 过滤“引用未打开模块文件”的 E0433/E0432 误报
///
/// LSP 虚拟项目只聚合当前打开的文件：`crate::日志设置` 引用的
/// `日志设置.zh` 未打开时，聚合 main.rs 中没有对应模块声明，
/// rust-analyzer 会报 E0433/E0432。该引用在本项目中实际有效
/// （同名文件存在于同目录，打开后即可解析），属于虚拟项目固有误报；
/// 条目同目录存在同名方言文件时过滤。文件确实不存在（如拼写错误）时
/// 诊断保留，供用户修正。
///
/// 覆盖的 rust-analyzer 消息格式（0.3.3025 起为 rustc 1.98 风格）：
/// - `cannot find module or crate `X` in this scope`（旧 E0433）
/// - 在包内找不到 X（新 E0433）
/// - 未解析导入包内路径 X（新 E0432，可含尾段）
/// - `unresolved import `X``（裸路径）
/// - 根作用域无 X（严重级 4 的同伴提示）
pub(super) fn 是未打开模块引用(
    诊断值: &Value,
    条目值: Option<&crate::翻译缓存::翻译条目>,
) -> bool {
    let Some(条目) = 条目值 else {
        return false;
    };
    let 消息 = 诊断值
        .get("message")
        .and_then(|值| 值.as_str())
        .unwrap_or("");
    let Some(名称) = 未打开模块名(消息) else {
        return false;
    };
    // 同目录存在 `名字.<原扩展名>` 时视为“模块文件未打开”的误报
    let (Some(目录), Some(扩展)) = (
        条目.原始路径.parent(),
        条目.原始路径.extension().and_then(|串| 串.to_str()),
    ) else {
        return false;
    };
    目录.join(format!("{名称}.{扩展}")).is_file()
}

/// 从诊断消息中提取“未打开模块”候选名（仅识别已知消息格式）
fn 未打开模块名(消息: &str) -> Option<&str> {
    const 保留字: &[&str] = &["crate", "self", "super", "std", "core", "alloc"];
    let 路径文本 = if let Some(剩余) = 消息.strip_prefix("cannot find module or crate `") {
        剩余.split_once('`')?.0
    } else if let Some(剩余) = 消息.strip_prefix("cannot find `") {
        let (名称, 尾部) = 剩余.split_once('`')?;
        if !尾部.starts_with(" in `crate`") {
            return None;
        }
        名称
    } else if let Some(剩余) = 消息.strip_prefix("unresolved import `") {
        剩余.split_once('`')?.0
    } else {
        let 剩余 = 消息.strip_prefix("no `")?;
        let (名称, 尾部) = 剩余.split_once('`')?;
        if !尾部.starts_with(" in the root") {
            return None;
        }
        名称
    };
    // 去掉 `crate::` 前缀后取首段（`crate::X::Y` → X）
    let 路径文本 = 路径文本.strip_prefix("crate::").unwrap_or(路径文本);
    let 首段 = 路径文本.split("::").next().unwrap_or(路径文本);
    // 模块名必须是单个安全路径段（防御恶意构造的路径穿越，如 `..`/`/`），
    // 且不能是保留字路径段（std/core/alloc 等系统包不可能是本地模块）
    if 首段.is_empty()
        || 首段.contains('/')
        || 首段.contains('\\')
        || 首段.contains("..")
        || 首段.starts_with('.')
        || 保留字.contains(&首段)
    {
        return None;
    }
    Some(首段)
}

/// 过滤“第三方依赖在 LSP 虚拟项目缺失”的误报
///
/// 虚拟项目 Cargo.toml 不含用户项目依赖，序列化库等已声明
/// 依赖的导入与引用在虚拟项目中必然无法解析。当消息中的候选包名
/// 出现在用户项目最近 Cargo.toml 的依赖表（dependencies/dev/build、
/// 工作区、目标平台各节；`-`/`_` 与大小写归一化，另含去全部
/// 分隔符的紧致形式——连字符包名的库名为去分隔符形式）中时，判定为虚拟项目
/// 固有误报并过滤；未列入依赖表的名字（拼写错误、真正缺失的库）保留
/// 诊断，继续提供 `rzc add` 教学提示。
pub(super) fn 是缺项目依赖(
    诊断值: &Value,
    条目值: Option<&crate::翻译缓存::翻译条目>,
) -> bool {
    let Some(条目) = 条目值 else {
        return false;
    };
    let 消息 = 诊断值
        .get("message")
        .and_then(|值| 值.as_str())
        .unwrap_or("");
    // 仅处理“导入/引用未声明包”类消息，避免误伤同名变量/函数的诊断
    let 是导入类 = 消息.starts_with("unresolved import `")
        || 消息.starts_with("cannot find module or crate `")
        || 消息.contains("use of undeclared crate or module `");
    if !是导入类 {
        return false;
    }
    let Some(依赖集) = 项目依赖名(&条目.原始路径) else {
        return false;
    };
    i18n_rust_engine::诊断::取反引号首段(消息)
        .iter()
        .any(|段| 段.is_ascii() && 包名形式(段).iter().any(|形式| 依赖集.contains(形式)))
}

/// 过滤“内嵌字符串与内嵌字节资源在虚拟项目中缺失”的误报
///
/// 虚拟 .rs 位于 `/tmp/i18n_lsp_virtual_*/src/` 下，`include_str!("界面.html")`
/// 相对虚拟目录解析必然失败（rust-analyzer 报无法读取源目录下的界面文件，
/// 路径按虚拟项目根显示）。去掉源目录前缀后能在原方言文件同目录找到该
/// 文件时判定为误报；原项目真实缺失该资源时诊断保留。
pub(super) fn 是缺内嵌资源(
    诊断值: &Value,
    条目值: Option<&crate::翻译缓存::翻译条目>,
) -> bool {
    let Some(条目) = 条目值 else {
        return false;
    };
    let 消息 = 诊断值
        .get("message")
        .and_then(|值| 值.as_str())
        .unwrap_or("");
    let Some((路径文本, _)) = 消息
        .split("couldn't read `")
        .nth(1)
        .and_then(|剩余| 剩余.split_once('`'))
    else {
        return false;
    };
    if 路径文本.is_empty() || 路径文本.contains("..") {
        return false;
    }
    let Some(目录) = 条目.原始路径.parent() else {
        return false;
    };
    let 路径值 = Path::new(路径文本);
    // 虚拟项目根相对路径（如源目录下的界面文件）：去源目录前缀后按原目录解析
    let 相对路径 = 路径值.strip_prefix("src").unwrap_or(路径值);
    目录.join(相对路径).is_file()
}

/// rust-analyzer 编译级诊断抑制判定：文件位于 cargo 项目时，分析器的
/// E 系列错误（严重级 1）与提示（严重级 4）不转发给客户端，
/// 编译级诊断以代理自跑的镜像/虚拟项目 cargo 检查（rustc 口径）为权威来源
///
/// 分析器在虚拟项目上对第三方依赖与过程宏的类型推断恒为假红（依赖缺失的
/// E0432/E0599、派生不展开的 E0277 显示特征级联等），且其原生诊断与
/// 真错无法从消息文本可靠区分；镜像检查（或不可用时的虚拟检查）在
/// didOpen/didSave 时提供与 rzc 构建同口径的权威结果。E 系列提示是
/// 编译错误的伴生信息拆分发布（“找不到外部包”、“由格式化
/// 参数”等），同为编译级判定的产物，一并抑制。非 cargo 项目
/// （单文件教学场景）不受影响，分析器诊断全量保留；非 E 系列（语法错误、
/// 分析器内部错误等）与警告（严重级 2，可能有价值）一律保留。
pub(crate) fn 抑制分析器编译错误(诊断值: &Value, 原始路径: &Path) -> bool {
    let 严重级 = 诊断值
        .get("severity")
        .and_then(|值| 值.as_u64())
        .unwrap_or(0);
    if 严重级 != 1 && 严重级 != 4 {
        return false;
    }
    let Some(错误码) = 诊断值.get("code").and_then(|值| 值.as_str()) else {
        return false;
    };
    if !是编译器错误码(错误码) {
        return false;
    }
    在货物项目(原始路径)
}

/// E 系列 rustc 错误码（E0432/E0277 等；分析器自带诊断沿用该格式）
fn 是编译器错误码(错误码: &str) -> bool {
    let Some(数字串) = 错误码.strip_prefix('E') else {
        return false;
    };
    !数字串.is_empty() && 数字串.len() <= 4 && 数字串.chars().all(|字符值| 字符值.is_ascii_digit())
}

/// 文件路径 → 是否位于 cargo 项目（最近上层存在 Cargo.toml）的进程级缓存
static 在货物项目缓存: OnceLock<Mutex<HashMap<PathBuf, bool>>> = OnceLock::new();

/// 判断文件是否位于 cargo 项目（向上查找最近 Cargo.toml），按路径缓存
fn 在货物项目(文件路径: &Path) -> bool {
    let 缓存值 = 在货物项目缓存.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(值) = 缓存值
        .lock()
        .unwrap_or_else(|错误值| 错误值.into_inner())
        .get(文件路径)
    {
        return *值;
    }
    let mut 目录 = 文件路径.parent();
    let mut 已有清单 = false;
    while let Some(目录值) = 目录 {
        if 目录值.join("Cargo.toml").is_file() {
            已有清单 = true;
            break;
        }
        目录 = 目录值.parent();
    }
    缓存值
        .lock()
        .unwrap_or_else(|错误值| 错误值.into_inner())
        .insert(文件路径.to_path_buf(), 已有清单);
    已有清单
}

/// 依赖清单解析结果缓存（Cargo.toml 路径 → (修改时间, 归一化依赖名集合)）
type 项目依赖缓存 = Mutex<HashMap<PathBuf, (SystemTime, Arc<HashSet<String>>)>>;

/// 项目依赖清单修改时间缓存（键为 Cargo.toml 路径）
static 项目依赖缓存槽: OnceLock<项目依赖缓存> = OnceLock::new();

/// 向上查找最近的 Cargo.toml 并解析依赖名集合（归一化），按修改时间缓存；
/// 未找到清单时返回 无（调用方保持原诊断行为）
fn 项目依赖名(文件路径: &Path) -> Option<Arc<HashSet<String>>> {
    let mut 目录 = 文件路径.parent();
    let 清单路径 = loop {
        let 目录值 = 目录?;
        let 候选路径 = 目录值.join("Cargo.toml");
        if 候选路径.is_file() {
            break 候选路径;
        }
        目录 = 目录值.parent();
    };
    let 修改时间值 = std::fs::metadata(&清单路径).ok()?.modified().ok()?;
    let 缓存值 = 项目依赖缓存槽.get_or_init(|| Mutex::new(HashMap::new()));
    let mut 守卫 = 缓存值.lock().unwrap_or_else(|错误值| 错误值.into_inner());
    if let Some((缓存时间, 依赖集)) = 守卫.get(&清单路径)
        && *缓存时间 == 修改时间值
    {
        return Some(依赖集.clone());
    }
    let 文本串 = std::fs::read_to_string(&清单路径).ok()?;
    let 依赖集 = Arc::new(解析依赖名(&文本串));
    守卫.insert(清单路径, (修改时间值, 依赖集.clone()));
    Some(依赖集)
}

/// 解析 Cargo.toml 中的全部依赖名（含 dev/build、工作区、目标平台各节）
fn 解析依赖名(文本串: &str) -> HashSet<String> {
    let mut 名称集 = HashSet::new();
    // toml 1.x 的 `Value::from_str` 解析的是「单个值」而非「整个文档」
    let Ok(文档值) = toml::from_str::<toml::Value>(文本串) else {
        return 名称集;
    };
    let mut 收集 = |表格选项: Option<&toml::Value>| {
        if let Some(表格值) = 表格选项.and_then(|值| 值.as_table()) {
            名称集.extend(表格值.keys().flat_map(|键名| 包名形式(键名)));
        }
    };
    for 节名 in ["dependencies", "dev-dependencies", "build-dependencies"] {
        收集(文档值.get(节名));
    }
    收集(
        文档值
            .get("workspace")
            .and_then(|工作区| 工作区.get("dependencies")),
    );
    if let Some(目标表) = 文档值.get("target").and_then(|目标| 目标.as_table()) {
        for (_, 配置表) in 目标表 {
            for 节名 in ["dependencies", "dev-dependencies", "build-dependencies"] {
                收集(配置表.get(节名));
            }
        }
    }
    名称集
}

/// 依赖名归一化：Cargo 视 `-`/`_` 等价，包名统一小写比较
fn 归一包名(名称: &str) -> String {
    名称.to_ascii_lowercase().replace('-', "_")
}

/// 包名比对候选集：归一形式外另收录去全部分隔符的紧致形式——
/// 包名与实际库名可能只在紧致意义上对应（连字符包名的库名为去分隔符形式，
/// 代码中 `使用 md5::...` 而清单声明 `md-5 = "0.10"`）
fn 包名形式(名称: &str) -> Vec<String> {
    let 下划线形式 = 归一包名(名称);
    let 紧致形式: String = 下划线形式.chars().filter(|字符值| *字符值 != '_').collect();
    if 紧致形式 == 下划线形式 {
        vec![下划线形式]
    } else {
        vec![下划线形式, 紧致形式]
    }
}

/// 从 LSP 诊断（rust-analyzer 格式）中提取所有权错误详情
///
/// 变量名取自原始消息中的反引号（如“移动值的使用”消息中的变量名）；
/// 移动/借用/再次使用位置取自已还原到母语文件的范围与关联信息，
/// LSP 的 0 起行号统一转为 1 起（与 rustc 诊断一致）。
/// 仅处理 E0382/E0502/E0507 及消息模式匹配的所有权错误。
pub(crate) fn 提取所有权详情(
    原始诊断: &Value,
    已还原: &Value,
    原始资源标识: &str,
) -> Option<所有权详情> {
    let 消息 = 原始诊断["message"].as_str()?;
    let 错误码 = 原始诊断["code"].as_str().unwrap_or("");
    let 是所有权错误 = matches!(错误码, "E0382" | "E0502" | "E0507")
        || 消息.contains("use of moved value")
        || 消息.contains("moved value")
        || 消息.contains("cannot borrow")
        || 消息.contains("cannot move out of");
    if !是所有权错误 {
        return None;
    }

    let 变量名 = 提取反引号变量名(消息)?;
    let 主位置 = 从范围构造位置(原始资源标识, &已还原["range"]);

    let mut 移动位置 = None;
    let mut 借用位置 = None;
    let mut 再次使用位置 = None;
    // E0502 的双方按标签专门认（见 借用冲突）：后方粗分类会把被拒的
    // 可变借用当成「借用发生」，与「仍在用」行号读反
    let mut 带标签: Vec<(&str, 诊断位置)> = Vec::new();

    if let Some(关联信息) = 已还原["relatedInformation"].as_array() {
        for 项 in 关联信息 {
            let 标签 = 项["message"].as_str().unwrap_or("");
            let Some(位置值) = 从范围构造位置(原始资源标识, &项["location"]["range"])
            else {
                continue;
            };
            带标签.push((标签, 位置值.clone()));
            // 注意顺序：“后续使用借用”同时含借用与再次使用，应归为再次使用
            if 标签.contains("used here")
                || 标签.contains("later used")
                || 标签.contains("after move")
            {
                再次使用位置.get_or_insert(位置值);
            } else if 标签.contains("move") {
                移动位置.get_or_insert(位置值);
            } else if 标签.contains("borrow") {
                借用位置.get_or_insert(位置值);
            }
        }
    }
    // 与引擎同口径：只对 E0502（或消息形态命中）认冲突双方；
    // rust-analyzer 的主范围不带标签，而它就是被拒绝的可变借用——兜底补上
    let 借用冲突值 = if matches!(错误码, "E0502") || 消息.contains("cannot borrow") {
        let (可变方, 不可变方) = 借用冲突::归类带标签跨度(带标签);
        借用冲突::自两端(可变方.or_else(|| 主位置.clone()), 不可变方)
    } else {
        None
    };

    // 主范围兜底：E0382 → 再次使用；E0502 → 借用发生；E0507 → 移动发生
    if let Some(位置值) = 主位置 {
        if matches!(错误码, "E0382") || 消息.contains("moved value") {
            再次使用位置.get_or_insert(位置值);
        } else if matches!(错误码, "E0502") || 消息.contains("cannot borrow") {
            借用位置.get_or_insert(位置值);
        } else if matches!(错误码, "E0507") || 消息.contains("cannot move out of") {
            移动位置.get_or_insert(位置值);
        }
    }

    if 移动位置.is_none() && 借用位置.is_none() && 再次使用位置.is_none() {
        return None;
    }
    Some(所有权详情 {
        变量名,
        移动发生: 移动位置,
        借用发生: 借用位置,
        再次使用: 再次使用位置,
        借用冲突: 借用冲突值,
    })
}

/// 提取消息中反引号包裹的变量名
///
/// 示例："use of moved value: `数据`" → "数据"。
fn 提取反引号变量名(消息: &str) -> Option<String> {
    let 起始位置 = 消息.find('`')?;
    let 剩余 = &消息[起始位置 + 1..];
    let 结束位置 = 剩余.find('`')?;
    Some(剩余[..结束位置].to_string())
}

/// 从 LSP 范围构造诊断位置（0 起行号转为 1 起）
fn 从范围构造位置(文件名称: &str, 范围值: &Value) -> Option<诊断位置> {
    let 起始行 = 范围值["start"]["line"].as_u64()? as u32;
    let 起始列 = 范围值["start"]["character"].as_u64()? as u32;
    let 结束行 = 范围值["end"]["line"].as_u64()? as u32;
    let 结束列 = 范围值["end"]["character"].as_u64()? as u32;
    Some(诊断位置 {
        文件名称: 文件名称.to_string(),
        起始行号: 起始行 + 1,
        起始列号: 起始列 + 1,
        结束行号: 结束行 + 1,
        结束列号: 结束列 + 1,
        源码文本: None,
        标签文本: None,
        是否主因: false,
    })
}

/// 计算文档的全角标点教学诊断（方言坐标，严重级为提示）
///
/// 在转译后的虚拟文本上扫描：代码位置的全角标点在转译中保留（映射表
/// 不含全角字符），字符串/注释内的全角标点是合法内容且被扫描器跳过；
/// 扫描得到虚拟坐标（转译不改变行结构，代码位置为 ASCII，字符列即
/// UTF-16 列），再经列映射还原为方言坐标，与 RA/内置诊断坐标系一致。
pub(crate) fn 全角诊断(条目值: &翻译条目) -> Vec<Value> {
    i18n_rust_engine::全角标点::检测全角标点(&条目值.英文内容)
        .iter()
        .map(|警告项| {
            let 行号 = (警告项.行号 - 1) as u32;
            let 英文列 = (警告项.列序 - 1) as u32;
            let 母语列 = 英文列转母语列(条目值, 行号, 英文列);
            serde_json::json!({
                "range": {
                    "start": { "line": 行号, "character": 母语列 },
                    "end": { "line": 行号, "character": 母语列 + 1 }
                },
                "severity": 3,
                "code": "fullwidth",
                "source": "i18n-rust",
                "message": 警告项.展示行(),
                // 修复动作数据：全角字符与建议的半角字符（供代码操作注入）
                "data": {
                    "character": 警告项.涉事字符.to_string(),
                    "replacement": 警告项.替代字符.map(|字符值| 字符值.to_string())
                }
            })
        })
        .collect()
}

/// 计算文档的教学检查诊断（方言坐标，严重级为提示）
///
/// 直接在母语原文上扫描（`让` 等关键字在转译后已不存在）：行列均为
/// 字符计数，中文代码在基本平面内字符列即 UTF-16 列，直接转换即可。
/// 分桶索引参数为易混方法名的索引（管理器缓存一次，含精确命中表），
/// 供易混方法名提示判定（#14）；歧义构造器参数为推导歧义构造器
/// 被调名集（新建/缺省及其方言词），仅类型无法自行推导的初始化式才提示
/// 未标注类型；关键字映射参数供保留字撞名提示（声明名字转译后是否为 Rust 保留字）。
///
/// 三组警告由一次扫描产出（顺序：教学 / 英文回落 / 保留字撞名，与早先
/// 三个独立入口依次扩展的顺序一致），与 CLI 同源。
pub(crate) fn 教学检查诊断(
    条目值: &翻译条目,
    分桶: &i18n_rust_engine::校验::词形分桶,
    歧义构造器: &HashSet<String>,
    回落映射: &HashMap<String, String>,
    关键词表: &HashMap<String, String>,
) -> Vec<Value> {
    let 字符们: Vec<char> = 条目值.母语内容.chars().collect();
    i18n_rust_engine::校验::按词形桶检查全部教学字符(
        &字符们,
        分桶,
        歧义构造器,
        回落映射,
        关键词表,
    )
    .iter()
    .map(|警告项| {
        let 行号 = (警告项.行号 - 1) as u32;
        let 列号 = (警告项.列号 - 1) as u32;
        serde_json::json!({
            "range": {
                "start": { "line": 行号, "character": 列号 },
                "end": { "line": 行号, "character": 列号 + 1 }
            },
            "severity": 3,
            "code": match 警告项.告警种类 {
                i18n_rust_engine::校验::告警类别::未标注类型 => "lint-untyped-let",
                i18n_rust_engine::校验::告警类别::魔法数字 => "lint-magic-number",
                i18n_rust_engine::校验::告警类别::嵌套过深 => "lint-deep-indent",
                i18n_rust_engine::校验::告警类别::易混标识符 => "lint-confusable-method",
                i18n_rust_engine::校验::告警类别::英文回退 => "lint-english-fallback",
                i18n_rust_engine::校验::告警类别::保留名撞词 => "lint-reserved-name-collision",
            },
            "source": "i18n-rust",
            "message": 警告项.生成说明()
        })
    })
    .collect()
}

/// 注入教学诊断（全角标点 + 教学检查）到方言坐标的诊断列表
///
/// 三条发布路径共用：rust-analyzer 诊断链（诊断发布处理）、
/// 虚拟项目检查、真实项目镜像检查。教学诊断由条目内容直接计算，
/// 不依赖内置缓存（缓存有时序窗口：代理自跑检查先于分析器首批发布
/// 时缓存为空，教学诊断会丢失）。注入前先移除旧的教学诊断（按
/// 诊断码与来源识别），保证与文档最新内容一致。
pub(crate) fn 注入教学诊断(
    诊断们: &mut Vec<Value>,
    条目值: &翻译条目,
    分桶: &i18n_rust_engine::校验::词形分桶,
    歧义构造器: &HashSet<String>,
    回落映射: &HashMap<String, String>,
    关键词表: &HashMap<String, String>,
) {
    诊断们.retain(|诊断值| !(是教学诊断(诊断值) && 诊断值["source"].as_str() == Some("i18n-rust")));
    诊断们.extend(全角诊断(条目值));
    if 教学检查已启用() {
        诊断们.extend(教学检查诊断(
            条目值,
            分桶,
            歧义构造器,
            回落映射,
            关键词表,
        ));
    }
}

/// 教学诊断开关：默认开启；`RZ_LSP_TEACHING_LINT=off` 关闭
///（重度开发者不需要教学提示时避免诊断噪音）
pub(crate) fn 教学检查已启用() -> bool {
    std::env::var("RZ_LSP_TEACHING_LINT")
        .map(|值| 值 != "off" && 值 != "0")
        .unwrap_or(true)
}

/// 判断诊断是否为我方注入的教学诊断（全角标点/教学检查）
pub(crate) fn 是教学诊断(诊断值: &Value) -> bool {
    诊断值["code"]
        .as_str()
        .is_some_and(|代码| 代码 == "fullwidth" || 代码.starts_with("lint-"))
}

#[cfg(test)]
mod 母语测试 {
    use super::{
        包名形式, 抑制分析器编译错误, 是编译器错误码, 查找期望实得标签, 解析依赖名
    };

    /// 分析器把期望与实际放在关联信息，应被归集为主消息标签
    #[test]
    fn 从关联信息查找期望实得标签() {
        let 诊断值 = serde_json::json!({
            "message": "mismatched types",
            "code": "E0308",
            "relatedInformation": [
                { "message": "expected `i32`, found `String`" },
            ]
        });
        assert_eq!(
            查找期望实得标签(&诊断值).as_deref(),
            Some("expected `i32`, found `String`")
        );
        // 无 expected/found 类标签时返回 无（渲染器自动回退消息表）
        let 普通值 = serde_json::json!({
            "message": "use of moved value: `x`",
            "relatedInformation": [ { "message": "moved value used here after move" } ]
        });
        assert_eq!(查找期望实得标签(&普通值), None);
        // 无关联信息也不崩
        assert_eq!(
            查找期望实得标签(&serde_json::json!({ "message": "x" })),
            None
        );
    }

    /// 紧致形式：连字符包名 → 下划线形式与去分隔符形式
    #[test]
    fn 包名形式紧致() {
        let 形式集 = 包名形式("md-5");
        assert!(形式集.contains(&"md_5".to_string()));
        assert!(形式集.contains(&"md5".to_string()));
        // 无分隔符的名字保持单一形式
        assert_eq!(包名形式("rayon"), vec!["rayon".to_string()]);
    }

    /// 依赖清单解析收录紧致形式，供 md-5 风格的包名匹配
    #[test]
    fn 解析依赖名紧致() {
        let 依赖集 = 解析依赖名("[dependencies]\nmd-5 = \"0.10\"\n");
        assert!(依赖集.contains("md_5"));
        assert!(依赖集.contains("md5"));
        assert!(!依赖集.contains("serde"));
    }

    /// E 系列错误码识别（E + 至多 4 位数字）
    #[test]
    fn 编译器错误码识别() {
        assert!(是编译器错误码("E0432"));
        assert!(是编译器错误码("E0277"));
        assert!(!是编译器错误码("e0432"));
        assert!(!是编译器错误码("E"));
        assert!(!是编译器错误码("E04a2"));
        assert!(!是编译器错误码("unused_imports"));
    }

    /// cargo 项目内的 E 系列错误抑制；项目外/非 E 码/非错误级不抑制
    #[test]
    fn 抑制分析器编译错误用例() {
        let 进程号值 = std::process::id();
        let 目录 = std::env::temp_dir().join(format!("diag_text_suppress_{进程号值}"));
        let 源目录 = 目录.join("src");
        std::fs::create_dir_all(&源目录).unwrap();
        std::fs::write(目录.join("Cargo.toml"), "[package]\nname = \"t\"\n").unwrap();
        let 文件路径 = 源目录.join("main.zh");
        std::fs::write(&文件路径, "").unwrap();

        let 错误值 = serde_json::json!({ "severity": 1, "code": "E0432", "message": "unresolved import `md5`" });
        assert!(抑制分析器编译错误(&错误值, &文件路径));
        // E 系列提示（编译错误的伴生信息拆分发布）同样抑制
        let 提示值 = serde_json::json!({ "severity": 4, "code": "E0432", "message": "no external crate `toml`" });
        assert!(抑制分析器编译错误(&提示值, &文件路径));
        // 警告不抑制（可能有价值）
        let 警告值 = serde_json::json!({ "severity": 2, "code": "E0432" });
        assert!(!抑制分析器编译错误(&警告值, &文件路径));
        // 非 E 系列（语法错误等）不抑制
        let 语法值 = serde_json::json!({ "severity": 1, "code": "syntax" });
        assert!(!抑制分析器编译错误(&语法值, &文件路径));
        // 非 E 系列提示不抑制（如主函数提示）
        let 普通提示 = serde_json::json!({ "severity": 4, "code": "unused" });
        assert!(!抑制分析器编译错误(&普通提示, &文件路径));
        // 非 cargo 项目（单文件教学场景）不抑制
        let 外部路径 = std::env::temp_dir().join(format!("diag_text_suppress_out_{进程号值}.zh"));
        std::fs::write(&外部路径, "").unwrap();
        assert!(!抑制分析器编译错误(&错误值, &外部路径));
        assert!(!抑制分析器编译错误(&提示值, &外部路径));

        let _ = std::fs::remove_dir_all(&目录);
        let _ = std::fs::remove_file(&外部路径);
    }

    // ==================== 噪声过滤器对偶测试 ====================
    //
    // 虚拟项目的 5 个过滤函数按消息文本判定"恒为误报"，两个失败方向
    // 都有真实代价：漏过滤 → 噪声淹没用户；误杀 → 真错被静默吞掉且用户
    // 无从知晓。本组测试钉住每条正例（固有误报应滤）与反例（真错应留），
    // rustc/RA 消息格式漂移时在这里第一时间暴露（文本模式匹配的回归面）。

    use super::{
        是主函数提示, 是未打开模块引用, 是缺依赖噪音, 是缺内嵌资源, 是缺项目依赖, 未打开模块名,
        翻译条目,
    };
    use std::collections::HashSet;
    use std::path::PathBuf;

    /// 构造指向临时目录方言文件的缓存条目（仅原始路径参与判定）
    fn 构造条目(原始路径: PathBuf) -> 翻译条目 {
        翻译条目 {
            原始资源标识: format!("file://{}", 原始路径.display()),
            原始路径,
            母语内容: String::new(),
            英文内容: String::new(),
            分析器内容: String::new(),
            虚拟资源标识: String::new(),
            虚拟路径: PathBuf::from("virtual_main.rs"),
            行映射: Vec::new(),
            列位映射: crate::翻译缓存::列位映射::default(),
            新增包前缀词汇: HashSet::new(),
            文档版本: 1,
            已打开: true,
            源元数据: None,
            已声明: None,
        }
    }

    /// 正例：主函数伴生提示被过滤；反例：非提示级/无关消息一律保留
    #[test]
    fn 主函数提示对偶() {
        let 提示值 = serde_json::json!({
            "severity": 4,
            "message": "here is a function named `main`, but it is not the real entrypoint"
        });
        assert!(是主函数提示(&提示值, "file:///v/main.rs"));
        let 从未使用 = serde_json::json!({
            "severity": 4,
            "message": "function `main` is never used"
        });
        assert!(是主函数提示(&从未使用, "file:///v/main.rs"));
        // 反例 1：同文案但错误级——严重级闸必须拦住，不得静默吞掉真错
        let 作为错误 = serde_json::json!({
            "severity": 1,
            "message": "function `main` is never used"
        });
        assert!(!是主函数提示(&作为错误, "file:///v/main.rs"));
        // 反例 2：无关的 never-used 提示（其它函数）必须保留
        let 其它值 = serde_json::json!({
            "severity": 4,
            "message": "function `helper` is never used"
        });
        assert!(!是主函数提示(&其它值, "file:///v/main.rs"));
        // 反例 3：无严重级字段不视为提示
        assert!(!是主函数提示(
            &serde_json::json!({ "message": "function `main` is never used" }),
            ""
        ));
    }

    /// 正例：过程宏未展开的辅助属性与派生宏缺失被过滤；
    /// 反例：未解析导入与找不到模块不过滤（"添加依赖"快速修复
    /// 的输入，且真实缺依赖时同样出现）
    #[test]
    fn 缺依赖噪音对偶() {
        assert!(是缺依赖噪音(
            &serde_json::json!({ "message": "cannot find attribute `arg` in this scope" })
        ));
        assert!(是缺依赖噪音(
            &serde_json::json!({ "message": "cannot find derive macro `Parser` in this scope" })
        ));
        // 反例：用户真写错了宏名——必须保留
        assert!(!是缺依赖噪音(
            &serde_json::json!({ "message": "cannot find macro `prinln` in this scope" })
        ));
        // 反例：未解析导入走缺项目依赖精细判定，
        // 不在本过滤器一刀切（行为钉住，防将来误扩此处的匹配面）
        assert!(!是缺依赖噪音(
            &serde_json::json!({ "message": "unresolved import `serde`" })
        ));
    }

    /// 候选名提取：四种消息格式 × 防御性拒绝（保留字/路径穿越/畸形段）
    #[test]
    fn 未打开模块名解析() {
        // 四种已知 RA/rustc 消息格式（0.3.3025 前后两代）
        assert_eq!(
            未打开模块名("cannot find module or crate `日志设置` in this scope"),
            Some("日志设置")
        );
        assert_eq!(
            未打开模块名("cannot find `日志设置` in `crate`"),
            Some("日志设置")
        );
        assert_eq!(
            未打开模块名("unresolved import `crate::日志设置::初始化`"),
            Some("日志设置")
        );
        assert_eq!(未打开模块名("no `日志设置` in the root"), Some("日志设置"));
        // 保留字/系统包不可能是本地模块
        assert_eq!(未打开模块名("cannot find `std` in `crate`"), None);
        assert_eq!(未打开模块名("no `core` in the root"), None);
        // 路径穿越防御：含 / \ .. 的段一律拒绝
        assert_eq!(未打开模块名("unresolved import `crate/../秘密`"), None);
        assert_eq!(未打开模块名("cannot find `a/b` in `crate`"), None);
        // 未知消息格式不强行解释（宁漏滤不误杀）
        assert_eq!(
            未打开模块名("expected one of `::`, found keyword`fn`"),
            None
        );
        // 非包后缀的“在……找不到 X”不命中
        assert_eq!(未打开模块名("cannot find `foo` in `self`"), None);
    }

    /// 正例：同目录存在同名方言文件 → "模块未打开"误报被滤；
    /// 反例：文件确实不存在（真拼错）→ 诊断保留供用户修正
    #[test]
    fn 未打开模块引用对偶() {
        let 临时目录 = tempfile::tempdir().unwrap();
        let 主程序路径 = 临时目录.path().join("主程序.zh");
        std::fs::write(&主程序路径, "").unwrap();
        std::fs::write(临时目录.path().join("日志设置.zh"), "").unwrap();
        let 条目值 = 构造条目(主程序路径.clone());
        let 构造诊断 = |消息: &str| serde_json::json!({ "message": 消息, "severity": 1 });

        // 正例：模块文件存在但未打开
        assert!(是未打开模块引用(
            &构造诊断("cannot find module or crate `日志设置` in this scope"),
            Some(&条目值)
        ));
        assert!(是未打开模块引用(
            &构造诊断("unresolved import `crate::日志设置`"),
            Some(&条目值)
        ));
        // 反例 1：目标文件不存在（真错误）——必须保留
        assert!(!是未打开模块引用(
            &构造诊断("cannot find module or crate `日志设置2` in this scope"),
            Some(&条目值)
        ));
        // 反例 2：同名词是 .rs 而非 .zh（扩展名隔离，不以其它类型文件为准）
        std::fs::write(临时目录.path().join("工具.rs"), "").unwrap();
        assert!(!是未打开模块引用(
            &构造诊断("cannot find module or crate `工具` in this scope"),
            Some(&条目值)
        ));
        // 反例 3：无缓存条目（非代理托管文件）不过滤
        assert!(!是未打开模块引用(
            &构造诊断("cannot find module or crate `日志设置` in this scope"),
            None
        ));
    }

    /// 正例：候选包名在用户 Cargo.toml 依赖表 → 虚拟项目缺失误报被滤；
    /// 反例：不在依赖表（真拼错/真没加依赖）→ 保留并继续给 rzc 添加依赖提示
    #[test]
    fn 缺项目依赖对偶() {
        let 临时目录 = tempfile::tempdir().unwrap();
        std::fs::write(
            临时目录.path().join("Cargo.toml"),
            "[dependencies]\nserde = \"1\"\nserde_json = \"1\"\n",
        )
        .unwrap();
        let 主程序路径 = 临时目录.path().join("主程序.zh");
        std::fs::write(&主程序路径, "").unwrap();
        let 条目值 = 构造条目(主程序路径.clone());
        let 构造诊断 = |消息: &str| serde_json::json!({ "message": 消息, "severity": 1 });

        // 正例：已声明依赖在虚拟项目缺失
        assert!(是缺项目依赖(
            &构造诊断("unresolved import `serde`"),
            Some(&条目值)
        ));
        assert!(是缺项目依赖(
            &构造诊断("use of undeclared crate or module `serde_json`"),
            Some(&条目值)
        ));
        // 反例 1：不在依赖表的名字是真错误，必须保留
        assert!(!是缺项目依赖(
            &构造诊断("unresolved import `serde_mistyped`"),
            Some(&条目值)
        ));
        // 反例 2：非导入类消息不碰（即使含同名反引号段）
        assert!(!是缺项目依赖(
            &构造诊断("no function or associated item named `serde` found"),
            Some(&条目值)
        ));
        // 反例 3：无 Cargo.toml 可寻（隔离目录上层无清单）时保持原样不过滤；
        // 注：本断言依赖临时目录祖先链无 Cargo.toml，系统临时区成立
        let 孤立目录 = tempfile::tempdir().unwrap();
        let 孤立路径 = 孤立目录.path().join("x.zh");
        std::fs::write(&孤立路径, "").unwrap();
        let 孤立条目 = 构造条目(孤立路径);
        assert!(!是缺项目依赖(
            &构造诊断("unresolved import `serde`"),
            Some(&孤立条目)
        ));
    }

    /// 正例：资源文件在原目录存在（虚拟目录相对路径解析失败）被滤；
    /// 反例：原项目真实缺资源 → 诊断必须保留
    #[test]
    fn 缺内嵌资源对偶() {
        let 临时目录 = tempfile::tempdir().unwrap();
        let 主程序路径 = 临时目录.path().join("主程序.zh");
        std::fs::write(&主程序路径, "").unwrap();
        std::fs::write(临时目录.path().join("界面.html"), "<html>").unwrap();
        let 条目值 = 构造条目(主程序路径);
        let 构造诊断 = |消息: &str| serde_json::json!({ "message": 消息, "severity": 1 });

        // 正例：虚拟根源目录前缀与无前缀两种形态
        assert!(是缺内嵌资源(
            &构造诊断("couldn't read `src/界面.html`: 系统找不到指定的文件。 (os error 2)"),
            Some(&条目值)
        ));
        assert!(是缺内嵌资源(
            &构造诊断("couldn't read `界面.html`: No such file or directory (os error 2)"),
            Some(&条目值)
        ));
        // 反例 1：原目录也没有该资源——真错误，必须保留
        assert!(!是缺内嵌资源(
            &构造诊断("couldn't read `src/不存在.html`: No such file or directory"),
            Some(&条目值)
        ));
        // 反例 2：路径穿越防御
        assert!(!是缺内嵌资源(
            &构造诊断("couldn't read `../界面.html`"),
            Some(&条目值)
        ));
        // 反例 3：非“无法读取”格式不命中
        assert!(!是缺内嵌资源(
            &构造诊断("file not found: 界面.html"),
            Some(&条目值)
        ));
    }
}
