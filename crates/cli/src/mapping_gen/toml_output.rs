//! 映射 TOML 构建：与现有 crates/*.toml 格式一致，含免责声明与生成时间戳。

use std::collections::HashMap;

use super::{免责声明文本, 接口条目};

/// TOML 字符串转义（含 \r 与其余控制字符，避免生成非法 TOML）
fn 转义文本(串: &str) -> String {
    let 转义后 = 串
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r");
    转义后
        .chars()
        .map(|每字符| match 每字符 {
            每字符 if (每字符 as u32) < 0x20 => format!("\\u{:04X}", 每字符 as u32),
            每字符 => 每字符.to_string(),
        })
        .collect()
}

/// 当前时间字符串（UTC，`YYYY-MM-DD HH:MM:SS`）
fn 当前时间戳() -> String {
    let 秒数 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let 天数 = (秒数 / 86_400) as i64;
    let 当日秒 = 秒数 % 86_400;
    let (年, 月, 日) = 分解日期(天数);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        年,
        月,
        日,
        当日秒 / 3_600,
        (当日秒 % 3_600) / 60,
        当日秒 % 60
    )
}

/// 天数 → (年, 月, 日)（civil_from_days 算法，与引擎 logger.rs 一致）
fn 分解日期(天数: i64) -> (i64, u32, u32) {
    let 序日 = 天数 + 719_468;
    let 纪元 = 序日.div_euclid(146_097);
    let 纪元内日 = 序日 - 纪元 * 146_097;
    let 年偏移 = (纪元内日 - 纪元内日 / 1_460 + 纪元内日 / 36_524 - 纪元内日 / 146_096) / 365;
    let 年 = 年偏移 + 纪元 * 400;
    let 年内日 = 纪元内日 - (365 * 年偏移 + 年偏移 / 4 - 年偏移 / 100);
    let 月偏移 = (5 * 年内日 + 2) / 153;
    let 日 = (年内日 - (153 * 月偏移 + 2) / 5 + 1) as u32;
    let 月 = (月偏移 + if 月偏移 < 10 { 3 } else { -9 }) as u32;
    let 年 = if 月 <= 2 { 年 + 1 } else { 年 };
    (年, 月, 日)
}

/// 构建语言包映射 TOML（与现有 crates/*.toml 格式一致：
/// `["模块路径"]` / `["标识符"]` 两节为映射管理器识别的 String 值格式，
/// `["解释"]` 节为扩展，映射管理器加载时自动忽略，保持兼容）。
/// `目标版本串` 为提取时实际解析到的基准版本（写入文件头，供复现与比对）。
pub fn 构建映射文本(
    语言: &str,
    crate英文名: &str,
    crate中文名: &str,
    目标版本串: Option<&str>,
    条目表: &[接口条目],
    中文名表: &[(String, String)],
    解说表: &HashMap<String, String>,
) -> String {
    let 界面 = crate::ui::界面::按语言加载(语言);
    let mut 输出串 = String::new();
    输出串.push_str(&format!("{}\n", 界面.取文("mg_header_title")));
    输出串.push_str(&format!(
        "{}\n",
        界面.取文带参("mg_header_crate", &[crate英文名])
    ));
    // 生成基准版本（--target-version 锁定依据；解析不到版本时省略整行）
    if let Some(版本串) = 目标版本串 {
        输出串.push_str(&format!(
            "{}\n",
            界面.取文带参("mg_header_target_version", &[版本串])
        ));
    }
    输出串.push_str(&format!(
        "{}\n",
        界面.取文带参("mg_header_generated_at", &[&当前时间戳()])
    ));
    输出串.push_str(&format!("# {}\n\n", 免责声明文本(语言)));

    输出串.push_str("[\"模块路径\"]\n");
    输出串.push_str(&format!(
        "\"{}\" = \"{}\"\n\n",
        转义文本(crate中文名),
        转义文本(crate英文名)
    ));

    输出串.push_str("[\"标识符\"]\n");
    // 预构建英文名→签名索引，避免 O(n²) 线性查找
    let 签名索引: HashMap<&str, &str> = 条目表
        .iter()
        .map(|项| (项.英文原名.as_str(), 项.类型签名.as_str()))
        .collect();
    for (中文名, 英文名) in 中文名表 {
        let 签名 = 签名索引.get(英文名.as_str()).copied().unwrap_or("");
        输出串.push_str(&format!(
            "\"{}\" = \"{}\"  # {}\n",
            转义文本(中文名),
            转义文本(英文名),
            签名
        ));
    }
    输出串.push('\n');

    输出串.push_str("[\"解释\"]\n");
    for (中文名, _) in 中文名表 {
        let 解说 = 解说表.get(中文名).map(String::as_str).unwrap_or("");
        输出串.push_str(&format!(
            "\"{}\" = \"{}\"\n",
            转义文本(中文名),
            转义文本(解说)
        ));
    }
    输出串
}

