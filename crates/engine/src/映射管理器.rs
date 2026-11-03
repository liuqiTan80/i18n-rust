//! 映射管理器：把语言包各来源的映射表装配成转译管线要用的那几张表
//!
//! 本模块是「加载器」与「查询面」的边界：上游（CLI / LSP / build.rs）负责把
//! TOML 内容送到门口，下游（词法转译、别名替换、教学校验、诊断反查）只从这里
//! 取表，不碰 TOML。
//!
//! 设计（中文原生重构，不是英文版的逐句对译）：
//! - **装配只有一条路径**。原版「从目录载入」与「从内置载入」两个入口是互为
//!   抄写的两份代码：同样的「节表 → 关键词表 + 派生特征表分离」、同样的双节
//!   合并、同样的十四字段构造，各写一遍；两份之间靠注释承诺「与另一边行为
//!   一致」，而合并顺序其实不同（读盘路三方包表在前、标准库收尾，内置路
//!   标准库在前、三方包表收尾）。同键不同值时两侧胜出的词条不同——开发模式与
//!   发布产物会拿到两张不一样的映射表。现在两个入口只负责「把文档取到手」，
//!   装配走同一个构造器，顺序差异在结构上不可能再出现（见 [`映射管理器::组装`]）。
//!   当前中文包三方（标准库 / 模块路径 / 三方包表）同键不同值的冲突为 0，
//!   所以这是一个**尚未发作的缺陷**；本轮把它修掉并补了对照测试。
//! - **装配不再返回 `结果`**。装配的输入是已解析文档，内部没有任何可失败操作；
//!   原版把 `结果` 一路带到 `从扁表构造`，逼得那个入口用 `断言(假)` 式的分支去
//!   接一个不可能发生的错误。现在可失败的是「读盘 / 解析」，仍归两个入口，
//!   装配直接产出结构体。
//! - **反查候选的来源从固定字段结构体改成有序数组**。原版
//!   `候选阶段` 五个字段，加一类来源要改结构体、改初始化、改
//!   汇总三处，优先序靠「字段声明顺序」这个隐式约定表达；现在是一张
//!   `(书写序条目, 生效正向表)` 数组，下标序即优先序。
//! - **派生缓存合成一组**。原版七个 `一次性锁` 字段平铺在结构体上，每个构造器
//!   都要重写一遍 `xxx_cache: OnceLock::new()`；其中四个（校验词表、分桶索引、
//!   歧义构造器、英文回落）只在教学校验路径用到，现在收进 [`懒算表`] 一个
//!   字段，派生逻辑也搬进去，管理器只留一次转发。
//! - **保留关键字判定改有序静态表 + 二分**。原版是 51 臂 `matches!` 字符串链，
//!   每次判定线性比较；换成 `&[&文本]` 配 `.二分查找()`，既省掉线性扫描，也让
//!   「漏插一个关键字」在码位序上无处藏身。
//! - **循环引用检测按判据去重**。原版对表内互指逐键各报一次，`甲 ↔ 乙` 会产出
//!   两条只差标签顺序的重复项，条目顺序还依赖哈希映射迭代序。现在只记
//!   「键 < 值」的方向、并落进有序集合，同一互指对只出一行且整体升序
//!   （`查循环` 的调用方只做空/非空判定与人工阅读，不依赖原有顺序；这一条
//!   行为差异由测试 `同一互指对只报一行且升序` 钉住）。
//!
//! 方言写法上的几处克制（都是本分支实测出来的，见 docs/zh-findings.md）：
//! - 生命周期位不吃关键字表，静态生命周期只能按英文原名写（F16）；
//! - 属性位要写 `#[派生(调试)]`，写英文 `#[derive(调试)]` 会得到小写形式
//!   `#[derive(debug)]`（本轮实测，台账 F26）；
//! - 声明名一律避开一切词表词形：项目声明豁免按目录聚簇，撞词形会让同目录
//!   所有方言源的该词方法位集体失效（F21）；
//! - `排序` 是歧义词（派生特征节与标识符节换出两个不同的英文词），代码区一律不出现，
//!   需要确定序的场合改用有序集合或有序映射，不写「先收集再排序」。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::故障::{载入故障, 载入目标};
use crate::映射源::{
    危险从节表, 危险等级, 危险详情, 已解析文档, 恒恐慌目标, 摊平节表, 是母语词条, 节表,
};
use crate::校验::词形分桶;
use crate::缓存::转译缓存;

/// 反查候选的一个阶段：按 TOML 书写序的条目 + 该来源对应的**生效**正向表
///
/// 只对「在生效表里确实是这个值」的条目做候选（被后续来源覆盖的旧写法不能再
/// 进诊断），见 [`组装反查候选`]。
type 候选阶段<'甲> = (Vec<(String, String)>, &'甲 HashMap<String, String>);

/// Rust 保留关键字（码位升序）：词法层替换值落在里面时，方法位直译必为非法
/// 产物（`.enum()`），只能让位给别名表
///
/// 「联合」一类弱关键字不在列——它们仍可作方法名，不算冲突。
static 保留关键字: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl", "in",
    "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "Self", "self", "static", "struct", "super", "trait", "true", "try", "type",
    "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

/// 来源类别：同一个来源在「读盘」与「内置」两条路上对应不同的错误标记
enum 源类别 {
    关键词,
    模块路径,
    标准源,
    第三方,
}

impl 源类别 {
    /// 该来源的解析失败该报哪个 [`载入目标`]（内置路径有独立变体）
    fn 载入目标(&self, 内置: bool) -> 载入目标 {
        match (self, 内置) {
            (Self::关键词, false) => 载入目标::关键字表,
            (Self::关键词, true) => 载入目标::内置关键字表,
            (Self::模块路径, false) => 载入目标::模块路径表,
            (Self::模块路径, true) => 载入目标::内置模块路径表,
            (Self::标准源, false) => 载入目标::标准库表,
            (Self::标准源, true) => 载入目标::内置标准库表,
            (Self::第三方, _) => 载入目标::第三方库,
        }
    }
}

/// 只在首次需要时派生、之后随管理器一起复用的四张教学校验用表
///
/// 语言包装配完就不再变，这四张表又都不在转译热路径上（只有校验与提示用到），
/// 所以构造时不算、用到才算。原版把它们的四个 `一次性锁` 平铺在管理器上，
/// 派生逻辑散成四个方法。
#[derive(Debug, Clone)]
struct 懒算表 {
    /// 全部映射表键的并集
    词表: OnceLock<HashSet<String>>,
    /// 词表按字符数分桶的索引（易混方法名笔误检查）
    分桶: OnceLock<词形分桶>,
    /// 「未标注类型」判定用的歧义构造器被调名集
    歧义构造器: OnceLock<HashSet<String>>,
    /// 英文原词 → 母语词条的回落索引
    英文回落: OnceLock<HashMap<String, String>>,
}

impl 懒算表 {
    /// 四张派生表都还没算
    fn 空白() -> Self {
        Self {
            词表: OnceLock::new(),
            分桶: OnceLock::new(),
            歧义构造器: OnceLock::new(),
            英文回落: OnceLock::new(),
        }
    }

    /// 教学校验已知词表：关键字/宏/派生/模块路径/别名五表键的并集
    fn 取词表(
        &self,
        关键词表: &HashMap<String, String>,
        宏表: &HashMap<String, String>,
        派生特征表: &HashMap<String, String>,
        模块路径表: &HashMap<String, String>,
        标识符别名: &HashMap<String, String>,
    ) -> &HashSet<String> {
        self.词表.get_or_init(|| {
            let mut 出集: HashSet<String> = HashSet::new();
            for 单词表 in [关键词表, 宏表, 派生特征表, 模块路径表, 标识符别名]
            {
                for 键名 in 单词表.keys() {
                    出集.insert(键名.clone());
                }
            }
            出集
        })
    }

