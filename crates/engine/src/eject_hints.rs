//! 导出冲突提示：eject 前扫一遍「转译会静默改变你代码含义」的位置
//!
//! 与教学 lint 同一定位：只提示、不阻断（`--no-lint` 一并关闭）。规则刻意保守，
//! 只报**能确定会被改写**的三类位置，避免把正常代码当问题：
//!
//! 1. 结构体字段名命中关键字/宏词条：字段定义被词法层改写，
//!    serde 键与字段名随之变化（`类型: 字符串` → `type: String`）；
//! 2. 声明的名字命中关键字/宏词条：`让 自身 = 1` → `let Self = 1`、
//!    `结构体 配置` → `struct cfg!`（`主函数` → `main` 是惯例，不报）；
//! 3. 会 panic 的词条被当文本用：`打印行!("{}", 解包(甲))` 在格式化实参里
//!    直接 panic，而不是产出文本。
//!
//! 位置一律按**母语源文件**的行列出（提示在写盘前扫描，不经产物）。
//! 别名词条（`长度`、`版本` 等）在声明位有豁免，不会被改写，故不提示。

use std::collections::{HashMap, HashSet};

use rustc_lexer::{TokenKind, tokenize};

use crate::映射管理器::映射管理器;

/// 提示种类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EjectHintKind {
    /// 结构体字段名命中词条：字段定义会被改写，serde 键随之变化
    FieldShadowed,
    /// 声明的名字（结构体/枚举/函数/变量）命中词条：声明本身被改写
    DeclaredNameRewritten,
    /// 会 panic 的词条的结果被当文本使用
    PanicAsText,
}

/// 单条导出冲突提示：位置（1 起行/列）+ 种类 + 相关词条
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EjectHint {
    /// 行号（从 1 开始）
    pub line: usize,
    /// 列号（从 1 开始，按字符计数）
    pub column: usize,
    /// 种类
    pub kind: EjectHintKind,
    /// 触发的词条（源码里写的母语词）
    pub word: String,
    /// 该词条的转译目标（`类型` → `type`）
    pub target: String,
}

impl EjectHint {
    /// 格式化为教学提示文本，模板随当前语言变化
    pub fn format(&self) -> String {
        let line = self.line.to_string();
        let column = self.column.to_string();
        match self.kind {
            EjectHintKind::FieldShadowed => crate::语言::f(
                "eject_hint_field_shadowed",
                &[&line, &column, &self.word, &self.target],
            ),
            EjectHintKind::DeclaredNameRewritten => crate::语言::f(
                "eject_hint_declared_name",
                &[&line, &column, &self.word, &self.target],
            ),
            EjectHintKind::PanicAsText => crate::语言::f(
                "eject_hint_panic_as_text",
                &[&line, &column, &self.word, &self.target],
            ),
        }
    }
}

/// 扫描母语源码，返回全部导出冲突提示（按出现顺序）
///
/// 只依赖映射表与源码文本：不读文件、不联网、不看环境变量。
pub fn eject_hints(source: &str, manager: &映射管理器) -> Vec<EjectHint> {
    let tokens = tokenize_code(source);
    let keywords = manager.取关键词表();
    let aliases = manager.取别名表();
    let macro_map = manager.取宏表();
    let mut sink = HintSink::default();

    // 方言拼写由词条表反查（语言无关）：值为 struct 的键就是「结构体」这个词
    let words_for = |target: &str| -> Vec<&str> {
        keywords
            .iter()
            .filter(|(_, value)| value.as_str() == target)
            .map(|(word, _)| word.as_str())
            .collect()
    };
    let struct_words = words_for("struct");
    let enum_words = words_for("enum");
    let fn_words = words_for("fn");
    let let_words = words_for("let");
    let pub_words = words_for("pub");

    for (index, token) in tokens.iter().enumerate() {
        let text = token.text;
        if struct_words.contains(&text) || enum_words.contains(&text) {
            if let Some(name) = tokens.get(index + 1) {
                report_declared_name(name, keywords, &mut sink);
            }
            if struct_words.contains(&text) {
                check_struct_fields(&tokens, index, keywords, &pub_words, &mut sink);
            }
        } else if (fn_words.contains(&text) || let_words.contains(&text))
            && let Some(name) = tokens.get(index + 1)
        {
            report_declared_name(name, keywords, &mut sink);
        }
    }

    check_panic_as_text(&tokens, manager, macro_map, aliases, &mut sink);
    sink.hints
}

