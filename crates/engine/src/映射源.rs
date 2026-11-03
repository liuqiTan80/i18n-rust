//! 映射源模块 - 从 TOML 文件加载映射表
//!
//! 提供映射数据的加载和管理，支持按类别组织：
//! - 关键字映射（词法处理阶段）
//! - 标准库映射（语义处理阶段）
//! - 第三方库映射（按需加载，来自 crates/ 子目录）

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::故障::{载入故障, 载入目标};

/// 节（section）组织的映射表：节名 → (母语词 → 英文)
pub type 节表 = HashMap<String, HashMap<String, String>>;

/// 按**书写序**组织的映射表：[(节名, [(母语词, 英文)])]
///
/// 与 [`节表`] 的唯一区别是保留 TOML 里的先后（HashMap 会把它抹掉）：
/// 诊断反查表要靠这个顺序在同一英文值的多个母语同义词里选主词条，
/// 见 [`parse_toml_sections_ordered`]。
pub type OrderedSections = Vec<(String, Vec<(String, String)>)>;

/// 危险等级元数据节：词条 → panic / rewrite / safe（不参与转译映射）
pub const DANGER_LEVEL_SECTION: &str = "危险等级";
/// 危险备注元数据节：词条 → 人类可读的风险说明（不参与转译映射）
pub const DANGER_NOTE_SECTION: &str = "危险备注";

/// 元数据节不进入关键字/别名映射（否则 `"取列" = "panic"` 会被当成词条）
pub fn is_metadata_section(name: &str) -> bool {
    name == DANGER_LEVEL_SECTION || name == DANGER_NOTE_SECTION
}

/// 词条危险等级：两类风险可同时存在，均为 false 即「纯安全」
///
/// - `panic`：映射目标在运行时可能 panic（越界、类型不符、unwrap/expect 等）；
/// - `rewrite`：词法层任意位置替换，同名自定义标识符也会被改写（关键字/宏类词条）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct 危险等级 {
    /// 会 panic
    pub 会恐慌: bool,
    /// 会改写标识符
    pub 会改写: bool,
}

impl 危险等级 {
    /// 纯安全
    pub const 全安全: Self = Self {
        会恐慌: false,
        会改写: false,
    };
    /// 会 panic
    pub const 必恐慌: Self = Self {
        会恐慌: true,
        会改写: false,
    };
    /// 会改写标识符
    pub const 必改写: Self = Self {
        会恐慌: false,
        会改写: true,
    };
    /// 两种风险并存
    pub const 恐慌加改写: Self = Self {
        会恐慌: true,
        会改写: true,
    };

    /// 解析标注值：`panic` / `rewrite` / `safe`，多风险用 `+`（也接受 `,` `|`）连接
    pub fn 解析等级(text: &str) -> Option<Self> {
        let mut level = Self::全安全;
        for part in text.split(['+', ',', '|']).map(str::trim) {
            if part.is_empty() {
                continue;
            }
            match part {
                "panic" => level.会恐慌 = true,
                "rewrite" => level.会改写 = true,
                "safe" => {}
                _ => return None,
            }
        }
        Some(level)
    }

    /// 是否纯安全
    pub fn 是安全(self) -> bool {
        self == Self::全安全
    }

    /// 稳定标识（Markdown/校验输出用）
    pub fn 代号(self) -> &'static str {
        match (self.会恐慌, self.会改写) {
            (false, false) => "safe",
            (true, false) => "panic",
            (false, true) => "rewrite",
            (true, true) => "panic+rewrite",
        }
    }

    /// 词条表显示标签（与表头图例一致）
    pub fn 标签(self) -> &'static str {
        match (self.会恐慌, self.会改写) {
            (false, false) => "纯安全",
            (true, false) => "会 panic",
            (false, true) => "会改写标识符",
            (true, true) => "会 panic + 会改写标识符",
        }
    }
}

/// 词条危险信息：等级 + 可选备注（多风险时必须写明）
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct 危险详情 {
    /// 危险等级
    pub 等级: 危险等级,
    /// 备注（空串表示无额外说明）
    pub 备注: String,
}

