//! 映射源模块 - 从 TOML 文件载入映射表
//!
//! 提供映射数据的载入和管理，支持按类别组织：
//! - 关键字映射（词法处理阶段）
//! - 标准库映射（语义处理阶段）
//! - 第三方库映射（按需载入，来自第三方库子目录）

use std::collections::HashMap;
use std::fmt::Formatter;
use std::fs;
use std::path::{Path, PathBuf};

use crate::故障::{载入故障, 载入目标};

/// 节组织的映射表：节名 → (母语词 → 英文)
pub type 节表 = HashMap<String, HashMap<String, String>>;

/// 按**书写序**组织的映射表：[(节名, [(母语词, 英文)])]
///
/// 与 [`节表`] 的唯一区别是保留 TOML 里的先后（哈希映射会把它抹掉）：
/// 诊断反查表要靠这个顺序在同一英文值的多个母语同义词里选主词条，
/// 见 [`解析有序映射节`]。
pub type 有序节序列 = Vec<(String, Vec<(String, String)>)>;

/// 危险等级元数据节：词条 → 会恐慌 / 会改写 / 纯安全（不参与转译映射）
pub const 危险等级节: &str = "危险等级";
/// 危险备注元数据节：词条 → 人类可读的风险说明（不参与转译映射）
pub const 危险备注节: &str = "危险备注";

/// 元数据节不进入关键字/别名映射（否则 `"取列" = "会恐慌"` 会被当成词条）
pub fn 是否元数据节(节名参数: &str) -> bool {
    节名参数 == 危险等级节 || 节名参数 == 危险备注节
}

/// 词条危险等级：两类风险可同时存在，均为假即「纯安全」
///
/// - 会恐慌：映射目标在运行时可能崩溃（越界、类型不符、解包、期望等）；
/// - 会改写：词法层任意位置替换，同名自定义标识符也会被改写（关键字/宏类词条）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct 危险等级 {
    /// 会恐慌
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
    /// 会恐慌
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

    /// 解析标注值：`会恐慌` / `会改写` / `纯安全`，多风险用 `+`（也接受 `,` `|`）连接
    pub fn 解析等级(标注文本: &str) -> Option<Self> {
        let mut 等级值 = Self::全安全;
        for 片段 in 标注文本.split(['+', ',', '|']).map(str::trim) {
            if 片段.is_empty() {
                continue;
            }
            match 片段 {
                "panic" => 等级值.会恐慌 = true,
                "rewrite" => 等级值.会改写 = true,
                "safe" => {}
                _ => return None,
            }
        }
        Some(等级值)
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

/// 命中即视为一定会恐慌的映射目标（与接收者无关，如选项/结果的解包）
///
/// 获取 / 移除 / 插入等是否恐慌取决于接收者类型，不在此自动判定，
/// 须由映射表在 `["危险等级"]` 中显式标注（见 docs/third-party-mapping.md）。
pub const 恒恐慌目标表: &[&str] = &[
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

/// 映射目标是否属于「一定恐慌」集合
pub fn 恒恐慌目标(编译目标: &str) -> bool {
    恒恐慌目标表.contains(&编译目标)
}

/// 从已解析的节表中提取危险元数据（`["危险等级"]` + `["危险备注"]`）
///
/// 非法等级值按「会恐慌」处理并记录警告：宁可高估风险，也不能把
/// 会恐慌的词条静默标成纯安全。
pub fn 危险从节表(节表值: &节表) -> HashMap<String, 危险详情> {
    let mut 危险表映射: HashMap<String, 危险详情> = HashMap::new();
    if let Some(等级表) = 节表值.get(危险等级节) {
        for (词条名, 原始文本) in 等级表 {
            let 等级值 = match 危险等级::解析等级(原始文本) {
                Some(已解析等级) => 已解析等级,
                None => {
                    crate::log_warn!(
                        "映射源",
                        "危险等级 [{}] 的值 {:?} 无法识别，按会恐慌处理",
                        词条名,
                        原始文本
                    );
                    危险等级::必恐慌
                }
            };
            危险表映射.entry(词条名.clone()).or_default().等级 = 等级值;
        }
    }
    if let Some(备注表) = 节表值.get(危险备注节) {
        for (词条名, 备注内容) in 备注表 {
            危险表映射.entry(词条名.clone()).or_default().备注 = 备注内容.clone();
        }
    }
    危险表映射
}

/// 解析 TOML 内容中的危险元数据（供载入器与校验工具共用）
pub fn 解析危险节表(
    内容文本: &str,
) -> Result<HashMap<String, 危险详情>, toml::de::Error> {
    let 节表值 = 解析映射节(内容文本)?;
    Ok(危险从节表(&节表值))
}

/// 按**书写顺序**解析「节 → 条目」清单（用向量而非哈希映射）
///
/// 这是映射源 TOML 的唯一解析实现：[`解析映射节`] 就是把这里的结果
/// 收敛成哈希映射（一个文件只解析一遍，不拿顺序的消费者也不会多付代价）。
/// 顺序保得住的原因：自定义访问器直接接收序列化框架按文档顺序投递的键值对，
/// 不把它们塞进哈希映射就丢不了序。注意 TOML 1.x 起默认表按键排序（0.8 走
/// TOML 编辑后端天然保序），故工作区层启用了保序特性，
/// 让 `toml::Value::Table` 同样保持书写序——两条通道顺序因此一致。
/// 诊断反查表需要在「同一英文值有多个母语同义词」时选出唯一胜出者，依据
/// 正是书写序（先写的是主词条，见诊断反查表构建处）。
///
/// 跳过口径与既有实现一致：非表节、非字符串条目记告警后跳过（`RZ_LOG=warn`
/// 可见），只有 TOML 本身语法非法才报错（由调用方按场景本地化错误消息）。
pub fn 解析有序映射节(内容文本: &str) -> Result<有序节序列, toml::de::Error> {
    let 文档: 有序文档 = toml::from_str(内容文本)?;
    Ok(文档
        .0
        .into_iter()
        .filter_map(|(节名, 节体值)| match 节体值 {
            有序节体::非表 => {
                crate::log_warn!("映射源", "节 [{}] 不是表结构，已跳过", 节名);
                None
            }
            有序节体::条目表(条目集) => {
                let mut 保留集: Vec<(String, String)> = Vec::new();
                for (母语词, 条目值) in 条目集 {
                    match 条目值 {
                        toml::Value::String(英文值) => 保留集.push((母语词, 英文值)),
                        _ => crate::log_warn!(
                            "映射源",
                            "节 [{}] 条目 {} 的值不是字符串，已跳过",
                            节名,
                            母语词
                        ),
                    }
                }
                (!保留集.is_empty()).then_some((节名, 保留集))
            }
        })
        .collect())
}

/// 文档本体：顶层一定是表（TOML 文档），收下的节按书写序排列
struct 有序文档(Vec<(String, 有序节体)>);

/// 节内容：表 → 按书写序的 `(母语词, 值)`；其他形态 → 记为「不是表结构」
enum 有序节体 {
    条目表(Vec<(String, toml::Value)>),
    非表,
}

impl<'甲> serde::Deserialize<'甲> for 有序文档 {
    fn deserialize<反序列化目标: serde::Deserializer<'甲>>(
        反序列化输入: 反序列化目标,
    ) -> Result<Self, 反序列化目标::Error> {
        use serde::de::Visitor;

        struct 文档访问器;
        impl<'甲> Visitor<'甲> for 文档访问器 {
            type Value = 有序文档;
            fn expecting(&self, 格式器: &mut Formatter<'_>) -> std::fmt::Result {
                格式器.write_str("映射表节")
            }
            fn visit_map<映射类型: serde::de::MapAccess<'甲>>(
                self,
                mut 访问源: 映射类型,
            ) -> Result<有序文档, 映射类型::Error> {
                let mut 节集 = Vec::new();
                while let Some(节名) = 访问源.next_key::<String>()? {
                    节集.push((节名, 访问源.next_value::<有序节体>()?));
                }
                Ok(有序文档(节集))
            }
        }

        反序列化输入.deserialize_map(文档访问器)
    }
}