    /// 词表分桶索引：与 [`懒算表::取词表`] 同一份词表，只是按字符数分桶
    ///
    /// 笔误检查只需比较名字长度 ±1 的词；带上精确命中表是因为真实语料里
    /// 方法调用位绝大多数是合法词，一次哈希探测就能免去扫三桶候选。
    fn 取分桶(&self, 词表: &HashSet<String>) -> &词形分桶 {
        self.分桶.get_or_init(|| 词形分桶::build(词表).带精确索引())
    }

    /// 歧义构造器被调名集：「新建」「默认值」两个方言词换出的英文原词，加上值恰为这两个词的其他方言键
    ///
    /// 仅供校验启发式判定 `X::新建()` 这类无法自行推导结果类型的关联构造调用。
    fn 取歧义构造器(
        &self,
        关键词表: &HashMap<String, String>,
        模块路径表: &HashMap<String, String>,
        标识符别名: &HashMap<String, String>,
    ) -> &HashSet<String> {
        self.歧义构造器.get_or_init(|| {
            let mut 出集: HashSet<String> = HashSet::new();
            出集.insert("new".to_string());
            出集.insert("default".to_string());
            for 单词表 in [关键词表, 模块路径表, 标识符别名] {
                for (母语, 英文) in 单词表.iter() {
                    if 英文 == "new" || 英文 == "default" {
                        出集.insert(母语.clone());
                    }
                }
            }
            出集
        })
    }

    /// 英文回落反向索引：值恰为单个 ASCII 标识符且与键不同（非恒等）的条目反转
    ///
    /// 同一英文词命中多个母语词时取「更短 → 码位序更小」的那一个，结果与
    /// 哈希映射迭代序无关；恒等映射的语言包（英文包）得到空表，回落校验随之静默。
    fn 取英文回落(
        &self,
        关键词表: &HashMap<String, String>,
        模块路径表: &HashMap<String, String>,
        标识符别名: &HashMap<String, String>,
        派生特征表: &HashMap<String, String>,
        宏表: &HashMap<String, String>,
    ) -> &HashMap<String, String> {
        self.英文回落.get_or_init(|| {
            let mut 出表: HashMap<String, String> = HashMap::new();
            for 单词表 in [关键词表, 模块路径表, 标识符别名, 派生特征表, 宏表]
            {
                for (母语, 英文) in 单词表.iter() {
                    if 母语 == 英文 || !是单个英文标识符(英文) {
                        continue;
                    }
                    // 同英文词多母语候选：取更短、再码位序更小者（结果确定）
                    let 该换 = match 出表.get(英文) {
                        Some(已值) => 更优母语(母语, 已值),
                        None => true,
                    };
                    if 该换 {
                        出表.insert(英文.clone(), 母语.clone());
                    }
                }
            }
            出表
        })
    }
}

/// 映射管理器：转译管线各阶段要用的映射表都从这里取
///
/// 构造后所有表不可变，派生视图（语境指纹、宏表、教学校验词表等）一律
/// 「算一次存着」。两种装载方式（读语言包目录 / 用编译时嵌入的字符串）共用
/// 同一个装配构造器，因此结果逐字段一致——这一点由测试
/// `两种装载方式对同一份内容得到同一张表` 钉住，不再只靠注释承诺。
#[derive(Debug, Clone)]
pub struct 映射管理器 {
    /// 关键字映射（词法转译阶段用）：keywords.toml 全部非元数据节的扁平合并
    pub 关键词表: HashMap<String, String>,
    /// 按节组织的关键字映射，保留「宏」「派生特征」「类型」等节边界
    节表: 节表,
    /// 派生特征映射：只在 `#[派生(...)]` 属性内生效，不并入关键词表——
    /// 否则「克隆」在派生位换出的写法会顶掉方法位那一条
    派生特征表: HashMap<String, String>,
    /// 模块路径映射（如 `标准库` 换出的那个英文包根名）
    pub 模块路径表: HashMap<String, String>,
    /// 标识符别名映射（标准库与第三方库的类型、函数名翻译）
    pub 标识符别名: HashMap<String, String>,
    /// 语境指纹缓存：任一映射表内容变化即变，多次转译共用一次计算
    语境指纹缓存: OnceLock<u64>,
    /// `["宏"]` 节的物化副本缓存
    宏表缓存: OnceLock<HashMap<String, String>>,
    /// 教学校验用的四张派生表
    懒算: 懒算表,
    /// 「使用」语句内的让位词：模块路径与别名键的并集，交由路径/别名阶段处理
    让位词: HashSet<String>,
    /// 方法调用位让位词：词法层替换值是保留关键字、而别名表另有有效词条
    方法让位词: HashSet<String>,
    /// 词条 → 危险声明（`["危险等级"]` / `["危险备注"]` 两节 + 默认归类）
    危险表: HashMap<String, 危险详情>,
    /// 「英文名 → 母语名」反查候选，已按优先序排好；只供诊断反查表使用
    反查候选: Vec<(String, String)>,
}

impl 映射管理器 {
    /// 从语言包目录加载全部映射（开发模式与 CLI / LSP 运行期走这条路）
    ///
    /// 只有 `keywords.toml` 是必需的；`module_paths.toml`、`标准库.toml`、
    /// `crates/*.toml` 缺失都不报错，但**存在而读不动 / 解析不了必须报错**——
    /// 静默跳过会让整张映射表消失而用户毫无察觉。
    pub fn 从目录载入(语言包目录: &Path) -> Result<Self, 载入故障> {
        let 关键词文件 = 语言包目录.join("keywords.toml");
        if !关键词文件.exists() {
            return Err(载入故障::文件缺失 {
                目标: 载入目标::关键字表,
                文件路径: 关键词文件.display().to_string(),
            });
        }
        let mut 入料 = 载入源 {
            关键词文档: Some(Self::载入文件(&关键词文件, 源类别::关键词)?),
            ..Default::default()
        };
        // module_paths.toml 与标准库.toml 都是「存在才读」：读盘失败必须报错，
        // 不存在则退化为空来源
        let 模块路径文件 = 语言包目录.join("module_paths.toml");
        if 模块路径文件.exists() {
            入料.模块路径文档 = Some(Self::载入文件(&模块路径文件, 源类别::模块路径)?);
        }
        let 标准库文件 = 语言包目录.join("stdlib.toml");
        if 标准库文件.exists() {
            入料.标准库文档 = Some(Self::载入文件(&标准库文件, 源类别::标准源)?);
        }

        // crates/*.toml：目录读取的返回顺序未定义，落进有序集合按路径升序收，
        // 同键不同值时合并结果才确定（与内置路按文件名排序同一口径）
        let 第三方目录 = 语言包目录.join("crates");
        if 第三方目录.exists() {
            let mut 路径集合: BTreeSet<PathBuf> = BTreeSet::new();
            let 目录读 =
                fs::read_dir(&第三方目录).map_err(|坏| 载入故障::目录读取失败 {
                    详情: 坏.to_string(),
                })?;
            for 条目项 in 目录读 {
                let 项路径 = 条目项
                    .map_err(|坏| 载入故障::目录读取失败 {
                        详情: 坏.to_string(),
                    })?
                    .path();
                if 项路径.extension().and_then(|后缀| 后缀.to_str()) == Some("toml") {
                    路径集合.insert(项路径);
                }
            }
            for 文件路径 in 路径集合 {
                入料.第三方文档们.push((
                    文件路径.display().to_string(),
                    Self::载入文件(&文件路径, 源类别::第三方)?,
                ));
            }
        }
        Ok(Self::组装(&入料))
    }

