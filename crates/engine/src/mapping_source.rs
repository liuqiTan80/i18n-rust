// 映射源模块 - 从 TOML 文件加载映射表
//
// 提供映射数据的加载和管理，支持按类别组织：
// - 关键字映射（词法处理阶段）
// - 标准库映射（语义处理阶段）
// - 第三方库映射（按需加载，来自 crates/ 子目录）

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value;

/// 映射表分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MappingCategory {
    /// 关键字（词法处理阶段）
    Keywords,
    /// 标准库（语义处理阶段）
    StdLib,
    /// 第三方库（按需加载）
    ThirdParty,
}

impl MappingCategory {
    /// 获取分类对应的默认文件名
    pub fn default_filename(&self) -> &'static str {
        match self {
            MappingCategory::Keywords => "keywords.toml",
            MappingCategory::StdLib => "stdlib.toml",
            MappingCategory::ThirdParty => "crates.toml",
        }
    }

    /// 获取分类的显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            MappingCategory::Keywords => "关键字",
            MappingCategory::StdLib => "标准库",
            MappingCategory::ThirdParty => "第三方库",
        }
    }
}

/// 映射表加载器 - 从 TOML 文件加载映射数据
///
/// 从语言包目录中按分类加载映射表，支持关键字/标准库单文件加载
/// 以及第三方库目录（crates/）下多文件合并加载。
#[derive(Debug, Clone)]
pub struct MappingLoader {
    /// 语言包根目录
    root_dir: PathBuf,
    /// 已加载的映射表（按分类和子分类组织）
    mappings: HashMap<MappingCategory, HashMap<String, HashMap<String, String>>>,
}

impl MappingLoader {
    /// 创建新的加载器
    pub fn new<P: AsRef<Path>>(lang_pack_path: P) -> Self {
        let root_dir = lang_pack_path.as_ref().to_path_buf();
        Self {
            root_dir,
            mappings: HashMap::new(),
        }
    }

    /// 加载指定分类的映射表
    pub fn load(&mut self, category: MappingCategory) -> Result<(), String> {
        // 第三方库是目录，包含多个文件
        if category == MappingCategory::ThirdParty {
            return self.load_third_party_dir();
        }

        let file_path = self.root_dir.join(category.default_filename());

        if !file_path.exists() {
            return Err(format!("映射表文件不存在: {:?}", file_path));
        }

        let content =
            fs::read_to_string(&file_path).map_err(|e| format!("读取映射表失败: {}", e))?;

        let value: Value = content.parse().map_err(|e| format!("解析映射表失败: {}", e))?;

        let category_map = self.parse_toml_value(value)?;
        self.mappings.insert(category, category_map);
        Ok(())
    }

    /// 解析 TOML 值为映射表
    fn parse_toml_value(
        &self,
        value: Value,
    ) -> Result<HashMap<String, HashMap<String, String>>, String> {
        let mut category_map = HashMap::new();

        if let Value::Table(table) = value {
            for (sub_name, sub_value) in table {
                if let Value::Table(sub_table) = sub_value {
                    let mut sub_map = HashMap::new();
                    for (zh, en_value) in sub_table {
                        if let Value::String(en) = en_value {
                            sub_map.insert(zh, en.clone());
                        }
                    }
                    if !sub_map.is_empty() {
                        category_map.insert(sub_name, sub_map);
                    }
                }
            }
        }

        Ok(category_map)
    }

    /// 加载第三方库目录（crates/）下的所有 TOML 文件
    fn load_third_party_dir(&mut self) -> Result<(), String> {
        let dir_path = self.root_dir.join("crates");

        if !dir_path.exists() {
            // 目录不存在，静默返回
            return Ok(());
        }

        let mut merged_map = HashMap::new();

        // 遍历目录中的所有 .toml 文件
        let entries =
            fs::read_dir(&dir_path).map_err(|e| format!("读取第三方库目录失败: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                // 获取文件名（不含扩展名）作为分类标识
                let file_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let content = fs::read_to_string(&path)
                    .map_err(|e| format!("读取映射表 {:?} 失败: {}", path, e))?;

                let value: Value = content
                    .parse()
                    .map_err(|e| format!("解析映射表 {:?} 失败: {}", path, e))?;

                // 将文件中的映射合并，加上文件分类前缀
                if let Value::Table(table) = value {
                    for (sub_name, sub_value) in table {
                        if let Value::Table(sub_table) = sub_value {
                            let mut sub_map = HashMap::new();
                            for (zh, en_value) in sub_table {
                                if let Value::String(en) = en_value {
                                    sub_map.insert(zh, en.clone());
                                }
                            }
                            if !sub_map.is_empty() {
                                // 使用 "文件名/子分类" 作为键
                                let category_key = format!("{}/{}", file_name, sub_name);
                                merged_map.insert(category_key, sub_map);
                            }
                        }
                    }
                }
            }
        }

        self.mappings
            .insert(MappingCategory::ThirdParty, merged_map);
        Ok(())
    }