/// 命中即视为一定会 panic 的映射目标（与接收者无关，如 Option/Result 的解包）
///
/// `get` / `remove` / `insert` 等是否 panic 取决于接收者类型，不在此自动判定，
/// 须由映射表在 `["危险等级"]` 中显式标注（见 docs/third-party-mapping.md）。
pub const ALWAYS_PANIC_TARGETS: &[&str] = &[
    "unwrap",
    "expect",
    "unwrap_err",
    "expect_err",
    "get_unwrap",
    "get_ref_unwrap",
    "panic",
    "todo",
    "unimplemented",
    "unreachable",
];

/// 映射目标是否属于「一定 panic」集合
pub fn 恒恐慌目标(target: &str) -> bool {
    ALWAYS_PANIC_TARGETS.contains(&target)
}

/// 从已解析的节表中提取危险元数据（`["危险等级"]` + `["危险备注"]`）
///
/// 非法等级值按「会 panic」处理并记录警告：宁可高估风险，也不能把
/// 会 panic 的词条静默标成纯安全。
pub fn 危险从节表(sections: &节表) -> HashMap<String, 危险详情> {
    let mut result: HashMap<String, 危险详情> = HashMap::new();
    if let Some(levels) = sections.get(DANGER_LEVEL_SECTION) {
        for (word, raw) in levels {
            let level = match 危险等级::解析等级(raw) {
                Some(level) => level,
                None => {
                    crate::log_warn!(
                        "映射源",
                        "危险等级 [{}] 的值 {:?} 无法识别，按会 panic 处理",
                        word,
                        raw
                    );
                    危险等级::必恐慌
                }
            };
            result.entry(word.clone()).or_default().等级 = level;
        }
    }
    if let Some(notes) = sections.get(DANGER_NOTE_SECTION) {
        for (word, note) in notes {
            result.entry(word.clone()).or_default().备注 = note.clone();
        }
    }
    result
}

/// 解析 TOML 内容中的危险元数据（供加载器与校验工具共用）
pub fn 解析危险节表(content: &str) -> Result<HashMap<String, 危险详情>, toml::de::Error> {
    let sections = parse_toml_sections(content)?;
    Ok(危险从节表(&sections))
}

/// 按**书写顺序**解析「节 → 条目」清单（`Vec` 而非 HashMap）
///
/// 这是映射源 TOML 的唯一解析实现：[`parse_toml_sections`] 就是把这里的结果
/// 收敛成 HashMap（一个文件只解析一遍，不拿顺序的消费者也不会多付代价）。
/// 顺序保得住的原因：自定义 visitor 直接接收 serde 通道按文档顺序投递的键值对，
/// 不把它们塞进 HashMap 就丢不了序。注意 toml 1.x 起默认表按键排序（0.8 走
/// toml_edit 后端天然保序），故 workspace 层启用了 `preserve_order` 特性，
/// 让 `toml::Value::Table` 同样保持书写序——两条通道顺序因此一致。
/// 诊断反查表需要在「同一英文值有多个母语同义词」时选出唯一胜出者，依据
/// 正是书写序（先写的是主词条，见 `crate::diagnostic::build_type_map`）。
///
/// 跳过口径与既有实现一致：非表节、非字符串条目记告警后跳过（RZ_LOG=warn
/// 可见），只有 TOML 本身语法非法才报错（由调用方按场景本地化错误消息）。
pub fn parse_toml_sections_ordered(content: &str) -> Result<OrderedSections, toml::de::Error> {
    let document: OrderedDocument = toml::from_str(content)?;
    Ok(document
        .0
        .into_iter()
        .filter_map(|(name, body)| match body {
            OrderedBody::NotATable => {
                crate::log_warn!("映射源", "节 [{}] 不是表结构，已跳过", name);
                None
            }
            OrderedBody::Entries(entries) => {
                let mut kept: Vec<(String, String)> = Vec::new();
                for (native, value) in entries {
                    match value {
                        toml::Value::String(english) => kept.push((native, english)),
                        _ => crate::log_warn!(
                            "映射源",
                            "节 [{}] 条目 {} 的值不是字符串，已跳过",
                            name,
                            native
                        ),
                    }
                }
                (!kept.is_empty()).then_some((name, kept))
            }
        })
        .collect())
}

/// 文档本体：顶层一定是表（TOML 文档），收下的节按书写序排列
struct OrderedDocument(Vec<(String, OrderedBody)>);