impl<'甲> serde::Deserialize<'甲> for 有序节体 {
    fn deserialize<反序列化目标: serde::Deserializer<'甲>>(
        反序列化输入: 反序列化目标,
    ) -> Result<Self, 反序列化目标::Error> {
        use serde::de::Visitor;

        /// 表按书写序收下条目；非表形态（字符串/整数/布尔/数组等）一律记为
        /// 「不是表结构」，由上层告警跳过（这些方法若不实现，序列化框架默认会
        /// 把它们当成类型错误，把「跳过」变成「整个文件解析失败」）
        struct 节体访问器;
        impl<'甲> Visitor<'甲> for 节体访问器 {
            type Value = 有序节体;
            fn expecting(&self, 格式器: &mut Formatter<'_>) -> std::fmt::Result {
                格式器.write_str("母语词 = \"英文原名\" 形式的条目表")
            }
            fn visit_map<映射类型: serde::de::MapAccess<'甲>>(
                self,
                mut 访问源: 映射类型,
            ) -> Result<有序节体, 映射类型::Error> {
                let mut 条目集 = Vec::new();
                while let Some(母语词) = 访问源.next_key::<String>()? {
                    // 叶子值走 TOML 值类型：任何 TOML 类型都不会因此报错
                    条目集.push((母语词, 访问源.next_value::<toml::Value>()?));
                }
                Ok(有序节体::条目表(条目集))
            }
            fn visit_str<Error: serde::de::Error>(self, _值: &str) -> Result<有序节体, Error> {
                Ok(有序节体::非表)
            }
            fn visit_bool<Error: serde::de::Error>(
                self, _值: bool
            ) -> Result<有序节体, Error> {
                Ok(有序节体::非表)
            }
            fn visit_i64<Error: serde::de::Error>(self, _值: i64) -> Result<有序节体, Error> {
                Ok(有序节体::非表)
            }
            fn visit_u64<Error: serde::de::Error>(self, _值: u64) -> Result<有序节体, Error> {
                Ok(有序节体::非表)
            }
            fn visit_f64<Error: serde::de::Error>(self, _值: f64) -> Result<有序节体, Error> {
                Ok(有序节体::非表)
            }
            fn visit_unit<Error: serde::de::Error>(self) -> Result<有序节体, Error> {
                Ok(有序节体::非表)
            }
            fn visit_seq<序列类型: serde::de::SeqAccess<'甲>>(
                self,
                mut 序列源: 序列类型,
            ) -> Result<有序节体, 序列类型::Error> {
                while 序列源.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(有序节体::非表)
            }
        }

        反序列化输入.deserialize_any(节体访问器)
    }
}

/// 从有序视图（[`已解析文档::有序视图`]）里取一节的有序条目（不存在则空）
pub fn 有序节条目<'甲>(
    有序视图值: &'甲 有序节序列,
    节名参数: &str,
) -> &'甲 [(String, String)] {
    有序视图值
        .iter()
        .find(|(节名, _)| 节名 == 节名参数)
        .map(|(_, 条目集)| 条目集.as_slice())
        .unwrap_or_default()
}

/// 是否「母语键」（含非 ASCII 字符）
///
/// `"format" = "fmt"` 这类纯英文修正项不能进诊断反查表（否则译文里会
/// 出现英文值），模块路径与标识符表里混有它们。
pub fn 是母语词条(键名参数: &str) -> bool {
    !键名参数
        .chars()
        .all(|字符值| 字符值.is_ascii_alphanumeric() || 字符值 == '_' || 字符值 == '-')
}

/// 单个映射源文件的**一次解析**结果（载入器专用）
///
/// 载入一个语言包时，同一份 TOML 往往同时需要三种视图：正向映射用的哈希映射
/// 节表、诊断反查候选用的书写序条目、危险元数据。逐条调用
/// [`解析映射节`] / [`解析有序映射节`] /
/// [`解析危险节表`] 会把同一个文件解析三遍（中文语言包 26 个文件，
/// 载入耗时接近翻倍），因此这里解析一次、按需取视图。几个旧函数签名不变，
/// 仍供只要一种视图的调用方（命令行校验工具等）使用。
#[derive(Debug, Clone)]
pub struct 已解析文档 {
    有序节: 有序节序列,
}

