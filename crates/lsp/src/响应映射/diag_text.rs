//! 诊断消息翻译与所有权详情提取（CLI 同源消息表）。
//!
//! 由服务器启动时初始化错误消息翻译器（语言包 errors.toml，缺失时
//! 回退引擎内嵌 zh），为 publishDiagnostics 提供消息中文化与
//! 所有权错误的叙事化详情。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use i18n_rust_engine::诊断::{所有权详情, 诊断位置};
use serde_json::{Value, json};

use crate::翻译缓存::{TranslationEntry, 英文列转中文列};

/// 诊断翻译器（errors.toml 消息表 + 错误码表 + type_map）：与 CLI 完全同源，
/// 走引擎 `渲染主消息` 统一口径，覆盖 rustc/rust-analyzer 的常见消息。
/// 由服务器启动时初始化（语言包目录 errors.toml + 映射管理器，缺失时回退内置
/// zh）；未初始化时为 None，翻译退化为下方轻量短语替换。
static 诊断翻译器槽: std::sync::OnceLock<Option<i18n_rust_engine::诊断::诊断翻译器>> =
    std::sync::OnceLock::new();

/// 初始化诊断翻译器（errors.toml 消息表 + 由映射管理器构建的 type_map；
/// 失败/缺失时回退内置 zh），使 LSP 主消息与 CLI 同 (code, message) 同译文。
pub fn 初始化诊断翻译器(
    语言包路径: &std::path::Path,
    管理器: &i18n_rust_engine::映射管理::映射管理器,
) {
    let 类型映射 = i18n_rust_engine::诊断::构建类型映射(管理器);
    let 译文管理器 = i18n_rust_engine::诊断::错误翻译管理器::自文件加载(
        &语言包路径.join("errors.toml"),
    )
    .ok()
    .or_else(内置中文错误翻译器);
    let 翻译器 =
        译文管理器.map(|m| i18n_rust_engine::诊断::诊断翻译器::新建翻译器(m, 类型映射));
    let _ = 诊断翻译器槽.set(翻译器);
}

/// 物化引擎内嵌的中文 errors.toml 并加载翻译器（语言包目录缺失时兜底）
fn 内置中文错误翻译器() -> Option<i18n_rust_engine::诊断::错误翻译管理器> {
    let 目录 = tempfile::tempdir().ok()?;
    let 内容 = i18n_rust_engine::语言::内置语言文件("zh")
        .iter()
        .find(|(f, _)| *f == "errors.toml")?
        .1;
    std::fs::write(目录.path().join("errors.toml"), 内容).ok()?;
    i18n_rust_engine::诊断::错误翻译管理器::自文件加载(
        &目录.path().join("errors.toml"),
    )
    .ok()
}

/// 翻译诊断主消息为当前界面语言（与 CLI 同口径）
///
/// `code`：诊断错误码（若有），供错误码表优先命中；`primary_label`：rustc 主
/// span 标签（镜像检查从真实 rustc JSON 可得，供提取期望/实际类型）。
/// 优先走引擎 `渲染主消息`（错误码表→消息表→占位符回填→类型中文化）；
/// 未命中时退化为轻量短语替换。多行消息按行逐条翻译后拼接（仅首行适用
/// 错误码/标签与教学提示）。仅反引号外文本参与兜底替换，避免误伤标识符。
///
/// pub(crate)：镜像检查（真实项目 rustc 诊断）与 publishDiagnostics 共用同一渲染。
pub(crate) fn 翻译诊断消息(
    错误码: Option<&str>,
    消息: &str,
    主跨度标签: Option<&str>,
) -> String {
    // 多行消息（rust-analyzer 的 E0004 等）逐行翻译；错误码/标签只属于首行
    if 消息.contains('\n') {
        let mut 首行 = true;
        let 行列表: Vec<String> = 消息
            .split('\n')
            .map(|line| {
                let 译文 = if 首行 {
                    翻译诊断消息单行(错误码, line, 主跨度标签, true)
                } else {
                    翻译诊断消息单行(None, line, None, false)
                };
                首行 = false;
                译文
            })
            .collect();
        return 行列表.join("\n");
    }
    翻译诊断消息单行(错误码, 消息, 主跨度标签, true)
}

/// 从 rust-analyzer 诊断里归集含 "expected …, found …" 的候选标签文本。
///
/// rust-analyzer 把类型不匹配的期望/实际放在 `relatedInformation` 而非主
/// span 标签（CLI 的 rustc JSON 则带在主 span label）。本函数从 related
/// 消息中取首个命中者，作为 `渲染主消息` 的 `primary_label`，使 RA
/// 直连路径也能回填 `{期望}`/`{实际}`，与 CLI/镜像检查同一完整译文。
/// 无命中时返回 None（渲染器自动回退消息表，不劣于旧行为）。
pub(crate) fn 查找期望实际标签(诊断: &Value) -> Option<String> {
    诊断
        .get("relatedInformation")
        .and_then(|v| v.as_array())?
        .iter()
        .filter_map(|条目项| 条目项.get("message").and_then(|v| v.as_str()))
        .find(|m| m.contains("expected ") && m.contains(", found "))
        .map(|s| s.to_string())
}

