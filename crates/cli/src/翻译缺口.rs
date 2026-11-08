//! `rzc 待译清单` —— 待译清单（英文残留）查看、导出与清零
//!
//! 编译器/构建工具的消息命中不了语言包消息表时，诊断照旧按英文原文显示
//! （兜底，绝不吞诊断），同时把这条未译记录写入 `~/.rz/待译清单/<语言>.tsv`。
//! 本命令把清单读回来：直接看、导出成补表素材，或补完后清零。
//!
//! 清单每行 `语言 → 通道 → 错误码 → 消息原文`，通道取主消息、建议或工具；
//! 消息原文里的换行与制表符以转义字面量存放（一类编译器消息本身就跨行），
//! 保证一行一条记录；去掉首列即与语料门禁的
//! `crates/engine/tests/data/diag-corpus.tsv` 同构，可直接并入。

use std::path::PathBuf;

use anyhow::Context;
use i18n_rust_engine::诊断::待译清单;

use crate::界面;

/// 列出/导出/清零待译清单
pub(crate) fn 清单命令(
    界面表: &界面::界面消息表,
    语言: Option<String>,
    导出: Option<PathBuf>,
    清空: bool,
) -> anyhow::Result<()> {
    let 语言代码 = 语言.unwrap_or_else(界面::探测界面语言);
    let 清单文件 = 待译清单::清单项文件(&语言代码);
    if 清空 {
        println!(
            "{}",
            界面表.取带参消息(
                "gaps_cleared",
                &[&语言代码, &清单文件.display().to_string()]
            )
        );
        if !待译清单::清空缺口(&语言代码) {
            // 文件本就不存在：清单已是空，补一句避免误以为删了东西
            println!(
                "{}",
                界面表.取带参消息("gaps_none", &[&清单文件.display().to_string()])
            );
        }
        return Ok(());
    }
    let 清单 = 待译清单::读取缺口(&语言代码);
    if 清单.is_empty() {
        println!(
            "{}",
            界面表.取带参消息("gaps_none", &[&清单文件.display().to_string()])
        );
        return Ok(());
    }
    let 文本串 = 渲染清单(&清单);
    if let Some(导出路径) = 导出 {
        std::fs::write(&导出路径, 文本串)
            .with_context(|| format!("写入 {} 失败", 导出路径.display()))?;
        println!(
            "{}",
            界面表.取带参消息(
                "gaps_written",
                &[&导出路径.display().to_string(), &清单.len().to_string()]
            )
        );
    } else {
        // 直接输出纯制表符分隔文本（无表头），便于管道给工具链与补表脚本消费
        print!("{文本串}");
    }
    Ok(())
}

/// 渲染清单：按通道分组排序，输出与语料门禁同构的三列制表符分隔文本
///
/// 消息里的换行/制表符走 [`待译清单::编码缺口字段`]：跨行消息（如错误码
/// 后跟「下列类型实现了该特征」清单）若原样输出，导出件一行就不只一条记录，
/// 下游补表脚本与 `解析清单行` 都要读坏。
fn 渲染清单(清单: &[待译清单::待译记录]) -> String {
    let mut 排序清单 = 清单.to_vec();
    // 通道（主消息 → 建议 → 工具）→ 消息原文，稳定输出便于对比差异
    排序清单.sort_by(|甲, 乙| (&甲.1, &甲.3).cmp(&(&乙.1, &乙.3)));
    let mut 输出文本 = String::new();
    for (_, 通道, 错误码, 消息原文) in 排序清单 {
        输出文本.push_str(&format!(
            "{通道}\t{错误码}\t{}\n",
            待译清单::编码缺口字段(&消息原文)
        ));
    }
    输出文本
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    /// 渲染：按通道排序且与引擎清单格式兼容（三列：通道 → 错误码 → 消息）
    #[test]
    fn 渲染按通道再按消息排序() {
        let 清单 = vec![
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
        let 文本串 = 渲染清单(&清单);
        let 行列表: Vec<&str> = 文本串.lines().collect();
        assert_eq!(行列表.len(), 3);
        assert!(行列表[0].starts_with("main\t") && 行列表[0].contains("another english"));
        assert!(行列表[1].starts_with("main\tE9999"));
        assert!(行列表[2].starts_with("tool\t"));
        // 去掉语言列后可直接被引擎解析（语料门禁同构）
        assert_eq!(待译清单::解析清单行(&文本串).len(), 3);
    }

    /// 跨行消息在导出件里仍是一行一条，且能被引擎逐字读回
    ///
    /// 不转义就会把导出产物拆成裸行：补表脚本按行读到的是半句，
    /// 而 `rzc 待译清单` 的条数也不再是待译句子数。
    #[test]
    fn 渲染转义跨行消息() {
        let 多行 = "the following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\nand 56 others";
        let 清单 = vec![
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
        let 文本串 = 渲染清单(&清单);
        let 行列表: Vec<&str> = 文本串.lines().collect();
        assert_eq!(行列表.len(), 2, "跨行消息不得拆行：{行列表:?}");
        let 跨行行 = 行列表
            .iter()
            .find(|本行| 本行.starts_with("help\tE0277\t"))
            .expect("建议通道的跨行记录应在列");
        assert!(
            跨行行.contains("trait `Add<Rhs>`:\\n  `&f16`"),
            "应以转义字面量存放：{跨行行}"
        );
        // 导出件去首列后仍与语料门禁同构，读回内容逐字相等
        let 读回项 = 待译清单::解析清单行(&文本串);
        assert_eq!(读回项.len(), 2);
        assert!(
            读回项.iter().any(|记录项| 记录项.3 == 多行),
            "跨行内容应无损还原：{读回项:?}"
        );
    }
}