#[cfg(test)]
mod 单元测试 {
    use super::super::rules::规则生成中文名;
    use super::super::rustdoc_extract::{提取公开接口, 样例文本};
    use super::*;

    #[test]
    fn 测试构建映射文本与免责声明() {
        let 条目表 = 提取公开接口(&样例文本()).unwrap();
        let 中文名表: Vec<(String, String)> = 条目表
            .iter()
            .map(|项| (规则生成中文名(&项.英文原名), 项.英文原名.clone()))
            .collect();
        let mut 解说表 = HashMap::new();
        解说表.insert("新建".to_string(), "创建新的值".to_string());
        let 生成文本 = 构建映射文本(
            "zh",
            "示例",
            "示例库",
            Some("1.2.3"),
            &条目表,
            &中文名表,
            &解说表,
        );
        // 免责声明必须存在
        assert!(生成文本.contains(&免责声明文本("zh")), "缺少免责声明");
        assert!(生成文本.contains("# crate: 示例"));
        // 生成基准版本写入文件头（--target-version 锁定依据）
        assert!(生成文本.contains("# 基准版本: 1.2.3"));
        // 可被标准 TOML 解析
        let 解析值: toml::Value = toml::from_str(&生成文本).expect("TOML 应可解析");
        assert!(解析值.get("模块路径").is_some());
        assert!(解析值.get("标识符").is_some());
        assert!(解析值.get("解释").is_some());
        assert_eq!(解析值["模块路径"]["示例库"].as_str(), Some("示例"));
        assert_eq!(解析值["标识符"]["新建"].as_str(), Some("new"));
        assert_eq!(解析值["标识符"]["状态"].as_str(), Some("State"));
        assert_eq!(解析值["解释"]["新建"].as_str(), Some("创建新的值"));
    }

    #[test]
    fn 测试映射文本可被管理器加载() {
        let 条目表 = 提取公开接口(&样例文本()).unwrap();
        let 中文名表: Vec<(String, String)> = 条目表
            .iter()
            .map(|项| (规则生成中文名(&项.英文原名), 项.英文原名.clone()))
            .collect();
        let 生成文本 = 构建映射文本(
            "zh",
            "示例",
            "示例库",
            None,
            &条目表,
            &中文名表,
            &HashMap::new(),
        );
        let 临时 = tempfile::tempdir().unwrap();
        // 模拟语言包目录结构：keywords.toml + crates/示例.toml（映射管理器要求 keywords.toml 存在）
        std::fs::write(
            临时.path().join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        let 目录 = 临时.path().join("crates");
        std::fs::create_dir_all(&目录).unwrap();
        std::fs::write(目录.join("示例.toml"), 生成文本).unwrap();
        let 管理器 = i18n_rust_engine::映射管理::映射管理器::自目录加载(临时.path())
            .expect("映射管理器应能加载");
        // ["解释"] 节被忽略，["标识符"]/["模块路径"] 正常生效
        assert_eq!(
            管理器.模块路径映射表.get("示例库").map(String::as_str),
            Some("示例")
        );
        assert_eq!(
            管理器.别名映射表.get("新建").map(String::as_str),
            Some("new")
        );
        assert_eq!(
            管理器.别名映射表.get("状态").map(String::as_str),
            Some("State")
        );
    }
}
