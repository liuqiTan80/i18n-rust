//! 英文残留兜底与待译清单
//!
//! 背景：消息表（errors.toml 的「消息翻译」节）覆盖哪些句子，静态检查看不全
//! ——只有真跑编译器/构建工具才知道今天会吐哪些新句子。旧行为是「未命中就
//! 静默原样输出英文」：用户看到中英夹杂却无从反馈，维护者也收不到「哪句漏了」
//! 的信号，只能靠实跑撞见。
//!
//! 本模块把这条反馈链固化成机制：
//!
//! 1. **兜底**：译文缺失/残段未译时，渲染照旧回拼原文（绝不吞诊断），
//!    但调用方在**本轮结束时**统一提示一次「有 N 条消息暂无译文，已按原文显示」，
//!    不再让用户以为这是产品行为；逐条加前缀会污染输出，故只在汇总行说明。
//! 2. **待译清单**：每条未译记录（语言/通道/错误码/消息原文）去重后按语言分档
//!    落盘到 `~/.rz/gaps/` 目录，格式与语料门禁清单同构
//!    （通道列取主消息级、建议级、工具级三值），维护者拿到清单即可补表；
//!    补完后用 `rzc gaps --clear` 清零。
//!
//! 恒等语言例外：英语的「原文」就是母语输出，所以 [`登记缺口`] 对
//! [`包::语言::是恒等语言`] 直接返回，不登记也不落盘。否则英语用户每次报错
//! 都会多一行「有 N 条消息暂无译文」，`~/.rz/gaps/en.tsv` 也会积成一份无法
//! 补表的清单（实测曾积 1000+ 行）。
//!
//! 清单每行只有一个字段能装下整条记录，所以消息原文里的换行与制表符在落盘前
//! 会被转成字面量（见 [`编码缺口字段`]）。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// 一条未译记录：`(语言, 通道, 错误码, 消息原文)`
pub type 待译记录 = (String, String, String, String);

/// 主消息通道（与语料门禁同义）
pub const 主消息级: &str = "main";
/// 建议子诊断通道（与语料门禁同义）
pub const 建议级: &str = "help";
/// 构建工具纯文本通道（非 JSON 输出）
pub const 工具级: &str = "tool";

/// 进程内去重登记表（同一轮重复报错只记一次）
fn 登记表<'甲>() -> &'甲 Mutex<BTreeSet<待译记录>> {
    static 登记缓存: OnceLock<Mutex<BTreeSet<待译记录>>> = OnceLock::new();
    登记缓存.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// 登记一条未译消息（原文照旧显示，仅记录待补线索）
///
/// 空消息不记（无补表价值）。语言为空时取当前界面语言。恒等语言（英语）不记。
pub fn 登记缺口(语言: &str, 此级: &str, 错误码: &str, 全文: &str) {
    let 全文 = 全文.trim();
    if 全文.is_empty() {
        return;
    }
    let 语言 = if 语言.is_empty() {
        crate::语言::当前语言()
    } else {
        语言.to_string()
    };
    if crate::语言::是恒等语言(&语言) {
        return;
    }
    let 记录 = (语言, 此级.to_string(), 错误码.to_string(), 全文.to_string());
    if let Ok(mut 登记集) = 登记表().lock() {
        登记集.insert(记录);
    }
}

/// 取出并清空本轮登记（调用方负责落盘与提示，避免重复处理）
pub fn 取出缺口() -> Vec<待译记录> {
    let Ok(mut 登记集) = 登记表().lock() else {
        return Vec::new();
    };
    std::mem::take(&mut *登记集).into_iter().collect()
}

/// 当前登记快照（不清空，供测试与只读查看）
pub fn 查看缺口() -> Vec<待译记录> {
    登记表()
        .lock()
        .map(|登记集| 登记集.iter().cloned().collect())
        .unwrap_or_default()
}

/// 待译清单目录（主目录不可用时回退临时目录）
pub fn 清单目录() -> PathBuf {
    match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(主目录) if !主目录.is_empty() => PathBuf::from(主目录).join(".rz").join("gaps"),
        _ => std::env::temp_dir().join("rz-gaps"),
    }
}