/// 节内容：表 → 按书写序的 `(母语词, 值)`；其他形态 → 记为「不是表结构」
enum OrderedBody {
    Entries(Vec<(String, toml::Value)>),
    NotATable,
}

impl<'de> serde::Deserialize<'de> for OrderedDocument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Visitor;

        struct DocumentVisitor;
        impl<'de> Visitor<'de> for DocumentVisitor {
            type Value = OrderedDocument;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("映射表节")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<OrderedDocument, A::Error> {
                let mut sections = Vec::new();
                while let Some(name) = map.next_key::<String>()? {
                    sections.push((name, map.next_value::<OrderedBody>()?));
                }
                Ok(OrderedDocument(sections))
            }
        }

        deserializer.deserialize_map(DocumentVisitor)
    }
}

impl<'de> serde::Deserialize<'de> for OrderedBody {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Visitor;

        /// 表按书写序收下条目；非表形态（字符串/整数/布尔/数组等）一律记为
        /// 「不是表结构」，由上层告警跳过（这些方法若不实现，serde 默认会
        /// 把它们当成类型错误，把「跳过」变成「整个文件解析失败」）
        struct BodyVisitor;
        impl<'de> Visitor<'de> for BodyVisitor {
            type Value = OrderedBody;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("母语词 = \"英文原名\" 形式的条目表")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<OrderedBody, A::Error> {
                let mut entries = Vec::new();
                while let Some(native) = map.next_key::<String>()? {
                    // 叶子值走 toml::Value：任何 TOML 类型都不会因此报错
                    entries.push((native, map.next_value::<toml::Value>()?));
                }
                Ok(OrderedBody::Entries(entries))
            }
            fn visit_str<E: serde::de::Error>(self, _v: &str) -> Result<OrderedBody, E> {
                Ok(OrderedBody::NotATable)
            }
            fn visit_bool<E: serde::de::Error>(self, _v: bool) -> Result<OrderedBody, E> {
                Ok(OrderedBody::NotATable)
            }
            fn visit_i64<E: serde::de::Error>(self, _v: i64) -> Result<OrderedBody, E> {
                Ok(OrderedBody::NotATable)
            }
            fn visit_u64<E: serde::de::Error>(self, _v: u64) -> Result<OrderedBody, E> {
                Ok(OrderedBody::NotATable)
            }
            fn visit_f64<E: serde::de::Error>(self, _v: f64) -> Result<OrderedBody, E> {
                Ok(OrderedBody::NotATable)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<OrderedBody, E> {
                Ok(OrderedBody::NotATable)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<OrderedBody, A::Error> {
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(OrderedBody::NotATable)
            }
        }

        deserializer.deserialize_any(BodyVisitor)
    }
}

/// 从有序视图（[`已解析文档::ordered`]）里取一节的有序条目（不存在则空）
pub fn ordered_section_entries<'a>(
    ordered: &'a OrderedSections,
    section_name: &str,
) -> &'a [(String, String)] {
    ordered
        .iter()
        .find(|(name, _)| name == section_name)
        .map(|(_, entries)| entries.as_slice())
        .unwrap_or_default()
}

