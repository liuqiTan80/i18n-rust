//! 母语编程指南渲染（`rzc cheat` / `rzc guide` / `rzc init --ai` 共用）
//!
//! 把「词条表渲染」从命令行下沉到引擎后，`rzc guide` 与 `rzc init --ai` 复用同一
//! 数据源与表格版式：语言包词条一旦更新，三条出口同步生效，不会出现
//! 指南与 `rzc cheat` 输出漂移。
//!
//! 文本模板取自各语言包 `ui.toml` 的 `["界面消息"]` 节（`guide_*` 键），
//! 调用方须先 [`crate::语言::设定语言`]，与错误/诊断输出同一约定。
//! 词条表本体（分类/表头/危险等级标签/内容）与 `rzc cheat` 一致——
//! 附录A、项目说明文档均以该版式为基准。
//!
//! 本模块的两条纪律（都是重构后才立得住的）：
//!
//! - **版式只有一处实现**：markdown 表体由 [`渲染分类表`] 独任，`rzc cheat --markdown`
//!   与 `rzc guide` 都调它，差别只在标题的井号数。此前两处各写
//!   一遍同样的表头与行格式，靠注释口头承诺「版式完全一致」——一个以「避免
//!   漂移」为由下沉到引擎的模块，自己留着两处会漂移的副本。
//! - **行序稳定可复现**：每个分类内按母语词形的码位序排列。原实现沿用
//!   `哈希映射` 迭代序，实测 `rzc cheat zh` 两次运行相差 2900+ 行（排序后
//!   才零差异），于是 `rzc init --ai` / `rzc relocalize` 写出的 AGENTS.md
//!   每次都全表重排，给读者仓库制造无谓的 `git diff`。现改用
//!   [`std::collections::BTreeMap`] 归集，键序即输出序，无需再排序。

use crate::映射管理器::映射管理器;
use std::collections::{BTreeMap, HashMap};

/// 词条表分类标题（双语固定，与历史 `rzc cheat` 输出一致）
const 标题关键字: &str = "Keywords / 关键字";
const 标题模块路径: &str = "Module paths / 模块路径";
const 标题别名: &str = "Aliases / 别名";
const 标题派生: &str = "Derives / 派生特征";

/// 词条表的一行：一条母语词条、它的英文名、危险等级标签与备注
///
/// 原先是四元组 `(String, String, String, String)`，调用点靠位置取值
/// （`(native, en, level, note)`），顺序写错编译器无法发现。
struct 词条行 {
    母语: String,
    英文名: String,
    危险标签: String,
    备注: String,
}

/// 词条表的一个分类：标题与该标题下的全部行（行序已稳定）
struct 词条分类<'a> {
    标题: &'a str,
    词条们: Vec<词条行>,
}

/// 取全量词条表：过滤恒等条目（母语 == 英文）后按分类返回行
///
/// 分类顺序固定为「关键字 → 模块路径 → 别名 → 派生特征」；同一分类内的行
/// 按母语词形的码位序排列（靠有序映射归集，见模块头说明）。
fn 词条分类表(管理器: &映射管理器) -> Vec<词条分类<'_>> {
    let 四张表: [(&str, &HashMap<String, String>); 4] = [
        (标题关键字, 管理器.取关键词表()),
        (标题模块路径, 管理器.取模块路径表()),
        (标题别名, 管理器.取别名表()),
        (标题派生, 管理器.取派生特征表()),
    ];
    四张表
        .into_iter()
        .map(|(标题, 表)| {
            // 键是母语词形：迭代即码位序，同时天然去掉同偏移的重复项
            let mut 归集: BTreeMap<String, 词条行> = BTreeMap::new();
            for (母语, 英文) in 表.iter() {
                if 母语 == 英文 {
                    continue;
                }
                let 危险 = 管理器.取危险详情(母语);
                归集.insert(
                    母语.clone(),
                    词条行 {
                        母语: 母语.clone(),
                        英文名: 英文.clone(),
                        危险标签: 危险.等级.标签().to_string(),
                        备注: 危险.备注,
                    },
                );
            }
            // 有序映射的取全部值按键升序交出所有权，消费掉映射即可，不必再排序
            词条分类 {
                标题,
                词条们: 归集.into_values().collect(),
            }
        })
        .collect()
}