    /// 加载所有默认映射表
    pub fn load_all(&mut self) -> Result<(), String> {
        self.load(MappingCategory::Keywords)?;
        self.load(MappingCategory::StdLib)?;
        // 第三方库可选加载
        if self.root_dir.join("crates").is_dir() {
            let _ = self.load(MappingCategory::ThirdParty);
        }
        Ok(())
    }

    /// 获取指定分类的完整映射表（扁平化合并所有子分类）
    pub fn get_mapping(&self, category: MappingCategory) -> HashMap<String, String> {
        let mut result = HashMap::new();

        if let Some(category_map) = self.mappings.get(&category) {
            for sub_map in category_map.values() {
                for (zh, en) in sub_map {
                    result.insert(zh.clone(), en.clone());
                }
            }
        }

        result
    }

    /// 获取指定分类和子分类的映射表
    pub fn get_sub_mapping(
        &self,
        category: MappingCategory,
        sub_category: &str,
    ) -> Option<&HashMap<String, String>> {
        self.mappings
            .get(&category)
            .and_then(|m| m.get(sub_category))
    }

    /// 查询单个映射条目
    pub fn query(&self, category: MappingCategory, zh: &str) -> Option<String> {
        if let Some(category_map) = self.mappings.get(&category) {
            for sub_map in category_map.values() {
                if let Some(en) = sub_map.get(zh) {
                    return Some(en.clone());
                }
            }
        }
        None
    }

    /// 反向查询（从英文查中文）
    pub fn reverse_query(&self, category: MappingCategory, en: &str) -> Option<String> {
        if let Some(category_map) = self.mappings.get(&category) {
            for sub_map in category_map.values() {
                for (zh, e) in sub_map {
                    if e == en {
                        return Some(zh.clone());
                    }
                }
            }
        }
        None
    }

    /// 获取所有子分类名称
    pub fn get_sub_categories(&self, category: MappingCategory) -> Vec<String> {
        self.mappings
            .get(&category)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// 统计映射条目数（扁平化后）
    pub fn entry_count(&self, category: MappingCategory) -> usize {
        self.get_mapping(category).len()
    }
}

/// 便捷函数：加载关键字映射
pub fn load_keyword_mapping<P: AsRef<Path>>(
    lang_pack_path: P,
) -> Result<HashMap<String, String>, String> {
    let mut loader = MappingLoader::new(lang_pack_path);
    loader.load(MappingCategory::Keywords)?;
    Ok(loader.get_mapping(MappingCategory::Keywords))
}

/// 便捷函数：加载标准库映射
pub fn load_stdlib_mapping<P: AsRef<Path>>(
    lang_pack_path: P,
) -> Result<HashMap<String, String>, String> {
    let mut loader = MappingLoader::new(lang_pack_path);
    loader.load(MappingCategory::StdLib)?;
    Ok(loader.get_mapping(MappingCategory::StdLib))
}

/// 便捷函数：加载所有映射
pub fn load_all_mappings<P: AsRef<Path>>(
    lang_pack_path: P,
) -> Result<HashMap<MappingCategory, HashMap<String, String>>, String> {
    let mut loader = MappingLoader::new(lang_pack_path);
    loader.load_all()?;

    let mut result = HashMap::new();
    result.insert(
        MappingCategory::Keywords,
        loader.get_mapping(MappingCategory::Keywords),
    );
    result.insert(
        MappingCategory::StdLib,
        loader.get_mapping(MappingCategory::StdLib),
    );
    result.insert(
        MappingCategory::ThirdParty,
        loader.get_mapping(MappingCategory::ThirdParty),
    );

    Ok(result)
}

/// 创建默认的关键字映射（内置备用）
///
/// 当语言包文件不可用时，使用此内置映射作为回退，
/// 覆盖 Rust 基础语法关键字、类型、错误处理、异步等常用构造。
pub fn create_builtin_keyword_mapping() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // 声明关键字
    map.insert("函数".into(), "fn".into());
    map.insert("变量".into(), "let".into());
    map.insert("可变".into(), "mut".into());
    map.insert("常量".into(), "const".into());
    map.insert("结构体".into(), "struct".into());
    map.insert("枚举".into(), "enum".into());
    map.insert("实现".into(), "impl".into());
    map.insert("特征".into(), "trait".into());
    map.insert("类型".into(), "type".into());
    map.insert("模块".into(), "mod".into());
    map.insert("公开".into(), "pub".into());
    map.insert("使用".into(), "use".into());
    map.insert("作为".into(), "as".into());
    map.insert("包".into(), "crate".into());
    map.insert("超级".into(), "super".into());
    map.insert("外部".into(), "extern".into());
    map.insert("静态".into(), "static".into());

