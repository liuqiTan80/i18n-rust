//! `rzc 待译清单` —— 待译清单（英文残留）查看、导出与清零
//!
//! 编译器/构建工具的消息命中不了语言包消息表时，诊断照旧按英文原文显示
//! （兜底，绝不吞诊断），同时把这条未译记录写入 `~/.rz/待译清单/<语言>.tsv`。
//! 本命令把清单读回来：直接看、导出成补表素材，或补完后清零。
//!
//! 清单每行 `语言\t通道\t错误码\t消息原文`，通道取 `main`（主消息）、
//! `help`（建议子诊断）、`tool`（cargo 纯文本错误行）；消息原文里的换行与
//! 制表符以 `\n`/`\t` 字面量存放（一类 rustc 消息本身就跨行），保证一行一条记录；
//! 去掉首列即与语料门禁的 `crates/engine/tests/data/diag-corpus.tsv` 同构，可直接并入。

use std::path::PathBuf;

use anyhow::Context;
use i18n_rust_engine::diagnostic::待译清单;

use crate::ui;

/// 列出/导出/清零待译清单
pub(crate) fn command(
    ui: &ui::Ui,
    lang: Option<String>,
    export: Option<PathBuf>,
    clear: bool,
) -> anyhow::Result<()> {
    let lang = lang.unwrap_or_else(crate::ui::detect_ui_lang);
    let file = 待译清单::清单项文件(&lang);
    if clear {
        println!(
            "{}",
            ui.f("gaps_cleared", &[&lang, &file.display().to_string()])
        );
        if !待译清单::清空缺口(&lang) {
            // 文件本就不存在：清单已是空，补一句避免误以为删了东西
            println!("{}", ui.f("gaps_none", &[&file.display().to_string()]));
        }
        return Ok(());
    }
    let list = 待译清单::读取缺口(&lang);
    if list.is_empty() {
        println!("{}", ui.f("gaps_none", &[&file.display().to_string()]));
        return Ok(());
    }
    let text = render(&list);
    match export {
        Some(path) => {
            std::fs::write(&path, text).with_context(|| format!("写入 {} 失败", path.display()))?;
            println!(
                "{}",
                ui.f(
                    "gaps_written",
                    &[&path.display().to_string(), &list.len().to_string()]
                )
            );
        }
        None => {
            // 直接输出纯 TSV（无表头），便于管道给工具链与补表脚本消费
            print!("{text}");
        }
    }
    Ok(())
}

/// 渲染清单：按通道分组排序，输出与语料门禁同构的三列 TSV
///
/// 消息里的换行/制表符走 [`待译清单::编码缺口字段`]：跨行消息（如 E0277 的
/// 「下列类型实现了该特征」清单）若原样输出，导出件一行就不只一条记录，
/// 下游补表脚本与 `解析清单行` 都要读坏。
fn render(list: &[待译清单::待译记录]) -> String {
    let mut sorted = list.to_vec();
    // 通道（main → help → tool）→ 消息原文，稳定输出便于 diff
    sorted.sort_by(|a, b| (&a.1, &a.3).cmp(&(&b.1, &b.3)));
    let mut out = String::new();
    for (_, level, code, message) in sorted {
        out.push_str(&format!(
            "{level}\t{code}\t{}\n",
            待译清单::编码缺口字段(&message)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 渲染：按通道排序且与引擎清单格式兼容（三列 LEVEL\tCODE\t消息）
    #[test]
    fn render_sorts_by_channel_then_message() {
        let list = vec![
            (
                "zh".to_string(),
                "tool".to_string(),
                String::new(),
                "failed to parse manifest".to_string(),
            ),
            (
                "zh".to_string(),
                "main".to_string(),
                "E9999".to_string(),
                "some english".to_string(),
            ),
            (
                "zh".to_string(),
                "main".to_string(),
                String::new(),
                "another english".to_string(),
            ),
        ];
        let text = render(&list);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("main\t") && lines[0].contains("another english"));
        assert!(lines[1].starts_with("main\tE9999"));
        assert!(lines[2].starts_with("tool\t"));
        // 去掉语言列后可直接被引擎解析（语料门禁同构）
        assert_eq!(待译清单::解析清单行(&text).len(), 3);
    }

    /// 跨行消息在导出件里仍是一行一条，且能被引擎逐字读回
    ///
    /// 不转义就会把 `--export` 产物拆成裸行：补表脚本按行读到的是半句，
    /// 而 `rzc 待译清单` 的条数也不再是待译句子数。
    #[test]
    fn render_escapes_multiline_message() {
        let 多行 = "the following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\nand 56 others";
        let list = vec![
            (
                "ja".to_string(),
                "help".to_string(),
                "E0277".to_string(),
                多行.to_string(),
            ),
            (
                "ja".to_string(),
                "main".to_string(),
                String::new(),
                "a single line message".to_string(),
            ),
        ];
        let text = render(&list);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "跨行消息不得拆行：{lines:?}");
        let multiline_line = lines
            .iter()
            .find(|l| l.starts_with("help\tE0277\t"))
            .expect("help 通道的跨行记录应在列");
        assert!(
            multiline_line.contains("trait `Add<Rhs>`:\\n  `&f16`"),
            "应以转义字面量存放：{multiline_line}"
        );
        // 导出件去首列后仍与语料门禁同构，读回内容逐字相等
        let read = 待译清单::解析清单行(&text);
        assert_eq!(read.len(), 2);
        assert!(
            read.iter().any(|g| g.3 == 多行),
            "跨行内容应无损还原：{read:?}"
        );
    }
}