/// markdown 表格单元转义：竖线与换行会破坏表格结构
fn 转义单元(内容: &str) -> String {
    内容.replace('|', "\\|").replace(['\n', '\r'], " ")
}

/// 危险等级图例（markdown 版式，逐字节对齐原 `rzc cheat --markdown` 输出）
fn 危险等级图例() -> String {
    let mut 出 = String::new();
    出.push_str("> **危险等级图例**：\n");
    出.push_str(
        "> - `纯安全`：只在受保护位置（声明位/方法调用位/use 路径段）替换，不改写自定义标识符；\n",
    );
    出.push_str("> - `会改写标识符`：词法层任意位置替换，同名自定义标识符也会被改写；\n");
    出.push_str(
        "> - `会 panic`：运行时可能 panic，默认请选安全同义条目，仅调用处显式选择时使用；\n",
    );
    出.push_str("> - 同一词条同时涉及多种风险时以 `+` 连接，并在「备注」列说明。\n\n");
    出
}

/// 渲染单个分类的 markdown 表：标题行 + 表头 + 行集合 + 表后空行
///
/// `井号` 决定标题层级——`rzc cheat --markdown` 用 `##`（表格直接贴一级
/// 标题之下），[`渲染母语指南`] 用 `###`（表格挂在「词条表」小节之下）。
/// 两个出口共用本函数，表头与行格式因此不可能各写一遍再漂移。
/// 空分类返回空串，调用方无需跳过。
fn 渲染分类表(分类: &词条分类<'_>, 语言码: &str, 井号: &str) -> String {
    if 分类.词条们.is_empty() {
        return String::new();
    }
    let mut 出 = String::new();
    出.push_str(&format!("{井号} {}\n\n", 分类.标题));
    出.push_str(&format!(
        "| {语言码} | Rust | 危险等级 | 备注 |\n|---|---|---|---|\n"
    ));
    for 项 in &分类.词条们 {
        出.push_str(&format!(
            "| {} | `{}` | {} | {} |\n",
            项.母语,
            项.英文名,
            项.危险标签,
            转义单元(&项.备注)
        ));
    }
    出.push('\n');
    出
}

/// 渲染单个分类的终端版式：母语词形按最宽者左对齐
fn 渲染终端分类(分类: &词条分类<'_>) -> String {
    if 分类.词条们.is_empty() {
        return String::new();
    }
    let 宽度 = 分类
        .词条们
        .iter()
        .map(|项| 项.母语.chars().count())
        .max()
        .unwrap_or(0);
    let mut 出 = format!("── {} ──\n", 分类.标题);
    for 项 in &分类.词条们 {
        let 补空 = 宽度 - 项.母语.chars().count();
        let 备注段 = if 项.备注.is_empty() {
            String::new()
        } else {
            format!("  // {}", 项.备注)
        };
        出.push_str(&format!(
            "  {}{}  {}  [{}]{}\n",
            项.母语,
            " ".repeat(补空),
            项.英文名,
            项.危险标签,
            备注段
        ));
    }
    出.push('\n');
    出
}