/// 轻量短语替换表（诊断翻译兜底）：UI 全局语言固定，首次构建后缓存。
/// 诊断每次按键都会发布，若每次翻译都重新构建 ~30 对 String 是纯浪费。
static 诊断短语表槽: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();

/// 获取（并惰性构建）轻量短语替换表
fn 诊断短语替换表() -> &'static Vec<(String, String)> {
    诊断短语表槽.get_or_init(|| {
        let 界面 = crate::本地化::全局();
        let mut 替换表: Vec<(String, String)> = vec![
            ("{integer}".to_string(), 界面.取文("diag_rustc_integer")),
            ("{float}".to_string(), 界面.取文("diag_rustc_float")),
            (
                "floating-point number".to_string(),
                界面.取文("diag_rustc_float"),
            ),
            ("integer".to_string(), 界面.取文("diag_rustc_integer")),
        ];

        // 常见错误模式翻译
        let 替换清单 = [
            (
                "cannot find value",
                界面.取文("lsp_phrase_cannot_find_value"),
            ),
            ("cannot find type", 界面.取文("lsp_phrase_cannot_find_type")),
            (
                "cannot find function",
                界面.取文("lsp_phrase_cannot_find_function"),
            ),
            (
                "cannot find module",
                界面.取文("lsp_phrase_cannot_find_module"),
            ),
            ("mismatched types", 界面.取文("lsp_phrase_mismatched_types")),
            ("type mismatch", 界面.取文("lsp_phrase_type_mismatch")),
            ("expected", 界面.取文("lsp_phrase_expected")),
            // "found" 仅在类型不匹配场景（"expected ..., found ..."）译为
            // 「实际为」：以带逗号的长键限定语境——裸词 "found" 会把 E0599 的
            // "no method named ... found for struct ..." 误译为「实际为」
            //（"找到"义），也会与 "method not found" 变体相互干扰
            (", found ", format!("，{} ", 界面.取文("lsp_phrase_found"))),
            ("unused variable", 界面.取文("lsp_phrase_unused_variable")),
            ("unused import", 界面.取文("lsp_phrase_unused_import")),
            ("cannot borrow", 界面.取文("lsp_phrase_cannot_borrow")),
            (
                "borrowed as immutable",
                界面.取文("lsp_phrase_borrowed_immutable"),
            ),
            (
                "borrowed as mutable",
                界面.取文("lsp_phrase_borrowed_mutable"),
            ),
            ("no method named", 界面.取文("lsp_phrase_no_method_named")),
            ("method not found", 界面.取文("lsp_phrase_method_not_found")),
            ("field", 界面.取文("lsp_phrase_field")),
            (
                "does not implement",
                界面.取文("lsp_phrase_does_not_implement"),
            ),
            ("the trait", 界面.取文("lsp_phrase_the_trait")),
            ("is not satisfied", 界面.取文("lsp_phrase_is_not_satisfied")),
            (
                "unresolved import",
                界面.取文("lsp_phrase_unresolved_import"),
            ),
            ("file not found", 界面.取文("lsp_phrase_file_not_found")),
            ("aborting due to", 界面.取文("lsp_phrase_aborting_due_to")),
            ("previous error", 界面.取文("lsp_phrase_previous_error")),
        ];

        for (英文, 本地化) in 替换清单 {
            替换表.push((英文.to_string(), 本地化));
        }
        // 长键优先：按键长降序应用，避免短串先替换打碎长短语
        //（如 "found" 先于 "file not found" 会把后者拆成 "file not 实际为"）。
        // 语境冲突已从源头消解（裸词 "found" 改为 ", found " 语境键），
        // 排序仍作为一般性防御保留。
        替换表.sort_by_key(|a| std::cmp::Reverse(a.0.len()));
        替换表
    })
}

/// 单行诊断主消息渲染：引擎同口径优先，轻量短语表兜底
fn 翻译诊断消息单行(
    错误码: Option<&str>,
    消息: &str,
    主跨度标签: Option<&str>,
    带提示: bool,
) -> String {
    let 界面 = crate::本地化::全局();

    // 1. 引擎同口径渲染（与 CLI 完全同源）：错误码表优先→消息表（精确/前缀/
    //    后缀/通配段）→ {qN}/占位符回填→ type_map 类型中文化。命中任一表即返回。
    if let Some(翻译器) = 诊断翻译器槽.get().and_then(|可选值| 可选值.as_ref())
        && let Some(渲染结果) = 翻译器.渲染主消息(错误码, 消息, 主跨度标签)
    {
        let mut 工作文本 = 渲染结果.主消息文本;
        if 带提示 && let Some(提示) = &渲染结果.教学提示 {
            工作文本.push('\n');
            工作文本.push_str(提示);
        }
        return 工作文本;
    }

    // 2. 轻量短语替换（兜底）：静态表首次构建后缓存，避免每次分配
    let 替换表 = 诊断短语替换表();
    let mut 结果串 = 反引号外替换(消息, 替换表);

    // 添加教学提示
    if 消息.contains("mismatched types") || 消息.contains("type mismatch") {
        结果串.push_str(&界面.取文("lsp_hint_mismatched_types"));
    } else if 消息.contains("cannot find") {
        结果串.push_str(&界面.取文("lsp_hint_cannot_find"));
    } else if 消息.contains("unused") {
        结果串.push_str(&界面.取文("lsp_hint_unused"));
    } else if i18n_rust_engine::诊断::是未解析导入消息(消息) {
        // 未解析导入：提示通过 `rzc add <crate>` 添加缺失依赖
        if let Some(包名) = i18n_rust_engine::诊断::未解析包候选(消息).first() {
            结果串.push_str(&界面.取文带参("lsp_hint_add_dependency", &[包名]));
        }
    }

    结果串
}