impl 已解析文档 {
    /// 解析整个文件（与 [`解析有序映射节`] 同口径：只有 TOML
    /// 本身语法非法才报错，非表节与非字符串条目告警后跳过）
    pub fn 解析文档(内容文本: &str) -> Result<Self, toml::de::Error> {
        Ok(Self {
            有序节: 解析有序映射节(内容文本)?,
        })
    }

    /// 按书写序排列的「节 → 条目」清单
    pub fn 有序视图(&self) -> &有序节序列 {
        &self.有序节
    }

    /// 取一节的有序条目（不存在则为空）
    pub fn 节条目们(&self, 节名参数: &str) -> &[(String, String)] {
        有序节条目(&self.有序节, 节名参数)
    }

    /// 收敛为「节 → (母语词 → 英文)」哈希映射（等价于 [`解析映射节`]）
    pub fn 节表(&self) -> 节表 {
        self.有序节
            .iter()
            .map(|(节名, 条目集)| (节名.clone(), 条目集.iter().cloned().collect()))
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
        模块路径映射: &mut HashMap<String, String>,
        别名映射: &mut HashMap<String, String>,
    ) {
        模块路径映射.extend(self.节条目们("模块路径").iter().cloned());
        别名映射.extend(self.节条目们("标识符").iter().cloned());
    }
}

/// 解析 TOML 内容为“节 → (母语词 → 英文)”二级表（两个载入器共用的单一解析实现）
///
/// 本函数是 [`解析有序映射节`] 的收敛视图（同一遍解析，只是丢掉
/// 顺序），跳过口径与告警文案也完全由那边负责。
pub fn 解析映射节(内容文本: &str) -> Result<节表, toml::de::Error> {
    Ok(解析有序映射节(内容文本)?
        .into_iter()
        .map(|(节名, 条目集)| (节名, 条目集.into_iter().collect()))
        .collect())
}

/// 把节表扁平化为单一映射：按节名升序合并，同名键冲突时胜出者确定
/// （哈希映射遍历顺序随机会导致多次载入结果不一致）
pub fn 摊平节表(节表值: &节表) -> HashMap<String, String> {
    let mut 结果表 = HashMap::new();
    let mut 名称表: Vec<&String> = 节表值.keys().filter(|节名| !是否元数据节(节名)).collect();
    名称表.sort();
    for 节名 in 名称表 {
        for (母语词, 英文名) in &节表值[节名] {
            结果表.insert(母语词.clone(), 英文名.clone());
        }
    }
    结果表
}

/// 解析“模块路径 + 标识符”两节格式的 TOML（stdlib.toml 与 crates/*.toml 通用）
///
/// - `["模块路径"]` 节 → 合并到模块路径映射（如 `"线程" = "std::thread"`）
/// - `["标识符"]` 节 → 合并到标识符别名映射（如 `"字符串" = "String"`）
///
/// 返回原始 TOML 错误，由调用方结合文件路径/数据源上下文转为 [`载入故障`]。
pub fn 合并模块路径与标识符节(
    内容文本: &str,
    模块路径映射: &mut HashMap<String, String>,
    别名映射: &mut HashMap<String, String>,
) -> Result<(), toml::de::Error> {
    let 节表值 = 解析映射节(内容文本)?;
    if let Some(条目集) = 节表值.get("模块路径") {
        模块路径映射.extend(
            条目集
                .iter()
                .map(|(键项, 值项)| (键项.clone(), 值项.clone())),
        );
    }
    if let Some(条目集) = 节表值.get("标识符") {
        别名映射.extend(
            条目集
                .iter()
                .map(|(键项, 值项)| (键项.clone(), 值项.clone())),
        );
    }
    Ok(())
}

/// 把文件名字节转成 UTF-8 字符串（作为映射分类键）
///
/// - 本身是 UTF-8 时直接返回；
/// - 非 UTF-8 字节按常见 CJK 编码依次尝试解码（国标编码 → 日文位移编码 → 大五码 →
///   韩文编码 → 日文扩展编码），首个解码无错误者即为转码结果，
///   把“非 UTF-8 文件名”真正转成 UTF-8（而非丢弃字节）；
/// - 所有编码都失败时回退有损转写（保留可显示部分，不会崩溃）。
///
/// 平台差异：只有 Unix 允许文件名是任意字节，因此才需要字节级转码；
/// Windows 的文件名在系统层面就是 UTF-16，正常文件名（含中文/日文等）
/// `to_str()` 总能成功，不会进入转码分支，也不会被误判成国标编码乱码；
/// 唯一极端情况是文件名字含孤立代理码元（正常工具造不出来），
/// 此时回退为明确的替换符 `�` 而非错误解码。
fn 解码系统文件名(文件名词: &std::ffi::OsStr) -> String {
    if let Some(文本视图) = 文件名词.to_str() {
        return 文本视图.to_string();
    }
    // Unix 下可取原始字节逐编码尝试；其他平台（Windows 等）直接走有损兜底
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let 字节数据 = 文件名词.as_bytes();
        // 按 CJK 使用频度排序：中文国标编码（覆盖国标扩展）优先，日文位移编码次之。
        // 注：部分字节序列在多种编码下都能无错误解码，顺序只是概率取舍，
        // 无法在没有额外元数据时做到 100% 正确。
        for 编码项 in [
            encoding_rs::GB18030,
            encoding_rs::SHIFT_JIS,
            encoding_rs::BIG5,
            encoding_rs::EUC_KR,
            encoding_rs::EUC_JP,
        ] {
            let (解码结果, _, 有错误) = 编码项.decode(字节数据);
            if !有错误 {
                return 解码结果.into_owned();
            }
        }
    }
    文件名词.to_string_lossy().into_owned()
}

/// 映射表分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum 映射类别 {
    /// 关键字（词法处理阶段）
    关键字类,
    /// 标准库（语义处理阶段）
    标准库类,
    /// 第三方库（按需载入）
    三方库类,
}

