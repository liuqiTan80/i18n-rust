//! 映射管理器 - 统一管理所有映射表（关键字、模块路径、标识符别名）
//!
//! 提供完整的映射加载、查询接口，是翻译管线的核心数据结构。
//! 支持从文件系统目录加载或从内置字符串数据加载（无需文件系统）。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

// 解析原语与 mapping_source 共用单一实现（节表解析/两节合并），
// 避免两个加载器对同一 TOML 格式的理解漂移
use crate::error::{LoadError, LoadTarget};
use crate::mapping_source::{
    DangerInfo, DangerLevel, ParsedDocument, danger_from_sections, flatten_sections,
    is_always_panic_target, is_native_key,
};

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
    /// 派生特征映射（`#[派生(...)]` 属性内专用：中文特征名 → 英文，
    /// 如 `克隆` → `Clone`；不并入 keyword_map，避免与方法名别名冲突）
    derive_map: HashMap<String, String>,
    /// 模块路径映射（如 `标准库` → `std`）
    pub module_path_map: HashMap<String, String>,
    /// 标识符别名映射（标准库/第三方库的类型与函数名翻译）
    pub alias_map: HashMap<String, String>,
    /// 语境指纹缓存：映射表构造后不可变，首次计算后复用。
    /// 每次转译都全量重算指纹（构建全部键值对 + 排序 + 哈希）
    /// 在批量转译场景是纯浪费（语言包不变则指纹恒定）。
    fingerprint_cache: OnceLock<u64>,
    /// use 语句内的“让位词”：模块路径与别名映射（标准库 + 第三方）键的并集。
    /// 词法转译在 use 段内跳过这些词的关键字/宏替换，交由模块路径与别名
    /// 阶段按标准库语义处理——同名词在宏表与标准库表重复时 use 段内标准库
    /// 优先（`文件` 宏节为 file、标准库为 File，`使用 标准库::文件系统::文件`
    /// 必须产出 `use std::fs::File`）。构造时一次性生成，转译时直接复用。
    use_defer_words: HashSet<String>,
    /// 方法调用位的“让位词”：词法层替换值为 Rust 保留关键字（`枚举`→enum、
    /// `匹配`→match）、而别名表（标准库 + 第三方）中该词另有不同有效词条
    /// （`枚举`=enumerate、`匹配`=matches）的词。词法转译在方法位（前面是
    /// `.`）跳过这些词的替换——`.枚举()` 直译会成非法的 `.enum()`，让位后
    /// 由别名阶段替换为 `.enumerate()`。构造时一次性生成，转译时直接复用。
    method_defer_words: HashSet<String>,
    /// 教学 lint 已知词表缓存：全部映射表键的并集（关键字/宏/派生/模块路径/
    /// 别名）。供“方法调用位未命中映射表”的易混词提示判定——词表大且
    /// lint 每文件调用，惰性计算一次后复用（映射表构造后不可变）。
    lint_words_cache: OnceLock<HashSet<String>>,
    /// 教学 lint 歧义构造器被调名集缓存（见 [`Self::ambiguous_constructor_words`]）
    ambiguous_constructors_cache: OnceLock<HashSet<String>>,
    /// 英文回落反向索引缓存（英文原词 → 母语词条）：见 [`Self::english_fallback_map`]
    english_fallback_cache: OnceLock<HashMap<String, String>>,
    /// 宏映射缓存（`["宏"]` 节的物化副本）：见 [`Self::get_macro_map`]
    macro_map_cache: OnceLock<HashMap<String, String>>,
    /// 危险元数据（`["危险等级"]` / `["危险备注"]` 节）：词条 → 风险声明
    danger_map: HashMap<String, DangerInfo>,
    /// 「英文名 → 母语名」反查候选，已按优先序排好（同一英文名取首个）。
    /// 只供诊断反查表（[`crate::diagnostic::build_type_map`]）使用，不参与正向转译。
    /// 顺序来自各来源 TOML 的书写序（先写的是主词条），见
    /// [`ReverseCandidateSources`]；正向表用 HashMap，节内顺序在解析时就已丢失，
    /// 若反查直接遍历 HashMap，同一英文名的多个母语同义词谁胜出就成了
    /// 跨进程随机数（译文会偶尔翻转，见 docs/strategy/README.md）。
    reverse_candidates: Vec<(String, String)>,
}

/// 诊断反查候选的有序来源清单（加载期收集，字段顺序即优先序）
///
/// 同一节内按 TOML 书写序；跨来源按以下优先级（与正向映射的
/// 「模块路径覆盖一切、标准库优先于第三方」一致）：
/// 1. stdlib.toml 的 `["模块路径"]`
/// 2. module_paths.toml 与 crates/*.toml 的 `["模块路径"]`（后者按文件名序）
/// 3. keywords.toml 的 `["类型"]`
/// 4. stdlib.toml 的 `["标识符"]`
/// 5. crates/*.toml 的 `["标识符"]`（按文件名序）
#[derive(Default)]
struct ReverseCandidateSources {
    module_path_stdlib: Vec<(String, String)>,
    module_path_other: Vec<(String, String)>,
    type_section: Vec<(String, String)>,
    ident_stdlib: Vec<(String, String)>,
    ident_crate: Vec<(String, String)>,
}

/// 候选汇总的一个阶段：按书写序的条目 + 该来源对应的**生效**正向表
///
/// 只对「在生效表里确实是这个值」的条目做候选（被后续来源覆盖的旧写法
/// 不能再进诊断），见 [`MappingManager::assemble_reverse_candidates`]。
type CandidateStage<'a> = (&'a [(String, String)], &'a HashMap<String, String>);