/// 收集提示（同一位置的同一类提示只报一次）
#[derive(Default)]
struct HintSink {
    hints: Vec<EjectHint>,
    seen: HashSet<(usize, usize, EjectHintKind)>,
}

impl HintSink {
    fn push(&mut self, token: &Token<'_>, kind: EjectHintKind, word: &str, target: &str) {
        if self.seen.insert((token.line, token.column, kind)) {
            self.hints.push(EjectHint {
                line: token.line,
                column: token.column,
                kind,
                word: word.to_string(),
                target: target.to_string(),
            });
        }
    }
}

/// 声明名命中关键字词条 → 声明本身会被改写
///
/// `主函数` → `main` 是项目惯例（入口函数），不提示。
fn report_declared_name(name: &Token<'_>, keywords: &HashMap<String, String>, sink: &mut HintSink) {
    if !name.is_plain_ident {
        return;
    }
    if let Some(target) = keywords.get(name.text)
        && target != "main"
        && target != name.text
    {
        sink.push(
            name,
            EjectHintKind::DeclaredNameRewritten,
            name.text,
            target,
        );
    }
}

/// 扫描结构体字段名，命中关键字/宏词条时报告（字段定义会被改写）
fn check_struct_fields(
    tokens: &[Token<'_>],
    struct_index: usize,
    keywords: &HashMap<String, String>,
    pub_words: &[&str],
    sink: &mut HintSink,
) {
    // 找结构体体的 `{`（跳过泛型/where 子句；`;` 结尾的单元结构体没有字段）
    let mut index = struct_index + 1;
    while index < tokens.len() && tokens[index].text != "{" && tokens[index].text != ";" {
        index += 1;
    }
    if index >= tokens.len() || tokens[index].text != "{" {
        return;
    }
    let mut depth = 0usize;
    while index < tokens.len() {
        match tokens[index].text {
            "{" => depth += 1,
            "}" => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
            ":" if depth == 1 => {
                if let Some(name) = field_name_before(tokens, index, pub_words)
                    && let Some(target) = keywords.get(name.text)
                    && target != name.text
                {
                    sink.push(name, EjectHintKind::FieldShadowed, name.text, target);
                }
            }
            _ => {}
        }
        index += 1;
    }
}

/// 判断 `:` 前的标识符是否字段名（字段以 `{`/`,`/`]`/可见性词开头，类型路径段不是）
fn field_name_before<'a, 't>(
    tokens: &'a [Token<'t>],
    colon_index: usize,
    pub_words: &[&str],
) -> Option<&'a Token<'t>> {
    let name = tokens.get(colon_index.checked_sub(1)?)?;
    if !name.is_plain_ident {
        return None;
    }
    let before = tokens.get(colon_index.checked_sub(2)?)?;
    let starts_field =
        matches!(before.text, "{" | "," | "]" | "pub") || pub_words.contains(&before.text);
    starts_field.then_some(name)
}