impl 映射类别 {
    /// 获取分类对应的默认文件名
    pub fn 默认文件名(&self) -> &'static str {
        match self {
            映射类别::关键字类 => "keywords.toml",
            映射类别::标准库类 => "stdlib.toml",
            映射类别::三方库类 => "crates.toml",
        }
    }

    /// 获取分类的显示名称（随当前语言变化）
    pub fn 显示名(&self) -> String {
        let 消息键 = match self {
            映射类别::关键字类 => "mapping_cat_keywords",
            映射类别::标准库类 => "mapping_cat_stdlib",
            映射类别::三方库类 => "mapping_cat_third_party",
        };
        crate::语言::取消息(消息键)
    }
}

/// 映射表载入器 - 从 TOML 文件载入映射数据
///
/// 从语言包目录中按分类载入映射表，支持关键字/标准库单文件载入
/// 以及第三方库目录下多文件合并载入。
#[derive(Debug, Clone)]
pub struct 映射载入器 {
    /// 语言包根目录
    根目录: PathBuf,
    /// 已载入的映射表（按分类和子分类组织）
    映射表们: HashMap<映射类别, HashMap<String, HashMap<String, String>>>,
}

impl 映射载入器 {
    /// 创建新的载入器
    pub fn 新建<PathParams: AsRef<Path>>(语言包路径: PathParams) -> Self {
        let 根目录 = 语言包路径.as_ref().to_path_buf();
        Self {
            根目录,
            映射表们: HashMap::new(),
        }
    }

    /// 载入指定分类的映射表
    pub fn 载入(&mut self, 类别: 映射类别) -> Result<(), 载入故障> {
        // 第三方库是目录，包含多个文件
        if 类别 == 映射类别::三方库类 {
            return self.载入第三方目录();
        }

        let 文件路径 = self.根目录.join(类别.默认文件名());

        if !文件路径.exists() {
            return Err(载入故障::文件缺失 {
                目标: 载入目标::映射表,
                文件路径: format!("{:?}", 文件路径),
            });
        }

        let 内容文本 = fs::read_to_string(&文件路径).map_err(|坏| 载入故障::读取失败 {
            目标: 载入目标::映射表,
            文件路径: None,
            详情: 坏.to_string(),
        })?;

        let 类别映射 = 解析映射节(&内容文本).map_err(|坏| 载入故障::解析失败 {
            目标: 载入目标::映射表,
            文件路径: None,
            详情: 坏.to_string(),
        })?;
        self.映射表们.insert(类别, 类别映射);
        Ok(())
    }

