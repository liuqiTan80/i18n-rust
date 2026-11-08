//! 映射 TOML 构建：与现有 crates/*.toml 格式一致，含免责声明与生成时间戳。

use std::collections::HashMap;

use i18n_rust_engine::映射源;

use super::{免责声明文本, 接口条目};

/// 映射文本转义（含回车与其余控制字符，避免生成非法映射表）
fn 转义映射文本(文本值: &str) -> String {
    let 已转义 = 文本值
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r");
    已转义
        .chars()
        .map(|本字| {
            if (本字 as u32) < 32 {
                format!("\\u{:04X}", 本字 as u32)
            } else {
                本字.to_string()
            }
        })
        .collect()
}

/// 当前时间字符串（世界标准时，`年-月-日 时:分:秒`）
fn 当前时间戳() -> String {
    let 总秒数 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|时长| 时长.as_secs())
        .unwrap_or_default();
    let 天数 = (总秒数 / 86_400) as i64;
    let 日内秒数 = 总秒数 % 86_400;
    let (年份, 月份, 日号) = 拆分日期(天数);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        年份,
        月份,
        日号,
        日内秒数 / 3_600,
        (日内秒数 % 3_600) / 60,
        日内秒数 % 60
    )
}

/// 天数 → (年, 月, 日)（民用历反算，与引擎日志模块同一算法）
fn 拆分日期(天数: i64) -> (i64, u32, u32) {
    let 移位日 = 天数 + 719_468;
    let 纪元 = 移位日.div_euclid(146_097);
    let 纪元内日 = 移位日 - 纪元 * 146_097;
    let 纪元内年 = (纪元内日 - 纪元内日 / 1_460 + 纪元内日 / 36_524 - 纪元内日 / 146_096) / 365;
    let 年份 = 纪元内年 + 纪元 * 400;
    let 年内日 = 纪元内日 - (365 * 纪元内年 + 纪元内年 / 4 - 纪元内年 / 100);
    let 年内月 = (5 * 年内日 + 2) / 153;
    let 日号 = (年内日 - (153 * 年内月 + 2) / 5 + 1) as u32;
    let 月份 = (年内月 + if 年内月 < 10 { 3 } else { -9 }) as u32;
    let 年份 = if 月份 <= 2 { 年份 + 1 } else { 年份 };
    (年份, 月份, 日号)
}

/// 构建语言包映射表（与现有 crates/*.toml 格式一致：
/// `模块路径` 与 `标识符` 两节为映射管理器识别的字符串值格式，
/// `解释` 节为扩展，映射管理器加载时自动忽略，保持兼容）。
/// 目标版本为提取时实际解析到的基准版本（写入文件头，供复现与比对）。
pub fn 构建映射表(
    语言代码: &str,
    包名: &str,
    包中文名: &str,
    目标版本: Option<&str>,
    条目表: &[接口条目],
    中文名表: &[(String, String)],
    解释表: &HashMap<String, String>,
) -> String {
    let 界面表 = crate::界面::界面消息表::按语言建表(语言代码);
    let mut 输出 = String::new();
    输出.push_str(&format!("{}\n", 界面表.取消息("mg_header_title")));
    输出.push_str(&format!(
        "{}\n",
        界面表.取带参消息("mg_header_crate", &[包名])
    ));
    // 生成基准版本（目标版本参数锁定依据；解析不到版本时省略整行）
    if let Some(版本文) = 目标版本 {
        输出.push_str(&format!(
            "{}\n",
            界面表.取带参消息("mg_header_target_version", &[版本文])
        ));
    }
    输出.push_str(&format!(
        "{}\n",
        界面表.取带参消息("mg_header_generated_at", &[&当前时间戳()])
    ));
    输出.push_str(&format!("# {}\n\n", 免责声明文本(语言代码)));

    输出.push_str("[\"模块路径\"]\n");
    输出.push_str(&format!(
        "\"{}\" = \"{}\"\n\n",
        转义映射文本(包中文名),
        转义映射文本(包名)
    ));

    输出.push_str("[\"标识符\"]\n");
    // 预构建英文名→签名索引，避免二次方线性查找
    let 签名索引: HashMap<&str, &str> = 条目表
        .iter()
        .map(|条目| (条目.英文名.as_str(), 条目.签名.as_str()))
        .collect();
    for (中文名, 英文名) in 中文名表 {
        let 签名 = 签名索引.get(英文名.as_str()).copied().unwrap_or("");
        输出.push_str(&format!(
            "\"{}\" = \"{}\"  # {}\n",
            转义映射文本(中文名),
            转义映射文本(英文名),
            签名
        ));
    }
    输出.push('\n');

    输出.push_str("[\"解释\"]\n");
    for (中文名, _) in 中文名表 {
        let 解释 = 解释表.get(中文名).map(String::as_str).unwrap_or("");
        输出.push_str(&format!(
            "\"{}\" = \"{}\"\n",
            转义映射文本(中文名),
            转义映射文本(解释)
        ));
    }

    // 危险等级：解包/期待 等目标调用即恐慌，必须显式写进映射文件，
    // 否则调用方会把它当纯安全接口（词条表也据此逐条标注）
    let mut 恐慌条目: Vec<&str> = Vec::new();
    for (中文名, 英文名) in 中文名表 {
        if 映射源::恒恐慌目标(英文名) && !恐慌条目.contains(&中文名.as_str()) {
            恐慌条目.push(中文名.as_str());
        }
    }
    if !恐慌条目.is_empty() {
        输出.push_str("\n[\"危险等级\"]\n");
        for 名称 in 恐慌条目 {
            输出.push_str(&format!("\"{}\" = \"panic\"\n", 转义映射文本(名称)));
        }
    }
    输出
}