/// 会 panic 的词条出现在文本语境：文本宏实参内，或结果上直接接文本方法
fn check_panic_as_text(
    tokens: &[Token<'_>],
    manager: &映射管理器,
    macro_map: &HashMap<String, String>,
    aliases: &HashMap<String, String>,
    sink: &mut HintSink,
) {
    // 文本宏：实参会被格式化，实参里 panic 等于直接崩
    let is_text_macro = |word: &str| -> bool {
        macro_map.get(word).is_some_and(|target| {
            matches!(
                target.as_str(),
                "format"
                    | "println"
                    | "print"
                    | "eprintln"
                    | "eprint"
                    | "panic"
                    | "write"
                    | "writeln"
                    | "assert"
                    | "assert_eq"
                    | "assert_ne"
                    | "debug_assert"
                    | "debug_assert_eq"
            )
        })
    };
    // 文本处理方法：作用在「结果」上等于把结果当文本用
    let is_text_method = |word: &str| -> bool {
        aliases.get(word).is_some_and(|target| {
            matches!(
                target.as_str(),
                "to_string"
                    | "to_lowercase"
                    | "to_uppercase"
                    | "to_ascii_lowercase"
                    | "to_ascii_uppercase"
                    | "trim"
                    | "trim_start"
                    | "trim_end"
                    | "len"
                    | "starts_with"
                    | "ends_with"
                    | "contains"
                    | "join"
                    | "replace"
                    | "split"
                    | "split_once"
                    | "push_str"
                    | "insert_str"
            )
        })
    };
    // 会 panic 的别名词条（宏词条排除：`恐慌!(…)` 本身就是显式 panic）
    let panic_target = |word: &str| -> Option<&str> {
        if macro_map.contains_key(word) {
            return None;
        }
        let target = aliases.get(word)?;
        manager
            .取危险详情(word)
            .等级
            .会恐慌
            .then_some(target.as_str())
    };

    // 每个 `(` 是否处在文本宏实参内（嵌套继承）
    let mut in_text_macro: Vec<bool> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.text {
            "(" => {
                // 宏调用写成 `词条 ! (`：`(` 前一个是 `!`，宏名再往前一个
                let mut before = index.checked_sub(1).and_then(|prev| tokens.get(prev));
                if before.is_some_and(|prev| prev.text == "!") {
                    before = index.checked_sub(2).and_then(|prev| tokens.get(prev));
                }
                let enclosing = in_text_macro.last().copied().unwrap_or(false)
                    || before.is_some_and(|prev| is_text_macro(prev.text));
                in_text_macro.push(enclosing);
                continue;
            }
            ")" => {
                in_text_macro.pop();
                continue;
            }
            _ => {}
        }
        if !token.is_plain_ident {
            continue;
        }
        // 宏调用（`恐慌!(`）是显式 panic，不提示
        if tokens.get(index + 1).is_some_and(|next| next.text == "!") {
            continue;
        }
        let Some(target) = panic_target(token.text) else {
            continue;
        };
        let as_text = in_text_macro.last().copied().unwrap_or(false)
            || call_result_used_as_text(tokens, index, &is_text_method);
        if as_text {
            sink.push(token, EjectHintKind::PanicAsText, token.text, target);
        }
    }
}

/// 调用结果（`词条(…)`）之后是否直接接文本方法（`.转字符串()` 等）
fn call_result_used_as_text(
    tokens: &[Token<'_>],
    word_index: usize,
    is_text_method: &dyn Fn(&str) -> bool,
) -> bool {
    if tokens.get(word_index + 1).map(|token| token.text) != Some("(") {
        return false;
    }
    let mut depth = 0usize;
    let mut index = word_index + 1;
    while index < tokens.len() {
        match tokens[index].text {
            "(" => depth += 1,
            ")" => {
                depth -= 1;
                if depth == 0 {
                    index += 1;
                    break;
                }
            }
            _ => {}
        }
        index += 1;
    }
    if tokens.get(index).map(|token| token.text) != Some(".") {
        return false;
    }
    tokens
        .get(index + 1)
        .is_some_and(|method| method.is_plain_ident && is_text_method(method.text))
}

/// 词法记号（仅保留代码位置的记号，附行列）
struct Token<'a> {
    text: &'a str,
    line: usize,
    column: usize,
    /// 普通标识符（`r#` 原始标识符不算：它不会被改写）
    is_plain_ident: bool,
}