/// 整词替换：仅当目标前后字符均非 ASCII 标识符字符时替换，
/// 避免裸词模式误伤标识符子串（如 unexpected 中的 expected）
fn 整词替换(相关文本: &str, 来自: &str, 目标: &str) -> String {
    let 是标识符字符 = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut 结果串 = String::with_capacity(相关文本.len());
    let mut 剩余 = 相关文本;
    while let Some(命中位置) = 剩余.find(来自) {
        let 结束 = 命中位置 + 来自.len();
        let 前合法 = 剩余[..命中位置]
            .chars()
            .next_back()
            .is_none_or(|c| !是标识符字符(c));
        let 后合法 = 剩余[结束..].chars().next().is_none_or(|c| !是标识符字符(c));
        结果串.push_str(&剩余[..命中位置]);
        结果串.push_str(if 前合法 && 后合法 {
            目标
        } else {
            来自
        });
        剩余 = &剩余[结束..];
    }
    结果串.push_str(剩余);
    结果串
}

/// 按顺序对反引号包裹之外的文本应用替换，反引号内的内容（标识符/
/// 类型名引用）保持原样；未成对的反引号后文本仍参与替换
fn 反引号外替换(输入: &str, 替换表: &[(String, String)]) -> String {
    let 应用 = |片段: &str| -> String {
        let mut 工作文本 = 片段.to_string();
        for (来自, 目标) in 替换表 {
            // 单词模式（如 integer/expected）用整词匹配，避免误伤
            // to_integer/unexpected 等标识符子串；短语模式保持子串替换
            工作文本 = if 来自.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                整词替换(&工作文本, 来自, 目标)
            } else {
                工作文本.replace(来自.as_str(), 目标.as_str())
            };
        }
        工作文本
    };

    let mut 结果串 = String::with_capacity(输入.len());
    let mut 剩余 = 输入;
    while let Some(开始) = 剩余.find('`') {
        结果串.push_str(&应用(&剩余[..开始]));
        let 之后 = &剩余[开始 + 1..];
        match 之后.find('`') {
            Some(结束) => {
                // 成对反引号：内部内容原样保留
                结果串.push('`');
                结果串.push_str(&之后[..结束]);
                结果串.push('`');
                剩余 = &之后[结束 + 1..];
            }
            None => {
                // 未成对：剩余文本照常替换
                结果串.push('`');
                结果串.push_str(&应用(之后));
                剩余 = "";
                break;
            }
        }
    }
    结果串.push_str(&应用(剩余));
    结果串
}

/// 判断一条诊断是否为虚拟项目 main.rs 中关于 main 函数的 Hint 级提示
///
/// 虚拟项目将用户代码作为模块聚合到 main.rs 中，`fn main()` 在模块内
/// 并非真正的程序入口，rust-analyzer 会发出 "here is a function named `main`"
/// 等教学无关的提示。此函数识别并过滤这类诊断。
pub(super) fn 是主函数提示(诊断: &Value, _虚拟资源定位: &str) -> bool {
    // 仅过滤 Hint 级别（severity = 4）
    let 严重度 = 诊断.get("severity").and_then(|v| v.as_u64()).unwrap_or(0);
    if 严重度 != 4 {
        return false;
    }
    let 消息 = 诊断.get("message").and_then(|v| v.as_str()).unwrap_or("");
    // 过滤 "here is a function named `main`" 类提示
    消息.contains("here is a function named `main`")
        || (消息.contains("function `main`") && 消息.contains("never used"))
}