/// 是否「母语键」（含非 ASCII 字符）
///
/// `"format" = "fmt"` 这类纯英文修正项不能进诊断反查表（否则译文里会
/// 出现英文值），模块路径与标识符表里混有它们。
pub fn 是母语词条(key: &str) -> bool {
    !key.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// 单个映射源文件的**一次解析**结果（加载器专用）
///
/// 加载一个语言包时，同一份 TOML 往往同时需要三种视图：正向映射用的 HashMap
/// 节表、诊断反查候选用的书写序条目、危险元数据。逐条调用
/// [`parse_toml_sections`] / [`parse_toml_sections_ordered`] /
/// [`解析危险节表`] 会把同一个文件解析三遍（zh 包 26 个文件，
/// 加载耗时接近翻倍），因此这里解析一次、按需取视图。几个旧函数签名不变，
/// 仍供只要一种视图的调用方（CLI 校验工具等）使用。
#[derive(Debug, Clone)]
pub struct 已解析文档 {
    ordered: OrderedSections,
}

impl 已解析文档 {
    /// 解析整个文件（与 [`parse_toml_sections_ordered`] 同口径：只有 TOML
    /// 本身语法非法才报错，非表节与非字符串条目告警后跳过）
    pub fn 解析文档(content: &str) -> Result<Self, toml::de::Error> {
        Ok(Self {
            ordered: parse_toml_sections_ordered(content)?,
        })
    }

    /// 按书写序排列的「节 → 条目」清单
    pub fn ordered(&self) -> &OrderedSections {
        &self.ordered
    }

    /// 取一节的有序条目（不存在则为空）
    pub fn 节条目们(&self, section_name: &str) -> &[(String, String)] {
        ordered_section_entries(&self.ordered, section_name)
    }

    /// 收敛为「节 → (母语词 → 英文)」HashMap（等价于 [`parse_toml_sections`]）
    pub fn 节表(&self) -> 节表 {
        self.ordered
            .iter()
            .map(|(name, entries)| (name.clone(), entries.iter().cloned().collect()))
            .collect()
    }

    /// 危险元数据（`["危险等级"]` / `["危险备注"]` 两节，等价于
    /// [`解析危险节表`]；语法已在本文件解析成功，这里不会再失败）
    pub fn 危险表(&self) -> HashMap<String, 危险详情> {
        危险从节表(&self.节表())
    }

    /// 把 `["模块路径"]` / `["标识符"]` 两节并入两张正向映射表（等价于
    /// [`合并模块路径与标识符节`]，但不重复解析）
    pub fn 合并模块路径与标识符(
        &self,
        module_path_map: &mut HashMap<String, String>,
        alias_map: &mut HashMap<String, String>,
    ) {
        module_path_map.extend(self.节条目们("模块路径").iter().cloned());
        alias_map.extend(self.节条目们("标识符").iter().cloned());
    }
}

/// 解析 TOML 内容为“节 → (母语词 → 英文)”二级表（两个加载器共用的单一解析实现）
///
/// 本函数是 [`parse_toml_sections_ordered`] 的收敛视图（同一遍解析，只是丢掉
/// 顺序），跳过口径与告警文案也完全由那边负责。
pub fn parse_toml_sections(content: &str) -> Result<节表, toml::de::Error> {
    Ok(parse_toml_sections_ordered(content)?
        .into_iter()
        .map(|(name, entries)| (name, entries.into_iter().collect()))
        .collect())
}

/// 把节表扁平化为单一映射：按节名升序合并，同名键冲突时胜出者确定
/// （HashMap 遍历顺序随机会导致多次加载结果不一致）
pub fn 摊平节表(sections: &节表) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let mut names: Vec<&String> = sections
        .keys()
        .filter(|name| !is_metadata_section(name))
        .collect();
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
/// 返回原始 toml 错误，由调用方结合文件路径/数据源上下文转为 [`载入故障`]。
pub fn 合并模块路径与标识符节(
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
    pub fn load(&mut self, category: MappingCategory) -> Result<(), 载入故障> {
        // 第三方库是目录，包含多个文件
        if category == MappingCategory::ThirdParty {
            return self.load_third_party_dir();
        }

        let file_path = self.root_dir.join(category.default_filename());

        if !file_path.exists() {
            return Err(载入故障::文件缺失 {
                目标: 载入目标::映射表,
                文件路径: format!("{:?}", file_path),
            });
        }

        let content = fs::read_to_string(&file_path).map_err(|e| 载入故障::读取失败 {
            目标: 载入目标::映射表,
            文件路径: None,
            详情: e.to_string(),
        })?;

        let category_map =
            parse_toml_sections(&content).map_err(|e| 载入故障::解析失败 {
                目标: 载入目标::映射表,
                文件路径: None,
                详情: e.to_string(),
            })?;
        self.mappings.insert(category, category_map);
        Ok(())
    }

    /// 加载第三方库目录（crates/）下的所有 TOML 文件
    fn load_third_party_dir(&mut self) -> Result<(), 载入故障> {
        let dir_path = self.root_dir.join("crates");

        if !dir_path.exists() {
            // 目录不存在，静默返回
            return Ok(());
        }

        let mut merged_map = HashMap::new();

        // 遍历目录中的所有 .toml 文件（按文件名排序，read_dir 顺序未定义，
        // 排序后合并结果与 映射管理器::load_from_dir 保持一致且确定）；
        // read_dir 或条目读取失败必须报错（不能静默吞掉，否则映射整体丢失）
        let mut toml_files: Vec<PathBuf> = fs::read_dir(&dir_path)
            .map_err(|e| 载入故障::目录读取失败 {
                详情: e.to_string(),
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| 载入故障::目录读取失败 {
                详情: e.to_string(),
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

            let content = fs::read_to_string(&path).map_err(|e| 载入故障::读取失败 {
                目标: 载入目标::第三方库,
                文件路径: Some(format!("{:?}", path)),
                详情: e.to_string(),
            })?;

            // 将文件中的映射合并，加上文件分类前缀
            let sections =
                parse_toml_sections(&content).map_err(|e| 载入故障::解析失败 {
                    目标: 载入目标::第三方库,
                    文件路径: Some(format!("{:?}", path)),
                    详情: e.to_string(),
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
    pub fn load_all(&mut self) -> Result<(), 载入故障> {
        self.load(MappingCategory::Keywords)?;
        // 标准库可选：缺失时静默跳过（与 映射管理器::load_from_dir 行为一致），
        // 存在但加载失败时必须报错
        if self.root_dir.join("stdlib.toml").exists() {
            self.load(MappingCategory::StdLib)?;
        }
        // module_paths.toml（可选）：模块路径映射并入标准库分类的 ["模块路径"] 子节。
        // 在 stdlib 之后合并且仅补充缺失键，保证 stdlib 的同名映射优先
        // （与 映射管理器 的 module_paths 先、stdlib 后的覆盖顺序语义一致）
        let module_paths_file = self.root_dir.join("module_paths.toml");
        if module_paths_file.exists() {
            let content =
                fs::read_to_string(&module_paths_file).map_err(|e| 载入故障::读取失败 {
                    目标: 载入目标::模块路径表,
                    文件路径: Some(module_paths_file.display().to_string()),
                    详情: e.to_string(),
                })?;
            let sections =
                parse_toml_sections(&content).map_err(|e| 载入故障::解析失败 {
                    目标: 载入目标::模块路径表,
                    文件路径: None,
                    详情: e.to_string(),
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
            .map(摊平节表)
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
) -> Result<HashMap<String, String>, 载入故障> {
    let mut loader = MappingLoader::new(lang_pack_path);
    loader.load(MappingCategory::Keywords)?;
    Ok(loader.get_mapping(MappingCategory::Keywords))
}

/// 便捷函数：加载标准库映射
pub fn load_stdlib_mapping<P: AsRef<Path>>(
    lang_pack_path: P,
) -> Result<HashMap<String, String>, 载入故障> {
    let mut loader = MappingLoader::new(lang_pack_path);
    loader.load(MappingCategory::StdLib)?;
    Ok(loader.get_mapping(MappingCategory::StdLib))
}

/// 便捷函数：加载所有映射
pub fn load_all_mappings<P: AsRef<Path>>(
    lang_pack_path: P,
) -> Result<HashMap<MappingCategory, HashMap<String, String>>, 载入故障> {
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
    map.insert("有值".into(), "Some".into());
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
    /// 且同名键 stdlib 优先（与 映射管理器::load_from_dir 的覆盖顺序语义一致）
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

    /// 加载错误分类：文件缺失 / TOML 解析失败；第三方目录不存在静默成功
    #[test]
    fn test_loader_error_branches() {
        let temp = tempfile::tempdir().unwrap();
        let mut loader = MappingLoader::new(temp.path());
        // 空目录加载关键字：FileMissing
        assert!(matches!(
            loader.load(MappingCategory::Keywords),
            Err(载入故障::文件缺失 { .. })
        ));
        // crates/ 目录不存在：静默 Ok
        loader.load(MappingCategory::ThirdParty).unwrap();
        assert_eq!(loader.entry_count(MappingCategory::ThirdParty), 0);
        // 未加载的分类查询全为 None / 空
        assert_eq!(
            loader.get_sub_categories(MappingCategory::StdLib),
            Vec::<String>::new()
        );
        assert!(
            loader
                .get_sub_mapping(MappingCategory::StdLib, "标识符")
                .is_none()
        );
        assert_eq!(loader.query(MappingCategory::StdLib, "x"), None);
        assert_eq!(loader.reverse_query(MappingCategory::StdLib, "x"), None);

        // 损坏 TOML：ParseFailed
        fs::write(
            temp.path().join("keywords.toml"),
            "[\"声明\"]\n\"a\"=\"1\"\n\"a\"=\"2\"\n",
        )
        .unwrap();
        assert!(matches!(
            loader.load(MappingCategory::Keywords),
            Err(载入故障::解析失败 { .. })
        ));
        // parse_toml_sections 直接校验同样报错
        assert!(parse_toml_sections("not = = toml").is_err());
    }

    /// 第三方目录：多文件、多子节按 `文件名/节名` 归类，非 toml 忽略，
    /// 扁平化查询/反查可用
    #[test]
    fn test_load_third_party_multifile() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        fs::write(dir.join("keywords.toml"), "[\"声明\"]\n\"函数\" = \"fn\"\n").unwrap();
        let crates = dir.join("crates");
        fs::create_dir_all(&crates).unwrap();
        fs::write(
            crates.join("serde.toml"),
            "[\"模块路径\"]\n\"序列化\" = \"serde\"\n[\"标识符\"]\n\"序列\" = \"Serialize\"\n",
        )
        .unwrap();
        fs::write(
            crates.join("tokio.toml"),
            "[\"模块路径\"]\n\"异步\" = \"tokio\"\n",
        )
        .unwrap();
        // 非 toml 文件必须忽略
        fs::write(crates.join("README.md"), "ignore me").unwrap();

        let mut loader = MappingLoader::new(dir);
        loader.load(MappingCategory::ThirdParty).unwrap();

        let mut subs = loader.get_sub_categories(MappingCategory::ThirdParty);
        subs.sort();
        assert_eq!(
            subs,
            vec![
                "serde/标识符".to_string(),
                "serde/模块路径".to_string(),
                "tokio/模块路径".to_string(),
            ]
        );
        assert_eq!(
            loader.query(MappingCategory::ThirdParty, "序列"),
            Some("Serialize".to_string())
        );
        assert_eq!(
            loader.reverse_query(MappingCategory::ThirdParty, "tokio"),
            Some("异步".to_string())
        );
        // 扁平化后共 3 条
        assert_eq!(loader.entry_count(MappingCategory::ThirdParty), 3);
    }

    /// load_all 全量链路：stdlib 存在则加载；module_paths 仅补充 stdlib
    /// 缺失的键（stdlib 优先）；crates 目录一并加载；
    /// load_all_mappings 便捷函数返回三个分类
    #[test]
    fn test_load_all_with_module_paths_merge_priority() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        fs::write(dir.join("keywords.toml"), "[\"声明\"]\n\"函数\" = \"fn\"\n").unwrap();
        fs::write(
            dir.join("stdlib.toml"),
            "[\"标识符\"]\n\"字符串\" = \"String\"\n[\"模块路径\"]\n\"格式化\" = \"fmt\"\n",
        )
        .unwrap();
        // module_paths：与 stdlib 同键（stdlib 胜）+ 独有键（补入）
        fs::write(
            dir.join("module_paths.toml"),
            "[\"模块路径\"]\n\"格式化\" = \"SHOULD_NOT_WIN\"\n\"输入输出\" = \"io\"\n",
        )
        .unwrap();
        let crates = dir.join("crates");
        fs::create_dir_all(&crates).unwrap();
        fs::write(
            crates.join("serde.toml"),
            "[\"标识符\"]\n\"序列\" = \"Serialize\"\n",
        )
        .unwrap();

        let all = load_all_mappings(dir).expect("全量加载应成功");
        assert_eq!(all.len(), 3, "三个分类都应存在");
        let mut loader = MappingLoader::new(dir);
        loader.load_all().unwrap();
        let mp = loader
            .get_sub_mapping(MappingCategory::StdLib, "模块路径")
            .unwrap();
        assert_eq!(
            mp.get("格式化"),
            Some(&"fmt".to_string()),
            "stdlib 同键优先"
        );
        assert_eq!(mp.get("输入输出"), Some(&"io".to_string()), "独有键应补入");
        assert_eq!(
            loader.query(MappingCategory::StdLib, "字符串"),
            Some("String".to_string())
        );
        assert_eq!(loader.entry_count(MappingCategory::ThirdParty), 1);

        // 便捷函数
        assert_eq!(
            load_stdlib_mapping(dir).unwrap().get("字符串"),
            Some(&"String".to_string())
        );
    }

    /// 分类元信息：默认文件名稳定，显示名非空且随分类变化
    #[test]
    fn test_category_metadata() {
        assert_eq!(
            MappingCategory::Keywords.default_filename(),
            "keywords.toml"
        );
        assert_eq!(MappingCategory::StdLib.default_filename(), "stdlib.toml");
        assert_eq!(
            MappingCategory::ThirdParty.default_filename(),
            "crates.toml"
        );
        let names = [
            MappingCategory::Keywords.display_name(),
            MappingCategory::StdLib.display_name(),
            MappingCategory::ThirdParty.display_name(),
        ];
        for name in &names {
            assert!(!name.is_empty());
        }
        assert_ne!(names[0], names[2]);
        // UTF-8 文件名走快速路径原样返回
        let name = std::ffi::OsStr::new("普通名字");
        assert_eq!(decode_os_file_name(name), "普通名字");
    }

    /// 语言包目录下的映射源文件（根目录 *.toml + crates/*.toml）
    fn lang_pack_toml_files(dir: &Path) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = fs::read_dir(dir)
            .expect("语言包目录应可读")
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
            .collect();
        let crates = dir.join("crates");
        if crates.is_dir() {
            files.extend(
                fs::read_dir(&crates)
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml")),
            );
        }
        files.sort();
        files
    }

    /// 一次解析得到的多种视图，必须与「各调一次独立解析」完全一致
    ///
    /// 加载器为了拿到书写序改用 [`已解析文档`]，正向映射与危险元数据的
    /// 口径不能被这次改动带偏，因此逐文件比对两条路径的结果。
    #[test]
    fn parsed_document_views_match_standalone_functions() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut checked = 0;
        for lang in ["zh", "ja", "ar"] {
            for file in lang_pack_toml_files(&manifest.join(format!("lang-packs/{lang}"))) {
                let content = fs::read_to_string(&file).unwrap();
                let doc = 已解析文档::解析文档(&content).unwrap();
                let label = file.display().to_string();
                assert_eq!(
                    doc.节表(),
                    parse_toml_sections(&content).unwrap(),
                    "{label} 节表不一致"
                );
                assert_eq!(
                    doc.危险表(),
                    解析危险节表(&content).unwrap(),
                    "{label} 危险元数据不一致"
                );

                let mut paths = HashMap::new();
                let mut idents = HashMap::new();
                doc.合并模块路径与标识符(&mut paths, &mut idents);
                let mut paths_ref = HashMap::new();
                let mut idents_ref = HashMap::new();
                合并模块路径与标识符节(&content, &mut paths_ref, &mut idents_ref).unwrap();
                assert_eq!(paths, paths_ref, "{label} 模块路径不一致");
                assert_eq!(idents, idents_ref, "{label} 标识符不一致");
                checked += 1;
            }
        }
        assert!(checked >= 15, "应比对足够多的语言包文件，实际 {checked} 个");
    }

    /// 非表节、非字符串条目、空节：两条路径都按同一口径跳过
    #[test]
    fn parsed_document_skips_non_string_entries_like_legacy_parser() {
        let tricky = "\"顶层标量\" = \"不是表\"\n[\"标识符\"]\n\"字符串\" = \"String\"\n\"数字\" = 42\n\"数组\" = [1, 2]\n[\"空节\"]\n";
        let doc = 已解析文档::解析文档(tricky).unwrap();
        assert_eq!(doc.节表(), parse_toml_sections(tricky).unwrap());
        assert_eq!(
            doc.节表()
                .get("标识符")
                .unwrap()
                .get("字符串")
                .map(String::as_str),
            Some("String")
        );
        // 跳过的条目不会进表，也不会让整份文件解析失败
        assert!(!doc.节表().contains_key("顶层标量"));
        assert!(!doc.节表().contains_key("空节"));
        assert_eq!(doc.节条目们("标识符").len(), 1, "只收字符串值的条目");
        assert!(解析危险节表(tricky).is_ok());
    }
}
