// 映射管理器 - 统一管理所有映射表（关键字、模块路径、标识符别名）
//
// 提供完整的映射加载、查询接口，是翻译管线的核心数据结构。
// 支持从文件系统目录加载或从内置字符串数据加载（无需文件系统）。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

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
    /// 标识符别名映射（第三方库的类型/函数名翻译）
    pub alias_map: HashMap<String, String>,
}

impl MappingManager {
    /// 从语言包目录加载全部映射（推荐用于开发模式）
    ///
    /// 加载顺序：
    /// 1. `keywords.toml` → 关键字映射
    /// 2. `module_paths.toml` → 模块路径映射
    /// 3. `crates/*.toml` → 第三方库的模块路径 + 标识符别名
    pub fn load_from_dir(lang_dir: &Path) -> Result<Self, String> {
        let keywords_path = lang_dir.join("keywords.toml");
        if !keywords_path.exists() {
            return Err(format!("关键字文件不存在: {}", keywords_path.display()));
        }

        // 1. 加载关键字映射
        let keywords_content = fs::read_to_string(&keywords_path)
            .map_err(|e| format!("无法读取关键字文件: {}", e))?;
        let root: toml::Value =
            toml::from_str(&keywords_content).map_err(|e| format!("解析关键字 TOML 失败: {}", e))?;

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
                .map_err(|e| format!("无法读取模块路径文件: {}", e))?;
            let root: toml::Value = toml::from_str(&content)
                .map_err(|e| format!("解析模块路径 TOML 失败: {}", e))?;
            if let toml::Value::Table(table) = root {
                if let Some(path_section) = table.get("模块路径") {
                    if let toml::Value::Table(entry_table) = path_section {
                        for (zh, en_value) in entry_table {
                            if let toml::Value::String(en) = en_value {
                                module_path_map.insert(zh.clone(), en.clone());
                            }
                        }
                    }
                }
            }
        }

        // 3. 扫描第三方库目录（crates/）
        let crates_dir = lang_dir.join("crates");
        let mut alias_map = HashMap::new();
        if crates_dir.exists() && crates_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&crates_dir) {
                for file_result in entries {
                    if let Ok(file) = file_result {
                        let file_path = file.path();
                        if file_path.extension().and_then(|e| e.to_str()) == Some("toml") {
                            if let Ok(content) = fs::read_to_string(&file_path) {
                                if let Ok(root) = toml::from_str::<toml::Value>(&content) {
                                    if let toml::Value::Table(table) = root {
                                        // 提取 ["模块路径"] → 合并到模块路径映射
                                        if let Some(module_section) = table.get("模块路径") {
                                            if let toml::Value::Table(entry_table) = module_section
                                            {
                                                for (zh, en_value) in entry_table {
                                                    if let toml::Value::String(en) = en_value {
                                                        module_path_map
                                                            .insert(zh.clone(), en.clone());
                                                    }
                                                }
                                            }
                                        }
                                        // 提取 ["标识符"] → 标识符别名映射
                                        if let Some(ident_section) = table.get("标识符") {
                                            if let toml::Value::Table(entry_table) = ident_section {
                                                for (zh, en_value) in entry_table {
                                                    if let toml::Value::String(en) = en_value {
                                                        alias_map.insert(zh.clone(), en.clone());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
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
    /// - `third_party_data`: 各第三方库 .toml 文件的 (文件名, 内容) 列表
    pub fn load_from_builtin(
        keywords_toml: &str,
        module_paths_toml: &str,
        third_party_data: &[(&str, &str)],
    ) -> Result<Self, String> {
        // 1. 解析关键字映射
        let root: toml::Value =
            toml::from_str(keywords_toml).map_err(|e| format!("解析内置关键字 TOML 失败: {}", e))?;

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
        let root: toml::Value = toml::from_str(module_paths_toml)
            .map_err(|e| format!("解析内置模块路径 TOML 失败: {}", e))?;
        if let toml::Value::Table(table) = root {
            if let Some(path_section) = table.get("模块路径") {
                if let toml::Value::Table(entry_table) = path_section {
                    for (zh, en_value) in entry_table {
                        if let toml::Value::String(en) = en_value {
                            module_path_map.insert(zh.clone(), en.clone());
                        }
                    }
                }
            }
        }

        // 3. 解析第三方库映射
        let mut alias_map = HashMap::new();
        for (_file_name, content) in third_party_data {
            if let Ok(root) = toml::from_str::<toml::Value>(content) {
                if let toml::Value::Table(table) = root {
                    if let Some(module_section) = table.get("模块路径") {
                        if let toml::Value::Table(entry_table) = module_section {
                            for (zh, en_value) in entry_table {
                                if let toml::Value::String(en) = en_value {
                                    module_path_map.insert(zh.clone(), en.clone());
                                }
                            }
                        }
                    }
                    if let Some(ident_section) = table.get("标识符") {
                        if let toml::Value::Table(entry_table) = ident_section {
                            for (zh, en_value) in entry_table {
                                if let toml::Value::String(en) = en_value {
                                    alias_map.insert(zh.clone(), en.clone());
                                }
                            }
                        }
                    }
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
