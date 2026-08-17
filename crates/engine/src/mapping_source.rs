// 映射源模块 - 从 TOML 文件加载映射表
//
// 提供映射数据的加载和管理，支持按类别组织：
// - 关键字映射（词法处理阶段）
// - 标准库映射（语义处理阶段）
// - 第三方库映射（按需加载，来自 crates/ 子目录）

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{LoadError, LoadTarget};

/// 节（section）组织的映射表：节名 → (母语词 → 英文)
pub type SectionMap = HashMap<String, HashMap<String, String>>;

/// 解析 TOML 内容为“节 → (母语词 → 英文)”二级表（两个加载器共用的单一解析实现）
///
/// 非表节与非字符串条目记录警告后跳过（数据格式错误不再完全静默，
/// RZ_LOG=warn 时可见）；返回原始 toml 错误，
/// 由调用方按场景本地化错误消息（文件/内置/路径等键不同）。
pub fn parse_toml_sections(content: &str) -> Result<SectionMap, toml::de::Error> {
    let value: toml::Value = toml::from_str(content)?;
    let mut sections = HashMap::new();
    if let toml::Value::Table(table) = value {
        for (sub_name, sub_value) in table {
            if let toml::Value::Table(sub_table) = sub_value {
                let mut sub_map = HashMap::new();
                for (zh, en_value) in sub_table {
                    if let toml::Value::String(en) = en_value {
                        sub_map.insert(zh, en);
                    } else {
                        crate::log_warn!(
                            "映射源",
                            "节 [{}] 条目 {} 的值不是字符串，已跳过",
                            sub_name,
                            zh
                        );
                    }
                }
                if !sub_map.is_empty() {
                    sections.insert(sub_name, sub_map);
                }
            } else {
                crate::log_warn!("映射源", "节 [{}] 不是表结构，已跳过", sub_name);
            }
        }
    }
    Ok(sections)
}

/// 把节表扁平化为单一映射：按节名升序合并，同名键冲突时胜出者确定
/// （HashMap 遍历顺序随机会导致多次加载结果不一致）
pub fn flatten_sections(sections: &SectionMap) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let mut names: Vec<&String> = sections.keys().collect();
    names.sort();
    for name in names {
        for (zh, en) in &sections[name] {
            result.insert(zh.clone(), en.clone());
        }
    }
    result
}