#[cfg(test)]
mod 母语测试 {
    use super::super::文档抽取::{提取公开接口, 示例文档};
    use super::super::规则::规则生成中文名;
    use super::*;

    #[test]
    fn 映射输出与免责声明() {
        let 条目表 = 提取公开接口(&示例文档()).unwrap();
        let 中文名表: Vec<(String, String)> = 条目表
            .iter()
            .map(|条目| (规则生成中文名(&条目.英文名), 条目.英文名.clone()))
            .collect();
        let mut 解释表 = HashMap::new();
        解释表.insert("新建".to_string(), "创建新的值".to_string());
        let 映射文本 = 构建映射表(
            "zh",
            "示例",
            "示例库",
            Some("1.2.3"),
            &条目表,
            &中文名表,
            &解释表,
        );
        // 免责声明必须存在
        assert!(映射文本.contains(&免责声明文本("zh")), "缺少免责声明");
        assert!(映射文本.contains("# crate: 示例"));
        // 生成基准版本写入文件头（目标版本参数锁定依据）
        assert!(映射文本.contains("# 基准版本: 1.2.3"));
        // 可被标准映射解析
        let 值: toml::Value = toml::from_str(&映射文本).expect("映射表应可解析");
        assert!(!值.get("模块路径").is_none());
        assert!(!值.get("标识符").is_none());
        assert!(!值.get("解释").is_none());
        assert_eq!(值["模块路径"]["示例库"].as_str(), Some("示例"));
        assert_eq!(值["标识符"]["新建"].as_str(), Some("new"));
        assert_eq!(值["标识符"]["状态"].as_str(), Some("State"));
        assert_eq!(值["解释"]["新建"].as_str(), Some("创建新的值"));
    }

    #[test]
    fn 映射输出标注恐慌目标() {
        let 条目表 = 提取公开接口(&示例文档()).unwrap();
        // 生成物里的解包类目标（调用即恐慌）必须显式标注危险等级
        let 中文名表 = vec![
            ("解包".to_string(), "unwrap".to_string()),
            ("新建".to_string(), "new".to_string()),
        ];
        let 映射文本 = 构建映射表(
            "zh",
            "示例",
            "示例库",
            None,
            &条目表,
            &中文名表,
            &HashMap::new(),
        );
        let 值: toml::Value = toml::from_str(&映射文本).expect("映射表应可解析");
        assert_eq!(
            值["危险等级"]["解包"].as_str(),
            Some("panic"),
            "会恐慌的目标必须显式标注: {映射文本}"
        );
        assert!(
            值["危险等级"].get("新建").is_none(),
            "安全目标不应被标注危险: {映射文本}"
        );
    }

    #[test]
    fn 映射可被映射管理器载入() {
        let 条目表 = 提取公开接口(&示例文档()).unwrap();
        let 中文名表: Vec<(String, String)> = 条目表
            .iter()
            .map(|条目| (规则生成中文名(&条目.英文名), 条目.英文名.clone()))
            .collect();
        let 映射文本 = 构建映射表(
            "zh",
            "示例",
            "示例库",
            None,
            &条目表,
            &中文名表,
            &HashMap::new(),
        );
        let 临时目录 = tempfile::tempdir().unwrap();
        // 模拟语言包目录结构：关键字表 + crates/示例.toml（映射管理器要求关键字表存在）
        std::fs::write(
            临时目录.path().join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        let 包目录 = 临时目录.path().join("crates");
        std::fs::create_dir_all(&包目录).unwrap();
        std::fs::write(包目录.join("示例.toml"), 映射文本).unwrap();
        let 管理器 =
            i18n_rust_engine::映射管理器::映射管理器::从目录载入(临时目录.path())
                .expect("映射管理器应能加载");
        // 解释节被忽略，标识符与模块路径正常生效
        assert_eq!(
            管理器.模块路径表.get("示例库").map(String::as_str),
            Some("示例")
        );
        assert_eq!(
            管理器.标识符别名.get("新建").map(String::as_str),
            Some("new")
        );
        assert_eq!(
            管理器.标识符别名.get("状态").map(String::as_str),
            Some("State")
        );
    }
}