    /// 从编译时嵌入的字符串加载（发布产物不依赖语言包目录）
    ///
    /// 参数依次是关键字 / 模块路径 / 标准库的完整文本，以及各第三方库的
    /// `(文件名, 内容)`；文件名排序在这里补上，与 [`映射管理器::从目录载入`]
    /// 的读盘序一致。
    pub fn 从内置载入(
        关键词文本: &str,
        模块路径文本: &str,
        标准库文本: &str,
        第三方数据: &[(&str, &str)],
    ) -> Result<Self, 载入故障> {
        let 关键词文档 = Self::载入文本(关键词文本, 源类别::关键词, true, None)?;
        let 模块路径文档 = Self::载入文本(模块路径文本, 源类别::模块路径, true, None)?;
        let 标准库文档 = Self::载入文本(标准库文本, 源类别::标准源, true, None)?;

        // 有序映射 = 按文件名升序；与读盘路的有序集合同一口径
        let mut 按名: BTreeMap<&str, &str> = BTreeMap::new();
        for (名字, 内容) in 第三方数据.iter() {
            按名.insert(*名字, *内容);
        }
        let mut 第三方文档们: Vec<(String, 已解析文档)> = Vec::new();
        for (名字, 内容) in 按名 {
            第三方文档们.push((
                名字.to_string(),
                Self::载入文本(内容, 源类别::第三方, true, Some(名字.to_string()))?,
            ));
        }

        let 入料 = 载入源 {
            关键词文档: Some(关键词文档),
            模块路径文档: Some(模块路径文档),
            标准库文档: Some(标准库文档),
            第三方文档们,
            ..Default::default()
        };
        Ok(Self::组装(&入料))
    }

    /// 向后兼容：从单个关键字文件的位置加载（同一目录下的其余文件一起读）
    pub fn 从文件载入(文件路径: &Path) -> Result<Self, 载入故障> {
        Self::从目录载入(文件路径.parent().unwrap_or(Path::new(".")))
    }

    /// 从已解析的映射表直接构造（旧格式语言包、硬编码兜底表）
    ///
    /// 这条路上没有节信息（宏表 / 派生表为空），也没有 TOML 书写序，
    /// 反查候选退化为「按母语键升序」——仍然确定，且与旧版只取
    /// 模块路径 / 别名两源的口径一致。
    pub fn 从扁表构造(
        关键词表: HashMap<String, String>,
        模块路径表: HashMap<String, String>,
        标识符别名: HashMap<String, String>,
    ) -> Self {
        let 入料 = 载入源 {
            扁关键词表: Some(关键词表),
            扁模块路径表: Some(模块路径表),
            扁标识符别名: Some(标识符别名),
            ..Default::default()
        };
        Self::组装(&入料)
    }

    /// 唯一的装配路径：把各来源文档折算成六张表 + 两个派生集合
    ///
    /// `扁表们`（来自 [`映射管理器::从扁表构造`]）给了就整份采用，不再从文档
    /// 合并。双节来源的合并序固定为「第三方 → 模块路径 → 标准库」，标准库
    /// 收尾，因此「标准库优先于第三方同名通用词」这条规则与装载方式无关。
    /// 本函数不碰 TOML，也就没有可失败之处。
    fn 组装(入料: &载入源) -> Self {
        // 1. keywords.toml：节表保留节边界，扁平表供词法层直接用
        let 节表 = 入料
            .关键词文档
            .as_ref()
            .map(|文档| 文档.节表())
            .unwrap_or_default();
        let 派生特征表 = 节表.get("派生特征").cloned().unwrap_or_default();
        let 关键词表 = match &入料.扁关键词表 {
            Some(现成表) => 现成表.clone(),
            None => {
                // 派生特征节单独存放，并从扁平表里摘掉，否则「克隆」在派生位换出的写法
                // 会顶掉方法位该有的那一条
                let mut 出表 = 摊平节表(&节表);
                for 键名 in 派生特征表.keys() {
                    出表.remove(键名);
                }
                出表
            }
        };

        // 2. 双节来源：按覆盖序排好，标准库收尾
        let mut 双节: Vec<&已解析文档> = Vec::new();
        for (_, 文档) in 入料.第三方文档们.iter() {
            双节.push(文档);
        }
        if let Some(文档) = 入料.模块路径文档.as_ref() {
            双节.push(文档);
        }
        if let Some(文档) = 入料.标准库文档.as_ref() {
            双节.push(文档);
        }

        let mut 模块路径表 = 入料.扁模块路径表.as_ref().cloned().unwrap_or_default();
        let mut 标识符别名 = 入料.扁标识符别名.as_ref().cloned().unwrap_or_default();
        let mut 危险表 = 危险从节表(&节表);
        for 文档 in 双节 {
            文档.合并模块路径与标识符(&mut 模块路径表, &mut 标识符别名);
            危险表.extend(文档.危险表());
        }

        // 3. 反查候选：按「来源优先序」收书写序条目，只保留在生效正向表里
        //    仍然成立的整对儿（母语键 + 英文值）
        let 空表: HashMap<String, String> = HashMap::new();
        let 类型节 = 节表.get("类型").unwrap_or(&空表);
        let 反查候选 = 组装反查候选(
            &入料.反查阶段们(&模块路径表, 类型节, &标识符别名),
            &模块路径表,
            节表.get("类型"),
            &标识符别名,
        );

        Self {
            让位词: Self::组装让位词(&模块路径表, &标识符别名),
            方法让位词: Self::组装方法让位词(&关键词表, &标识符别名),
            反查候选,
            关键词表,
            节表,
            派生特征表,
            模块路径表,
            标识符别名,
            危险表,
            语境指纹缓存: OnceLock::new(),
            宏表缓存: OnceLock::new(),
            懒算: 懒算表::空白(),
        }
    }

    /// 读取并解析一个语言包文件：读盘与解析失败都带上真实路径
    ///
    /// 路径无条件带上：[`载入故障`] 的可显示实现只在「标准库 / 第三方」两类模板
    /// 里读它，其余类别的消息本身点名，多带一个字段不改任何文案。原版在这里按
    /// 类别判要不要带，读盘路的标准库解析失败于是拿到一个空路径占位。
    fn 载入文件(
        文件路径: &Path, 类别: 源类别
    ) -> Result<已解析文档, 载入故障> {
        let 路径文字 = 文件路径.display().to_string();
        let 内容 = fs::read_to_string(文件路径).map_err(|坏| 载入故障::读取失败 {
            目标: 类别.载入目标(false),
            文件路径: Some(路径文字.clone()),
            详情: 坏.to_string(),
        })?;
        Self::载入文本(&内容, 类别, false, Some(路径文字))
    }

    /// 解析一段 TOML：`来源名` 只用于错误上下文（内置数据没有真实路径）
    fn 载入文本(
        内容: &str,
        类别: 源类别,
        内置: bool,
        来源名: Option<String>,
    ) -> Result<已解析文档, 载入故障> {
        已解析文档::解析文档(内容).map_err(|坏| 载入故障::解析失败 {
            目标: 类别.载入目标(内置),
            文件路径: 来源名,
            详情: 坏.to_string(),
        })
    }

    /// 「使用」段让位词：模块路径与别名映射键的并集
    fn 组装让位词(
        模块路径表: &HashMap<String, String>,
        标识符别名: &HashMap<String, String>,
    ) -> HashSet<String> {
        let mut 出集: HashSet<String> = HashSet::new();
        for 单词表 in [模块路径表, 标识符别名] {
            for 键名 in 单词表.keys() {
                出集.insert(键名.clone());
            }
        }
        出集
    }