/// 渲染 `rzc cheat` 完整输出（markdown 与终端两种版式）
///
/// 与下沉前的 CLI 实现内容一致（分类/版式/条目相同），同一分类内的行序
/// 由稳定排序取代原来的 `哈希映射` 迭代序；`语言码` 只影响标题与表头列名。
pub fn 渲染速查表(管理器: &映射管理器, 语言码: &str, 是否标记: bool) -> String {
    let 分类们 = 词条分类表(管理器);
    let 总条数: usize = 分类们.iter().map(|分| 分.词条们.len()).sum();
    let mut 出 = String::new();
    if 总条数 == 0 {
        // 恒等语言（如英文语言码）：整表映射到自身，没有可速查的内容。下面三行
        // 英文是本模块唯一不进 ui.toml 的文案——它刻意不随界面语言切换：
        // 读者既然在用恒等语言，要的就是「照常写英文 Rust」这句本字。
        出.push_str(&format!("rzc cheat — {语言码} ↔ Rust\n\n"));
        出.push_str("This language pack maps every identifier to itself (identity mapping).\n");
        出.push_str("No cheat sheet is needed — write Rust as usual.\n");
        return 出;
    }
    if 是否标记 {
        出.push_str(&format!("# rzc cheat — {语言码} ↔ Rust ({总条数})\n\n"));
        出.push_str(&危险等级图例());
        for 分类 in &分类们 {
            出.push_str(&渲染分类表(分类, 语言码, "##"));
        }
        出.push_str(&format!(
            "> Generated by `rzc cheat {语言码} --markdown`.\n"
        ));
    } else {
        出.push_str(&format!(
            "rzc cheat — {语言码} ↔ Rust（共 {总条数} 条）\n\n"
        ));
        出.push_str(
            "危险等级：纯安全 = 受保护位置替换｜会改写标识符 = 任意位置替换（同名标识符会被改写）｜会 panic = 运行时可能 panic（仅显式选择）\n\n",
        );
        for 分类 in &分类们 {
            出.push_str(&渲染终端分类(分类));
        }
    }
    出
}

/// 渲染 markdown 词条表本体（危险等级图例 + 全部分类表，不含标题行）
///
/// 供 [`渲染母语指南`] 组装：指南自身有一级标题，此处只出图例与表格，
/// 版式与 `rzc cheat --markdown` 完全一致（同一函数，只差井号数）。
pub fn 渲染词条表(管理器: &映射管理器, 语言码: &str) -> String {
    渲染分类表本体(&词条分类表(管理器), 语言码)
}