/// 用 rustc_lexer 切分源码并换算行列（空白/注释之外的记号按序返回）
fn tokenize_code(source: &str) -> Vec<Token<'_>> {
    let mut line_starts = vec![0usize];
    line_starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));

    let mut tokens = Vec::new();
    let mut offset = 0usize;
    for token in tokenize(source) {
        let start = offset;
        offset += token.len;
        if matches!(
            token.kind,
            TokenKind::Whitespace
                | TokenKind::LineComment
                | TokenKind::BlockComment { .. }
                | TokenKind::Unknown
        ) {
            continue;
        }
        let line_index = match line_starts.binary_search(&start) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        let column = source[line_starts[line_index]..start].chars().count() + 1;
        tokens.push(Token {
            text: &source[start..offset],
            line: line_index + 1,
            column,
            is_plain_ident: matches!(token.kind, TokenKind::Ident),
        });
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 真实 zh 内置语言包（与 CLI 同源）
    fn manager() -> 映射管理器 {
        let lang_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
        映射管理器::从目录载入(&lang_dir).expect("内置语言包应可加载")
    }

    fn kinds(hints: &[EjectHint]) -> Vec<EjectHintKind> {
        hints.iter().map(|hint| hint.kind).collect()
    }

    #[test]
    fn test_field_name_rewritten_is_reported() {
        let source = "结构体 甲 {\n    类型: 字符串,\n    长度: 整数,\n}\n";
        let hints = eject_hints(source, &manager());
        assert_eq!(kinds(&hints), vec![EjectHintKind::FieldShadowed]);
        assert_eq!(hints[0].word, "类型");
        assert_eq!(hints[0].target, "type");
        assert_eq!((hints[0].line, hints[0].column), (2, 5));
    }

    #[test]
    fn test_alias_field_name_is_silent() {
        // 别名类词条（长度/版本）在声明位有豁免，不会改写，不应提示
        let source = "结构体 甲 {\n    长度: 整数,\n    版本: 字符串,\n}\n";
        assert!(eject_hints(source, &manager()).is_empty());
    }

    #[test]
    fn test_field_type_path_is_not_reported() {
        // 字段类型里的路径段（`模块::类型`）不是字段名，不得误报
        let source = "结构体 甲 {\n    计数: 模块::类型,\n}\n";
        assert!(eject_hints(source, &manager()).is_empty());
    }

    #[test]
    fn test_declared_name_hits_keyword_is_reported() {
        let source = "函数 主函数() {\n    让 自身 = 1;\n}\n";
        let hints = eject_hints(source, &manager());
        assert_eq!(kinds(&hints), vec![EjectHintKind::DeclaredNameRewritten]);
        assert_eq!(hints[0].word, "自身");
        assert_eq!(hints[0].target, "Self");
        // 第 2 行 `    让 自身 = 1;`：四个空格 + 「让」+ 空格 → 「自身」在第 7 列
        assert_eq!((hints[0].line, hints[0].column), (2, 7));
    }

    #[test]
    fn test_main_entry_is_not_reported() {
        // 主函数 → main 是项目惯例，不该当问题报出来
        let source = "函数 主函数() {\n    让 甲 = 1;\n}\n";
        assert!(eject_hints(source, &manager()).is_empty());
    }

    #[test]
    fn test_struct_name_hits_keyword_is_reported() {
        // 结构体 配置 → struct cfg!（该词在宏表里也有，词法层直接改写）
        let hints = eject_hints("结构体 配置 {\n    甲: 整数,\n}\n", &manager());
        assert_eq!(kinds(&hints), vec![EjectHintKind::DeclaredNameRewritten]);
        assert_eq!(hints[0].word, "配置");
    }

    #[test]
    fn test_panic_word_inside_format_args_is_reported() {
        let source = "函数 主函数() {\n    让 甲 = 有值(3);\n    打印行!(\"{}\", 解包(甲));\n}\n";
        let hints = eject_hints(source, &manager());
        assert_eq!(kinds(&hints), vec![EjectHintKind::PanicAsText]);
        assert_eq!(hints[0].word, "解包");
        assert_eq!(hints[0].target, "unwrap");
        assert_eq!(hints[0].line, 3);
    }

    #[test]
    fn test_panic_word_result_used_as_text_is_reported() {
        let source = "函数 主函数() {\n    让 甲 = 有值(1).解包().转字符串();\n}\n";
        let hints = eject_hints(source, &manager());
        assert_eq!(kinds(&hints), vec![EjectHintKind::PanicAsText]);
        assert_eq!(hints[0].word, "解包");
    }

    #[test]
    fn test_panic_word_elsewhere_is_silent() {
        // 只当值用、不进文本语境：不提示（避免把正常代码刷成一片警告）
        let source = "函数 主函数() {\n    让 甲 = 有值(3).解包();\n    让 乙 = 甲 + 1;\n}\n";
        assert!(eject_hints(source, &manager()).is_empty());
    }

    #[test]
    fn test_explicit_panic_macro_is_silent() {
        // `恐慌!(…)` 本身就是显式选择，不提示
        let source = "函数 主函数() {\n    恐慌!(\"{}\", 1);\n}\n";
        assert!(eject_hints(source, &manager()).is_empty());
    }

    #[test]
    fn test_safe_database_column_word_is_silent() {
        // 「取列」已改为安全默认（get_opt），在文本位置也不再触发 panic 提示
        let source = "函数 主函数() {\n    打印行!(\"{}\", 取列(0));\n}\n";
        assert!(eject_hints(source, &manager()).is_empty());
    }
}