    /// 方法调用位让位词：词法层替换值是保留关键字、且别名表里该词另有不同
    /// 且有效（非保留字）的词条
    ///
    /// 仅此冲突类让位；其余同名词（如 `文件`：宏节与别名两节各有一条、只差大小写）在方法位
    /// 保留词法层行为，避免把本就合法的产物改掉（`.文件()` 依旧是 `.file()`）。
    fn 组装方法让位词(
        关键词表: &HashMap<String, String>,
        标识符别名: &HashMap<String, String>,
    ) -> HashSet<String> {
        let mut 出集: HashSet<String> = HashSet::new();
        for (词语, 英文) in 关键词表.iter() {
            if !是保留关键字(英文) {
                continue;
            }
            // 别名表里该词另有不同、且本身不是保留字的有效词条 → 让位
            if 标识符别名
                .get(词语)
                .is_some_and(|另值| 另值 != 英文 && !是保留关键字(另值))
            {
                出集.insert(词语.clone());
            }
        }
        出集
    }

    /// 查询关键字映射
    pub fn 查键(&self, 母语词: &str) -> Option<&String> {
        self.关键词表.get(母语词)
    }

    /// 获取完整关键字映射表
    pub fn 取关键词表(&self) -> &HashMap<String, String> {
        &self.关键词表
    }

    /// 获取指定节的映射表
    pub fn 取节映射(&self, 节名: &str) -> Option<&HashMap<String, String>> {
        self.节表.get(节名)
    }

    /// 获取模块路径映射表
    pub fn 取模块路径表(&self) -> &HashMap<String, String> {
        &self.模块路径表
    }

    /// 获取标识符别名映射表
    pub fn 取别名表(&self) -> &HashMap<String, String> {
        &self.标识符别名
    }

    /// 获取派生特征映射（中文特征名 → 英文），来自 `["派生特征"]` 节
    ///
    /// 仅在 `#[派生(...)]` 属性内生效；方法调用 `值.克隆()` 仍走别名表（产物
    /// `值.clone()`）。返回引用而非副本：它本身就是字段，克隆纯属浪费。
    pub fn 取派生特征表(&self) -> &HashMap<String, String> {
        &self.派生特征表
    }

    /// 获取宏映射表（母语宏名 → 英文宏名），来自 `["宏"]` 节
    ///
    /// 宏名同时在类型节与宏节定义时（如 `向量`：类型节换出大写的容器原名、宏节换出小写的宏名），
    /// 关键词表里的值被类型节覆盖，宏调用必须用本映射才拿得到英文宏名。
    /// 返回引用：该表由 `节表` 派生，每次调用都克隆整表在批量转译与项目上下文
    /// 收集中是纯浪费（映射表构造后不可变），故惰性物化一次后复用。
    pub fn 取宏表(&self) -> &HashMap<String, String> {
        self.宏表缓存
            .get_or_init(|| self.节表.get("宏").cloned().unwrap_or_default())
    }

    /// 获取全部宏名集合（不含感叹号），供词法层的宏感叹号自动补充
    pub fn 取宏名们(&self) -> HashSet<String> {
        let mut 出集: HashSet<String> = HashSet::new();
        for 键名 in self.取宏表().keys() {
            出集.insert(键名.clone());
        }
        出集
    }

    /// 「使用」段让位词集合（见字段 `让位词` 说明）
    pub fn 取让位词(&self) -> &HashSet<String> {
        &self.让位词
    }

    /// 方法调用位让位词集合（见字段 `方法让位词` 说明）
    pub fn 取方法让位词(&self) -> &HashSet<String> {
        &self.方法让位词
    }

    /// 「英文名 → 母语名」反查候选（已按优先序排列）
    ///
    /// 调用方按「同一英文名取首个命中」遍历，结果与进程无关。仅供诊断反查表
    /// 使用，不参与正向转译。
    pub fn 取反查候选(&self) -> &[(String, String)] {
        &self.反查候选
    }

    /// 教学校验已知词表：五张映射表键的并集，惰性算一次后复用
    pub fn 取校验词表(&self) -> &HashSet<String> {
        self.懒算.取词表(
            &self.关键词表,
            self.取宏表(),
            &self.派生特征表,
            &self.模块路径表,
            &self.标识符别名,
        )
    }

    /// 教学校验词表分桶索引（易混方法名笔误检查用）
    pub fn 取校验分桶(&self) -> &词形分桶 {
        self.懒算.取分桶(self.取校验词表())
    }

    /// 教学校验「未标注类型」用的歧义构造器被调名集
    pub fn 取歧义构造器(&self) -> &HashSet<String> {
        self.懒算
            .取歧义构造器(&self.关键词表, &self.模块路径表, &self.标识符别名)
    }

    /// 英文回落反向索引：供教学校验提示「方言文件里写了英文的函数关键字，
    /// 它有母语词条 `函数`」
    pub fn 取英文回落(&self) -> &HashMap<String, String> {
        self.懒算.取英文回落(
            &self.关键词表,
            &self.模块路径表,
            &self.标识符别名,
            &self.派生特征表,
            self.取宏表(),
        )
    }

    /// 翻译语境指纹（惰性计算并缓存）：任一映射表内容变化时指纹变化
    ///
    /// 映射表构造后不可变，多次转译共用同一个指纹，避免每次全量重算
    /// （建全部键值对 + 排定 + 哈希在批量转译场景是纯浪费）。
    pub fn 取语境指纹(&self) -> u64 {
        *self.语境指纹缓存.get_or_init(|| {
            转译缓存::生成语境指纹(
                &self.关键词表,
                &self.模块路径表,
                &self.标识符别名,
                &self.派生特征表,
            )
        })
    }

    /// 查询词条的危险等级与备注（词条表逐条标注的唯一入口）
    ///
    /// 默认归类：关键字/宏词条 → 会改写标识符（词法层无位置保护）；映射目标
    /// 属于「恒恐慌」清单 → 会恐慌（如 `解包` 换出的取成功值方法）；其余 → 纯安全。
    /// 显式 `["危险等级"]` 只能**提高**风险、不能降低自动判定——即使标注
    /// 「安全」，自动识别出的恐慌与改写仍保留（防漏标）。
    pub fn 取危险详情(&self, 词语: &str) -> 危险详情 {
        let 目标 = self
            .标识符别名
            .get(词语)
            .or_else(|| self.模块路径表.get(词语))
            .or_else(|| self.关键词表.get(词语))
            .or_else(|| self.派生特征表.get(词语));
        let mut 等级 = 危险等级 {
            会恐慌: 目标.is_some_and(|引文| 恒恐慌目标(引文)),
            会改写: self.关键词表.contains_key(词语),
        };
        let mut 备注 = String::new();
        if let Some(已标) = self.危险表.get(词语) {
            等级.会恐慌 = 等级.会恐慌 || 已标.等级.会恐慌;
            等级.会改写 = 等级.会改写 || 已标.等级.会改写;
            备注 = 已标.备注.clone();
        }
        if 备注.is_empty() && 等级.会恐慌 {
            备注 = "运行时可能 panic：仅在调用处显式选择时使用".to_string();
        }
        危险详情 { 等级, 备注 }
    }

