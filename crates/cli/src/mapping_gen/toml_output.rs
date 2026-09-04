//! 映射 TOML 构建：与现有 crates/*.toml 格式一致，含免责声明与生成时间戳。

use std::collections::HashMap;

use super::{ApiEntry, disclaimer_text};

/// TOML 字符串转义（含 \r 与其余控制字符，避免生成非法 TOML）
fn escape_toml(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r");
    escaped
        .chars()
        .map(|c| match c {
            c if (c as u32) < 0x20 => format!("\\u{:04X}", c as u32),
            c => c.to_string(),
        })
        .collect()
}

/// 当前时间字符串（UTC，`YYYY-MM-DD HH:MM:SS`）
fn current_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (secs / 86_400) as i64;
    let day_secs = secs % 86_400;
    let (year, month, day) = decompose_date(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year,
        month,
        day,
        day_secs / 3_600,
        (day_secs % 3_600) / 60,
        day_secs % 60
    )
}

/// 天数 → (年, 月, 日)（civil_from_days 算法，与引擎 logger.rs 一致）
fn decompose_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_of_year = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_of_year + 2) / 5 + 1) as u32;
    let month = (month_of_year + if month_of_year < 10 { 3 } else { -9 }) as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

/// 构建语言包映射 TOML（与现有 crates/*.toml 格式一致：
/// `["模块路径"]` / `["标识符"]` 两节为映射管理器识别的 String 值格式，
/// `["解释"]` 节为扩展，映射管理器加载时自动忽略，保持兼容）
pub fn build_mapping_toml(
    lang: &str,
    crate_name: &str,
    crate_chinese_name: &str,
    entries: &[ApiEntry],
    chinese_name_table: &[(String, String)],
    explanation_table: &HashMap<String, String>,
) -> String {
    let ui = crate::ui::Ui::for_lang(lang);
    let mut output = String::new();
    output.push_str(&format!("{}\n", ui.t("mg_header_title")));
    output.push_str(&format!("{}\n", ui.f("mg_header_crate", &[crate_name])));
    output.push_str(&format!(
        "{}\n",
        ui.f("mg_header_generated_at", &[&current_timestamp()])
    ));
    output.push_str(&format!("# {}\n\n", disclaimer_text(lang)));

    output.push_str("[\"模块路径\"]\n");
    output.push_str(&format!(
        "\"{}\" = \"{}\"\n\n",
        escape_toml(crate_chinese_name),
        escape_toml(crate_name)
    ));

    output.push_str("[\"标识符\"]\n");
    // 预构建英文名→签名索引，避免 O(n²) 线性查找
    let sig_index: HashMap<&str, &str> = entries
        .iter()
        .map(|e| (e.english_name.as_str(), e.signature.as_str()))
        .collect();
    for (chinese_name, english_name) in chinese_name_table {
        let sig = sig_index.get(english_name.as_str()).copied().unwrap_or("");
        output.push_str(&format!(
            "\"{}\" = \"{}\"  # {}\n",
            escape_toml(chinese_name),
            escape_toml(english_name),
            sig
        ));
    }
    output.push('\n');

    output.push_str("[\"解释\"]\n");
    for (chinese_name, _) in chinese_name_table {
        let explanation = explanation_table
            .get(chinese_name)
            .map(String::as_str)
            .unwrap_or("");
        output.push_str(&format!(
            "\"{}\" = \"{}\"\n",
            escape_toml(chinese_name),
            escape_toml(explanation)
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::super::rules::rule_generate_chinese_name;
    use super::super::rustdoc_extract::{extract_public_api, sample_json};
    use super::*;

    #[test]
    fn test_toml_output_and_disclaimer() {
        let entries = extract_public_api(&sample_json()).unwrap();
        let chinese_name_table: Vec<(String, String)> = entries
            .iter()
            .map(|e| {
                (
                    rule_generate_chinese_name(&e.english_name),
                    e.english_name.clone(),
                )
            })
            .collect();
        let mut explanation_table = HashMap::new();
        explanation_table.insert("新建".to_string(), "创建新的值".to_string());
        let toml = build_mapping_toml(
            "zh",
            "示例",
            "示例库",
            &entries,
            &chinese_name_table,
            &explanation_table,
        );
        // 免责声明必须存在
        assert!(toml.contains(&disclaimer_text("zh")), "缺少免责声明");
        assert!(toml.contains("# crate: 示例"));
        // 可被标准 TOML 解析
        let value: toml::Value = toml::from_str(&toml).expect("TOML 应可解析");
        assert!(value.get("模块路径").is_some());
        assert!(value.get("标识符").is_some());
        assert!(value.get("解释").is_some());
        assert_eq!(value["模块路径"]["示例库"].as_str(), Some("示例"));
        assert_eq!(value["标识符"]["新建"].as_str(), Some("new"));
        assert_eq!(value["标识符"]["状态"].as_str(), Some("State"));
        assert_eq!(value["解释"]["新建"].as_str(), Some("创建新的值"));
    }

    #[test]
    fn test_toml_loadable_by_mapping_manager() {
        let entries = extract_public_api(&sample_json()).unwrap();
        let chinese_name_table: Vec<(String, String)> = entries
            .iter()
            .map(|e| {
                (
                    rule_generate_chinese_name(&e.english_name),
                    e.english_name.clone(),
                )
            })
            .collect();
        let toml = build_mapping_toml(
            "zh",
            "示例",
            "示例库",
            &entries,
            &chinese_name_table,
            &HashMap::new(),
        );
        let temp = tempfile::tempdir().unwrap();
        // 模拟语言包目录结构：keywords.toml + crates/示例.toml（映射管理器要求 keywords.toml 存在）
        std::fs::write(
            temp.path().join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        let crates_dir = temp.path().join("crates");
        std::fs::create_dir_all(&crates_dir).unwrap();
        std::fs::write(crates_dir.join("示例.toml"), toml).unwrap();
        let manager = i18n_rust_engine::mapping_manager::MappingManager::load_from_dir(temp.path())
            .expect("映射管理器应能加载");
        // ["解释"] 节被忽略，["标识符"]/["模块路径"] 正常生效
        assert_eq!(
            manager.module_path_map.get("示例库").map(String::as_str),
            Some("示例")
        );
        assert_eq!(
            manager.alias_map.get("新建").map(String::as_str),
            Some("new")
        );
        assert_eq!(
            manager.alias_map.get("状态").map(String::as_str),
            Some("State")
        );
    }
}
