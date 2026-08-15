// 映射管理器 - 统一管理所有映射表（关键字、模块路径、标识符别名）
//
// 提供完整的映射加载、查询接口，是翻译管线的核心数据结构。
// 支持从文件系统目录加载或从内置字符串数据加载（无需文件系统）。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// 解析"模块路径 + 标识符"两节格式的 TOML（stdlib.toml 与 crates/*.toml 通用）
///
/// - `["模块路径"]` 节 → 合并到模块路径映射（如 `"线程" = "std::thread"`）
/// - `["标识符"]` 节 → 合并到标识符别名映射（如 `"字符串" = "String"`）
fn merge_module_and_ident_sections(
    content: &str,
    module_path_map: &mut HashMap<String, String>,
    alias_map: &mut HashMap<String, String>,
) {
    if let Ok(root) = toml::from_str::<toml::Value>(content)
        && let toml::Value::Table(table) = root
    {
        if let Some(module_section) = table.get("模块路径")
            && let toml::Value::Table(entry_table) = module_section
        {
            for (zh, en_value) in entry_table {
                if let toml::Value::String(en) = en_value {
                    module_path_map.insert(zh.clone(), en.clone());
                }
            }
        }
        if let Some(ident_section) = table.get("标识符")
            && let toml::Value::Table(entry_table) = ident_section
        {
            for (zh, en_value) in entry_table {
                if let toml::Value::String(en) = en_value {
                    alias_map.insert(zh.clone(), en.clone());
                }
            }
        }
    }
}

/// 映射管理器：统一管理关键字映射、模块路径映射、标识符别名映射
///
/// 翻译管线中各阶段（词法转译、模块路径替换、别名替换）均从此处获取映射表。
/// 支持两种加载方式：
/// - `load_from_dir`: 从语言包目录读取 TOML 文件
/// - `load_from_builtin`: 从编译时嵌入的字符串数据加载（无需文件系统）
#[derive(Debug, Clone)]
pub struct MappingManager {
    /// 关键字映射（词法转译阶段使用）
    pub keyword_map: HashMap<String, String>,
    /// 按节（section）组织的关键字映射（用于查询宏名称等）
    section_map: HashMap<String, HashMap<String, String>>,
    /// 模块路径映射（如 `标准库` → `std`）
    pub module_path_map: HashMap<String, String>,
    /// 标识符别名映射（标准库/第三方库的类型与函数名翻译）
    pub alias_map: HashMap<String, String>,
}

impl MappingManager {
    /// 从语言包目录加载全部映射（推荐用于开发模式）
    ///
    /// 加载顺序：
    /// 1. `keywords.toml` → 关键字映射
    /// 2. `module_paths.toml` → 模块路径映射
    /// 3. `stdlib.toml` → 标准库的模块路径 + 标识符别名（可选）
    /// 4. `crates/*.toml` → 第三方库的模块路径 + 标识符别名
    pub fn load_from_dir(lang_dir: &Path) -> Result<Self, String> {
        let keywords_path = lang_dir.join("keywords.toml");
        if !keywords_path.exists() {
            return Err(crate::语言::f(
                "load_keywords_missing",
                &[&keywords_path.display().to_string()],
            ));
        }

        // 1. 加载关键字映射
        let keywords_content = fs::read_to_string(&keywords_path)
            .map_err(|e| crate::语言::f("load_read_keywords_failed", &[&e.to_string()]))?;
        let root: toml::Value = toml::from_str(&keywords_content)
            .map_err(|e| crate::语言::f("load_parse_keywords_failed", &[&e.to_string()]))?;

        let mut keyword_map = HashMap::new();
        let mut section_map = HashMap::new();
        if let toml::Value::Table(table) = root {
            for (section_name, section_content) in table {
                if let toml::Value::Table(entry_table) = section_content {
                    let mut section_mapping = HashMap::new();
                    for (zh, en_value) in entry_table {
                        if let toml::Value::String(en) = en_value {
                            section_mapping.insert(zh.clone(), en.clone());
                            keyword_map.insert(zh.clone(), en.clone());
                        }
                    }
                    section_map.insert(section_name.clone(), section_mapping);
                }
            }
        }

        // 2. 加载模块路径映射（来自 module_paths.toml）
        let module_paths_file = lang_dir.join("module_paths.toml");
        let mut module_path_map = HashMap::new();
        if module_paths_file.exists() {
            let content = fs::read_to_string(&module_paths_file)
                .map_err(|e| crate::语言::f("load_read_module_paths_failed", &[&e.to_string()]))?;
            let root: toml::Value = toml::from_str(&content).map_err(|e| {
                crate::语言::f("load_parse_module_paths_failed", &[&e.to_string()])
            })?;
            if let toml::Value::Table(table) = root
                && let Some(path_section) = table.get("模块路径")
                && let toml::Value::Table(entry_table) = path_section
            {
                for (zh, en_value) in entry_table {
                    if let toml::Value::String(en) = en_value {
                        module_path_map.insert(zh.clone(), en.clone());
                    }
                }
            }
        }

        // 3. 加载标准库映射（stdlib.toml，可选）
        //    与 crates/*.toml 相同格式：["模块路径"] + ["标识符"] 两节
        let stdlib_file = lang_dir.join("stdlib.toml");
        let mut alias_map = HashMap::new();
        if stdlib_file.exists()
            && let Ok(content) = fs::read_to_string(&stdlib_file)
        {
            merge_module_and_ident_sections(&content, &mut module_path_map, &mut alias_map);
        }

        // 4. 扫描第三方库目录（crates/）
        let crates_dir = lang_dir.join("crates");
        if crates_dir.exists()
            && crates_dir.is_dir()
            && let Ok(entries) = fs::read_dir(&crates_dir)
        {
            for file in entries.flatten() {
                let file_path = file.path();
                if file_path.extension().and_then(|e| e.to_str()) == Some("toml")
                    && let Ok(content) = fs::read_to_string(&file_path)
                {
                    merge_module_and_ident_sections(&content, &mut module_path_map, &mut alias_map);
                }
            }
        }

        Ok(Self {
            keyword_map,
            section_map,
            module_path_map,
            alias_map,
        })
    }