/// 某语言的待译清单路径
pub fn 清单项文件(语言: &str) -> PathBuf {
    清单目录().join(format!("{}.tsv", 语言))
}

/// 一行清单文本（与 [`解析清单行`] 互逆）
fn 清单行(记录: &待译记录) -> String {
    let (语言, 此级, 错误码, 全文) = 记录;
    format!("{语言}\t{此级}\t{错误码}\t{}", 编码缺口字段(全文))
}

/// 把消息原文编码成单行清单字段（先转义反斜杠，再处理换行、回车与制表符）
pub fn 编码缺口字段(全文: &str) -> String {
    全文
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// [`编码缺口字段`] 的逆操作：把转义序列还原成真实字符
pub fn 解码缺口字段(原始: &str) -> String {
    let mut 拼接串 = String::with_capacity(原始.len());
    let mut 字符流 = 原始.chars();
    while let Some(每字符) = 字符流.next() {
        if 每字符 != '\\' {
            拼接串.push(每字符);
            continue;
        }
        match 字符流.next() {
            Some('n') => 拼接串.push('\n'),
            Some('r') => 拼接串.push('\r'),
            Some('t') => 拼接串.push('\t'),
            Some('\\') => 拼接串.push('\\'),
            Some(其他) => {
                拼接串.push('\\');
                拼接串.push(其他);
            }
            // 结尾孤立反斜杠：原样保留，不吞字符
            None => 拼接串.push('\\'),
        }
    }
    拼接串
}

/// 把行尾多出来的列拼回消息（历史未转义行里消息本身带制表符）
fn 制表尾(额外列: &[&str]) -> String {
    额外列
        .iter()
        .map(|片段| format!("\t{}", 片段))
        .collect::<Vec<_>>()
        .join("")
}

/// 是否为已知通道名（清单行形状的唯一依据）
fn 是已知通道(此列: &str) -> bool {
    此列 == 主消息级 || 此列 == 建议级 || 此列 == 工具级
}

/// 解析清单文本：容忍注释行与缺列（缺列按主消息级/空码处理）
///
/// 行形状按「第二列是不是通道名」判，不看首列长度；几类历史残缺（裸续行、
/// 未转义制表符）在这里一并收拾，续行拼回上一条记录的消息（还原真实换行）。
pub fn 解析清单行(原始: &str) -> Vec<待译记录> {
    let mut 已解析: Vec<待译记录> = Vec::new();
    for 文本行 in 原始
        .lines()
        .map(|每行| 每行.strip_suffix('\r').unwrap_or(每行))
        .filter(|每行| !每行.trim().is_empty() && !每行.starts_with('#'))
    {
        let 列集: Vec<&str> = 文本行.split('\t').collect();
        // 拼回上一条记录用：统一带换行前缀，尾随空白不带走格式信息
        let 折叠 = |主记录: &mut 待译记录, 尾段: &str| {
            主记录.3.push('\n');
            主记录.3.push_str(尾段.trim_end());
        };
        let 记录 = match 列集.as_slice() {
            // 「语言\t通道\t错误码\t消息」：消息列本身可能含制表符
            [语言, 此级, 错误码, 其余 @ ..] if 是已知通道(此级) => (
                语言.to_string(),
                此级.to_string(),
                错误码.to_string(),
                解码缺口字段(&其余.join("\t")),
            ),
            // 语料门禁三列格式，语言取默认
            [此级, 错误码, 消息文, 额外列 @ ..] if 是已知通道(此级) => (
                crate::语言::当前语言(),
                此级.to_string(),
                错误码.to_string(),
                解码缺口字段(&format!("{}{}", 消息文, 制表尾(额外列))),
            ),
            // 三列但第二列不是通道名：历史裸续行残行
            [语言, 错误码, 消息文, 额外列 @ ..] if !语言.is_empty() => {
                let 尾段 = match (错误码.is_empty(), 消息文.is_empty()) {
                    (true, _) => *错误码,
                    (_, true) => *消息文,
                    _ => &format!("{}\t{}{}", 错误码, 消息文, 制表尾(额外列)),
                };
                if 已解析.last().is_some_and(|记录| 记录.0 == *语言) {
                    if let Some(主记录) = 已解析.last_mut() {
                        折叠(主记录, 尾段);
                    }
                    continue;
                }
                (
                    语言.to_string(),
                    主消息级.to_string(),
                    错误码.to_string(),
                    解码缺口字段(消息文),
                )
            }
            // 两列且首列是通道名：语料格式的缺消息列变体
            [此级, 消息文] if 是已知通道(此级) => (
                crate::语言::当前语言(),
                此级.to_string(),
                String::new(),
                解码缺口字段(消息文),
            ),
            // 两列且首列不是通道名：缺列变体
            [语言, 消息文] if !语言.is_empty() => (
                语言.to_string(),
                主消息级.to_string(),
                String::new(),
                解码缺口字段(消息文),
            ),
            // 单列：优先当作上一条记录的裸续行拼回
            [消息文] => {
                if let Some(主记录) = 已解析.last_mut() {
                    折叠(主记录, 消息文);
                    continue;
                }
                (
                    crate::语言::当前语言(),
                    主消息级.to_string(),
                    String::new(),
                    解码缺口字段(消息文),
                )
            }
            // 无主记录的残行：拼回上一条记录
            [..] => {
                if let Some(主记录) = 已解析.last_mut() {
                    折叠(主记录, 文本行);
                }
                continue;
            }
        };
        if !已解析.contains(&记录) {
            已解析.push(记录);
        }
    }
    已解析
}

/// 把本轮登记合并写入清单文件（去重），返回新增条数
///
/// 采用**整档重写**而非追加，顺带完成历史裸行的格式迁移；写盘失败不阻断主流程。
pub fn 落盘缺口至目录(目录: &Path, 记录集: &[待译记录]) -> usize {
    if 记录集.is_empty() {
        return 0;
    }
    let mut 新增数 = 0usize;
    for 语言 in 记录集
        .iter()
        .map(|记录| 记录.0.clone())
        .collect::<BTreeSet<_>>()
    {
        let 本轮: Vec<&待译记录> = 记录集.iter().filter(|记录| 记录.0 == 语言).collect();
        let 文件路径 = 目录.join(format!("{}.tsv", 语言));
        let 原始 = std::fs::read_to_string(&文件路径).unwrap_or_default();
        let mut 已存在 = 解析清单行(&原始);
        let mut 新增: Vec<&待译记录> = Vec::new();
        for 记录 in 本轮 {
            if !已存在.contains(记录) {
                已存在.push(记录.clone());
                新增.push(记录);
            }
        }
        新增数 += 新增.len();
        if 新增.is_empty() {
            // 本轮无新记录：不动盘
            continue;
        }
        if let Some(父目录) = 文件路径.parent()
            && std::fs::create_dir_all(父目录).is_err()
        {
            continue;
        }
        已存在.sort();
        let 文本内容 = 已存在.iter().map(清单行).collect::<Vec<_>>().join("\n");
        let 临时档 = 目录.join(format!(".{}.tsv.tmp", 语言));
        if std::fs::write(&临时档, format!("{}\n", 文本内容)).is_err()
            || std::fs::rename(&临时档, &文件路径).is_err()
        {
            let _ = std::fs::remove_file(&临时档);
        }
    }
    新增数
}

/// [`落盘缺口至目录`] 的默认目录版本
pub fn 落盘缺口(记录集: &[待译记录]) -> usize {
    落盘缺口至目录(&清单目录(), 记录集)
}

/// 读取某语言已积累的待译清单
pub fn 读取缺口(语言: &str) -> Vec<待译记录> {
    std::fs::read_to_string(清单项文件(语言))
        .map(|原始| 解析清单行(&原始))
        .unwrap_or_default()
}

/// 清空某语言的待译清单（补完表后归零），返回是否确实删除了文件
pub fn 清空缺口(语言: &str) -> bool {
    std::fs::remove_file(清单项文件(语言)).is_ok()
}

#[cfg(test)]
mod 母语测试 {
    use super::*;
    use std::sync::MutexGuard;

    static 登记测试锁: Mutex<()> = Mutex::new(());

    fn 串行登记<'甲>() -> MutexGuard<'甲, ()> {
        match 登记测试锁.lock() {
            Ok(守卫) => 守卫,
            Err(中毒) => 中毒.into_inner(),
        }
    }

    #[test]
    fn 记录取出查看往返() {
        let _串行 = 串行登记();
        let 记录 = (
            "tstlang".to_string(),
            主消息级.to_string(),
            "E9999".to_string(),
            "unique message A".to_string(),
        );
        登记表().lock().unwrap().insert(记录.clone());
        assert!(查看缺口().contains(&记录));
        let 已取出 = 取出缺口();
        assert!(已取出.contains(&记录), "取出应含已登记记录");
        assert!(!查看缺口().contains(&记录), "取出后应清空");
    }

    #[test]
    fn 空白消息不登记() {
        let _串行 = 串行登记();
        登记缺口("tstempty", 建议级, "", "   ");
        assert!(
            !查看缺口().iter().any(|记录| 记录.0 == "tstempty"),
            "空白消息不应进入待译登记"
        );
    }

    #[test]
    fn 恒等语言不登记() {
        let _串行 = 串行登记();
        let 待译消息 = "crate name `计算器` passed to `--extern` is not a valid ASCII identifier";
        登记缺口("en", 工具级, "", 待译消息);
        assert!(
            !查看缺口()
                .iter()
                .any(|记录| 记录.0 == "en" && 记录.3 == 待译消息),
            "en 是恒等语言，未命中消息表不算待译"
        );
        登记缺口("de", 工具级, "", 待译消息);
        assert!(
            查看缺口()
                .iter()
                .any(|记录| 记录.0 == "de" && 记录.3 == 待译消息),
            "de 不是恒等语言，同一句应照常登记待译"
        );
    }

    #[test]
    fn 恒等语言作用于当前语言回退() {
        let _串行 = 串行登记();
        let _守卫 = crate::语言::钉住语言("en");
        let 待译消息 = "an english message with no entry in the identity pack";
        登记缺口("", 主消息级, "", 待译消息);
        assert!(
            !查看缺口().iter().any(|记录| 记录.3 == 待译消息),
            "界面语言为 en 时也不得登记待译"
        );
    }

    #[test]
    fn 清单行往返与容错() {
        let 记录 = (
            "zh".to_string(),
            工具级.to_string(),
            String::new(),
            "failed to parse manifest at `/x/Cargo.toml`".to_string(),
        );
        let 文本内容 = format!("# 注释\n\n{}\n", 清单行(&记录));
        let 已解析 = 解析清单行(&文本内容);
        assert_eq!(已解析, vec![记录]);
        let 三列 = "help\t\tthe trait `?` is not implemented";
        let 已解析 = 解析清单行(三列);
        assert_eq!(已解析.len(), 1);
        assert_eq!(已解析[0].1, "help");
        assert_eq!(已解析[0].3, "the trait `?` is not implemented");
    }

    #[test]
    fn 多行消息落盘为一行() {
        let 待译消息 = "the following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\n  `&f64` implements `Add<f64>`\nand 56 others";
        let 记录 = (
            "ztml".to_string(),
            主消息级.to_string(),
            "E0277".to_string(),
            待译消息.to_string(),
        );
        let 文本行 = 清单行(&记录);
        assert_eq!(文本行.lines().count(), 1, "多行消息不得拆成多行");
        assert_eq!(文本行.split('\t').count(), 4, "一行恰好四列");
        assert_eq!(解析清单行(&文本行), vec![记录.clone()]);

        let 带页 = (
            "ztml".to_string(),
            工具级.to_string(),
            String::new(),
            "first\tsecond".to_string(),
        );
        let 文本行 = 清单行(&带页);
        assert_eq!(文本行.split('\t').count(), 4, "消息里的制表符不得多开列");
        assert_eq!(解析清单行(&文本行), vec![带页]);

        let 含字面量 = r#"expected `\n` in input"#;
        assert_eq!(
            解码缺口字段(&编码缺口字段(含字面量)),
            含字面量,
            "消息里的换行字面量应原样往返"
        );
    }

    #[test]
    fn 历史裸续行拼回() {
        let 原始 = "ja\thelp\tE0277\tthe following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\n  `&f64` implements `Add<f64>`\nand 56 others\nja\tmain\t\tstandalone message\n";
        let 已解析 = 解析清单行(原始);
        assert_eq!(已解析.len(), 2, "跨行记录 + 独立记录 = 2 条，不是 5 条");
        assert_eq!(已解析[0].0, "ja", "拼回的续行不得改变语言归属");
        assert_eq!(
            已解析[0].3,
            "the following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\n  `&f64` implements `Add<f64>`\nand 56 others"
        );
        assert_eq!(已解析[1].3, "standalone message");

        let 文本行 = 清单行(&已解析[0]);
        assert_eq!(文本行.lines().count(), 1);
        assert_eq!(解析清单行(&文本行), vec![已解析[0].clone()]);

        let 开头裸行 = "orphan tail\nzh\tmain\t\treal message\n";
        let 已解析 = 解析清单行(开头裸行);
        assert_eq!(已解析.len(), 2, "无主续行不能吞掉后面的正常记录");
        assert_eq!(已解析[0].3, "orphan tail");
        assert_eq!(已解析[1].3, "real message");
    }

    #[test]
    fn 落盘跨轮去重() {
        let 目录 = tempfile::tempdir().unwrap();
        let mut 记录集 = vec![
            (
                "zhtest".to_string(),
                主消息级.to_string(),
                "E0000".to_string(),
                "first message".to_string(),
            ),
            (
                "zhtest".to_string(),
                建议级.to_string(),
                String::new(),
                "help message".to_string(),
            ),
        ];
        记录集.sort();
        assert_eq!(落盘缺口至目录(目录.path(), &记录集), 2);
        assert_eq!(落盘缺口至目录(目录.path(), &记录集), 0, "重复登记不应翻倍");
        let 读回 =
            解析清单行(&std::fs::read_to_string(目录.path().join("zhtest.tsv")).unwrap());
        assert_eq!(读回, 记录集);
    }

    #[test]
    fn 落盘迁移历史多行文件() {
        let 目录 = tempfile::tempdir().unwrap();
        let 文件路径 = 目录.path().join("mlang.tsv");
        std::fs::write(
            &文件路径,
            "mlang\thelp\tE0277\tthe following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\nand 56 others\n",
        )
        .unwrap();

        let 同一句 = (
            "mlang".to_string(),
            建议级.to_string(),
            "E0277".to_string(),
            "the following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\nand 56 others".to_string(),
        );
        let 新记录 = (
            "mlang".to_string(),
            主消息级.to_string(),
            String::new(),
            "a brand new english message".to_string(),
        );
        assert_eq!(
            落盘缺口至目录(目录.path(), &[同一句.clone(), 新记录]),
            1,
            "已存在的多行记录不得再算新增"
        );
        let 文本内容 = std::fs::read_to_string(&文件路径).unwrap();
        assert_eq!(文本内容.lines().count(), 2, "迁移后一行一条记录");
        assert_eq!(
            文本内容.matches("and 56 others").count(),
            1,
            "同一句不得重复落盘"
        );

        let 先前 = std::fs::read_to_string(&文件路径).unwrap();
        assert_eq!(落盘缺口至目录(目录.path(), &[同一句]), 0);
        assert_eq!(
            std::fs::read_to_string(&文件路径).unwrap(),
            先前,
            "无新增时不应改动文件"
        );
        assert_eq!(
            解析清单行(&先前)
                .iter()
                .filter(|记录| 记录.0 == "mlang")
                .count(),
            2,
            "rzc gaps 的条数应等于待译句子数"
        );
    }
}