impl MappingManager {
    /// 从语言包目录加载全部映射（推荐用于开发模式）
    ///
    /// 加载顺序：
    /// 1. `keywords.toml` → 关键字映射
    /// 2. `module_paths.toml` → 模块路径映射
    /// 3. `stdlib.toml` → 标准库的模块路径 + 标识符别名（可选）
    /// 4. `crates/*.toml` → 第三方库的模块路径 + 标识符别名
    pub fn load_from_dir(lang_dir: &Path) -> Result<Self, LoadError> {
        let keywords_path = lang_dir.join("keywords.toml");
        if !keywords_path.exists() {
            return Err(LoadError::FileMissing {
                target: LoadTarget::Keywords,
                path: keywords_path.display().to_string(),
            });
        }

        // 1. 加载关键字映射
        let keywords_content =
            fs::read_to_string(&keywords_path).map_err(|e| LoadError::ReadFailed {
                target: LoadTarget::Keywords,
                path: Some(keywords_path.display().to_string()),
                detail: e.to_string(),
            })?;
        let keywords_doc =
            ParsedDocument::parse(&keywords_content).map_err(|e| LoadError::ParseFailed {
                target: LoadTarget::Keywords,
                path: None,
                detail: e.to_string(),
            })?;
        let section_map = keywords_doc.sections();
        // 危险等级与关键字同文件（`["危险等级"]` / `["危险备注"]`）
        let mut danger_map = danger_from_sections(&section_map);
        // 派生特征节单独存放（不并入关键字表，避免与方法名别名冲突），
        // 并从扁平化关键字表中移除对应键
        let derive_map = section_map.get("派生特征").cloned().unwrap_or_default();
        let mut keyword_map = flatten_sections(&section_map);
        for key in derive_map.keys() {
            keyword_map.remove(key);
        }
        // 诊断反查候选：各来源的 TOML 条目按书写序留存（正向映射仍用上面的
        // HashMap，两者来自同一次解析，不改变任何映射内容）
        let mut reverse_sources = ReverseCandidateSources {
            type_section: keywords_doc.entries("类型").to_vec(),
            ..Default::default()
        };

        // 2. 加载模块路径映射（来自 module_paths.toml）
        let module_paths_file = lang_dir.join("module_paths.toml");
        let mut module_path_map = HashMap::new();
        if module_paths_file.exists() {
            let content =
                fs::read_to_string(&module_paths_file).map_err(|e| LoadError::ReadFailed {
                    target: LoadTarget::ModulePaths,
                    path: Some(module_paths_file.display().to_string()),
                    detail: e.to_string(),
                })?;
            let doc = ParsedDocument::parse(&content).map_err(|e| LoadError::ParseFailed {
                target: LoadTarget::ModulePaths,
                path: None,
                detail: e.to_string(),
            })?;
            let sections = doc.sections();
            if let Some(entries) = sections.get("模块路径") {
                module_path_map.extend(entries.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
            reverse_sources
                .module_path_other
                .extend_from_slice(doc.entries("模块路径"));
            danger_map.extend(danger_from_sections(&sections));
        }

        // 3. 扫描第三方库目录（crates/）
        // 文件按文件名排序后合并：read_dir 顺序未定义，同键不同值时
        // 合并结果必须确定（与 load_from_builtin 的排序行为一致）
        let crates_dir = lang_dir.join("crates");
        let mut alias_map = HashMap::new();
        if crates_dir.exists() && crates_dir.is_dir() {
            // read_dir 或条目读取失败必须报错（不能静默吞掉），
            // 否则第三方库映射整体丢失且用户无感知
            let mut toml_files = Vec::new();
            for entry in fs::read_dir(&crates_dir).map_err(|e| LoadError::DirReadFailed {
                detail: e.to_string(),
            })? {
                let entry = entry.map_err(|e| LoadError::DirReadFailed {
                    detail: e.to_string(),
                })?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                    toml_files.push(path);
                }
            }
            toml_files.sort();
            for file_path in toml_files {
                let content =
                    fs::read_to_string(&file_path).map_err(|e| LoadError::ReadFailed {
                        target: LoadTarget::ThirdParty,
                        path: Some(file_path.display().to_string()),
                        detail: e.to_string(),
                    })?;
                let doc = ParsedDocument::parse(&content).map_err(|e| LoadError::ParseFailed {
                    target: LoadTarget::ThirdParty,
                    path: Some(file_path.display().to_string()),
                    detail: e.to_string(),
                })?;
                doc.merge_module_and_ident(&mut module_path_map, &mut alias_map);
                reverse_sources
                    .module_path_other
                    .extend_from_slice(doc.entries("模块路径"));
                reverse_sources
                    .ident_crate
                    .extend_from_slice(doc.entries("标识符"));
                danger_map.extend(doc.danger());
            }
        }

        // 4. 加载标准库映射（stdlib.toml，可选，最后加载以保证优先）
        //    与 crates/*.toml 相同格式：["模块路径"] + ["标识符"] 两节
        //    后加载会覆盖第三方库中同名的通用词（如 mpsc 的 "发送" 不被 HTTP 库的 "post" 覆盖）
        //    文件存在但读取失败必须报错（不能静默跳过，否则标准库映射整体丢失）
        let stdlib_file = lang_dir.join("stdlib.toml");
        if stdlib_file.exists() {
            let content = fs::read_to_string(&stdlib_file).map_err(|e| LoadError::ReadFailed {
                target: LoadTarget::Stdlib,
                path: Some(stdlib_file.display().to_string()),
                detail: e.to_string(),
            })?;
            let doc = ParsedDocument::parse(&content).map_err(|e| LoadError::ParseFailed {
                target: LoadTarget::Stdlib,
                path: Some(stdlib_file.display().to_string()),
                detail: e.to_string(),
            })?;
            doc.merge_module_and_ident(&mut module_path_map, &mut alias_map);
            reverse_sources
                .module_path_stdlib
                .extend_from_slice(doc.entries("模块路径"));
            reverse_sources.ident_stdlib = doc.entries("标识符").to_vec();
            danger_map.extend(doc.danger());
        }

        let use_defer_words = Self::build_use_defer_words(&module_path_map, &alias_map);
        let method_defer_words = Self::build_method_defer_words(&keyword_map, &alias_map);
        let reverse_candidates = Self::assemble_reverse_candidates(
            &reverse_sources,
            &module_path_map,
            section_map.get("类型"),
            &alias_map,
        );
        Ok(Self {
            keyword_map,
            section_map,
            derive_map,
            use_defer_words,
            method_defer_words,
            module_path_map,
            alias_map,
            fingerprint_cache: OnceLock::new(),
            lint_words_cache: OnceLock::new(),
            ambiguous_constructors_cache: OnceLock::new(),
            english_fallback_cache: OnceLock::new(),
            macro_map_cache: OnceLock::new(),
            danger_map,
            reverse_candidates,
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
    ) -> Result<Self, LoadError> {
        // 1. 解析关键字映射
        let keywords_doc =
            ParsedDocument::parse(keywords_toml).map_err(|e| LoadError::ParseFailed {
                target: LoadTarget::BuiltinKeywords,
                path: None,
                detail: e.to_string(),
            })?;
        let section_map = keywords_doc.sections();
        // 派生特征节单独存放（不并入关键字表），并从扁平化关键字表中移除
        let derive_map = section_map.get("派生特征").cloned().unwrap_or_default();
        let mut keyword_map = flatten_sections(&section_map);
        for key in derive_map.keys() {
            keyword_map.remove(key);
        }
        let mut danger_map = danger_from_sections(&section_map);
        // 诊断反查候选：与 load_from_dir 同一套优先序（按来源而非读取顺序），
        // 因此两种加载方式得到的反查表一致
        let mut reverse_sources = ReverseCandidateSources {
            type_section: keywords_doc.entries("类型").to_vec(),
            ..Default::default()
        };

        // 2. 解析模块路径映射
        let mut module_path_map = HashMap::new();
        let module_paths_doc =
            ParsedDocument::parse(module_paths_toml).map_err(|e| LoadError::ParseFailed {
                target: LoadTarget::BuiltinModulePaths,
                path: None,
                detail: e.to_string(),
            })?;
        let sections = module_paths_doc.sections();
        if let Some(entries) = sections.get("模块路径") {
            module_path_map.extend(entries.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        reverse_sources
            .module_path_other
            .extend_from_slice(module_paths_doc.entries("模块路径"));
        danger_map.extend(danger_from_sections(&sections));

        // 3. 解析标准库映射（模块路径 + 标识符别名）
        let mut alias_map = HashMap::new();
        let stdlib_doc =
            ParsedDocument::parse(stdlib_toml).map_err(|e| LoadError::ParseFailed {
                target: LoadTarget::BuiltinStdlib,
                path: None,
                detail: e.to_string(),
            })?;
        let sections = stdlib_doc.sections();
        if let Some(entries) = sections.get("模块路径") {
            module_path_map.extend(entries.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        if let Some(entries) = sections.get("标识符") {
            alias_map.extend(entries.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        reverse_sources
            .module_path_stdlib
            .extend_from_slice(stdlib_doc.entries("模块路径"));
        reverse_sources.ident_stdlib = stdlib_doc.entries("标识符").to_vec();
        danger_map.extend(danger_from_sections(&sections));

        // 4. 解析第三方库映射（按文件名排序合并，与 load_from_dir 行为一致）
        let mut sorted: Vec<&(&str, &str)> = third_party_data.iter().collect();
        sorted.sort_by_key(|(name, _)| *name);
        for (file_name, content) in sorted {
            let doc = ParsedDocument::parse(content).map_err(|e| LoadError::ParseFailed {
                target: LoadTarget::ThirdParty,
                path: Some((*file_name).to_string()),
                detail: e.to_string(),
            })?;
            doc.merge_module_and_ident(&mut module_path_map, &mut alias_map);
            reverse_sources
                .module_path_other
                .extend_from_slice(doc.entries("模块路径"));
            reverse_sources
                .ident_crate
                .extend_from_slice(doc.entries("标识符"));
            danger_map.extend(doc.danger());
        }

        let use_defer_words = Self::build_use_defer_words(&module_path_map, &alias_map);
        let method_defer_words = Self::build_method_defer_words(&keyword_map, &alias_map);
        let reverse_candidates = Self::assemble_reverse_candidates(
            &reverse_sources,
            &module_path_map,
            section_map.get("类型"),
            &alias_map,
        );
        Ok(Self {
            keyword_map,
            section_map,
            derive_map,
            use_defer_words,
            method_defer_words,
            module_path_map,
            alias_map,
            fingerprint_cache: OnceLock::new(),
            lint_words_cache: OnceLock::new(),
            ambiguous_constructors_cache: OnceLock::new(),
            english_fallback_cache: OnceLock::new(),
            macro_map_cache: OnceLock::new(),
            danger_map,
            reverse_candidates,
        })
    }

    /// 向后兼容：从单个关键字文件加载（委托给 load_from_dir）
    pub fn load_from_file(path: &Path) -> Result<Self, LoadError> {
        let dir = path.parent().unwrap_or(Path::new("."));
        Self::load_from_dir(dir)
    }

    /// 从已解析的映射表直接构造（旧格式语言包/降级场景）
    ///
    /// 不包含节信息（宏表/派生表为空），仅提供关键字/模块路径/别名三类映射；
    /// 用于 LSP 旧"映射表"目录格式与硬编码兜底表的接入。
    pub fn from_flat_maps(
        keyword_map: HashMap<String, String>,
        module_path_map: HashMap<String, String>,
        alias_map: HashMap<String, String>,
    ) -> Self {
        let use_defer_words = Self::build_use_defer_words(&module_path_map, &alias_map);
        let method_defer_words = Self::build_method_defer_words(&keyword_map, &alias_map);
        // 没有 TOML 书写序可用（旧格式/硬编码兜底表），反查候选退化为按母语键排序：
        // 仍然确定，且 ["类型"] 不在这些表里，与旧版只取模块路径/别名两源的口径一致
        let reverse_candidates = Self::assemble_reverse_candidates(
            &ReverseCandidateSources::default(),
            &module_path_map,
            None,
            &alias_map,
        );
        Self {
            keyword_map,
            section_map: HashMap::new(),
            derive_map: HashMap::new(),
            use_defer_words,
            method_defer_words,
            module_path_map,
            alias_map,
            fingerprint_cache: OnceLock::new(),
            lint_words_cache: OnceLock::new(),
            ambiguous_constructors_cache: OnceLock::new(),
            english_fallback_cache: OnceLock::new(),
            macro_map_cache: OnceLock::new(),
            danger_map: HashMap::new(),
            reverse_candidates,
        }
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

    /// 「英文名 → 母语名」反查候选（已按 [`ReverseCandidateSources`] 的优先序排列）
    ///
    /// 调用方按「同一英文名取首个命中」遍历，结果与进程无关。
    /// 仅供诊断反查表使用，不参与正向转译。
    pub fn get_reverse_candidates(&self) -> &[(String, String)] {
        &self.reverse_candidates
    }

    /// 汇总诊断反查候选（母语键 → 英文，按优先序）
    ///
    /// 三条规则：
    /// - 顺序 = `sources` 的字段顺序，同一来源内按 TOML 书写序。语言包允许
    ///   同一英文值有多个母语同义词（正向写法都合法），反查时必须有一个
    ///   确定胜出者；先写的那条是主词条（如 zh `"可显示" = "Display"` 写在
    ///   特征区、`"显示" = "Display"` 写在后面的格式化区）。
    /// - 只保留正向表里仍然成立的条目：同一母语键在本类映射里被后续文件覆盖时，
    ///   旧值不再参与反查，否则反查会给出一个正向写不出来的组合。
    ///   同一母语键在不同映射里可以各有有效词条（`字符串` 在 keywords `[类型]`
    ///   里是 `String`、在 module_paths.toml 里是路径段 `string`），故去重只能按
    ///   「母语键 + 英文值」整对儿做，不能按母语键抹平。
    /// - 有序视图里没出现的条目（如某文件书写序解析失败、或由
    ///   [`Self::from_flat_maps`] 这种无书写序的入口构造）按母语键排序补在
    ///   最后，保证反查表覆盖全部英文值，不会因缺少顺序信息而少译。
    fn assemble_reverse_candidates(
        sources: &ReverseCandidateSources,
        module_path_map: &HashMap<String, String>,
        type_section: Option<&HashMap<String, String>>,
        alias_map: &HashMap<String, String>,
    ) -> Vec<(String, String)> {
        let empty = HashMap::new();
        let type_section = type_section.unwrap_or(&empty);
        let mut candidates: Vec<(String, String)> = Vec::new();
        let mut seen_pair: HashSet<(String, String)> = HashSet::new();
        let stages: [CandidateStage<'_>; 5] = [
            (&sources.module_path_stdlib, module_path_map),
            (&sources.module_path_other, module_path_map),
            (&sources.type_section, type_section),
            (&sources.ident_stdlib, alias_map),
            (&sources.ident_crate, alias_map),
        ];
        for (entries, effective) in stages {
            for (native, english) in entries {
                // 纯英文键（`"format" = "fmt"` 这类修正项）不能进反查表，
                // 否则译文里会出现英文值
                if !is_native_key(native)
                    || effective.get(native).map(String::as_str) != Some(english.as_str())
                    || !seen_pair.insert((native.clone(), english.clone()))
                {
                    continue;
                }
                candidates.push((native.clone(), english.clone()));
            }
        }
        // 兜底：正向表里有、上面没收到的条目（按母语键排序，结果确定）
        let mut leftovers: Vec<(String, String)> = Vec::new();
        for map in [module_path_map, type_section, alias_map] {
            for (native, english) in map {
                if is_native_key(native) && seen_pair.insert((native.clone(), english.clone())) {
                    leftovers.push((native.clone(), english.clone()));
                }
            }
        }
        leftovers.sort();
        candidates.extend(leftovers);
        candidates
    }

    /// 获取 use 语句内的让位词集合（见字段 `use_defer_words` 说明）
    pub fn get_use_defer_words(&self) -> &HashSet<String> {
        &self.use_defer_words
    }

    /// 获取方法调用位让位词集合（见字段 `method_defer_words` 说明）
    pub fn get_method_defer_words(&self) -> &HashSet<String> {
        &self.method_defer_words
    }

    /// 获取教学 lint 已知词表（见字段 `lint_words_cache` 说明）：
    /// 关键字/宏/派生/模块路径/别名五表键的并集，惰性计算一次后复用。
    pub fn get_lint_words(&self) -> &HashSet<String> {
        self.lint_words_cache.get_or_init(|| {
            let mut words: HashSet<String> = HashSet::new();
            words.extend(self.keyword_map.keys().cloned());
            if let Some(macro_section) = self.section_map.get("宏") {
                words.extend(macro_section.keys().cloned());
            }
            words.extend(self.derive_map.keys().cloned());
            words.extend(self.module_path_map.keys().cloned());
            words.extend(self.alias_map.keys().cloned());
            words
        })
    }

    /// 教学 lint「未标注类型」用：类型推导歧义构造器被调名集合
    ///
    /// 含英文原词 `new`/`default` 与全部映射表中值恰为这些词的方言键
    /// （如 zh：`新建`→new、`默认值`/`缺省`→default）。仅供 lint 启发式
    /// 判定 `X::新建()` 这类无法自行推导结果类型的关联构造调用。
    pub fn ambiguous_constructor_words(&self) -> HashSet<String> {
        self.ambiguous_constructors_cache
            .get_or_init(|| {
                let mut words: HashSet<String> = ["new".to_string(), "default".to_string()]
                    .into_iter()
                    .collect();
                for map in [&self.keyword_map, &self.module_path_map, &self.alias_map] {
                    words.extend(
                        map.iter()
                            .filter(|(_, en)| matches!(en.as_str(), "new" | "default"))
                            .map(|(zh, _)| zh.clone()),
                    );
                }
                words
            })
            .clone()
    }

    /// 英文回落反向索引（英文原词 → 母语词条）：供教学 lint 提示「方言
    /// 文件里写了英文 `fn`，它有母语词条 `函数`」。
    ///
    /// 取全部映射表（关键字/模块路径/别名/派生/宏）中**值恰为单个 Rust
    /// 标识符**且与键不同（非恒等）的条目反转。同一英文词命中多个母语词时，
    /// 按「更短 → 字典序更小」确定性选一个（提示只需一个建议词）。
    /// 惰性计算一次后复用（映射表构造后不可变）；恒等映射的英文语言包
    /// 得到空表 → 回落 lint 自动静默，不误伤英文原生写法。
    pub fn english_fallback_map(&self) -> &HashMap<String, String> {
        self.english_fallback_cache.get_or_init(|| {
            let mut map: HashMap<String, String> = HashMap::new();
            let tables = [
                &self.keyword_map,
                &self.module_path_map,
                &self.alias_map,
                &self.derive_map,
                self.get_macro_map(),
            ];
            for table in tables {
                for (native, en) in table {
                    if native == en || !is_single_rust_ident(en) {
                        continue;
                    }
                    // 同英文词多母语候选：取更短、再字典序更小者（结果确定）
                    let replace = match map.get(en) {
                        Some(cur) => prefer_native(native, cur),
                        None => true,
                    };
                    if replace {
                        map.insert(en.clone(), native.clone());
                    }
                }
            }
            map
        })
    }

    /// 构建 use 段让位词集合：模块路径与别名映射键的并集
    fn build_use_defer_words(
        module_path_map: &HashMap<String, String>,
        alias_map: &HashMap<String, String>,
    ) -> HashSet<String> {
        module_path_map
            .keys()
            .chain(alias_map.keys())
            .cloned()
            .collect()
    }

    /// 构建方法调用位让位词集合：词法层替换值为 Rust 保留关键字、且别名表
    /// 中该词另有不同且有效（非保留字）的词条——仅此冲突类让位，其余词
    /// （如 `文件`：宏节 file / 别名 File）在方法位保留词法层行为，避免
    /// 改变本就合法的产物（`.文件()` 依旧是 `.file()`）。
    fn build_method_defer_words(
        keyword_map: &HashMap<String, String>,
        alias_map: &HashMap<String, String>,
    ) -> HashSet<String> {
        keyword_map
            .iter()
            .filter(|(word, en)| {
                Self::is_rust_reserved_keyword(en)
                    && alias_map
                        .get(*word)
                        .is_some_and(|alias| alias != *en && !Self::is_rust_reserved_keyword(alias))
            })
            .map(|(word, _)| word.clone())
            .collect()
    }

    /// Rust 保留关键字（不能用作标识符；`union` 等弱关键字不在列，仍可作
    /// 方法名）。用于方法位让位词判定：替换值是保留关键字的词在方法位直译
    /// 必为非法产物（`.enum()`），应让位给别名表。
    fn is_rust_reserved_keyword(word: &str) -> bool {
        matches!(
            word,
            "abstract"
                | "as"
                | "async"
                | "await"
                | "become"
                | "box"
                | "break"
                | "const"
                | "continue"
                | "crate"
                | "do"
                | "dyn"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "final"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "macro"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "override"
                | "priv"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "try"
                | "type"
                | "typeof"
                | "unsafe"
                | "unsized"
                | "use"
                | "virtual"
                | "where"
                | "while"
                | "yield"
        )
    }

    /// 翻译语境指纹（惰性计算并缓存）：任一映射表内容变化时指纹变化。
    /// 映射表在构造后不可变，多次转译共用同一指纹，避免每次全量重算。
    pub fn context_fingerprint(&self) -> u64 {
        *self.fingerprint_cache.get_or_init(|| {
            crate::cache::TranslationCache::generate_context_fingerprint(
                &self.keyword_map,
                &self.module_path_map,
                &self.alias_map,
                &self.derive_map,
            )
        })
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

    /// 获取宏映射表（中文宏名 → 英文宏名），来自 `["宏"]` 节
    ///
    /// 与 [`get_macro_names`] 的区别：保留每个宏名的英文替换值。
    /// 宏名同时在类型节与宏节定义时（如 `向量` 类型节为 `Vec`、宏节为 `vec`），
    /// keyword_map 中值被类型节覆盖，宏调用必须用本映射才能得到正确的英文宏名。
    ///
    /// 返回引用而非副本：该映射由 `section_map` 派生，每次调用都克隆整表
    /// 在批量转译/项目上下文收集中是纯浪费（映射表构造后不可变），
    /// 故惰性物化一次缓存后复用（与 [`Self::get_lint_words`] 同一策略）。
    pub fn get_macro_map(&self) -> &HashMap<String, String> {
        self.macro_map_cache
            .get_or_init(|| self.section_map.get("宏").cloned().unwrap_or_default())
    }

    /// 获取派生特征映射（中文特征名 → 英文），来自 `["派生特征"]` 节
    ///
    /// 仅在 `#[派生(...)]` 属性内生效（词法转译的派生参数态），
    /// 如 `克隆` → `Clone`；方法调用 `值.克隆()` 仍走别名表（`clone` 小写）。
    /// 返回引用而非副本：`derive_map` 本身已是字段，克隆纯属浪费。
    pub fn get_derive_map(&self) -> &HashMap<String, String> {
        &self.derive_map
    }

    /// 查询词条危险等级与备注（词条表逐条标注的唯一入口）
    ///
    /// 默认归类：关键字/宏词条 → 会改写标识符（词法层无位置保护）；
    /// 映射目标属于 [`ALWAYS_PANIC_TARGETS`] → 会 panic（如 `解包`→unwrap）；
    /// 其余 → 纯安全。显式 `["危险等级"]` 只能**提高**风险，不能降低自动判定：
    /// 即使标注 `safe`，自动识别出的 panic/rewrite 仍会保留（防漏标）。
    pub fn danger_of(&self, word: &str) -> DangerInfo {
        let target = self
            .alias_map
            .get(word)
            .or_else(|| self.module_path_map.get(word))
            .or_else(|| self.keyword_map.get(word))
            .or_else(|| self.derive_map.get(word));
        let mut level = DangerLevel {
            panic: target.is_some_and(|value| is_always_panic_target(value)),
            rewrite: self.keyword_map.contains_key(word),
        };
        let explicit_note = match self.danger_map.get(word) {
            Some(info) => {
                level.panic |= info.level.panic;
                level.rewrite |= info.level.rewrite;
                info.note.clone()
            }
            None => String::new(),
        };
        let note = if explicit_note.is_empty() && level.panic {
            "运行时可能 panic：仅在调用处显式选择时使用".to_string()
        } else {
            explicit_note
        };
        DangerInfo { level, note }
    }

    /// 检测映射表中的循环引用（A→B 且 B→A 的互指对）
    ///
    /// 覆盖三类：关键字表内部（含宏节）、别名表内部、关键字↔别名跨表。
    /// 循环引用会使正向转译与反向转译来回抖动（译出又译回），
    /// 是语言包配置的严重错误；返回人类可读的循环描述列表（空 = 无循环）。
    /// 自映射（A→A）不视为循环（回译无抖动，但属冗余条目）。
    pub fn find_mapping_cycles(&self) -> Vec<String> {
        let mut cycles = Vec::new();
        let check_self = |map: &HashMap<String, String>, label: &str, cycles: &mut Vec<String>| {
            for (k, v) in map {
                if k != v && map.get(v).is_some_and(|back| back == k) {
                    cycles.push(format!("{label}: {k} ↔ {v}"));
                }
            }
        };
        check_self(&self.keyword_map, "关键字", &mut cycles);
        check_self(&self.alias_map, "别名", &mut cycles);
        // 跨表：关键字阶段替换后的英文值若命中别名表键、且别名替换回原键，
        // 则管线末尾中文残留（别名阶段把关键字阶段的结果又译了回去）
        for (k, v) in &self.keyword_map {
            if k != v && self.alias_map.get(v).is_some_and(|back| back == k) {
                cycles.push(format!("关键字↔别名: {k} ↔ {v}"));
            }
        }
        cycles
    }
}

/// 判定字符串是否为单个 ASCII Rust 标识符（首字符字母/下划线、余下字母数字/下划线）。
/// 排除含 `::`、空格、`<` 等的复合值（如模块路径 `std::fs`），
/// 仅这些单标识符值才能作为回落 lint 的反转键。
fn is_single_rust_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 同一英文词的多个母语候选中，`candidate` 是否优于 `current`（更短、
/// 同长取字典序更小），保证反向索引结果确定（HashMap 迭代序无关）。
fn prefer_native(candidate: &str, current: &str) -> bool {
    let (cc, cu) = (candidate.chars().count(), current.chars().count());
    cc < cu || (cc == cu && candidate < current)
}

#[cfg(test)]
mod tests {
    use super::*;
    // 旧的一次性解析入口：加载器已改用 [`ParsedDocument`]（同一文件不重解析），
    // 这里继续用它的用例守住「非法 TOML 必须报错、不能静默丢映射」这条底线
    use crate::mapping_source::merge_module_and_ident_sections;
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
        // 方法位让位词：合成包无「保留字替换值 + 别名表另有有效词条」的冲突词
        assert!(manager.get_method_defer_words().is_empty());
    }

    #[test]
    fn test_build_method_defer_words() {
        // 保留字替换值 + 别名表另有不同有效词条 → 让位；
        // 替换值非保留字、别名表无词条、别名词条同为保留字 → 均不让位
        let keyword_map = HashMap::from([
            ("枚举".to_string(), "enum".to_string()),
            ("函数".to_string(), "fn".to_string()),
            ("文件".to_string(), "file".to_string()),
            ("让".to_string(), "let".to_string()),
            ("类型".to_string(), "type".to_string()),
        ]);
        let alias_map = HashMap::from([
            ("枚举".to_string(), "enumerate".to_string()),
            ("函数".to_string(), "Fn".to_string()),
            ("文件".to_string(), "File".to_string()),
            ("类型".to_string(), "enum".to_string()),
        ]);
        let words = MappingManager::build_method_defer_words(&keyword_map, &alias_map);
        assert_eq!(
            words,
            HashSet::from(["枚举".to_string(), "函数".to_string()])
        );
    }

    #[test]
    fn test_load_from_dir_missing_keywords() {
        let _guard = crate::语言::test_language("zh");
        let temp = tempfile::tempdir().unwrap();
        let err = MappingManager::load_from_dir(temp.path()).unwrap_err();
        assert!(
            err.to_string().contains("关键字文件不存在"),
            "错误信息应指明缺失文件: {}",
            err
        );
    }

    #[test]
    fn test_load_from_dir_invalid_keywords() {
        let _guard = crate::语言::test_language("zh");
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("keywords.toml"), "这不是合法 TOML [[[").unwrap();
        let err = MappingManager::load_from_dir(temp.path()).unwrap_err();
        assert!(
            err.to_string().contains("解析关键字 TOML 失败"),
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
        let _guard = crate::语言::test_language("zh");
        let keywords = "[\"声明\"]\n";
        let module_paths = "[\"模块路径\"]\n";
        let err =
            MappingManager::load_from_builtin(keywords, module_paths, "非法内容", &[]).unwrap_err();
        assert!(
            err.to_string().contains("解析内置标准库 TOML 失败"),
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
    fn test_merge_sections_reports_invalid_content() {
        let mut module_path_map = HashMap::new();
        let mut alias_map = HashMap::new();
        // 非法 TOML 不应 panic，但必须报告错误（不再静默忽略，防止映射丢失）
        let err = merge_module_and_ident_sections("[[[", &mut module_path_map, &mut alias_map)
            .expect_err("非法 TOML 应返回错误");
        assert!(!err.to_string().is_empty());
        assert!(module_path_map.is_empty());
        assert!(alias_map.is_empty());
        // 合法内容正常合并
        merge_module_and_ident_sections(
            "[\"模块路径\"]\n\"文件系统\" = \"std::fs\"\n[\"标识符\"]\n\"字符串\" = \"String\"\n",
            &mut module_path_map,
            &mut alias_map,
        )
        .expect("合法 TOML 应合并成功");
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

    /// 循环引用检测：关键字表内部互指
    #[test]
    fn test_find_mapping_cycles_keyword_self_cycle() {
        let manager = MappingManager::from_flat_maps(
            HashMap::from([
                ("函数".to_string(), "fn".to_string()),
                ("方法".to_string(), "函数".to_string()),
                ("函数".to_string(), "方法".to_string()), // 后写覆盖：函数→方法
            ]),
            HashMap::new(),
            HashMap::new(),
        );
        let cycles = manager.find_mapping_cycles();
        assert!(
            cycles.iter().any(|c| c.contains("方法 ↔ 函数")),
            "应检测到关键字互指循环：{cycles:?}"
        );
    }

    /// 循环引用检测：别名表内部互指
    #[test]
    fn test_find_mapping_cycles_alias_self_cycle() {
        let manager = MappingManager::from_flat_maps(
            HashMap::new(),
            HashMap::new(),
            HashMap::from([
                ("甲".to_string(), "乙".to_string()),
                ("乙".to_string(), "甲".to_string()),
            ]),
        );
        let cycles = manager.find_mapping_cycles();
        assert!(
            cycles
                .iter()
                .any(|c| c.contains("别名") && c.contains("甲 ↔ 乙")),
            "应检测到别名互指循环：{cycles:?}"
        );
    }

    /// 循环引用检测：关键字↔别名跨表互指（管线末尾中文残留）
    #[test]
    fn test_find_mapping_cycles_cross_table() {
        let manager = MappingManager::from_flat_maps(
            HashMap::from([("甲".to_string(), "乙".to_string())]),
            HashMap::new(),
            HashMap::from([("乙".to_string(), "甲".to_string())]),
        );
        let cycles = manager.find_mapping_cycles();
        assert!(
            cycles
                .iter()
                .any(|c| c.contains("关键字↔别名") && c.contains("甲 ↔ 乙")),
            "应检测到跨表互指循环：{cycles:?}"
        );
    }

    /// 正常映射表（含同值不同键、自映射豁免）不产生循环报告
    #[test]
    fn test_find_mapping_cycles_clean_maps() {
        let manager = MappingManager::from_flat_maps(
            HashMap::from([
                ("函数".to_string(), "fn".to_string()),
                ("让".to_string(), "let".to_string()),
            ]),
            HashMap::new(),
            HashMap::from([("字符串".to_string(), "String".to_string())]),
        );
        assert!(manager.find_mapping_cycles().is_empty());
    }

    /// 危险等级：显式标注、默认归类与「自动判定不可被降级」三条规则
    #[test]
    fn test_danger_levels_metadata_and_defaults() {
        let keywords = concat!(
            "[\"声明\"]\n",
            "\"函数\" = \"fn\"\n",
            "[\"宏\"]\n",
            "\"恐慌\" = \"panic\"\n",
            "[\"危险等级\"]\n",
            "\"恐慌\" = \"panic+rewrite\"\n",
            "\"函数\" = \"safe\"\n",
        );
        let stdlib = concat!(
            "[\"标识符\"]\n",
            "\"解包\" = \"unwrap\"\n",
            "\"末尾\" = \"last\"\n",
            "[\"危险等级\"]\n",
            "\"末尾\" = \"panic\"\n",
            "[\"危险备注\"]\n",
            "\"末尾\" = \"显式选择才使用\"\n",
        );
        let manager = MappingManager::load_from_builtin(keywords, "", stdlib, &[]).unwrap();

        // 关键字默认「会改写标识符」；显式 safe 不能降低自动判定
        let keyword = manager.danger_of("函数");
        assert!(keyword.level.rewrite, "关键字词条默认标记会改写标识符");
        assert!(!keyword.level.panic);

        // 多风险词条：panic + rewrite，且备注不为空
        let panic_macro = manager.danger_of("恐慌");
        assert!(panic_macro.level.panic && panic_macro.level.rewrite);
        assert!(!panic_macro.note.is_empty());

        // 目标为 unwrap：即使没有显式标注也自动判为会 panic
        let unwrap = manager.danger_of("解包");
        assert!(unwrap.level.panic);
        assert!(!unwrap.level.rewrite);
        assert!(!unwrap.note.is_empty(), "panic 词条应有默认提醒备注");

        // 显式标注 panic 的安全目标 + 备注透传
        let explicit = manager.danger_of("末尾");
        assert!(explicit.level.panic);
        assert_eq!(explicit.note, "显式选择才使用");

        // 未登记者默认纯安全；元数据节不得污染映射表
        assert!(manager.danger_of("不存在的词").level.is_safe());
        assert!(!manager.keyword_map.contains_key("危险等级"));
        assert!(!manager.alias_map.contains_key("危险备注"));
    }

    /// 回归：「取列」必须是安全默认（get_opt，编译期强制处理 Option），
    /// 会 panic 的 get 只能由「取列或崩溃」显式写出（防被改回隐式 panic）
    #[test]
    fn test_zh_database_get_requires_explicit_panic() {
        let lang_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
        let manager = MappingManager::load_from_dir(&lang_dir).expect("内置语言包应可加载");

        // 默认路径：取列 本身就走 get_opt，缺失列返回「无」，不隐式 panic
        assert_eq!(
            manager.alias_map.get("取列").map(String::as_str),
            Some("get_opt")
        );
        assert!(
            manager.danger_of("取列").level.is_safe(),
            "取列 应为纯安全默认路径"
        );
        // 旧写法 取列或空 保留为同义词
        assert_eq!(
            manager.alias_map.get("取列或空").map(String::as_str),
            Some("get_opt")
        );

        // 会 panic 的 get 拆成独立词条，只能显式写，且必须标注 + 给备注
        assert_eq!(
            manager.alias_map.get("取列或崩溃").map(String::as_str),
            Some("get")
        );
        let panic_word = manager.danger_of("取列或崩溃");
        assert!(panic_word.level.panic, "取列或崩溃 必须标注会 panic");
        assert!(!panic_word.note.is_empty(), "会 panic 的词条必须有备注说明");
    }

    /// 等级标注解析：三类基础值、多风险连接符与非法值
    #[test]
    fn test_danger_level_parse() {
        assert_eq!(DangerLevel::parse("safe"), Some(DangerLevel::SAFE));
        assert_eq!(DangerLevel::parse("panic"), Some(DangerLevel::PANIC));
        assert_eq!(DangerLevel::parse("rewrite"), Some(DangerLevel::REWRITE));
        assert_eq!(
            DangerLevel::parse("panic + rewrite"),
            Some(DangerLevel::PANIC_REWRITE)
        );
        assert_eq!(
            DangerLevel::parse("rewrite|panic"),
            Some(DangerLevel::PANIC_REWRITE)
        );
        assert_eq!(DangerLevel::parse("boom"), None);
        assert_eq!(DangerLevel::PANIC_REWRITE.code(), "panic+rewrite");
        assert!(DangerLevel::SAFE.is_safe());
    }

    /// 英文回落反向索引：英文原词→母语词条；恒等映射/复合值被过滤
    #[test]
    fn test_english_fallback_map() {
        let lang_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
        let manager = MappingManager::load_from_dir(&lang_dir).expect("内置语言包应可加载");
        let fb = manager.english_fallback_map();
        // 关键字/类型/方法常见英文词均能反查到某个母语词
        for en in ["fn", "let", "Vec", "new", "collect"] {
            assert!(fb.contains_key(en), "回落表应含 `{en}`：{fb:?}");
            let native = &fb[en];
            assert_ne!(native, en, "反向索引应过滤恒等条目");
            assert!(
                !native.is_ascii(),
                "zh 回落建议词应为母语（含非 ASCII）：{en}→{native}"
            );
        }
        // 含 `::` 的模块路径值不进入反向索引（非单标识符）
        assert!(
            !fb.keys().any(|k| k.contains("::")),
            "反向索引键应为单标识符"
        );

        // 英文语言包（恒等映射）得到空表 → 回落 lint 自动静默
        let en_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/en");
        let en_manager = MappingManager::load_from_dir(&en_dir).expect("英文包应可加载");
        assert!(
            en_manager.english_fallback_map().is_empty(),
            "英文恒等包回落反向索引应为空"
        );
    }
}