    /// 检测映射表中的循环引用（A→B 且 B→A 的互指对）
    ///
    /// 覆盖三类：关键字表内部（含宏节）、别名表内部、关键字↔别名跨表。循环引用
    /// 会让正向与反向转译来回抖动（译出又译回），是语言包配置的严重错误；
    /// 返回人类可读的循环描述列表（空 = 无循环）。自映射（A→A）不算循环——
    /// 回译无抖动，只是冗余条目。
    ///
    /// 有序集合同时给出「同一互指对只报一行」与「整体升序」，不需要先收进
    /// 向量再排。
    pub fn 查循环(&self) -> Vec<String> {
        let mut 出集合: BTreeSet<String> = BTreeSet::new();
        Self::记表内循环(&self.关键词表, "关键字", &mut 出集合);
        Self::记表内循环(&self.标识符别名, "别名", &mut 出集合);
        for (键名, 值名) in self.关键词表.iter() {
            if 键名 != 值名 && self.标识符别名.get(值名).is_some_and(|回指| 回指 == 键名)
            {
                出集合.insert(format!("关键字↔别名: {键名} ↔ {值名}"));
            }
        }
        出集合.into_iter().collect()
    }

    /// 一张表内部的互指对：`表内: 键 ↔ 值`，只记「键 < 值」的那一个方向
    fn 记表内循环(
        单词表: &HashMap<String, String>,
        标记: &str,
        出集合: &mut BTreeSet<String>,
    ) {
        for (键名, 值名) in 单词表.iter() {
            if 键名 < 值名 && 单词表.get(值名).is_some_and(|回指| 回指 == 键名) {
                出集合.insert(format!("{标记}: {键名} ↔ {值名}"));
            }
        }
    }
}

/// 装配的输入：各来源的已解析文档 + 扁表旁路
///
/// 原版没有这一层，两个加载器各自把文档解析结果直接喂进两份重复的装配代码。
/// 有了它，「读盘」与「内置」的区别就只剩「文档从哪来」，装配面完全共用。
#[derive(Default)]
struct 载入源 {
    /// keywords.toml 的解析结果；扁表旁路时是 `无`
    关键词文档: Option<已解析文档>,
    /// module_paths.toml（可选）
    模块路径文档: Option<已解析文档>,
    /// 标准库.toml（可选）
    标准库文档: Option<已解析文档>,
    /// crates/*.toml，已按文件名升序
    第三方文档们: Vec<(String, 已解析文档)>,
    /// 扁表旁路（[`映射管理器::从扁表构造`]）：给了就整份采用
    扁关键词表: Option<HashMap<String, String>>,
    扁模块路径表: Option<HashMap<String, String>>,
    扁标识符别名: Option<HashMap<String, String>>,
}

impl 载入源 {
    /// 反查候选的来源阶段，按优先序：标准库模块路径 → 其余模块路径 →
    /// 关键字类型节 → 标准库标识符 → 第三方标识符
    ///
    /// 优先序与正向映射的「模块路径覆盖一切、标准库优先于第三方」一致。
    /// 顺序来自各来源 TOML 的书写序（先写的是主词条）；正向表用 `哈希映射`，
    /// 节内顺序在解析时就已丢失，若反查直接遍历正向表，同一英文名的多个母语
    /// 同义词谁胜出就成了跨进程随机数。
    fn 反查阶段们<'甲>(
        &'甲 self,
        模块路径表: &'甲 HashMap<String, String>,
        类型节: &'甲 HashMap<String, String>,
        标识符别名: &'甲 HashMap<String, String>,
    ) -> Vec<候选阶段<'甲>> {
        // module_paths.toml 与各 crates/*.toml 同属「其余模块路径」一档，
        // 三方包表已按文件名序，接在后面即可
        let mut 其他模块路径 = 节条目(&self.模块路径文档, "模块路径");
        let mut 第三方标识符: Vec<(String, String)> = Vec::new();
        for (_, 文档) in self.第三方文档们.iter() {
            其他模块路径.extend(文档.节条目们("模块路径").to_vec());
            第三方标识符.extend(文档.节条目们("标识符").to_vec());
        }
        Vec::from([
            (节条目(&self.标准库文档, "模块路径"), 模块路径表),
            (其他模块路径, 模块路径表),
            (节条目(&self.关键词文档, "类型"), 类型节),
            (节条目(&self.标准库文档, "标识符"), 标识符别名),
            (第三方标识符, 标识符别名),
        ])
    }
}

/// 取某个可选来源文档某一节的书写序条目；来源缺失（扁表旁路、文件不存在）给空表
fn 节条目(文档: &Option<已解析文档>, 节名: &str) -> Vec<(String, String)> {
    match 文档 {
        Some(已解析) => 已解析.节条目们(节名).to_vec(),
        None => Vec::new(),
    }
}