/// 指南正文里的词条表：图例 + 各分类（`###` 层级）
///
/// [`渲染词条表`] 与 [`渲染母语指南`] 共用的本体；指南已经取过一次表，
/// 故本函数直接吃分类集合，不再重复加载。
fn 渲染分类表本体(分类们: &[词条分类<'_>], 语言码: &str) -> String {
    let mut 出 = 危险等级图例();
    for 分类 in 分类们 {
        出.push_str(&渲染分类表(分类, 语言码, "###"));
    }
    出
}

/// 渲染面向学习者与人工智能助手的母语编程指南（markdown）
///
/// 结构：标题 → 引言语 → 书写规则（标识符/保留字转义/标点/危险等级/工作流/实测坑/待译清单）
/// → 入口示例（语言包 `template_main`）→ 全量词条表 → 尾注。
/// 全部文案来自 `ui.toml` 的 `guide_*` 键，随 `设定语言` 切换。
pub fn 渲染母语指南(管理器: &映射管理器, 语言码: &str) -> String {
    use crate::语言::{f as 带参消息, t as 消息};
    let 分类们 = 词条分类表(管理器);
    let 总条数: usize = 分类们.iter().map(|分| 分.词条们.len()).sum();
    let mut 出 = String::new();
    出.push_str(&format!("{}\n\n", 带参消息("guide_title", &[语言码])));
    出.push_str(&format!("{}\n\n", 带参消息("guide_intro", &[语言码])));
    出.push_str(&format!("{}\n", 消息("guide_rules_head")));
    // 不带占位符的规则条目按此顺序成组输出；工作流那条带参，单独走
    for 键 in [
        "guide_rule_keywords",
        "guide_rule_ident",
        "guide_rule_raw_ident",
        "guide_rule_punct",
        "guide_rule_danger",
    ] {
        出.push_str(&format!("{}\n", 消息(键)));
    }
    // 工作流规则含 `{}` 占位（入口文件扩展名 = 语言码），须带参渲染
    出.push_str(&format!("{}\n", 带参消息("guide_rule_workflow", &[语言码])));
    // 实测坑（三引号多行字符串 / 取父路径返回借用）：来自真实项目踩坑，
    // 写进指南让人工智能助手少绕弯路；词条补齐母语支持后可删除本条规则
    出.push_str(&format!("{}\n", 消息("guide_rule_pitfalls")));
    出.push_str(&format!("{}\n", 消息("guide_rule_gaps")));
    出.push('\n');
    出.push_str(&format!("{}\n\n", 消息("guide_example_head")));
    出.push_str("```rust\n");
    出.push_str(消息("template_main").trim_end());
    出.push_str("\n```\n\n");
    出.push_str(&format!(
        "{}\n\n",
        带参消息("guide_table_head", &[&总条数.to_string()])
    ));
    // 词条表本体与 `rzc cheat --markdown` 同一份实现，只差标题井号数
    出.push_str(&渲染分类表本体(&分类们, 语言码));
    出.push_str(&format!("{}\n", 消息("guide_footer")));
    出
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    /// 造一个只含几条已知词条的管理器：测试不依赖真实语言包的内容
    fn 样本管理器() -> 映射管理器 {
        let mut 关键词: HashMap<String, String> = HashMap::new();
        关键词.insert("乙词".to_string(), "beta".to_string());
        关键词.insert("甲词".to_string(), "alpha".to_string());
        关键词.insert("恒等".to_string(), "恒等".to_string());
        let mut 别名: HashMap<String, String> = HashMap::new();
        别名.insert("取值".to_string(), "unwrap".to_string());
        映射管理器::从扁表构造(关键词, HashMap::new(), 别名)
    }

    /// 直接造分类集合，用于只测版式而不经过取表逻辑
    ///
    /// 生命周期写成不约束的 `'甲` 而非「常驻」写法：生命周期位不吃关键字表，
    /// 「静态」在那儿不会被换出（台账 F16），而方言源码区不允许留英文；
    /// 标题取的都是 `&'static 文本` 常量，收缩到任意 `'甲` 由协变自动完成。
    fn 样本分类们<'甲>() -> Vec<词条分类<'甲>> {
        vec![
            词条分类 {
                标题: 标题关键字,
                词条们: vec![
                    词条行 {
                        母语: "乙词".to_string(),
                        英文名: "beta".to_string(),
                        危险标签: "纯安全".to_string(),
                        备注: "带 | 竖线\n与换行".to_string(),
                    },
                    词条行 {
                        母语: "甲词".to_string(),
                        英文名: "alpha".to_string(),
                        危险标签: "会 panic".to_string(),
                        备注: String::new(),
                    },
                ],
            },
            词条分类 {
                标题: 标题别名,
                词条们: Vec::new(),
            },
        ]
    }

    #[test]
    fn 行序按母语词形码位稳定() {
        let 管理器 = 样本管理器();
        let 分类们 = 词条分类表(&管理器);
        assert!(分类们.len() == 4, "四张表都该出一个分类");
        let 关键行 = &分类们[0].词条们;
        // 插入序是 乙→甲；`乙`(U+4E59) 码位小于 `甲`(U+7532)，故稳定序把 乙 排前
        assert!(关键行.len() == 2, "恒等词条须被过滤");
        assert!(
            关键行[0].母语 == "乙词" && 关键行[1].母语 == "甲词",
            "行序须按码位而非插入序"
        );
        assert!(关键行[0].英文名 == "beta", "具名字段须与母语同行取对");
    }

    #[test]
    fn 危险等级随词条来源判定() {
        let 管理器 = 样本管理器();
        let 分类们 = 词条分类表(&管理器);
        // 关键字表词条：词法层无位置保护 → 会改写标识符
        assert!(分类们[0].词条们[0].危险标签 == "会改写标识符");
        // 别名表词条映射到「取成功值」：命中一定会恐慌的目标
        assert!(分类们[2].词条们[0].危险标签 == "会 panic");
        assert!(分类们[1].词条们.is_empty() && 分类们[3].词条们.is_empty());
    }

    #[test]
    fn 同一输入渲染两次逐字节一致() {
        // 旧实现沿用哈希映射迭代序，这条测试会时好时坏；有序归集后应恒等
        let 标记 = 渲染速查表(&样本管理器(), "zh", true);
        assert!(渲染速查表(&样本管理器(), "zh", true) == 标记);
        let 终端 = 渲染速查表(&样本管理器(), "zh", false);
        assert!(渲染速查表(&样本管理器(), "zh", false) == 终端);
    }

    #[test]
    fn 转义竖线与换行() {
        assert!(转义单元("a|b") == "a\\|b");
        assert!(转义单元("a\nb\rc") == "a b c");
    }

    #[test]
    fn 空分类不出版面() {
        let 分类们 = 样本分类们();
        assert!(渲染分类表(&分类们[1], "zh", "##").is_empty());
        assert!(渲染终端分类(&分类们[1]).is_empty());
    }

    #[test]
    fn 两个出口的表体只差标题层级() {
        let 分类们 = 样本分类们();
        let 甲 = 渲染分类表(&分类们[0], "zh", "##");
        let 乙 = 渲染分类表(&分类们[0], "zh", "###");
        assert!(甲.starts_with("## Keywords / 关键字\n\n"));
        assert!(乙.starts_with("### Keywords / 关键字\n\n"));
        let 去标题 =
            |首行: &str| -> String { 首行.split_once('\n').unwrap_or(("", "")).1.to_string() };
        assert!(去标题(&甲) == 去标题(&乙), "表头与行格式须逐字节相同");
        assert!(甲.contains("| 甲词 | `alpha` | 会 panic |  |\n"));
        assert!(甲.contains("| 乙词 | `beta` | 纯安全 | 带 \\| 竖线 与换行 |\n"));
    }

    #[test]
    fn 终端版式按最宽词形对齐() {
        let 出 = 渲染终端分类(&样本分类们()[0]);
        assert!(出.contains("── Keywords / 关键字 ──\n"));
        // 两条词形都是 2 字符 → 补空为 0，英文名紧跟两格空格
        assert!(出.contains("  乙词  beta  [纯安全]  // 带 | 竖线\n与换行\n"));
        assert!(出.contains("  甲词  alpha  [会 panic]\n"));
        assert!(
            出.ends_with("\n\n") || 出.ends_with("panic]\n\n"),
            "分类后须留一个空行"
        );
    }

    #[test]
    fn 恒等语言只出三行提示() {
        let 管理器 =
            映射管理器::从扁表构造(HashMap::new(), HashMap::new(), HashMap::new());
        for 是否标记 in [false, true] {
            let 出 = 渲染速查表(&管理器, "en", 是否标记);
            assert!(出.contains("rzc cheat — en ↔ Rust\n\n"));
            assert!(出.contains("identity mapping"));
            assert!(出.contains("write Rust as usual"));
            assert!(!出.contains("危险等级"), "恒等语言不该出图例");
        }
    }

    #[test]
    fn 图例与表头文案保持原样() {
        let 图例 = 危险等级图例();
        assert!(图例.starts_with("> **危险等级图例**：\n"));
        assert!(图例.ends_with("并在「备注」列说明。\n\n"));
        let 表 = 渲染分类表(&样本分类们()[0], "ja", "##");
        assert!(表.contains("| ja | Rust | 危险等级 | 备注 |\n|---|---|---|---|\n"));
    }

    #[test]
    fn 指南正文按语言包键组装() {
        use crate::语言::{当前语言, 设定语言};
        // 界面语言是进程级全局态：先存后复原，免得与并行用例互相干扰
        let 原语言 = 当前语言();
        设定语言("zh");
        let 出 = 渲染母语指南(&样本管理器(), "zh");
        设定语言(&原语言);
        assert!(出.contains("```rust\n"), "入口示例须带 rust 围栏");
        assert!(
            出.contains("### Keywords / 关键字"),
            "词条表须以三级标题挂在指南下"
        );
        assert!(出.ends_with("\n"), "指南须以换行收尾");
    }
}