/// 过滤虚拟项目固有的过程宏误报（#3 症状二）
///
/// 虚拟项目禁用了过程宏（`procMacro.enable=false`）且不含第三方依赖，
/// `#[派生(解析器)]`→`#[derive(Parser)]`（clap 等）无法展开，其辅助属性与
/// 派生宏的名字解析必然失败：
/// - `cannot find attribute `arg``（helper attribute，如 `#[arg(长参数)]`）
/// - `cannot find derive macro `Parser``
///
/// 这两类诊断在虚拟项目语境下恒为误报（用户项目中 `rzc check` 正常通过），
/// 直接过滤；`unresolved import` 不过滤——它是“添加依赖”快速修复的输入，
/// 且用户项目真实缺依赖时同样出现。
pub(super) fn 是缺失依赖噪声(诊断: &Value) -> bool {
    let 消息 = 诊断.get("message").and_then(|v| v.as_str()).unwrap_or("");
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
/// - `cannot find `X` in `crate``（新 E0433）
/// - `unresolved import `crate::X``（新 E0432，可含 `::Y` 尾段）
/// - `unresolved import `X``（裸路径）
/// - `no `X` in the root`（severity=4 同伴 hint）
pub(super) fn 是未打开模块引用(
    诊断: &Value,
    条目: Option<&crate::翻译缓存::TranslationEntry>,
) -> bool {
    let Some(条目) = 条目 else {
        return false;
    };
    let 消息 = 诊断.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let Some(名字) = 未打开模块候选名(消息) else {
        return false;
    };
    // 同目录存在 `名字.<原扩展名>` 时视为“模块文件未打开”的误报
    let (Some(目录), Some(后缀)) = (
        条目.原始路径.parent(),
        条目.原始路径.extension().and_then(|s| s.to_str()),
    ) else {
        return false;
    };
    目录.join(format!("{名字}.{后缀}")).is_file()
}

/// 从诊断消息中提取“未打开模块”候选名（仅识别已知消息格式）
fn 未打开模块候选名(消息: &str) -> Option<&str> {
    const 保留字: &[&str] = &["crate", "self", "super", "std", "core", "alloc"];
    let 路径 = if let Some(剩余) = 消息.strip_prefix("cannot find module or crate `") {
        剩余.split_once('`')?.0
    } else if let Some(剩余) = 消息.strip_prefix("cannot find `") {
        let (名字, 尾部) = 剩余.split_once('`')?;
        if !尾部.starts_with(" in `crate`") {
            return None;
        }
        名字
    } else if let Some(剩余) = 消息.strip_prefix("unresolved import `") {
        剩余.split_once('`')?.0
    } else {
        let 剩余 = 消息.strip_prefix("no `")?;
        let (名字, 尾部) = 剩余.split_once('`')?;
        if !尾部.starts_with(" in the root") {
            return None;
        }
        名字
    };
    // 去掉 `crate::` 前缀后取首段（`crate::X::Y` → X）
    let 路径 = 路径.strip_prefix("crate::").unwrap_or(路径);
    let 首段 = 路径.split("::").next().unwrap_or(路径);
    // 模块名必须是单个安全路径段（防御恶意构造的路径穿越，如 `..`/`/`），
    // 且不能是保留字路径段（std/core/alloc 等系统 crate 不可能是本地模块）
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
/// 虚拟项目 Cargo.toml 不含用户项目依赖，`serde`/`serde_json` 等已声明
/// 依赖的导入与引用在虚拟项目中必然无法解析。当消息中的候选 crate 名
/// 出现在用户项目最近 Cargo.toml 的依赖表（dependencies/dev/build、
/// workspace、target 各节；`-`/`_` 与大小写归一化，另含去全部
/// 分隔符的紧致形式——`md-5` 的 lib 名为 `md5`）中时，判定为虚拟项目
/// 固有误报并过滤；未列入依赖表的名字（拼写错误、真正缺失的库）保留
/// 诊断，继续提供 `rzc add` 教学提示。
pub(super) fn 是缺失项目依赖(
    诊断: &Value,
    条目: Option<&crate::翻译缓存::TranslationEntry>,
) -> bool {
    let Some(条目) = 条目 else {
        return false;
    };
    let 消息 = 诊断.get("message").and_then(|v| v.as_str()).unwrap_or("");
    // 仅处理“导入/引用未声明 crate”类消息，避免误伤同名变量/函数的诊断
    let 像导入 = 消息.starts_with("unresolved import `")
        || 消息.starts_with("cannot find module or crate `")
        || 消息.contains("use of undeclared crate or module `");
    if !像导入 {
        return false;
    }
    let Some(依赖) = 项目依赖名集合(&条目.原始路径) else {
        return false;
    };
    i18n_rust_engine::诊断::抽取反引号首段(消息)
        .iter()
        .any(|段| 段.is_ascii() && 包名候选集(段).iter().any(|形式| 依赖.contains(形式)))
}

/// 过滤“include_str!/include_bytes! 资源在虚拟项目中缺失”的误报
///
/// 虚拟 .rs 位于 `/tmp/i18n_lsp_virtual_*/src/` 下，`include_str!("界面.html")`
/// 相对虚拟目录解析必然失败（rust-analyzer 报 couldn't read `src/界面.html`，
/// 路径按虚拟项目根显示）。去掉 `src` 前缀后能在原方言文件同目录找到该
/// 文件时判定为误报；原项目真实缺失该资源时诊断保留。
pub(super) fn 是缺失包含资源(
    诊断: &Value,
    条目: Option<&crate::翻译缓存::TranslationEntry>,
) -> bool {
    let Some(条目) = 条目 else {
        return false;
    };
    let 消息 = 诊断.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let Some((路径, _)) = 消息
        .split("couldn't read `")
        .nth(1)
        .and_then(|剩余| 剩余.split_once('`'))
    else {
        return false;
    };
    if 路径.is_empty() || 路径.contains("..") {
        return false;
    }
    let Some(目录) = 条目.原始路径.parent() else {
        return false;
    };
    let 路径 = Path::new(路径);
    // 虚拟项目根相对路径（如 `src/界面.html`）：去 `src` 前缀后按原目录解析
    let 相对 = 路径.strip_prefix("src").unwrap_or(路径);
    目录.join(相对).is_file()
}

/// rust-analyzer 编译级诊断抑制判定：文件位于 cargo 项目时，RA 的
/// E 系列 error（severity=1）与 hint（severity=4）不转发给客户端，
/// 编译级诊断以代理自跑的镜像/虚拟项目 cargo check（rustc 口径）为权威来源
///
/// RA 在虚拟项目上对第三方依赖与过程宏的类型推断恒为假红（依赖缺失的
/// E0432/E0599、derive 不展开的 E0277 Display 级联等），且其原生诊断与
/// 真错无法从消息文本可靠区分；镜像检查（或不可用时的虚拟检查）在
/// didOpen/didSave 时提供与 rzc 构建同口径的权威结果。E 系列 hint 是
/// 编译错误的伴生信息拆分发布（“no external crate ...”、“由 this / formatting
/// parameter”等），同为编译级判定的产物，一并抑制。非 cargo 项目
/// （单文件教学场景）不受影响，RA 诊断全量保留；非 E 系列（语法错误、
/// RA 内部错误等）与 warning（severity=2，可能有价值）一律保留。
pub(crate) fn 抑制分析器编译错误(诊断: &Value, 原始路径: &Path) -> bool {
    let 严重度 = 诊断.get("severity").and_then(|v| v.as_u64()).unwrap_or(0);
    if 严重度 != 1 && 严重度 != 4 {
        return false;
    }
    let Some(码) = 诊断.get("code").and_then(|v| v.as_str()) else {
        return false;
    };
    if !是rustc错误码(码) {
        return false;
    }
    位于cargo项目(原始路径)
}

/// E 系列 rustc 错误码（E0432/E0277 等；RA 自带诊断沿用该格式）
fn 是rustc错误码(码: &str) -> bool {
    let Some(数字) = 码.strip_prefix('E') else {
        return false;
    };
    !数字.is_empty() && 数字.len() <= 4 && 数字.chars().all(|c| c.is_ascii_digit())
}

/// 文件路径 → 是否位于 cargo 项目（最近上层存在 Cargo.toml）的进程级缓存
static 工程缓存槽: OnceLock<Mutex<HashMap<PathBuf, bool>>> = OnceLock::new();

/// 判断文件是否位于 cargo 项目（向上查找最近 Cargo.toml），按路径缓存
fn 位于cargo项目(文件路径: &Path) -> bool {
    let 缓存 = 工程缓存槽.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = 缓存.lock().unwrap_or_else(|e| e.into_inner()).get(文件路径) {
        return *v;
    }
    let mut 目录 = 文件路径.parent();
    let mut 有清单 = false;
    while let Some(d) = 目录 {
        if d.join("Cargo.toml").is_file() {
            有清单 = true;
            break;
        }
        目录 = d.parent();
    }
    缓存
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(文件路径.to_path_buf(), 有清单);
    有清单
}

/// 依赖清单解析结果缓存（Cargo.toml 路径 → (mtime, 归一化依赖名集合)）
type 项目依赖缓存类型 = Mutex<HashMap<PathBuf, (SystemTime, Arc<HashSet<String>>)>>;

/// 项目依赖清单 mtime 缓存（键为 Cargo.toml 路径）
static 项目依赖缓存槽: OnceLock<项目依赖缓存类型> = OnceLock::new();

/// 向上查找最近的 Cargo.toml 并解析依赖名集合（归一化），按 mtime 缓存；
/// 未找到清单时返回 None（调用方保持原诊断行为）
fn 项目依赖名集合(文件路径: &Path) -> Option<Arc<HashSet<String>>> {
    let mut 目录 = 文件路径.parent();
    let 清单 = loop {
        let d = 目录?;
        let 候选 = d.join("Cargo.toml");
        if 候选.is_file() {
            break 候选;
        }
        目录 = d.parent();
    };
    let 修改时间 = std::fs::metadata(&清单).ok()?.modified().ok()?;
    let 缓存 = 项目依赖缓存槽.get_or_init(|| Mutex::new(HashMap::new()));
    let mut 守卫 = 缓存.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((已缓存时间, 依赖)) = 守卫.get(&清单)
        && *已缓存时间 == 修改时间
    {
        return Some(依赖.clone());
    }
    let 工作文本 = std::fs::read_to_string(&清单).ok()?;
    let 依赖 = Arc::new(解析依赖名(&工作文本));
    守卫.insert(清单, (修改时间, 依赖.clone()));
    Some(依赖)
}

/// 解析 Cargo.toml 中的全部依赖名（含 dev/build、workspace、target 各节）
fn 解析依赖名(相关文本: &str) -> HashSet<String> {
    let mut 名字 = HashSet::new();
    let Ok(解析值) = 相关文本.parse::<toml::Value>() else {
        return 名字;
    };
    let mut 收集 = |表: Option<&toml::Value>| {
        if let Some(t) = 表.and_then(|v| v.as_table()) {
            名字.extend(t.keys().flat_map(|k| 包名候选集(k)));
        }
    };
    for 节 in ["dependencies", "dev-dependencies", "build-dependencies"] {
        收集(解析值.get(节));
    }
    收集(解析值.get("workspace").and_then(|w| w.get("dependencies")));
    if let Some(目标) = 解析值.get("target").and_then(|t| t.as_table()) {
        for (_, cfg) in 目标 {
            for 节 in ["dependencies", "dev-dependencies", "build-dependencies"] {
                收集(cfg.get(节));
            }
        }
    }
    名字
}

/// 依赖名归一化：Cargo 视 `-`/`_` 等价，crate 名统一小写比较
fn 归一化包名(名字: &str) -> String {
    名字.to_ascii_lowercase().replace('-', "_")
}

/// crate 名比对候选集：归一形式外另收录去全部分隔符的紧致形式——
/// 包名与实际 lib 名可能只在紧致意义上对应（`md-5` 的 lib 名为 `md5`，
/// 代码中 `使用 md5::...` 而清单声明 `md-5 = "0.10"`）
fn 包名候选集(名字: &str) -> Vec<String> {
    let 下划线 = 归一化包名(名字);
    let 紧致: String = 下划线.chars().filter(|c| *c != '_').collect();
    if 紧致 == 下划线 {
        vec![下划线]
    } else {
        vec![下划线, 紧致]
    }
}

/// 从 LSP 诊断（rust-analyzer 格式）中提取所有权错误详情
///
/// 变量名取自原始消息中的反引号（如 use of moved value: `x`）；
/// 移动/借用/再次使用位置取自已还原到母语文件的 range 与 relatedInformation，
/// LSP 的 0-based 行号统一转为 1-based（与 rustc 诊断一致）。
/// 仅处理 E0382/E0502/E0507 及消息模式匹配的所有权错误。
pub(crate) fn 提取所有权详情(
    原始诊断: &Value,
    还原值: &Value,
    原始资源定位: &str,
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
    let 主位置 = 由范围构造位置(原始资源定位, &还原值["range"]);

    let mut 移动位置 = None;
    let mut 借用位置 = None;
    let mut 再用法位置 = None;

    if let Some(关联信息) = 还原值["relatedInformation"].as_array() {
        for 项 in 关联信息 {
            let 标签 = 项["message"].as_str().unwrap_or("");
            let Some(命中位置) = 由范围构造位置(原始资源定位, &项["location"]["range"])
            else {
                continue;
            };
            // 注意顺序："borrow later used here" 同时含 borrow 与 used here，应归为再次使用
            if 标签.contains("used here")
                || 标签.contains("later used")
                || 标签.contains("after move")
            {
                再用法位置.get_or_insert(命中位置);
            } else if 标签.contains("move") {
                移动位置.get_or_insert(命中位置);
            } else if 标签.contains("borrow") {
                借用位置.get_or_insert(命中位置);
            }
        }
    }

    // 主 range 兜底：E0382 → 再次使用；E0502 → 借用发生；E0507 → 移动发生
    if let Some(命中位置) = 主位置 {
        if matches!(错误码, "E0382") || 消息.contains("moved value") {
            再用法位置.get_or_insert(命中位置);
        } else if matches!(错误码, "E0502") || 消息.contains("cannot borrow") {
            借用位置.get_or_insert(命中位置);
        } else if matches!(错误码, "E0507") || 消息.contains("cannot move out of") {
            移动位置.get_or_insert(命中位置);
        }
    }

    if 移动位置.is_none() && 借用位置.is_none() && 再用法位置.is_none() {
        return None;
    }
    Some(所有权详情 {
        变量名,
        移动发生: 移动位置,
        借用发生: 借用位置,
        再次使用: 再用法位置,
    })
}

/// 提取消息中反引号包裹的变量名
///
/// 示例："use of moved value: `数据`" → "数据"。
fn 提取反引号变量名(消息: &str) -> Option<String> {
    let 开始 = 消息.find('`')?;
    let 剩余 = &消息[开始 + 1..];
    let 结束 = 剩余.find('`')?;
    Some(剩余[..结束].to_string())
}

/// 从 LSP range 构造诊断位置（0-based 行号转为 1-based）
fn 由范围构造位置(文件名: &str, 目标范围: &Value) -> Option<诊断位置> {
    let 起始行 = 目标范围["start"]["line"].as_u64()? as u32;
    let 起始列 = 目标范围["start"]["character"].as_u64()? as u32;
    let 结束行 = 目标范围["end"]["line"].as_u64()? as u32;
    let 结束列 = 目标范围["end"]["character"].as_u64()? as u32;
    Some(诊断位置 {
        源文件名: 文件名.to_string(),
        起始行: 起始行 + 1,
        起始列: 起始列 + 1,
        结束行: 结束行 + 1,
        结束列: 结束列 + 1,
        源码文本: None,
        标签: None,
        是主跨度: false,
    })
}

/// 计算文档的全角标点教学诊断（方言坐标，severity 为 Hint）
///
/// 在转译后的虚拟文本上扫描：代码位置的全角标点在转译中保留（映射表
/// 不含全角字符），字符串/注释内的全角标点是合法内容且被扫描器跳过；
/// 扫描得到虚拟坐标（转译不改变行结构，代码位置为 ASCII，char 列即
/// UTF-16 列），再经列映射还原为方言坐标，与 RA/内置诊断坐标系一致。
pub(crate) fn 全角标点诊断(条目: &TranslationEntry) -> Vec<Value> {
    i18n_rust_engine::全角标点::查找全角标点(&条目.英文源码)
        .iter()
        .map(|警告| {
            let 所在行 = (警告.行号 - 1) as u32;
            let 英文列 = (警告.列号 - 1) as u32;
            let 方言列 = 英文列转中文列(条目, 所在行, 英文列);
            json!({
                "range": {
                    "start": { "line": 所在行, "character": 方言列 },
                    "end": { "line": 所在行, "character": 方言列 + 1 }
                },
                "severity": 3,
                "code": "fullwidth",
                "source": "i18n-rust",
                "message": 警告.格式化输出(),
                // 修复动作数据：全角字符与建议的半角字符（供 codeAction 注入）
                "data": {
                    "character": 警告.全角字.to_string(),
                    "replacement": 警告.替换建议.map(|c| c.to_string())
                }
            })
        })
        .collect()
}

/// 计算文档的教学 lint 诊断（方言坐标，severity 为 Hint）
///
/// 直接在母语原文上扫描（`让` 等关键字在转译后已不存在）：行列均为
/// 字符计数，中文代码在 BMP 内 char 列即 UTF-16 列，直接转换即可。
/// `known_words` 为映射表键集合，供易混方法名提示判定（#14）；
/// `ambiguous_constructors` 为推导歧义构造器被调名集（new/default 及其
/// 方言词），仅类型无法自行推导的初始化式才提示未标注类型。
pub(crate) fn 生成教学lint诊断(
    条目: &TranslationEntry,
    已知词集: &HashSet<String>,
    歧义构造集: &HashSet<String>,
) -> Vec<Value> {
    i18n_rust_engine::教学检查::执行教学检查并词表(
        &条目.中文原文,
        已知词集,
        歧义构造集,
    )
    .iter()
    .map(|警告| {
        let 所在行 = (警告.行号 - 1) as u32;
        let 所在列 = (警告.列号 - 1) as u32;
        json!({
            "range": {
                "start": { "line": 所在行, "character": 所在列 },
                "end": { "line": 所在行, "character": 所在列 + 1 }
            },
            "severity": 3,
            "code": lint诊断码(警告.种类),
            "source": "i18n-rust",
            "message": 警告.格式化提示()
        })
    })
    .collect()
}

/// 注入教学诊断（全角标点 + 教学 lint）到方言坐标的诊断列表
///
/// 三条发布路径共用：rust-analyzer 诊断链（publishDiagnostics 处理）、
/// 虚拟项目检查、真实项目镜像检查。教学诊断由 entry 内容直接计算，
/// 不依赖 builtin 缓存（缓存有时序窗口：代理自跑检查先于 RA 首批发布
/// 时缓存为空，教学诊断会丢失）。注入前先移除旧的教学诊断（按
/// code + source 识别），保证与文档最新内容一致。
pub(crate) fn 注入教学诊断(
    诊断列表: &mut Vec<Value>,
    条目: &TranslationEntry,
    已知词集: &HashSet<String>,
    歧义构造集: &HashSet<String>,
) {
    诊断列表.retain(|d| !(是教学诊断(d) && d["source"].as_str() == Some("i18n-rust")));
    诊断列表.extend(全角标点诊断(条目));
    if 教学检查开关() {
        诊断列表.extend(生成教学lint诊断(条目, 已知词集, 歧义构造集));
    }
}

/// 教学诊断开关：默认开启；`RZ_LSP_TEACHING_LINT=off` 关闭
///（重度开发者不需要教学提示时避免诊断噪音）
pub(crate) fn 教学检查开关() -> bool {
    std::env::var("RZ_LSP_TEACHING_LINT")
        .map(|v| v != "off" && v != "0")
        .unwrap_or(true)
}

/// 判断诊断是否为我方注入的教学诊断（全角标点/教学 lint）
pub(crate) fn 是教学诊断(d: &Value) -> bool {
    d["code"]
        .as_str()
        .is_some_and(|c| c == "fullwidth" || c.starts_with("lint-"))
}

/// 教学 lint 规则 → LSP 诊断 code
fn lint诊断码(种类: i18n_rust_engine::教学检查::检查种类) -> &'static str {
    match 种类 {
        i18n_rust_engine::教学检查::检查种类::未标注类型 => "lint-untyped-let",
        i18n_rust_engine::教学检查::检查种类::魔法数字 => "lint-magic-number",
        i18n_rust_engine::教学检查::检查种类::嵌套过深 => "lint-deep-indent",
        i18n_rust_engine::教学检查::检查种类::易混方法名 => "lint-confusable-method",
    }
}

#[cfg(test)]
mod 单元测试 {
    use super::{
        包名候选集, 抑制分析器编译错误, 是rustc错误码, 查找期望实际标签, 解析依赖名
    };

    /// RA 把 expected/found 放在 relatedInformation，应被归集为主消息标签
    #[test]
    fn 测试期望实际标签取自关联() {
        let 诊断 = serde_json::json!({
            "message": "mismatched types",
            "code": "E0308",
            "relatedInformation": [
                { "message": "expected `i32`, found `String`" },
            ]
        });
        assert_eq!(
            查找期望实际标签(&诊断).as_deref(),
            Some("expected `i32`, found `String`")
        );
        // 无 expected/found 类标签时返回 None（渲染器自动回退消息表）
        let 普通 = serde_json::json!({
            "message": "use of moved value: `x`",
            "relatedInformation": [ { "message": "moved value used here after move" } ]
        });
        assert_eq!(查找期望实际标签(&普通), None);
        // 无 relatedInformation 也不崩
        assert_eq!(
            查找期望实际标签(&serde_json::json!({ "message": "x" })),
            None
        );
    }

    /// 紧致形式：`md-5` → md_5 + md5（lib 名去分隔符）
    #[test]
    fn 测试包名候选集紧致形式() {
        let 候选集 = 包名候选集("md-5");
        assert!(候选集.contains(&"md_5".to_string()));
        assert!(候选集.contains(&"md5".to_string()));
        // 无分隔符的名字保持单一形式
        assert_eq!(包名候选集("rayon"), vec!["rayon".to_string()]);
    }

    /// 依赖清单解析收录紧致形式，供 md-5 风格的包名匹配
    #[test]
    fn 测试解析依赖名含紧致形式() {
        let 依赖 = 解析依赖名("[dependencies]\nmd-5 = \"0.10\"\n");
        assert!(依赖.contains("md_5"));
        assert!(依赖.contains("md5"));
        assert!(!依赖.contains("serde"));
    }

    /// E 系列错误码识别（E + 至多 4 位数字）
    #[test]
    fn 测试rustc错误码识别() {
        assert!(是rustc错误码("E0432"));
        assert!(是rustc错误码("E0277"));
        assert!(!是rustc错误码("e0432"));
        assert!(!是rustc错误码("E"));
        assert!(!是rustc错误码("E04a2"));
        assert!(!是rustc错误码("unused_imports"));
    }

    /// cargo 项目内的 E 系列 error 抑制；项目外/非 E 码/非 error 级不抑制
    #[test]
    fn 测试抑制分析器编译错误() {
        let 进程号 = std::process::id();
        let 目录 = std::env::temp_dir().join(format!("diag_text_suppress_{进程号}"));
        let 源码 = 目录.join("src");
        std::fs::create_dir_all(&源码).unwrap();
        std::fs::write(目录.join("Cargo.toml"), "[package]\nname = \"t\"\n").unwrap();
        let file = 源码.join("main.zh");
        std::fs::write(&file, "").unwrap();

        let 错误诊断 = serde_json::json!({ "severity": 1, "code": "E0432", "message": "unresolved import `md5`" });
        assert!(抑制分析器编译错误(&错误诊断, &file));
        // E 系列 hint（编译错误的伴生信息拆分发布）同样抑制
        let 提示 = serde_json::json!({ "severity": 4, "code": "E0432", "message": "no external crate `toml`" });
        assert!(抑制分析器编译错误(&提示, &file));
        // warning 不抑制（可能有价值）
        let 警告 = serde_json::json!({ "severity": 2, "code": "E0432" });
        assert!(!抑制分析器编译错误(&警告, &file));
        // 非 E 系列（语法错误等）不抑制
        let 语法 = serde_json::json!({ "severity": 1, "code": "syntax" });
        assert!(!抑制分析器编译错误(&语法, &file));
        // 非 E 系列 hint 不抑制（如 main 函数提示）
        let 普通提示 = serde_json::json!({ "severity": 4, "code": "unused" });
        assert!(!抑制分析器编译错误(&普通提示, &file));
        // 非 cargo 项目（单文件教学场景）不抑制
        let 项目外 = std::env::temp_dir().join(format!("diag_text_suppress_out_{进程号}.zh"));
        std::fs::write(&项目外, "").unwrap();
        assert!(!抑制分析器编译错误(&错误诊断, &项目外));
        assert!(!抑制分析器编译错误(&提示, &项目外));

        let _ = std::fs::remove_dir_all(&目录);
        let _ = std::fs::remove_file(&项目外);
    }
}