/// 汇总诊断反查候选（母语键 → 英文，按优先序）
///
/// 三条规则：
/// - 顺序 = `阶段们` 的数组序，同一阶段内按 TOML 书写序。语言包允许同一英文值
///   有多个母语同义词（正向写法都合法），反查时必须有一个确定胜出者；先写的
///   那条是主词条（如中文包里 `"可显示" = "Display"` 写在特征区、`"显示" = "Display"`
///   写在后面的格式化区）。
/// - 只保留正向表里仍然成立的条目：同一母语键在本类映射里被后续文件覆盖时，
///   旧值不再参与反查，否则反查会给出一个正向写不出来的组合。同一母语键在不同
///   映射里可以各有有效词条（`字符串` 在关键字的类型节里换出大写的容器原名、在
///   模块路径表里换出小写的路径段），故去重只能按「母语键 + 英文值」
///   整对儿做，不能按母语键抹平。
/// - 有序视图里没出现的条目（如某文件书写序解析失败、或由
///   [`映射管理器::从扁表构造`] 这种无书写序的入口构造）补在最后；补入走有序
///   集合，因此既去重又升序，结果与迭代序无关。
fn 组装反查候选(
    阶段们: &[候选阶段<'_>],
    模块路径表: &HashMap<String, String>,
    类型节: Option<&HashMap<String, String>>,
    标识符别名: &HashMap<String, String>,
) -> Vec<(String, String)> {
    let 空表: HashMap<String, String> = HashMap::new();
    let 类型节表 = 类型节.unwrap_or(&空表);
    let mut 出列表: Vec<(String, String)> = Vec::new();
    let mut 已收: HashSet<(String, String)> = HashSet::new();
    for (条目们, 生效表) in 阶段们.iter() {
        for (母语, 英文) in 条目们 {
            // 纯英文键（`"format" = "fmt"` 这类修正项）不能进反查表，否则译文
            // 里会出现英文值
            let 仍成立 = 生效表.get(母语).map(|引文| 引文.as_str()) == Some(英文.as_str());
            if !是母语词条(母语) || !仍成立 {
                continue;
            }
            let 对儿 = (母语.clone(), 英文.clone());
            if !已收.insert(对儿.clone()) {
                continue;
            }
            出列表.push(对儿);
        }
    }
    // 兜底：正向表里有、上面没收到的条目（有序集合 = 去重 + 升序）
    let mut 剩余: BTreeSet<(String, String)> = BTreeSet::new();
    for 单词表 in [模块路径表, 类型节表, 标识符别名] {
        for (母语, 英文) in 单词表.iter() {
            if 是母语词条(母语) {
                剩余.insert((母语.clone(), 英文.clone()));
            }
        }
    }
    for 对儿 in 剩余 {
        if !已收.contains(&对儿) {
            出列表.push(对儿.clone());
        }
    }
    出列表
}

/// 词法层替换值是否撞上 Rust 保留关键字（有序静态表二分，见 [`保留关键字`]）
fn 是保留关键字(词: &str) -> bool {
    保留关键字.binary_search(&词).is_ok()
}

/// 判定字符串是否为单个 ASCII Rust 标识符（首字符字母/下划线、余下字母数字/
/// 下划线）；排除含 `::`、空格、`<` 的复合值（如模块路径 `std::fs`），
/// 只有单标识符值才能作为回落索引的反转键
fn 是单个英文标识符(串: &str) -> bool {
    let mut 字们 = 串.chars();
    match 字们.next() {
        Some(字) => {
            (字.is_ascii_alphabetic() || 字 == '_')
                && 串
                    .chars()
                    .all(|尾字| 尾字.is_ascii_alphanumeric() || 尾字 == '_')
        }
        None => false,
    }
}

/// 同一英文词的多个母语候选中，`候选` 是否优于 `现行`（更短、同长取码位序更小）
///
/// 保证回落索引的结果与哈希映射迭代序无关。
fn 更优母语(候选: &str, 现行: &str) -> bool {
    let 候选数 = 候选.chars().count();
    let 现行数 = 现行.chars().count();
    候选数 < 现行数 || (候选数 == 现行数 && 候选 < 现行)
}

#[cfg(test)]
mod 母语测试 {
    use super::*;
    // 旧的一次性解析入口：加载器已改用 [`已解析文档`]（同一文件不重解析），
    // 这里继续用它的用例守住「非法 TOML 必须报错、不能静默丢映射」这条底线
    use crate::映射源::合并模块路径与标识符节;

    /// 在临时目录下造一个完整语言包，返回目录路径
    fn 造语言包(根: &Path, 名字: &str) -> PathBuf {
        let 目录 = 根.join(名字);
        fs::create_dir_all(目录.join("crates")).unwrap();
        fs::write(
            目录.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n[\"宏\"]\n\"打印行\" = \"println\"\n",
        )
        .unwrap();
        fs::write(
            目录.join("module_paths.toml"),
            "[\"模块路径\"]\n\"标准库\" = \"std\"\n",
        )
        .unwrap();
        fs::write(
            目录.join("stdlib.toml"),
            "[\"模块路径\"]\n\"文件系统\" = \"std::fs\"\n[\"标识符\"]\n\"字符串\" = \"String\"\n\"新\" = \"new\"\n",
        )
        .unwrap();
        fs::write(
            目录.join("crates").join("web.toml"),
            "[\"模块路径\"]\n\"网络库\" = \"reqwest\"\n[\"标识符\"]\n\"客户端\" = \"Client\"\n",
        )
        .unwrap();
        目录
    }

    /// 指向本包的 `lang-packs/<名>` 目录
    fn 语言包目录(代号: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("lang-packs")
            .join(代号)
    }

    #[test]
    fn 完整语言包读盘() {
        let 临时 = tempfile::tempdir().unwrap();
        let 目录 = 造语言包(临时.path(), "zh");

        let 管理器 = 映射管理器::从目录载入(&目录).expect("语言包应能完整加载");

        // 关键字映射（keywords.toml 全部节合并）
        assert_eq!(管理器.查键("函数"), Some(&"fn".to_string()));
        assert_eq!(管理器.查键("让"), Some(&"let".to_string()));
        // 模块路径映射（module_paths.toml + 标准库.toml 合并）
        assert_eq!(管理器.模块路径表.get("标准库"), Some(&"std".to_string()));
        assert_eq!(
            管理器.模块路径表.get("文件系统"),
            Some(&"std::fs".to_string())
        );
        // 标识符别名映射（标准库.toml + crates/*.toml 合并）
        assert_eq!(管理器.标识符别名.get("字符串"), Some(&"String".to_string()));
        assert_eq!(管理器.标识符别名.get("新"), Some(&"new".to_string()));
        assert_eq!(管理器.标识符别名.get("客户端"), Some(&"Client".to_string()));
        // 宏集合（仅宏节）
        assert!(管理器.取宏名们().contains("打印行"));
        assert!(!管理器.取宏名们().contains("函数"));
        // 方法位让位词：合成包无「保留字替换值 + 别名表另有有效词条」的冲突词
        assert!(管理器.取方法让位词().is_empty());
    }

    /// 保留字替换值 + 别名表另有不同有效词条 → 让位；
    /// 替换值非保留字、别名表无词条、别名词条同为保留字 → 均不让位
    #[test]
    fn 方法让位词判定() {
        let 关键词表 = HashMap::from([
            ("枚举".to_string(), "enum".to_string()),
            ("函数".to_string(), "fn".to_string()),
            ("文件".to_string(), "file".to_string()),
            ("让".to_string(), "let".to_string()),
            ("类型".to_string(), "type".to_string()),
        ]);
        let 别名表 = HashMap::from([
            ("枚举".to_string(), "enumerate".to_string()),
            ("函数".to_string(), "Fn".to_string()),
            ("文件".to_string(), "File".to_string()),
            ("类型".to_string(), "enum".to_string()),
        ]);
        assert_eq!(
            映射管理器::组装方法让位词(&关键词表, &别名表),
            HashSet::from(["枚举".to_string(), "函数".to_string()])
        );
    }

    #[test]
    fn 缺关键字文件报错() {
        let _护 = crate::语言::钉住语言("zh");
        let 临时 = tempfile::tempdir().unwrap();
        let 坏 = 映射管理器::从目录载入(临时.path()).unwrap_err();
        assert!(
            坏.to_string().contains("关键字文件不存在"),
            "错误信息应指明缺失文件: {:?}",
            坏
        );
    }

    #[test]
    fn 关键字非法报错() {
        let _护 = crate::语言::钉住语言("zh");
        let 临时 = tempfile::tempdir().unwrap();
        fs::write(临时.path().join("keywords.toml"), "这不是合法 TOML [[[").unwrap();
        let 坏 = 映射管理器::从目录载入(临时.path()).unwrap_err();
        assert!(
            坏.to_string().contains("解析关键字 TOML 失败"),
            "错误信息应含解析失败: {:?}",
            坏
        );
    }

    /// 仅有 keywords.toml 时，可选文件缺失不应报错
    #[test]
    fn 可选文件缺失可载() {
        let 临时 = tempfile::tempdir().unwrap();
        fs::write(
            临时.path().join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        let 管理器 = 映射管理器::从目录载入(临时.path()).expect("仅有关键字文件也应可加载");
        assert!(管理器.模块路径表.is_empty());
        assert!(管理器.标识符别名.is_empty());
    }

    #[test]
    fn 内置双节合并() {
        let 关键词文本 = "[\"声明\"]\n\"函数\" = \"fn\"\n";
        let 模块路径文本 = "[\"模块路径\"]\n\"标准库\" = \"std\"\n";
        let 标准库文本 =
            "[\"模块路径\"]\n\"文件系统\" = \"std::fs\"\n[\"标识符\"]\n\"字符串\" = \"String\"\n";
        let 三方数据: [(&str, &str); 1] = [("web.toml", "[\"标识符\"]\n\"客户端\" = \"Client\"\n")];

        let 管理器 =
            映射管理器::从内置载入(关键词文本, 模块路径文本, 标准库文本, &三方数据)
                .expect("内置数据应能加载");

        assert_eq!(管理器.查键("函数"), Some(&"fn".to_string()));
        assert_eq!(管理器.模块路径表.get("标准库"), Some(&"std".to_string()));
        assert_eq!(
            管理器.模块路径表.get("文件系统"),
            Some(&"std::fs".to_string())
        );
        assert_eq!(管理器.标识符别名.get("字符串"), Some(&"String".to_string()));
        assert_eq!(管理器.标识符别名.get("客户端"), Some(&"Client".to_string()));
    }

    #[test]
    fn 内置标准库非法报错() {
        let _护 = crate::语言::钉住语言("zh");
        let 坏 =
            映射管理器::从内置载入("[\"声明\"]\n", "[\"模块路径\"]\n", "非法内容", &[])
                .unwrap_err();
        assert!(
            坏.to_string().contains("解析内置标准库 TOML 失败"),
            "错误信息应含解析失败: {:?}",
            坏
        );
    }

    #[test]
    fn 文件载入委托目录() {
        let 临时 = tempfile::tempdir().unwrap();
        let 目录 = 造语言包(临时.path(), "zh");
        let 管理器 = 映射管理器::从文件载入(&目录.join("keywords.toml")).expect("委托加载应成功");
        assert_eq!(管理器.查键("函数"), Some(&"fn".to_string()));
    }

    #[test]
    fn 双节合并拒绝非法() {
        let mut 模块路径表 = HashMap::new();
        let mut 别名表 = HashMap::new();
        // 非法 TOML 不应恐慌，但必须报告错误（不再静默忽略，防止映射丢失）
        let 坏 = 合并模块路径与标识符节("[[[", &mut 模块路径表, &mut 别名表).unwrap_err();
        assert!(!坏.to_string().is_empty());
        assert!(模块路径表.is_empty());
        assert!(别名表.is_empty());
        // 合法内容正常合并
        合并模块路径与标识符节(
            "[\"模块路径\"]\n\"文件系统\" = \"std::fs\"\n[\"标识符\"]\n\"字符串\" = \"String\"\n",
            &mut 模块路径表,
            &mut 别名表,
        )
        .expect("合法 TOML 应合并成功");
        assert_eq!(模块路径表.get("文件系统"), Some(&"std::fs".to_string()));
        assert_eq!(别名表.get("字符串"), Some(&"String".to_string()));
    }

    #[test]
    fn 节映射与查键() {
        let 临时 = tempfile::tempdir().unwrap();
        let 目录 = 造语言包(临时.path(), "zh");
        let 管理器 = 映射管理器::从目录载入(&目录).unwrap();

        let 声明节 = 管理器.取节映射("声明").expect("声明节应存在");
        assert_eq!(声明节.get("函数"), Some(&"fn".to_string()));
        assert_eq!(管理器.取节映射("不存在的节"), None);
        assert_eq!(管理器.查键("不存在的关键字"), None);
    }

    /// 循环引用检测：关键字表内部互指
    #[test]
    fn 循环检测_关键字自环() {
        let 管理器 = 映射管理器::从扁表构造(
            HashMap::from([
                ("函数".to_string(), "fn".to_string()),
                ("方法".to_string(), "函数".to_string()),
                ("函数".to_string(), "方法".to_string()), // 后写覆盖：函数→方法
            ]),
            HashMap::new(),
            HashMap::new(),
        );
        let 循环清单 = 管理器.查循环();
        assert!(
            循环清单.iter().any(|项| 项.contains("函数 ↔ 方法")),
            "应检测到关键字互指循环: {:?}",
            循环清单
        );
    }

    /// 循环引用检测：别名表内部互指
    #[test]
    fn 循环检测_别名自环() {
        let 管理器 = 映射管理器::从扁表构造(
            HashMap::new(),
            HashMap::new(),
            HashMap::from([
                ("甲".to_string(), "乙".to_string()),
                ("乙".to_string(), "甲".to_string()),
            ]),
        );
        let 循环清单 = 管理器.查循环();
        assert!(
            循环清单
                .iter()
                .any(|项| 项.contains("别名") && 项.contains("乙 ↔ 甲")),
            "应检测到别名互指循环: {:?}",
            循环清单
        );
    }

    /// 循环引用检测：关键字↔别名跨表互指（管线末尾中文残留）
    #[test]
    fn 循环检测_跨表() {
        let 管理器 = 映射管理器::从扁表构造(
            HashMap::from([("甲".to_string(), "乙".to_string())]),
            HashMap::new(),
            HashMap::from([("乙".to_string(), "甲".to_string())]),
        );
        let 循环清单 = 管理器.查循环();
        assert!(
            循环清单
                .iter()
                .any(|项| 项.contains("关键字↔别名") && 项.contains("甲 ↔ 乙")),
            "应检测到跨表互指循环: {:?}",
            循环清单
        );
    }

    /// 正常映射表（含同值不同键、自映射豁免）不产生循环报告
    #[test]
    fn 循环检测_干净表() {
        let 管理器 = 映射管理器::从扁表构造(
            HashMap::from([
                ("函数".to_string(), "fn".to_string()),
                ("让".to_string(), "let".to_string()),
            ]),
            HashMap::new(),
            HashMap::from([("字符串".to_string(), "String".to_string())]),
        );
        assert!(管理器.查循环().is_empty());
    }

    /// 去重与升序：同一互指对只报一行，整体顺序与哈希映射迭代序无关
    ///
    /// 原版按表逐键扫，A→B 与 B→A 各报一行，两行的先后跟着哈希映射迭代序跨
    /// 进程漂移；本轮用有序集合把「同一对只留一个方向」和「整体升序」一次做掉。
    #[test]
    fn 循环检测_去重升序() {
        let 管理器 = 映射管理器::从扁表构造(
            HashMap::from([
                ("方法".to_string(), "函数".to_string()),
                ("函数".to_string(), "方法".to_string()),
                ("乙".to_string(), "甲".to_string()),
                ("甲".to_string(), "乙".to_string()),
            ]),
            HashMap::new(),
            HashMap::new(),
        );
        let 循环清单 = 管理器.查循环();
        assert_eq!(
            循环清单,
            Vec::from([
                // 整行升序：乙 U+4E59 < 函 U+51FD < 方 U+65B9 < 甲 U+7532；
                // 且同一互指对只记「键 < 值」的那一个方向（乙 ↔ 甲，不是甲 ↔ 乙）
                "关键字: 乙 ↔ 甲".to_string(),
                "关键字: 函数 ↔ 方法".to_string(),
            ])
        );
    }

    /// 两条装载路对同一份内容必须给出同一张表
    ///
    /// 原版读盘路与内置路各自合并，且标准库与三方包表的先后相反（读盘先
    /// 三方包表后标准库，内置先标准库后三方包表）。只要同一个键同时出现在
    /// 标准库.toml 与 crates/*.toml 且取值不同，开发模式（读盘）与发布产物
    /// （内置）就会拿到两张不同的映射表。本轮合并成单一 [`映射管理器::组装`]
    /// 后，双节覆盖序固定为「第三方 → 模块路径 → 标准库」。
    #[test]
    fn 两路装载同表() {
        let 临时 = tempfile::tempdir().unwrap();
        let 目录 = 临时.path().join("zh");
        fs::create_dir_all(目录.join("crates")).unwrap();
        let 关键词文本 = "[\"声明\"]\n\"函数\" = \"fn\"\n";
        let 模块路径文本 = "[\"模块路径\"]\n\"标准库\" = \"std\"\n";
        let 标准库文本 = "[\"标识符\"]\n\"客户端\" = \"StdClient\"\n\"字符串\" = \"String\"\n";
        let 三方文本 = "[\"标识符\"]\n\"客户端\" = \"WebClient\"\n";
        fs::write(目录.join("keywords.toml"), 关键词文本).unwrap();
        fs::write(目录.join("module_paths.toml"), 模块路径文本).unwrap();
        fs::write(目录.join("stdlib.toml"), 标准库文本).unwrap();
        fs::write(目录.join("crates").join("web.toml"), 三方文本).unwrap();

        let 读盘 = 映射管理器::从目录载入(&目录).expect("读盘路应能加载");
        let 内置 = 映射管理器::从内置载入(
            关键词文本,
            模块路径文本,
            标准库文本,
            &[("web.toml", 三方文本)],
        )
        .expect("内置路应能加载");

        assert_eq!(读盘.取关键词表(), 内置.取关键词表());
        assert_eq!(读盘.取模块路径表(), 内置.取模块路径表());
        assert_eq!(读盘.取别名表(), 内置.取别名表());
        assert_eq!(读盘.取派生特征表(), 内置.取派生特征表());
        assert_eq!(读盘.取让位词(), 内置.取让位词());
        assert_eq!(读盘.取方法让位词(), 内置.取方法让位词());
        assert_eq!(读盘.取反查候选(), 内置.取反查候选());
        // 冲突键归标准库收尾，且两条路一致
        assert_eq!(
            读盘.标识符别名.get("客户端"),
            Some(&"StdClient".to_string())
        );
        assert_eq!(内置.标识符别名.get("客户端"), 读盘.标识符别名.get("客户端"));
    }

    /// 读盘路上标准库解析失败必须带上真实路径
    ///
    /// `载入故障::解析失败` 对「标准库 / 第三方」用的消息模板含路径占位符，
    /// 原版从目录载入时一律传「无」，报错里就出现一个空路径。
    /// 本轮改成路径无条件带上，由可显示实现决定读不读。
    #[test]
    fn 读盘标准库报错带路径() {
        let _护 = crate::语言::钉住语言("zh");
        let 临时 = tempfile::tempdir().unwrap();
        fs::write(
            临时.path().join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        fs::write(临时.path().join("stdlib.toml"), "非法 [[[").unwrap();
        let 坏 = 映射管理器::从目录载入(临时.path()).unwrap_err();
        let 文字 = 坏.to_string();
        assert!(
            文字.contains("解析标准库 TOML 失败") || 文字.contains("失败"),
            "应有解析失败说明: {:?}",
            文字
        );
        assert!(
            文字.contains("stdlib.toml"),
            "读盘路的报错应含真实路径: {:?}",
            文字
        );
    }

    /// 危险等级：显式标注、默认归类与「自动判定不可被降级」三条规则
    #[test]
    fn 危险等级_标注与默认() {
        let 关键词文本 = concat!(
            "[\"声明\"]\n",
            "\"函数\" = \"fn\"\n",
            "[\"宏\"]\n",
            "\"恐慌\" = \"panic\"\n",
            "[\"危险等级\"]\n",
            "\"恐慌\" = \"panic+rewrite\"\n",
            "\"函数\" = \"safe\"\n",
        );
        let 标准库文本 = concat!(
            "[\"标识符\"]\n",
            "\"解包\" = \"unwrap\"\n",
            "\"末尾\" = \"last\"\n",
            "[\"危险等级\"]\n",
            "\"末尾\" = \"panic\"\n",
            "[\"危险备注\"]\n",
            "\"末尾\" = \"显式选择才使用\"\n",
        );
        let 管理器 = 映射管理器::从内置载入(关键词文本, "", 标准库文本, &[]).unwrap();

        // 关键字默认「会改写标识符」；显式标「安全」不能降低自动判定
        let 关键词 = 管理器.取危险详情("函数");
        assert!(关键词.等级.会改写, "关键字词条默认标记会改写标识符");
        assert!(!关键词.等级.会恐慌);

        // 多风险词条：恐慌 + 改写，且备注不为空
        let 恐慌宏 = 管理器.取危险详情("恐慌");
        assert!(恐慌宏.等级.会恐慌 && 恐慌宏.等级.会改写);
        assert!(!恐慌宏.备注.is_empty());

        // 目标为取成功值那个方法：即使没有显式标注也自动判为会恐慌
        let 解包词 = 管理器.取危险详情("解包");
        assert!(解包词.等级.会恐慌);
        assert!(!解包词.等级.会改写);
        assert!(!解包词.备注.is_empty(), "panic 词条应有默认提醒备注");

        // 显式标注恐慌的安全目标 + 备注透传
        let 已标 = 管理器.取危险详情("末尾");
        assert!(已标.等级.会恐慌);
        assert_eq!(已标.备注, "显式选择才使用".to_string());

        // 未登记者默认纯安全；元数据节不得污染映射表
        assert!(管理器.取危险详情("不存在的词").等级.是安全());
        assert!(!管理器.关键词表.contains_key("危险等级"));
        assert!(!管理器.标识符别名.contains_key("危险备注"));
    }

    /// 回归：「取列」必须是安全默认（返回选项、编译期强制处理缺失列），
    /// 会恐慌的那一条只能由「取列或崩溃」显式写出（防被改回隐式恐慌）
    #[test]
    fn 取列_不隐式崩溃() {
        let 管理器 = 映射管理器::从目录载入(&语言包目录("zh")).expect("内置语言包应可加载");

        // 默认路径：取列本身走的就是返回选项那条，缺失列返回「无」，不隐式恐慌
        assert_eq!(
            管理器.标识符别名.get("取列").cloned(),
            Some("get_opt".to_string())
        );
        assert!(
            管理器.取危险详情("取列").等级.是安全(),
            "取列 应为纯安全默认路径"
        );
        // 旧写法取列或空保留为同义词
        assert_eq!(
            管理器.标识符别名.get("取列或空").cloned(),
            Some("get_opt".to_string())
        );

        // 会恐慌的那一条拆成独立词条，只能显式写，且必须标注 + 给备注
        assert_eq!(
            管理器.标识符别名.get("取列或崩溃").cloned(),
            Some("get".to_string())
        );
        let 崩溃词 = 管理器.取危险详情("取列或崩溃");
        assert!(崩溃词.等级.会恐慌, "取列或崩溃 必须标注会 panic");
        assert!(!崩溃词.备注.is_empty(), "会 panic 的词条必须有备注说明");
    }

    /// 等级标注解析：三类基础值、多风险连接符与非法值
    #[test]
    fn 危险等级_解析() {
        assert_eq!(危险等级::解析等级("safe"), Some(危险等级::全安全));
        assert_eq!(危险等级::解析等级("panic"), Some(危险等级::必恐慌));
        assert_eq!(危险等级::解析等级("rewrite"), Some(危险等级::必改写));
        assert_eq!(
            危险等级::解析等级("panic + rewrite"),
            Some(危险等级::恐慌加改写)
        );
        assert_eq!(
            危险等级::解析等级("rewrite|panic"),
            Some(危险等级::恐慌加改写)
        );
        assert_eq!(危险等级::解析等级("boom"), None);
        assert_eq!(危险等级::恐慌加改写.代号(), "panic+rewrite");
        assert!(危险等级::全安全.是安全());
    }

    /// 英文回落反向索引：英文原词→母语词条；恒等映射/复合值被过滤
    #[test]
    fn 英文回落索引() {
        let 管理器 = 映射管理器::从目录载入(&语言包目录("zh")).expect("内置语言包应可加载");
        let 回落 = 管理器.取英文回落();
        // 关键字/类型/方法常见英文词均能反查到某个母语词
        for 英文 in ["fn", "let", "Vec", "new", "collect"] {
            assert!(回落.contains_key(英文), "回落表应含 `{}`: {:?}", 英文, 回落);
            let 母语 = 回落.get(英文).unwrap();
            assert_ne!(母语.clone(), 英文.to_string(), "反向索引应过滤恒等条目");
            assert!(
                !母语.is_ascii(),
                "zh 回落建议词应为母语（含非 ASCII）: {} -> {:?}",
                英文,
                母语
            );
        }
        // 含 `::` 的模块路径值不进入反向索引（非单标识符）
        assert!(
            !回落.keys().any(|键名| 键名.contains("::")),
            "反向索引键应为单标识符"
        );

        // 英文语言包（恒等映射）得到空表 → 回落校验自动静默
        let 英文包 = 映射管理器::从目录载入(&语言包目录("en")).expect("英文包应可加载");
        assert!(
            英文包.取英文回落().is_empty(),
            "英文恒等包回落反向索引应为空"
        );
    }
}