    // 控制流
    map.insert("如果".into(), "if".into());
    map.insert("否则".into(), "else".into());
    map.insert("匹配".into(), "match".into());
    map.insert("循环".into(), "loop".into());
    map.insert("当".into(), "while".into());
    map.insert("对于".into(), "for".into());
    map.insert("在".into(), "in".into());
    map.insert("中断".into(), "break".into());
    map.insert("继续".into(), "continue".into());
    map.insert("返回".into(), "return".into());

    // 基本类型
    map.insert("整数".into(), "i32".into());
    map.insert("长整数".into(), "i64".into());
    map.insert("浮点数".into(), "f64".into());
    map.insert("单精度浮点数".into(), "f32".into());
    map.insert("文本".into(), "str".into());
    map.insert("布尔".into(), "bool".into());
    map.insert("字符".into(), "char".into());
    map.insert("字节".into(), "u8".into());

    // 特殊值
    map.insert("真".into(), "true".into());
    map.insert("假".into(), "false".into());
    map.insert("空".into(), "()".into());
    map.insert("自我".into(), "self".into());
    map.insert("自身".into(), "Self".into());

    // 错误处理
    map.insert("结果".into(), "Result".into());
    map.insert("选项".into(), "Option".into());
    map.insert("有些".into(), "Some".into());
    map.insert("无".into(), "None".into());
    map.insert("成功".into(), "Ok".into());
    map.insert("错误".into(), "Err".into());

    // 内存
    map.insert("引用".into(), "&".into());
    map.insert("解引用".into(), "*".into());
    map.insert("移动".into(), "move".into());
    map.insert("盒子".into(), "Box".into());

    // 异步
    map.insert("异步".into(), "async".into());
    map.insert("等待".into(), "await".into());
    map.insert("不安全".into(), "unsafe".into());
    map.insert("动态".into(), "dyn".into());

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_keyword_mapping() {
        // 创建临时测试目录
        let temp_dir = std::env::temp_dir().join("i18n_mapping_test");
        fs::create_dir_all(&temp_dir).unwrap();

        // 写入测试映射表
        let test_content = r#"
["声明"]
"函数" = "fn"
"变量" = "let"

["控制流"]
"如果" = "if"
"否则" = "else"
"#;
        fs::write(temp_dir.join("keywords.toml"), test_content).unwrap();

        // 测试加载
        let mut loader = MappingLoader::new(&temp_dir);
        assert!(loader.load(MappingCategory::Keywords).is_ok());

        // 测试查询
        assert_eq!(
            loader.query(MappingCategory::Keywords, "函数"),
            Some("fn".to_string())
        );
        assert_eq!(
            loader.query(MappingCategory::Keywords, "如果"),
            Some("if".to_string())
        );
        assert_eq!(loader.query(MappingCategory::Keywords, "不存在"), None);

        // 测试反向查询
        assert_eq!(
            loader.reverse_query(MappingCategory::Keywords, "fn"),
            Some("函数".to_string())
        );

        // 测试子分类
        let sub_categories = loader.get_sub_categories(MappingCategory::Keywords);
        assert!(sub_categories.contains(&"声明".to_string()));
        assert!(sub_categories.contains(&"控制流".to_string()));

        // 测试条目数
        assert_eq!(loader.entry_count(MappingCategory::Keywords), 4);

        // 清理
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_get_flattened_mapping() {
        let temp_dir = std::env::temp_dir().join("i18n_mapping_test2");
        fs::create_dir_all(&temp_dir).unwrap();

        let test_content = r#"
["分类A"]
"甲" = "alpha"
"乙" = "beta"

["分类B"]
"丙" = "gamma"
"#;
        fs::write(temp_dir.join("stdlib.toml"), test_content).unwrap();

        let mut loader = MappingLoader::new(&temp_dir);
        loader.load(MappingCategory::StdLib).unwrap();

        let mapping = loader.get_mapping(MappingCategory::StdLib);
        assert_eq!(mapping.len(), 3);
        assert_eq!(mapping.get("甲"), Some(&"alpha".to_string()));
        assert_eq!(mapping.get("丙"), Some(&"gamma".to_string()));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_builtin_keyword_mapping() {
        let map = create_builtin_keyword_mapping();

        assert_eq!(map.get("函数"), Some(&"fn".to_string()));
        assert_eq!(map.get("如果"), Some(&"if".to_string()));
        assert_eq!(map.get("整数"), Some(&"i32".to_string()));
        assert!(map.len() > 30);
    }
}