/// 解析“模块路径 + 标识符”两节格式的 TOML（stdlib.toml 与 crates/*.toml 通用）
///
/// - `["模块路径"]` 节 → 合并到模块路径映射（如 `"线程" = "std::thread"`）
/// - `["标识符"]` 节 → 合并到标识符别名映射（如 `"字符串" = "String"`）
///
/// 返回原始 toml 错误，由调用方结合文件路径/数据源上下文转为 [`LoadError`]。
pub fn merge_module_and_ident_sections(
    content: &str,
    module_path_map: &mut HashMap<String, String>,
    alias_map: &mut HashMap<String, String>,
) -> Result<(), toml::de::Error> {
    let sections = parse_toml_sections(content)?;
    if let Some(entries) = sections.get("模块路径") {
        module_path_map.extend(entries.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    if let Some(entries) = sections.get("标识符") {
        alias_map.extend(entries.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    Ok(())
}

/// 把文件名字节转成 UTF-8 字符串（作为映射分类键）
///
/// - 本身是 UTF-8 时直接返回；
/// - 非 UTF-8 字节按常见 CJK 编码依次尝试解码（GB18030 → Shift_JIS → Big5 →
///   EUC-KR → EUC-JP），首个解码无错误者即为转码结果，
///   把“非 UTF-8 文件名”真正转成 UTF-8（而非丢弃字节）；
/// - 所有编码都失败时回退 lossy 转写（保留可显示部分，不 panic）。
///
/// 平台差异：只有 Unix 允许文件名是任意字节，因此才需要字节级转码；
/// Windows 的文件名在系统层面就是 UTF-16，正常文件名（含中文/日文等）
/// `to_str()` 总能成功，不会进入转码分支，也不会被误判成 GBK 乱码；
/// 唯一极端情况是文件名字含孤立代理码元（正常工具造不出来），
/// 此时回退为明确的替换符 `�` 而非错误解码。
fn decode_os_file_name(name: &std::ffi::OsStr) -> String {
    if let Some(text) = name.to_str() {
        return text.to_string();
    }
    // Unix 下可取原始字节逐编码尝试；其他平台（Windows 等）直接走 lossy 兜底
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let bytes = name.as_bytes();
        // 按 CJK 使用频度排序：中文 GB18030（覆盖 GBK/GB2312）优先，日文 Shift_JIS 次之。
        // 注：部分字节序列在多种编码下都能无错误解码，顺序只是概率取舍，
        // 无法在没有额外元数据时做到 100% 正确。
        for encoding in [
            encoding_rs::GB18030,
            encoding_rs::SHIFT_JIS,
            encoding_rs::BIG5,
            encoding_rs::EUC_KR,
            encoding_rs::EUC_JP,
        ] {
            let (decoded, _, had_errors) = encoding.decode(bytes);
            if !had_errors {
                return decoded.into_owned();
            }
        }
    }
    name.to_string_lossy().into_owned()
}

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

    /// 获取分类的显示名称（随当前语言变化）
    pub fn display_name(&self) -> String {
        let key = match self {
            MappingCategory::Keywords => "mapping_cat_keywords",
            MappingCategory::StdLib => "mapping_cat_stdlib",
            MappingCategory::ThirdParty => "mapping_cat_third_party",
        };
        crate::语言::t(key)
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
    pub fn load(&mut self, category: MappingCategory) -> Result<(), LoadError> {
        // 第三方库是目录，包含多个文件
        if category == MappingCategory::ThirdParty {
            return self.load_third_party_dir();
        }

        let file_path = self.root_dir.join(category.default_filename());

        if !file_path.exists() {
            return Err(LoadError::FileMissing {
                target: LoadTarget::Mapping,
                path: format!("{:?}", file_path),
            });
        }

        let content = fs::read_to_string(&file_path).map_err(|e| LoadError::ReadFailed {
            target: LoadTarget::Mapping,
            path: None,
            detail: e.to_string(),
        })?;

        let category_map = parse_toml_sections(&content).map_err(|e| LoadError::ParseFailed {
            target: LoadTarget::Mapping,
            path: None,
            detail: e.to_string(),
        })?;
        self.mappings.insert(category, category_map);
        Ok(())
    }

    /// 加载第三方库目录（crates/）下的所有 TOML 文件
    fn load_third_party_dir(&mut self) -> Result<(), LoadError> {
        let dir_path = self.root_dir.join("crates");

        if !dir_path.exists() {
            // 目录不存在，静默返回
            return Ok(());
        }

        let mut merged_map = HashMap::new();

        // 遍历目录中的所有 .toml 文件（按文件名排序，read_dir 顺序未定义，
        // 排序后合并结果与 MappingManager::load_from_dir 保持一致且确定）；
        // read_dir 或条目读取失败必须报错（不能静默吞掉，否则映射整体丢失）
        let mut toml_files: Vec<PathBuf> = fs::read_dir(&dir_path)
            .map_err(|e| LoadError::DirReadFailed {
                detail: e.to_string(),
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| LoadError::DirReadFailed {
                detail: e.to_string(),
            })?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("toml"))
            .collect();
        toml_files.sort();

        for path in toml_files {
            // 获取文件名（不含扩展名）作为分类标识；
            // 非 UTF-8 文件名按常见编码转码为 UTF-8（decode_os_file_name），
            // 避免多个文件碰撞合并或分类键变为替换符
            let file_name = path
                .file_stem()
                .map(decode_os_file_name)
                .unwrap_or_else(|| decode_os_file_name(path.as_os_str()));

            let content = fs::read_to_string(&path).map_err(|e| LoadError::ReadFailed {
                target: LoadTarget::ThirdParty,
                path: Some(format!("{:?}", path)),
                detail: e.to_string(),
            })?;

            // 将文件中的映射合并，加上文件分类前缀
            let sections = parse_toml_sections(&content).map_err(|e| LoadError::ParseFailed {
                target: LoadTarget::ThirdParty,
                path: Some(format!("{:?}", path)),
                detail: e.to_string(),
            })?;
            // 节名排序后合并，保证 "文件名/子分类" 键的插入顺序确定
            let mut sub_names: Vec<&String> = sections.keys().collect();
            sub_names.sort();
            for sub_name in sub_names {
                let category_key = format!("{}/{}", file_name, sub_name);
                merged_map.insert(category_key, sections[sub_name].clone());
            }
        }

        self.mappings
            .insert(MappingCategory::ThirdParty, merged_map);
        Ok(())
    }

    /// 加载所有默认映射表
    pub fn load_all(&mut self) -> Result<(), LoadError> {
        self.load(MappingCategory::Keywords)?;
        // 标准库可选：缺失时静默跳过（与 MappingManager::load_from_dir 行为一致），
        // 存在但加载失败时必须报错
        if self.root_dir.join("stdlib.toml").exists() {
            self.load(MappingCategory::StdLib)?;
        }
        // module_paths.toml（可选）：模块路径映射并入标准库分类的 ["模块路径"] 子节。
        // 在 stdlib 之后合并且仅补充缺失键，保证 stdlib 的同名映射优先
        // （与 MappingManager 的 module_paths 先、stdlib 后的覆盖顺序语义一致）
        let module_paths_file = self.root_dir.join("module_paths.toml");
        if module_paths_file.exists() {
            let content =
                fs::read_to_string(&module_paths_file).map_err(|e| LoadError::ReadFailed {
                    target: LoadTarget::ModulePaths,
                    path: Some(module_paths_file.display().to_string()),
                    detail: e.to_string(),
                })?;
            let sections = parse_toml_sections(&content).map_err(|e| LoadError::ParseFailed {
                target: LoadTarget::ModulePaths,
                path: None,
                detail: e.to_string(),
            })?;
            if let Some(entries) = sections.get("模块路径") {
                let stdlib_map = self.mappings.entry(MappingCategory::StdLib).or_default();
                let mp_section = stdlib_map.entry("模块路径".to_string()).or_default();
                for (k, v) in entries {
                    mp_section.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
        }
        // 第三方库可选：目录不存在时静默跳过，存在但加载失败时必须报错
        // （否则映射静默丢失，用户看到的是未翻译的标识符而非错误提示）
        if self.root_dir.join("crates").is_dir() {
            self.load(MappingCategory::ThirdParty)?;
        }
        Ok(())
    }

    /// 获取指定分类的完整映射表（扁平化合并所有子分类）
    ///
    /// 子分类按名称排序后合并，保证同名键冲突时的胜出者确定
    /// （HashMap 遍历顺序随机会导致多次加载结果不一致）
    pub fn get_mapping(&self, category: MappingCategory) -> HashMap<String, String> {
        self.mappings
            .get(&category)
            .map(flatten_sections)
            .unwrap_or_default()
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

    /// 查询单个映射条目（按子分类名升序查找，同名键命中顺序确定）
    pub fn query(&self, category: MappingCategory, zh: &str) -> Option<String> {
        if let Some(category_map) = self.mappings.get(&category) {
            let mut names: Vec<&String> = category_map.keys().collect();
            names.sort();
            for name in names {
                if let Some(en) = category_map[name].get(zh) {
                    return Some(en.clone());
                }
            }
        }
        None
    }

    /// 反向查询（从英文查中文；按子分类名升序遍历，结果确定）
    pub fn reverse_query(&self, category: MappingCategory, en: &str) -> Option<String> {
        if let Some(category_map) = self.mappings.get(&category) {
            let mut names: Vec<&String> = category_map.keys().collect();
            names.sort();
            for name in names {
                for (zh, e) in &category_map[name] {
                    if e == en {
                        return Some(zh.clone());
                    }
                }
            }
        }
        None
    }

    /// 获取所有子分类名称（排序后返回，输出确定）
    pub fn get_sub_categories(&self, category: MappingCategory) -> Vec<String> {
        let mut names = self
            .mappings
            .get(&category)
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        names.sort();
        names
    }

    /// 统计映射条目数（扁平化后）
    pub fn entry_count(&self, category: MappingCategory) -> usize {
        self.get_mapping(category).len()
    }
}

/// 便捷函数：加载关键字映射
pub fn load_keyword_mapping<P: AsRef<Path>>(
    lang_pack_path: P,
) -> Result<HashMap<String, String>, LoadError> {
    let mut loader = MappingLoader::new(lang_pack_path);
    loader.load(MappingCategory::Keywords)?;
    Ok(loader.get_mapping(MappingCategory::Keywords))
}

/// 便捷函数：加载标准库映射
pub fn load_stdlib_mapping<P: AsRef<Path>>(
    lang_pack_path: P,
) -> Result<HashMap<String, String>, LoadError> {
    let mut loader = MappingLoader::new(lang_pack_path);
    loader.load(MappingCategory::StdLib)?;
    Ok(loader.get_mapping(MappingCategory::StdLib))
}

/// 便捷函数：加载所有映射
pub fn load_all_mappings<P: AsRef<Path>>(
    lang_pack_path: P,
) -> Result<HashMap<MappingCategory, HashMap<String, String>>, LoadError> {
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
        // 独立临时目录：固定路径会在并行测试/多次运行间交叉污染
        let temp = tempfile::tempdir().unwrap();
        let temp_dir = temp.path();

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
        let mut loader = MappingLoader::new(temp_dir);
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
    }

    #[test]
    fn test_get_flattened_mapping() {
        let temp = tempfile::tempdir().unwrap();
        let temp_dir = temp.path();

        let test_content = r#"
["分类A"]
"甲" = "alpha"
"乙" = "beta"

["分类B"]
"丙" = "gamma"
"#;
        fs::write(temp_dir.join("stdlib.toml"), test_content).unwrap();

        let mut loader = MappingLoader::new(temp_dir);
        loader.load(MappingCategory::StdLib).unwrap();

        let mapping = loader.get_mapping(MappingCategory::StdLib);
        assert_eq!(mapping.len(), 3);
        assert_eq!(mapping.get("甲"), Some(&"alpha".to_string()));
        assert_eq!(mapping.get("丙"), Some(&"gamma".to_string()));
        // 子分类名排序后返回，输出确定
        assert_eq!(
            loader.get_sub_categories(MappingCategory::StdLib),
            vec!["分类A", "分类B"]
        );
    }

    #[test]
    fn test_builtin_keyword_mapping() {
        let map = create_builtin_keyword_mapping();

        assert_eq!(map.get("函数"), Some(&"fn".to_string()));
        assert_eq!(map.get("如果"), Some(&"if".to_string()));
        assert_eq!(map.get("整数"), Some(&"i32".to_string()));
        assert!(map.len() > 30);
    }

    /// load_all 把 module_paths.toml 并入标准库分类的 ["模块路径"] 子节，
    /// 且同名键 stdlib 优先（与 MappingManager::load_from_dir 的覆盖顺序语义一致）
    #[test]
    fn test_load_all_merges_module_paths_with_stdlib_priority() {
        let temp = tempfile::tempdir().unwrap();
        let temp_dir = temp.path();

        fs::write(
            temp_dir.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        // stdlib 与 module_paths 含同名键，stdlib 必须优先
        fs::write(
            temp_dir.join("stdlib.toml"),
            "[\"模块路径\"]\n\"标准库\" = \"std\"\n\"文件系统\" = \"std::fs\"\n",
        )
        .unwrap();
        fs::write(
            temp_dir.join("module_paths.toml"),
            "[\"模块路径\"]\n\"文件系统\" = \"fs\"\n\"字符串\" = \"string\"\n",
        )
        .unwrap();

        let mut loader = MappingLoader::new(temp_dir);
        loader.load_all().unwrap();

        let mp = loader
            .get_sub_mapping(MappingCategory::StdLib, "模块路径")
            .unwrap();
        // module_paths 的独有键并入标准库分类
        assert_eq!(mp.get("字符串"), Some(&"string".to_string()));
        // 同名键 stdlib 优先（module_paths 仅补充缺失键）
        assert_eq!(mp.get("文件系统"), Some(&"std::fs".to_string()));
    }

    /// 仅 keywords.toml 时 load_all 不应报错（stdlib/module_paths/crates 均可选）
    #[test]
    fn test_load_all_stdlib_optional() {
        let temp = tempfile::tempdir().unwrap();
        let temp_dir = temp.path();
        fs::write(
            temp_dir.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();

        let mut loader = MappingLoader::new(temp_dir);
        loader.load_all().unwrap();
        assert_eq!(
            loader.query(MappingCategory::Keywords, "函数"),
            Some("fn".to_string())
        );
    }

    /// 非 UTF-8（GBK 编码）的文件名被正确转码为 UTF-8 分类键，
    /// 而不是被 lossy 替换或与其它文件碰撞
    #[test]
    #[cfg(unix)]
    fn test_non_utf8_file_name_transcoded_to_utf8() {
        use std::os::unix::ffi::OsStringExt;

        let temp = tempfile::tempdir().unwrap();
        let temp_dir = temp.path();
        let crates_dir = temp_dir.join("crates");
        fs::create_dir_all(&crates_dir).unwrap();
        // keywords.toml 为必需文件
        fs::write(
            temp_dir.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();

        // “序列化”的 GBK 字节：序=d0f2 列=c1d0 化=bbaf
        let gbk_name = std::ffi::OsString::from_vec(vec![0xD0, 0xF2, 0xC1, 0xD0, 0xBB, 0xAF]);
        let path = crates_dir.join(gbk_name).with_extension("toml");
        fs::write(&path, "[\"标识符\"]\n\"服务器\" = \"Server\"\n").unwrap();

        // 前置断言：该文件名的字节确实不是合法 UTF-8
        assert!(path.file_name().unwrap().to_str().is_none());

        let mut loader = MappingLoader::new(temp_dir);
        loader.load_all().unwrap();

        // 分类键应为转码后的“序列化/标识符”（GB18030 解码），映射可正常查询
        let sub = loader
            .get_sub_mapping(MappingCategory::ThirdParty, "序列化/标识符")
            .unwrap();
        assert_eq!(sub.get("服务器"), Some(&"Server".to_string()));
    }
}