    /// 从内置数据加载全部映射（无需文件系统，用于编译时嵌入）
    ///
    /// 参数：
    /// - `keywords_toml`: keywords.toml 的完整内容
    /// - `module_paths_toml`: module_paths.toml 的完整内容
    /// - `stdlib_toml`: stdlib.toml 的完整内容（标准库的模块路径 + 标识符别名）
    /// - `third_party_data`: 各第三方库 .toml 文件的 (文件名, 内容) 列表
    pub fn load_from_builtin(
        keywords_toml: &str,
        module_paths_toml: &str,
        stdlib_toml: &str,
        third_party_data: &[(&str, &str)],
    ) -> Result<Self, String> {
        // 1. 解析关键字映射
        let root: toml::Value = toml::from_str(keywords_toml).map_err(|e| {
            crate::语言::f("load_parse_builtin_keywords_failed", &[&e.to_string()])
        })?;

        let mut keyword_map = HashMap::new();
        let mut section_map = HashMap::new();
        if let toml::Value::Table(table) = root {
            for (section_name, section_content) in table {
                if let toml::Value::Table(entry_table) = section_content {
                    let mut section_mapping = HashMap::new();
                    for (zh, en_value) in entry_table {
                        if let toml::Value::String(en) = en_value {
                            section_mapping.insert(zh.clone(), en.clone());
                            keyword_map.insert(zh.clone(), en.clone());
                        }
                    }
                    section_map.insert(section_name, section_mapping);
                }
            }
        }

        // 2. 解析模块路径映射
        let mut module_path_map = HashMap::new();
        let root: toml::Value = toml::from_str(module_paths_toml).map_err(|e| {
            crate::语言::f("load_parse_builtin_paths_failed", &[&e.to_string()])
        })?;
        if let toml::Value::Table(table) = root
            && let Some(path_section) = table.get("模块路径")
            && let toml::Value::Table(entry_table) = path_section
        {
            for (zh, en_value) in entry_table {
                if let toml::Value::String(en) = en_value {
                    module_path_map.insert(zh.clone(), en.clone());
                }
            }
        }

        // 3. 解析标准库映射（模块路径 + 标识符别名）
        let mut alias_map = HashMap::new();
        let stdlib_root: toml::Value = toml::from_str(stdlib_toml).map_err(|e| {
            crate::语言::f("load_parse_builtin_stdlib_failed", &[&e.to_string()])
        })?;
        if let toml::Value::Table(table) = stdlib_root {
            if let Some(path_section) = table.get("模块路径")
                && let toml::Value::Table(entry_table) = path_section
            {
                for (zh, en_value) in entry_table {
                    if let toml::Value::String(en) = en_value {
                        module_path_map.insert(zh.clone(), en.clone());
                    }
                }
            }
            if let Some(ident_section) = table.get("标识符")
                && let toml::Value::Table(entry_table) = ident_section
            {
                for (zh, en_value) in entry_table {
                    if let toml::Value::String(en) = en_value {
                        alias_map.insert(zh.clone(), en.clone());
                    }
                }
            }
        }

        // 4. 解析第三方库映射
        for (_file_name, content) in third_party_data {
            merge_module_and_ident_sections(content, &mut module_path_map, &mut alias_map);
        }

        Ok(Self {
            keyword_map,
            section_map,
            module_path_map,
            alias_map,
        })
    }