    /// 载入第三方库目录下的所有 TOML 文件
    fn 载入第三方目录(&mut self) -> Result<(), 载入故障> {
        let 目录路径 = self.根目录.join("crates");

        if !目录路径.exists() {
            // 目录不存在，静默返回
            return Ok(());
        }

        let mut 合并表 = HashMap::new();

        // 遍历目录中的所有 .toml 文件（按文件名排序，读取目录顺序未定义，
        // 排序后合并结果与 映射管理器::从目录载入 保持一致且确定）；
        // 读取目录或条目读取失败必须报错（不能静默吞掉，否则映射整体丢失）
        let mut 映射文件表: Vec<PathBuf> = fs::read_dir(&目录路径)
            .map_err(|坏| 载入故障::目录读取失败 {
                详情: 坏.to_string(),
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|坏| 载入故障::目录读取失败 {
                详情: 坏.to_string(),
            })?
            .into_iter()
            .map(|条目结果| 条目结果.path())
            .filter(|路径项| 路径项.extension().and_then(|后缀| 后缀.to_str()) == Some("toml"))
            .collect();
        映射文件表.sort();

        for 文件路径 in 映射文件表 {
            // 获取文件名（不含扩展名）作为分类标识；
            // 非 UTF-8 文件名按常见编码转码为 UTF-8（解码系统文件名），
            // 避免多个文件碰撞合并或分类键变为替换符
            let 分类名 = 文件路径
                .file_stem()
                .map(解码系统文件名)
                .unwrap_or_else(|| 解码系统文件名(文件路径.as_os_str()));

            let 内容文本 =
                fs::read_to_string(&文件路径).map_err(|坏| 载入故障::读取失败 {
                    目标: 载入目标::第三方库,
                    文件路径: Some(format!("{:?}", 文件路径)),
                    详情: 坏.to_string(),
                })?;

            // 将文件中的映射合并，加上文件分类前缀
            let 节表值 = 解析映射节(&内容文本).map_err(|坏| 载入故障::解析失败 {
                目标: 载入目标::第三方库,
                文件路径: Some(format!("{:?}", 文件路径)),
                详情: 坏.to_string(),
            })?;
            // 节名排序后合并，保证 "文件名/子分类" 键的插入顺序确定
            let mut 子节名表: Vec<&String> = 节表值.keys().collect();
            子节名表.sort();
            for 子节名 in 子节名表 {
                let 类别键 = format!("{}/{}", 分类名, 子节名);
                合并表.insert(类别键, 节表值[子节名].clone());
            }
        }

        self.映射表们.insert(映射类别::三方库类, 合并表);
        Ok(())
    }

    /// 载入所有默认映射表
    pub fn 载入全部(&mut self) -> Result<(), 载入故障> {
        self.载入(映射类别::关键字类)?;
        // 标准库可选：缺失时静默跳过（与 映射管理器::从目录载入 行为一致），
        // 存在但载入失败时必须报错
        if self.根目录.join("stdlib.toml").exists() {
            self.载入(映射类别::标准库类)?;
        }
        // module_paths.toml（可选）：模块路径映射并入标准库分类的 ["模块路径"] 子节。
        // 在标准库之后合并且仅补充缺失键，保证标准库的同名映射优先
        // （与 映射管理器 的 module_paths.toml 先、标准库后的覆盖顺序语义一致）
        let 模块路径文件 = self.根目录.join("module_paths.toml");
        if 模块路径文件.exists() {
            let 内容文本 =
                fs::read_to_string(&模块路径文件).map_err(|坏| 载入故障::读取失败 {
                    目标: 载入目标::模块路径表,
                    文件路径: Some(模块路径文件.display().to_string()),
                    详情: 坏.to_string(),
                })?;
            let 节表值 = 解析映射节(&内容文本).map_err(|坏| 载入故障::解析失败 {
                目标: 载入目标::模块路径表,
                文件路径: None,
                详情: 坏.to_string(),
            })?;
            if let Some(条目集) = 节表值.get("模块路径") {
                let 标准库表 = self.映射表们.entry(映射类别::标准库类).or_default();
                let 模块路径节 = 标准库表.entry("模块路径".to_string()).or_default();
                for (键项, 值项) in 条目集 {
                    模块路径节
                        .entry(键项.clone())
                        .or_insert_with(|| 值项.clone());
                }
            }
        }
        // 第三方库可选：目录不存在时静默跳过，存在但载入失败时必须报错
        // （否则映射静默丢失，用户看到的是未翻译的标识符而非错误提示）
        if self.根目录.join("crates").is_dir() {
            self.载入(映射类别::三方库类)?;
        }
        Ok(())
    }

    /// 获取指定分类的完整映射表（扁平化合并所有子分类）
    ///
    /// 子分类按名称排序后合并，保证同名键冲突时的胜出者确定
    /// （哈希映射遍历顺序随机会导致多次载入结果不一致）
    pub fn 取映射(&self, 类别: 映射类别) -> HashMap<String, String> {
        self.映射表们.get(&类别).map(摊平节表).unwrap_or_default()
    }

    /// 获取指定分类和子分类的映射表
    pub fn 取子映射(
        &self,
        类别: 映射类别,
        子类别参数: &str,
    ) -> Option<&HashMap<String, String>> {
        self.映射表们
            .get(&类别)
            .and_then(|类别表| 类别表.get(子类别参数))
    }

    /// 查询单个映射条目（按子分类名升序查找，同名键命中顺序确定）
    pub fn 查询(&self, 类别: 映射类别, 母语词条: &str) -> Option<String> {
        if let Some(类别表) = self.映射表们.get(&类别) {
            let mut 名称表: Vec<&String> = 类别表.keys().collect();
            名称表.sort();
            for 子节名 in 名称表 {
                if let Some(英文名) = 类别表[子节名].get(母语词条) {
                    return Some(英文名.clone());
                }
            }
        }
        None
    }

    /// 反向查询（从英文查中文；按子分类名升序遍历，结果确定）
    pub fn 反查(&self, 类别: 映射类别, 英文值: &str) -> Option<String> {
        if let Some(类别表) = self.映射表们.get(&类别) {
            let mut 名称表: Vec<&String> = 类别表.keys().collect();
            名称表.sort();
            for 子节名 in 名称表 {
                for (母语项, 英文项) in &类别表[子节名] {
                    if 英文项 == 英文值 {
                        return Some(母语项.clone());
                    }
                }
            }
        }
        None
    }

    /// 获取所有子分类名称（排序后返回，输出确定）
    pub fn 取子类别(&self, 类别: 映射类别) -> Vec<String> {
        let mut 名称表 = self
            .映射表们
            .get(&类别)
            .map(|类别表| 类别表.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        名称表.sort();
        名称表
    }

    /// 统计映射条目数（扁平化后）
    pub fn 条目数(&self, 类别: 映射类别) -> usize {
        self.取映射(类别).len()
    }
}

/// 便捷函数：载入关键字映射
pub fn 载入关键字映射<PathParams: AsRef<Path>>(
    语言包路径: PathParams,
) -> Result<HashMap<String, String>, 载入故障> {
    let mut 载入器 = 映射载入器::新建(语言包路径);
    载入器.载入(映射类别::关键字类)?;
    Ok(载入器.取映射(映射类别::关键字类))
}

/// 便捷函数：载入标准库映射
pub fn 载入标准库映射<PathParams: AsRef<Path>>(
    语言包路径: PathParams,
) -> Result<HashMap<String, String>, 载入故障> {
    let mut 载入器 = 映射载入器::新建(语言包路径);
    载入器.载入(映射类别::标准库类)?;
    Ok(载入器.取映射(映射类别::标准库类))
}

/// 便捷函数：载入所有映射
pub fn 载入全部映射<PathParams: AsRef<Path>>(
    语言包路径: PathParams,
) -> Result<HashMap<映射类别, HashMap<String, String>>, 载入故障> {
    let mut 载入器 = 映射载入器::新建(语言包路径);
    载入器.载入全部()?;

    let mut 结果表 = HashMap::new();
    结果表.insert(映射类别::关键字类, 载入器.取映射(映射类别::关键字类));
    结果表.insert(映射类别::标准库类, 载入器.取映射(映射类别::标准库类));
    结果表.insert(映射类别::三方库类, 载入器.取映射(映射类别::三方库类));

    Ok(结果表)
}

/// 创建默认的关键字映射（内置备用）
///
/// 当语言包文件不可用时，使用此内置映射作为回退，
/// 覆盖 Rust 基础语法关键字、类型、错误处理、异步等常用构造。
pub fn 创建内置关键字映射() -> HashMap<String, String> {
    let mut 内置表 = HashMap::new();

    // 声明关键字
    内置表.insert("函数".into(), "fn".into());
    内置表.insert("变量".into(), "let".into());
    内置表.insert("可变".into(), "mut".into());
    内置表.insert("常量".into(), "const".into());
    内置表.insert("结构体".into(), "struct".into());
    内置表.insert("枚举".into(), "enum".into());
    内置表.insert("实现".into(), "impl".into());
    内置表.insert("特征".into(), "trait".into());
    内置表.insert("类型".into(), "type".into());
    内置表.insert("模块".into(), "mod".into());
    内置表.insert("公开".into(), "pub".into());
    内置表.insert("使用".into(), "use".into());
    内置表.insert("作为".into(), "as".into());
    内置表.insert("包".into(), "crate".into());
    内置表.insert("超级".into(), "super".into());
    内置表.insert("外部".into(), "extern".into());
    内置表.insert("静态".into(), "static".into());

    // 控制流
    内置表.insert("如果".into(), "if".into());
    内置表.insert("否则".into(), "else".into());
    内置表.insert("匹配".into(), "match".into());
    内置表.insert("循环".into(), "loop".into());
    内置表.insert("当".into(), "while".into());
    内置表.insert("对于".into(), "for".into());
    内置表.insert("在".into(), "in".into());
    内置表.insert("中断".into(), "break".into());
    内置表.insert("继续".into(), "continue".into());
    内置表.insert("返回".into(), "return".into());

    // 基本类型
    内置表.insert("整数".into(), "i32".into());
    内置表.insert("长整数".into(), "i64".into());
    内置表.insert("浮点数".into(), "f64".into());
    内置表.insert("单精度浮点数".into(), "f32".into());
    内置表.insert("文本".into(), "str".into());
    内置表.insert("布尔".into(), "bool".into());
    内置表.insert("字符".into(), "char".into());
    内置表.insert("字节".into(), "u8".into());

    // 特殊值
    内置表.insert("真".into(), "true".into());
    内置表.insert("假".into(), "false".into());
    内置表.insert("空".into(), "()".into());
    内置表.insert("自我".into(), "self".into());
    内置表.insert("自身".into(), "Self".into());

    // 错误处理
    内置表.insert("结果".into(), "Result".into());
    内置表.insert("选项".into(), "Option".into());
    内置表.insert("有值".into(), "Some".into());
    内置表.insert("无".into(), "None".into());
    内置表.insert("成功".into(), "Ok".into());
    内置表.insert("错误".into(), "Err".into());

    // 内存
    内置表.insert("引用".into(), "&".into());
    内置表.insert("解引用".into(), "*".into());
    内置表.insert("移动".into(), "move".into());
    内置表.insert("盒子".into(), "Box".into());

    // 异步
    内置表.insert("异步".into(), "async".into());
    内置表.insert("等待".into(), "await".into());
    内置表.insert("不安全".into(), "unsafe".into());
    内置表.insert("动态".into(), "dyn".into());

    内置表
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    #[test]
    fn 载入关键字映射用例() {
        // 独立临时目录：固定路径会在并行测试/多次运行间交叉污染
        let 临时对象 = tempfile::tempdir().unwrap();
        let 临时目录 = 临时对象.path();

        // 写入测试映射表
        let 测试内容 = r#"
["声明"]
"函数" = "fn"
"变量" = "let"

["控制流"]
"如果" = "if"
"否则" = "else"
"#;
        fs::write(临时目录.join("keywords.toml"), 测试内容).unwrap();

        // 测试载入
        let mut 载入器 = 映射载入器::新建(临时目录);
        assert!(载入器.载入(映射类别::关键字类).is_ok());

        // 测试查询
        assert_eq!(
            载入器.查询(映射类别::关键字类, "函数"),
            Some("fn".to_string())
        );
        assert_eq!(
            载入器.查询(映射类别::关键字类, "如果"),
            Some("if".to_string())
        );
        assert_eq!(载入器.查询(映射类别::关键字类, "不存在"), None);

        // 测试反向查询
        assert_eq!(
            载入器.反查(映射类别::关键字类, "fn"),
            Some("函数".to_string())
        );

        // 测试子分类
        let 子类别表 = 载入器.取子类别(映射类别::关键字类);
        assert!(子类别表.contains(&"声明".to_string()));
        assert!(子类别表.contains(&"控制流".to_string()));

        // 测试条目数
        assert_eq!(载入器.条目数(映射类别::关键字类), 4);
    }

    #[test]
    fn 取扁平映射用例() {
        let 临时对象 = tempfile::tempdir().unwrap();
        let 临时目录 = 临时对象.path();

        let 测试内容 = r#"
["分类A"]
"甲" = "alpha"
"乙" = "beta"

["分类B"]
"丙" = "gamma"
"#;
        fs::write(临时目录.join("stdlib.toml"), 测试内容).unwrap();

        let mut 载入器 = 映射载入器::新建(临时目录);
        载入器.载入(映射类别::标准库类).unwrap();

        let 映射表 = 载入器.取映射(映射类别::标准库类);
        assert_eq!(映射表.len(), 3);
        assert_eq!(映射表.get("甲"), Some(&"alpha".to_string()));
        assert_eq!(映射表.get("丙"), Some(&"gamma".to_string()));
        // 子分类名排序后返回，输出确定
        assert_eq!(载入器.取子类别(映射类别::标准库类), vec!["分类A", "分类B"]);
    }

    #[test]
    fn 内置关键字映射用例() {
        let 内置表 = 创建内置关键字映射();

        assert_eq!(内置表.get("函数"), Some(&"fn".to_string()));
        assert_eq!(内置表.get("如果"), Some(&"if".to_string()));
        assert_eq!(内置表.get("整数"), Some(&"i32".to_string()));
        assert!(内置表.len() > 30);
    }

    /// 载入全部把 module_paths.toml 并入标准库分类的 ["模块路径"] 子节，
    /// 且同名键标准库映射优先（与 映射管理器::从目录载入 的覆盖顺序语义一致）
    #[test]
    fn 载入全部合并模块路径且标准库优先() {
        let 临时对象 = tempfile::tempdir().unwrap();
        let 临时目录 = 临时对象.path();

        fs::write(
            临时目录.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        // 标准库映射与模块路径文件含同名键，标准库映射必须优先
        fs::write(
            临时目录.join("stdlib.toml"),
            "[\"模块路径\"]\n\"标准库\" = \"std\"\n\"文件系统\" = \"std::fs\"\n",
        )
        .unwrap();
        fs::write(
            临时目录.join("module_paths.toml"),
            "[\"模块路径\"]\n\"文件系统\" = \"fs\"\n\"字符串\" = \"string\"\n",
        )
        .unwrap();

        let mut 载入器 = 映射载入器::新建(临时目录);
        载入器.载入全部().unwrap();

        let 模块路径表 = 载入器.取子映射(映射类别::标准库类, "模块路径").unwrap();
        // 模块路径文件的独有键并入标准库分类
        assert_eq!(模块路径表.get("字符串"), Some(&"string".to_string()));
        // 同名键标准库映射优先（模块路径文件仅补充缺失键）
        assert_eq!(模块路径表.get("文件系统"), Some(&"std::fs".to_string()));
    }

    /// 仅 keywords.toml 时载入全部不应报错（标准库/模块路径/三方库均可选）
    #[test]
    fn 载入全部时标准库可选() {
        let 临时对象 = tempfile::tempdir().unwrap();
        let 临时目录 = 临时对象.path();
        fs::write(
            临时目录.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();

        let mut 载入器 = 映射载入器::新建(临时目录);
        载入器.载入全部().unwrap();
        assert_eq!(
            载入器.查询(映射类别::关键字类, "函数"),
            Some("fn".to_string())
        );
    }

    /// 非标准编码（国标编码）的文件名被正确转码为 UTF-8 分类键，
    /// 而不是被有损替换或与其它文件碰撞
    #[test]
    #[cfg(unix)]
    fn 非标准编码文件名转码用例() {
        use std::os::unix::ffi::OsStringExt;

        let 临时对象 = tempfile::tempdir().unwrap();
        let 临时目录 = 临时对象.path();
        let 第三方目录 = 临时目录.join("crates");
        fs::create_dir_all(&第三方目录).unwrap();
        // keywords.toml 为必需文件
        fs::write(
            临时目录.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();

        // “序列化”三字的国标编码字节（见下方字节向量）
        let 国标文件名 = std::ffi::OsString::from_vec(vec![0xD0, 0xF2, 0xC1, 0xD0, 0xBB, 0xAF]);
        let 文件路径 = 第三方目录.join(国标文件名).with_extension("toml");
        fs::write(&文件路径, "[\"标识符\"]\n\"服务器\" = \"Server\"\n").unwrap();

        // 前置断言：该文件名的字节确实不是合法 UTF-8
        assert!(文件路径.file_name().unwrap().to_str().is_none());

        let mut 载入器 = 映射载入器::新建(临时目录);
        载入器.载入全部().unwrap();

        // 分类键应为转码后的“序列化/标识符”（国标 18030 解码），映射可正常查询
        let 子映射表 = 载入器
            .取子映射(映射类别::三方库类, "序列化/标识符")
            .unwrap();
        assert_eq!(子映射表.get("服务器"), Some(&"Server".to_string()));
    }

    /// 载入错误分类：文件缺失 / TOML 解析失败；第三方目录不存在静默成功
    #[test]
    fn 载入器错误分支用例() {
        let 临时对象 = tempfile::tempdir().unwrap();
        let mut 载入器 = 映射载入器::新建(临时对象.path());
        // 空目录载入关键字：文件缺失
        assert!(matches!(
            载入器.载入(映射类别::关键字类),
            Err(载入故障::文件缺失 { .. })
        ));
        // 第三方目录不存在：静默成功
        载入器.载入(映射类别::三方库类).unwrap();
        assert_eq!(载入器.条目数(映射类别::三方库类), 0);
        // 未载入的分类查询全为无 / 空
        assert_eq!(载入器.取子类别(映射类别::标准库类), Vec::<String>::new());
        assert!(载入器.取子映射(映射类别::标准库类, "标识符").is_none());
        assert_eq!(载入器.查询(映射类别::标准库类, "x"), None);
        assert_eq!(载入器.反查(映射类别::标准库类, "x"), None);

        // 损坏 TOML：解析失败
        fs::write(
            临时对象.path().join("keywords.toml"),
            "[\"声明\"]\n\"a\"=\"1\"\n\"a\"=\"2\"\n",
        )
        .unwrap();
        assert!(matches!(
            载入器.载入(映射类别::关键字类),
            Err(载入故障::解析失败 { .. })
        ));
        // 解析映射节直接校验同样报错
        assert!(解析映射节("not = = toml").is_err());
    }

    /// 第三方目录：多文件、多子节按 `文件名/节名` 归类，非 TOML 忽略，
    /// 扁平化查询/反查可用
    #[test]
    fn 载入三方库多文件用例() {
        let 临时对象 = tempfile::tempdir().unwrap();
        let 目录 = 临时对象.path();
        fs::write(
            目录.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        let 第三方目录 = 目录.join("crates");
        fs::create_dir_all(&第三方目录).unwrap();
        fs::write(
            第三方目录.join("serde.toml"),
            "[\"模块路径\"]\n\"序列化\" = \"serde\"\n[\"标识符\"]\n\"序列\" = \"Serialize\"\n",
        )
        .unwrap();
        fs::write(
            第三方目录.join("tokio.toml"),
            "[\"模块路径\"]\n\"异步\" = \"tokio\"\n",
        )
        .unwrap();
        // 非 TOML 文件必须忽略
        fs::write(第三方目录.join("README.md"), "ignore me").unwrap();

        let mut 载入器 = 映射载入器::新建(目录);
        载入器.载入(映射类别::三方库类).unwrap();

        let mut 子类别表 = 载入器.取子类别(映射类别::三方库类);
        子类别表.sort();
        assert_eq!(
            子类别表,
            vec![
                "serde/标识符".to_string(),
                "serde/模块路径".to_string(),
                "tokio/模块路径".to_string(),
            ]
        );
        assert_eq!(
            载入器.查询(映射类别::三方库类, "序列"),
            Some("Serialize".to_string())
        );
        assert_eq!(
            载入器.反查(映射类别::三方库类, "tokio"),
            Some("异步".to_string())
        );
        // 扁平化后共 3 条
        assert_eq!(载入器.条目数(映射类别::三方库类), 3);
    }

    /// 载入全部全量链路：标准库存在则载入；模块路径文件仅补充标准库映射
    /// 缺失的键（标准库优先）；三方库目录一并载入；
    /// 载入全部映射便捷函数返回三个分类
    #[test]
    fn 载入全部合并模块路径优先用例() {
        let 临时对象 = tempfile::tempdir().unwrap();
        let 目录 = 临时对象.path();
        fs::write(
            目录.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        fs::write(
            目录.join("stdlib.toml"),
            "[\"标识符\"]\n\"字符串\" = \"String\"\n[\"模块路径\"]\n\"格式化\" = \"fmt\"\n",
        )
        .unwrap();
        // 模块路径文件：与标准库映射同键（标准库映射胜）+ 独有键（补入）
        fs::write(
            目录.join("module_paths.toml"),
            "[\"模块路径\"]\n\"格式化\" = \"SHOULD_NOT_WIN\"\n\"输入输出\" = \"io\"\n",
        )
        .unwrap();
        let 第三方目录 = 目录.join("crates");
        fs::create_dir_all(&第三方目录).unwrap();
        fs::write(
            第三方目录.join("serde.toml"),
            "[\"标识符\"]\n\"序列\" = \"Serialize\"\n",
        )
        .unwrap();

        let 全部表 = 载入全部映射(目录).expect("全量载入应成功");
        assert_eq!(全部表.len(), 3, "三个分类都应存在");
        let mut 载入器 = 映射载入器::新建(目录);
        载入器.载入全部().unwrap();
        let 模块路径表 = 载入器.取子映射(映射类别::标准库类, "模块路径").unwrap();
        assert_eq!(
            模块路径表.get("格式化"),
            Some(&"fmt".to_string()),
            "标准库映射同键优先"
        );
        assert_eq!(
            模块路径表.get("输入输出"),
            Some(&"io".to_string()),
            "独有键应补入"
        );
        assert_eq!(
            载入器.查询(映射类别::标准库类, "字符串"),
            Some("String".to_string())
        );
        assert_eq!(载入器.条目数(映射类别::三方库类), 1);

        // 便捷函数
        assert_eq!(
            载入标准库映射(目录).unwrap().get("字符串"),
            Some(&"String".to_string())
        );
    }

    /// 分类元信息：默认文件名稳定，显示名非空且随分类变化
    #[test]
    fn 分类元信息用例() {
        assert_eq!(映射类别::关键字类.默认文件名(), "keywords.toml");
        assert_eq!(映射类别::标准库类.默认文件名(), "stdlib.toml");
        assert_eq!(映射类别::三方库类.默认文件名(), "crates.toml");
        let 名称表 = [
            映射类别::关键字类.显示名(),
            映射类别::标准库类.显示名(),
            映射类别::三方库类.显示名(),
        ];
        for 名项 in &名称表 {
            assert!(!名项.is_empty());
        }
        assert_ne!(名称表[0], 名称表[2]);
        // UTF-8 文件名走快速路径原样返回
        let 普通名字 = std::ffi::OsStr::new("普通名字");
        assert_eq!(解码系统文件名(普通名字), "普通名字");
    }

    /// 语言包目录下的映射源文件（根目录 *.toml + crates/*.toml）
    fn 语言包映射文件(目录: &Path) -> Vec<PathBuf> {
        let mut 文件表: Vec<PathBuf> = fs::read_dir(目录)
            .expect("语言包目录应可读")
            .map(|条目结果| 条目结果.unwrap().path())
            .filter(|路径项| 路径项.extension().and_then(|后缀| 后缀.to_str()) == Some("toml"))
            .collect();
        let 第三方目录 = 目录.join("crates");
        if 第三方目录.is_dir() {
            文件表.extend(
                fs::read_dir(&第三方目录)
                    .unwrap()
                    .map(|条目结果| 条目结果.unwrap().path())
                    .filter(|路径项| {
                        路径项.extension().and_then(|后缀| 后缀.to_str()) == Some("toml")
                    }),
            );
        }
        文件表.sort();
        文件表
    }

    /// 一次解析得到的多种视图，必须与「各调一次独立解析」完全一致
    ///
    /// 载入器为了拿到书写序改用 [`已解析文档`]，正向映射与危险元数据的
    /// 口径不能被这次改动带偏，因此逐文件比对两条路径的结果。
    #[test]
    fn 解析文档视图与独立函数一致用例() {
        let 清单目录 = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut 已检查数 = 0;
        for 语言码 in ["zh", "ja", "ar"] {
            for 文件项 in 语言包映射文件(&清单目录.join(format!("lang-packs/{}", 语言码)))
            {
                let 内容文本 = fs::read_to_string(&文件项).unwrap();
                let 文档 = 已解析文档::解析文档(&内容文本).unwrap();
                let 标记名 = 文件项.display().to_string();
                assert_eq!(
                    文档.节表(),
                    解析映射节(&内容文本).unwrap(),
                    "{} 节表不一致",
                    标记名
                );
                assert_eq!(
                    文档.危险表(),
                    解析危险节表(&内容文本).unwrap(),
                    "{} 危险元数据不一致",
                    标记名
                );

                let mut 模块路径表 = HashMap::new();
                let mut 标识符表 = HashMap::new();
                文档.合并模块路径与标识符(&mut 模块路径表, &mut 标识符表);
                let mut 模块路径参照表 = HashMap::new();
                let mut 标识符参照表 = HashMap::new();
                合并模块路径与标识符节(
                    &内容文本,
                    &mut 模块路径参照表,
                    &mut 标识符参照表,
                )
                .unwrap();
                assert_eq!(模块路径表, 模块路径参照表, "{} 模块路径不一致", 标记名);
                assert_eq!(标识符表, 标识符参照表, "{} 标识符不一致", 标记名);
                已检查数 += 1;
            }
        }
        assert!(
            已检查数 >= 15,
            "应比对足够多的语言包文件，实际 {} 个",
            已检查数
        );
    }

    /// 非表节、非字符串条目、空节：两条路径都按同一口径跳过
    #[test]
    fn 解析文档跳过非字符串条目用例() {
        let 刁钻内容 = "\"顶层标量\" = \"不是表\"\n[\"标识符\"]\n\"字符串\" = \"String\"\n\"数字\" = 42\n\"数组\" = [1, 2]\n[\"空节\"]\n";
        let 文档 = 已解析文档::解析文档(刁钻内容).unwrap();
        assert_eq!(文档.节表(), 解析映射节(刁钻内容).unwrap());
        assert_eq!(
            文档
                .节表()
                .get("标识符")
                .unwrap()
                .get("字符串")
                .map(String::as_str),
            Some("String")
        );
        // 跳过的条目不会进表，也不会让整份文件解析失败
        assert!(!文档.节表().contains_key("顶层标量"));
        assert!(!文档.节表().contains_key("空节"));
        assert_eq!(文档.节条目们("标识符").len(), 1, "只收字符串值的条目");
        assert!(解析危险节表(刁钻内容).is_ok());
    }
}