    /// 向后兼容：从单个关键字文件加载（委托给 load_from_dir）
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let dir = path.parent().unwrap_or(Path::new("."));
        Self::load_from_dir(dir)
    }

    /// 查询关键字映射
    pub fn query(&self, zh_keyword: &str) -> Option<&String> {
        self.keyword_map.get(zh_keyword)
    }

    /// 获取完整关键字映射表
    pub fn get_keyword_map(&self) -> &HashMap<String, String> {
        &self.keyword_map
    }

    /// 获取指定节的映射表
    pub fn get_section_mapping(&self, section_name: &str) -> Option<&HashMap<String, String>> {
        self.section_map.get(section_name)
    }

    /// 获取模块路径映射表
    pub fn get_module_path_map(&self) -> &HashMap<String, String> {
        &self.module_path_map
    }

    /// 获取标识符别名映射表
    pub fn get_alias_map(&self) -> &HashMap<String, String> {
        &self.alias_map
    }

    /// 获取所有在 `["宏"]` 节中定义的中文宏名集合（不含感叹号）
    ///
    /// 用于词法转译阶段的宏感叹号自动补充：
    /// 当标识符是宏名称且后面跟着 `(`/`[`/`{` 时，自动插入 `!`。
    pub fn get_macro_names(&self) -> HashSet<String> {
        let mut macro_set = HashSet::new();
        if let Some(macro_section) = self.section_map.get("宏") {
            for zh in macro_section.keys() {
                macro_set.insert(zh.clone());
            }
        }
        macro_set
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 在临时目录下构造一个完整的语言包目录并返回其路径
    fn make_lang_pack(root: &std::path::Path, name: &str) -> std::path::PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(dir.join("crates")).unwrap();
        fs::write(
            dir.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n[\"宏\"]\n\"打印行\" = \"println\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("module_paths.toml"),
            "[\"模块路径\"]\n\"标准库\" = \"std\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("stdlib.toml"),
            "[\"模块路径\"]\n\"文件系统\" = \"std::fs\"\n[\"标识符\"]\n\"字符串\" = \"String\"\n\"新\" = \"new\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("crates").join("web.toml"),
            "[\"模块路径\"]\n\"网络库\" = \"reqwest\"\n[\"标识符\"]\n\"客户端\" = \"Client\"\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn test_load_from_dir_full_pack() {
        let temp = tempfile::tempdir().unwrap();
        let dir = make_lang_pack(temp.path(), "zh");

        let manager = MappingManager::load_from_dir(&dir).expect("语言包应能完整加载");

        // 关键字映射（keywords.toml 全部节合并）
        assert_eq!(manager.query("函数"), Some(&"fn".to_string()));
        assert_eq!(manager.query("让"), Some(&"let".to_string()));
        // 模块路径映射（module_paths.toml + stdlib.toml 合并）
        assert_eq!(
            manager.module_path_map.get("标准库"),
            Some(&"std".to_string())
        );
        assert_eq!(
            manager.module_path_map.get("文件系统"),
            Some(&"std::fs".to_string())
        );
        // 标识符别名映射（stdlib.toml + crates/*.toml 合并）
        assert_eq!(manager.alias_map.get("字符串"), Some(&"String".to_string()));
        assert_eq!(manager.alias_map.get("新"), Some(&"new".to_string()));
        assert_eq!(manager.alias_map.get("客户端"), Some(&"Client".to_string()));
        // 宏集合（仅宏节）
        assert!(manager.get_macro_names().contains("打印行"));
        assert!(!manager.get_macro_names().contains("函数"));
    }

    #[test]
    fn test_load_from_dir_missing_keywords() {
        let temp = tempfile::tempdir().unwrap();
        let err = MappingManager::load_from_dir(temp.path()).unwrap_err();
        assert!(
            err.contains("关键字文件不存在"),
            "错误信息应指明缺失文件: {}",
            err
        );
    }

    #[test]
    fn test_load_from_dir_invalid_keywords() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("keywords.toml"), "这不是合法 TOML [[[").unwrap();
        let err = MappingManager::load_from_dir(temp.path()).unwrap_err();
        assert!(
            err.contains("解析关键字 TOML 失败"),
            "错误信息应含解析失败: {}",
            err
        );
    }

    #[test]
    fn test_load_from_dir_missing_optional_files() {
        // 仅有 keywords.toml 时，可选文件缺失不应报错
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        let manager = MappingManager::load_from_dir(temp.path()).expect("仅有关键字文件也应可加载");
        assert!(manager.module_path_map.is_empty());
        assert!(manager.alias_map.is_empty());
    }

    #[test]
    fn test_load_from_builtin_stdlib_merge() {
        let keywords = "[\"声明\"]\n\"函数\" = \"fn\"\n";
        let module_paths = "[\"模块路径\"]\n\"标准库\" = \"std\"\n";
        let stdlib =
            "[\"模块路径\"]\n\"文件系统\" = \"std::fs\"\n[\"标识符\"]\n\"字符串\" = \"String\"\n";
        let third_party = [("web.toml", "[\"标识符\"]\n\"客户端\" = \"Client\"\n")];

        let manager =
            MappingManager::load_from_builtin(keywords, module_paths, stdlib, &third_party)
                .expect("内置数据应能加载");

        assert_eq!(manager.query("函数"), Some(&"fn".to_string()));
        assert_eq!(
            manager.module_path_map.get("标准库"),
            Some(&"std".to_string())
        );
        assert_eq!(
            manager.module_path_map.get("文件系统"),
            Some(&"std::fs".to_string())
        );
        assert_eq!(manager.alias_map.get("字符串"), Some(&"String".to_string()));
        assert_eq!(manager.alias_map.get("客户端"), Some(&"Client".to_string()));
    }

    #[test]
    fn test_load_from_builtin_invalid_stdlib() {
        let keywords = "[\"声明\"]\n";
        let module_paths = "[\"模块路径\"]\n";
        let err =
            MappingManager::load_from_builtin(keywords, module_paths, "非法内容", &[]).unwrap_err();
        assert!(
            err.contains("解析内置标准库 TOML 失败"),
            "错误信息应含解析失败: {}",
            err
        );
    }

    #[test]
    fn test_load_from_file_delegates_to_dir() {
        let temp = tempfile::tempdir().unwrap();
        let dir = make_lang_pack(temp.path(), "zh");
        let manager =
            MappingManager::load_from_file(&dir.join("keywords.toml")).expect("委托加载应成功");
        assert_eq!(manager.query("函数"), Some(&"fn".to_string()));
    }

    #[test]
    fn test_merge_sections_ignores_invalid_content() {
        let mut module_path_map = HashMap::new();
        let mut alias_map = HashMap::new();
        // 非法 TOML 不应 panic，也不应产生任何映射
        merge_module_and_ident_sections("[[[", &mut module_path_map, &mut alias_map);
        assert!(module_path_map.is_empty());
        assert!(alias_map.is_empty());
        // 合法内容正常合并
        merge_module_and_ident_sections(
            "[\"模块路径\"]\n\"文件系统\" = \"std::fs\"\n[\"标识符\"]\n\"字符串\" = \"String\"\n",
            &mut module_path_map,
            &mut alias_map,
        );
        assert_eq!(
            module_path_map.get("文件系统"),
            Some(&"std::fs".to_string())
        );
        assert_eq!(alias_map.get("字符串"), Some(&"String".to_string()));
    }

    #[test]
    fn test_get_section_mapping_and_query() {
        let temp = tempfile::tempdir().unwrap();
        let dir = make_lang_pack(temp.path(), "zh");
        let manager = MappingManager::load_from_dir(&dir).unwrap();

        let declarations = manager.get_section_mapping("声明").expect("声明节应存在");
        assert_eq!(declarations.get("函数"), Some(&"fn".to_string()));
        assert_eq!(manager.get_section_mapping("不存在的节"), None);
        assert_eq!(manager.query("不存在的关键字"), None);
    }
}
