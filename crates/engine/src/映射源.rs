//! 映射源模块【zh 自举·批次1】
//! 本文件是源真相：由引导 rzc 转译为 映射源.rs 后交 cargo 编译。
//! 再生成方式见 tools/zh-selfhost/regen.sh；请勿手改同目录下的 映射源.rs。
//!
//! 从 TOML 文件加载映射表。
//! 提供映射数据的加载和管理，支持按类别组织：
//! - 关键字映射（词法处理阶段）
//! - 标准库映射（语义处理阶段）
//! - 第三方库映射（按需加载，来自 crates/ 子目录）
//!
//! 注：模块路径保持英文 `mapping_source`（被 engine/cli/lsp 按名调用，属跨 包
//! 稳定 ABI）；公开类型/函数/方法/枚举变体（节表/映射类别/映射加载器
//! 及 解析配置节/摊平节表/合并模块路径与标识符节/
//! 加载关键词映射/加载标准库映射/加载全部映射/
//! 创建内置关键词映射 等）同为跨 包 数据契约，保持英文。
//! `toml::JSON值`/`toml::解析字符串`/`toml::反序列化模块::错误特征`/`标准库::文件系统`/`标准库::外部接口模块::操作系统串`/
//! `编码库::*`/`临时文件`/`作为引用`/`路径类型`/`路径缓冲类型`(路径缓冲)/`无符号机器整数` 及无干净
//! 词条或易撞词典的方法链（取全部键/迭代/变换/收集/克隆副本/排序/扩展/取条目/
//! 或用缺省/或用插入/而后映射/过滤/解包*/期望/包含/是否为空/
//! 是否为无/是否为成功/是否为错误/连接/存在/是否为目录/展示/文件名干/设扩展名/
//! 作为系统串/作为引用方法/转路径缓冲/取字符串引用/转为字节/解码/转所有权/
//! 转有损字符串/转为/读取目录/读取字符串/写入/递归创建目录 等）
//! 按英文透传（rzc 合法特性）；内部私有函数、局部变量、控制流、类型别名与
//! 测试均已中文化。加载层的 `加载错误`/`加载目标` 经 lib.rs 重导出到 包 根，
//! 以 `包::{加载目标, 加载错误}` 引用（第三方 salvo 表把「错误类型」劫持为 `错误特征`，
//! 无法安全写出模块路径 `错误类型::`）。

use crate::{加载目标, 加载错误};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 节（section）组织的映射表：节名 → (母语词 → 英文)
pub type 节表 = HashMap<String, HashMap<String, String>>;

/// 解析 TOML 内容为“节 → (母语词 → 英文)”二级表（两个加载器共用的单一解析实现）
///
/// 非表节与非字符串条目记录警告后跳过（数据格式错误不再完全静默，
/// RZ_LOG=warn 时可见）；返回原始 toml 错误，
/// 由调用方按场景本地化错误消息（文件/内置/路径等键不同）。
pub fn 解析配置节(内容: &str) -> Result<节表, toml::de::Error> {
    let 顶层值: toml::Value = toml::from_str(内容)?;
    let mut 节表 = HashMap::new();
    if let toml::Value::Table(顶层表) = 顶层值 {
        for (子节名, 子节值) in 顶层表 {
            if let toml::Value::Table(子节映射) = 子节值 {
                let mut 子项表 = HashMap::new();
                for (母语词, 英文值) in 子节映射 {
                    if let toml::Value::String(英文词) = 英文值 {
                        子项表.insert(母语词, 英文词);
                    } else {
                        crate::警告日志!(
                            "映射源",
                            "节 [{}] 条目 {} 的值不是字符串，已跳过",
                            子节名,
                            母语词
                        );
                    }
                }
                if !子项表.is_empty() {
                    节表.insert(子节名, 子项表);
                }
            } else {
                crate::警告日志!("映射源", "节 [{}] 不是表结构，已跳过", 子节名);
            }
        }
    }
    Ok(节表)
}

/// 把节表扁平化为单一映射：按节名升序合并，同名键冲突时胜出者确定
/// （HashMap 遍历顺序随机会导致多次加载结果不一致）
pub fn 摊平节表(节表: &节表) -> HashMap<String, String> {
    let mut 合并结果 = HashMap::new();
    let mut 子节名列表: Vec<&String> = 节表.keys().collect();
    子节名列表.sort();
    for 子节名 in 子节名列表 {
        for (母语词, 英文词) in &节表[子节名] {
            合并结果.insert(母语词.clone(), 英文词.clone());
        }
    }
    合并结果
}

/// 解析“模块路径 + 标识符”两节格式的 TOML（stdlib.toml 与 crates/*.toml 通用）
///
/// - `["模块路径"]` 节 → 合并到模块路径映射（如 `"线程" = "std::thread"`）
/// - `["标识符"]` 节 → 合并到标识符别名映射（如 `"字符串" = "String"`）
///
/// 返回原始 toml 错误，由调用方结合文件路径/数据源上下文转为 [`加载错误`]。
pub fn 合并模块路径与标识符节(
    内容: &str,
    模块路径表: &mut HashMap<String, String>,
    别名表: &mut HashMap<String, String>,
) -> Result<(), toml::de::Error> {
    let 节表 = 解析配置节(内容)?;
    if let Some(条目项) = 节表.get("模块路径") {
        模块路径表.extend(
            条目项
                .iter()
                .map(|(键名, 值项)| (键名.clone(), 值项.clone())),
        );
    }
    if let Some(条目项) = 节表.get("标识符") {
        别名表.extend(
            条目项
                .iter()
                .map(|(键名, 值项)| (键名.clone(), 值项.clone())),
        );
    }
    Ok(())
}

/// 把文件名字节转成 UTF-8 字符串（作为映射分类键）
///
/// - 本身是 UTF-8 时直接返回；
/// - 非 UTF-8 字节按常见 CJK 编码依次尝试解码（GB18030 → Shift_JIS → Big5 →
///   EUC-KR → EUC-JP），首个解码无错误者即为转码结果，
///   把“非 UTF-8 文件名”真正转成 UTF-8（而非丢弃字节）；
/// - 所有编码都失败时回退 lossy 转写（保留可显示部分，不 崩溃）。
///
/// 平台差异：只有 Unix 允许文件名是任意字节，因此才需要字节级转码；
/// Windows 的文件名在系统层面就是 UTF-16，正常文件名（含中文/日文等）
/// `取字符串引用()` 总能成功，不会进入转码分支，也不会被误判成 GBK 乱码；
/// 唯一极端情况是文件名字含孤立代理码元（正常工具造不出来），
/// 此时回退为明确的替换符 `` 而非错误解码。
fn 解码系统名(待解码: &std::ffi::OsStr) -> String {
    if let Some(可读文本) = 待解码.to_str() {
        return 可读文本.to_string();
    }
    // Unix 下可取原始字节逐编码尝试；其他平台（Windows 等）直接走 lossy 兜底
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let 原始字节 = 待解码.as_bytes();
        // 按 CJK 使用频度排序：中文 GB18030（覆盖 GBK/GB2312）优先，日文 Shift_JIS 次之。
        // 注：部分字节序列在多种编码下都能无错误解码，顺序只是概率取舍，
        // 无法在没有额外元数据时做到 100% 正确。
        for 编码项 in [
            encoding_rs::GB18030,
            encoding_rs::SHIFT_JIS,
            encoding_rs::BIG5,
            encoding_rs::EUC_KR,
            encoding_rs::EUC_JP,
        ] {
            let (解码文本, _, 解码有错) = 编码项.decode(原始字节);
            if !解码有错 {
                return 解码文本.into_owned();
            }
        }
    }
    待解码.to_string_lossy().into_owned()
}

/// 映射表分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum 映射类别 {
    /// 关键字（词法处理阶段）
    关键词表项,
    /// 标准库（语义处理阶段）
    标准库表项,
    /// 第三方库（按需加载）
    三方库表项,
}

impl 映射类别 {
    /// 获取分类对应的默认文件名
    pub fn 默认文件名(&self) -> &'static str {
        match self {
            映射类别::关键词表项 => "keywords.toml",
            映射类别::标准库表项 => "stdlib.toml",
            映射类别::三方库表项 => "crates.toml",
        }
    }

    /// 获取分类的显示名称（随当前语言变化）
    pub fn 显示名称(&self) -> String {
        let 键名 = match self {
            映射类别::关键词表项 => "mapping_cat_keywords",
            映射类别::标准库表项 => "mapping_cat_stdlib",
            映射类别::三方库表项 => "mapping_cat_third_party",
        };
        crate::语言::查句(键名)
    }
}

/// 映射表加载器 - 从 TOML 文件加载映射数据
///
/// 从语言包目录中按分类加载映射表，支持关键字/标准库单文件加载
/// 以及第三方库目录（crates/）下多文件合并加载。
#[derive(Debug, Clone)]
pub struct 映射加载器 {
    /// 语言包根目录
    根目录: PathBuf,
    /// 已加载的映射表（按分类和子分类组织）
    映射表: HashMap<映射类别, HashMap<String, HashMap<String, String>>>,
}

impl 映射加载器 {
    /// 创建新的加载器
    pub fn 新建加载器<路径参: AsRef<Path>>(语言包路径: 路径参) -> Self {
        let 根目录 = 语言包路径.as_ref().to_path_buf();
        Self {
            根目录,
            映射表: HashMap::new(),
        }
    }

    /// 加载指定分类的映射表
    pub fn 加载类别(&mut self, 分类: 映射类别) -> Result<(), 加载错误> {
        // 第三方库是目录，包含多个文件
        if 分类 == 映射类别::三方库表项 {
            return self.加载第三方库目录();
        }

        let 磁盘路径 = self.根目录.join(分类.默认文件名());

        if !磁盘路径.exists() {
            return Err(加载错误::新建文件缺失(
                加载目标::映射表,
                format!("{:?}", 磁盘路径),
            ));
        }

        let 内容 = std::fs::read_to_string(&磁盘路径).map_err(|错误值| {
            加载错误::新建读取失败(加载目标::映射表, None, 错误值.to_string())
        })?;

        let 分类映射 = 解析配置节(&内容).map_err(|错误值| {
            加载错误::新建解析失败(加载目标::映射表, None, 错误值.to_string())
        })?;
        self.映射表.insert(分类, 分类映射);
        Ok(())
    }

    /// 加载第三方库目录（crates/）下的所有 TOML 文件
    fn 加载第三方库目录(&mut self) -> Result<(), 加载错误> {
        let 目录路径 = self.根目录.join("crates");

        if !目录路径.exists() {
            // 目录不存在，静默返回
            return Ok(());
        }

        let mut 合并表 = HashMap::new();

        // 遍历目录中的所有 .toml 文件（按文件名排序，读取目录 顺序未定义，
        // 排序后合并结果与 映射管理器::自目录加载 保持一致且确定）；
        // 读取目录 或条目读取失败必须报错（不能静默吞掉，否则映射整体丢失）
        let mut 配置表路径列表: Vec<PathBuf> = std::fs::read_dir(&目录路径)
            .map_err(|错误值| 加载错误::目录读取失败 {
                详情: 错误值.to_string(),
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|错误值| 加载错误::目录读取失败 {
                详情: 错误值.to_string(),
            })?
            .into_iter()
            .map(|项| 项.path())
            .filter(|前索引| {
                前索引.extension().and_then(|字符串项| 字符串项.to_str()) == Some("toml")
            })
            .collect();
        配置表路径列表.sort();

        for 磁盘路径 in 配置表路径列表 {
            // 获取文件名（不含扩展名）作为分类标识；
            // 非 UTF-8 文件名按常见编码转码为 UTF-8（解码系统名），
            // 避免多个文件碰撞合并或分类键变为替换符
            let 基名 = 磁盘路径
                .file_stem()
                .map(解码系统名)
                .unwrap_or_else(|| 解码系统名(磁盘路径.as_os_str()));

            let 内容 = std::fs::read_to_string(&磁盘路径).map_err(|错误值| {
                加载错误::新建读取失败(
                    加载目标::第三方库,
                    Some(format!("{:?}", 磁盘路径)),
                    错误值.to_string(),
                )
            })?;

            // 将文件中的映射合并，加上文件分类前缀
            let 节表 = 解析配置节(&内容).map_err(|错误值| {
                加载错误::新建解析失败(
                    加载目标::第三方库,
                    Some(format!("{:?}", 磁盘路径)),
                    错误值.to_string(),
                )
            })?;
            // 节名排序后合并，保证 "文件名/子分类" 键的插入顺序确定
            let mut 子节名列表: Vec<&String> = 节表.keys().collect();
            子节名列表.sort();
            for 子节名 in 子节名列表 {
                let 分类键 = format!("{}/{}", 基名, 子节名);
                合并表.insert(分类键, 节表[子节名].clone());
            }
        }

        self.映射表.insert(映射类别::三方库表项, 合并表);
        Ok(())
    }

    /// 加载所有默认映射表
    pub fn 加载全部(&mut self) -> Result<(), 加载错误> {
        self.加载类别(映射类别::关键词表项)?;
        // 标准库可选：缺失时静默跳过（与 映射管理器::自目录加载 行为一致），
        // 存在但加载失败时必须报错
        if self.根目录.join("stdlib.toml").exists() {
            self.加载类别(映射类别::标准库表项)?;
        }
        // module_paths.toml（可选）：模块路径映射并入标准库分类的 ["模块路径"] 子节。
        // 在 stdlib 之后合并且仅补充缺失键，保证 stdlib 的同名映射优先
        // （与 映射管理器 的 module_paths 先、stdlib 后的覆盖顺序语义一致）
        let 模块路径磁盘项 = self.根目录.join("module_paths.toml");
        if 模块路径磁盘项.exists() {
            let 内容 = std::fs::read_to_string(&模块路径磁盘项).map_err(|错误值| {
                加载错误::新建读取失败(
                    加载目标::模块路径表,
                    Some(模块路径磁盘项.display().to_string()),
                    错误值.to_string(),
                )
            })?;
            let 节表 = 解析配置节(&内容).map_err(|错误值| {
                加载错误::新建解析失败(加载目标::模块路径表, None, 错误值.to_string())
            })?;
            if let Some(条目项) = 节表.get("模块路径") {
                let 标准库映射 = self.映射表.entry(映射类别::标准库表项).or_default();
                let 模块路径子节 = 标准库映射.entry("模块路径".to_string()).or_default();
                for (键名, 值项) in 条目项 {
                    模块路径子节
                        .entry(键名.clone())
                        .or_insert_with(|| 值项.clone());
                }
            }
        }
        // 第三方库可选：目录不存在时静默跳过，存在但加载失败时必须报错
        // （否则映射静默丢失，用户看到的是未翻译的标识符而非错误提示）
        if self.根目录.join("crates").is_dir() {
            self.加载类别(映射类别::三方库表项)?;
        }
        Ok(())
    }

    /// 获取指定分类的完整映射表（扁平化合并所有子分类）
    ///
    /// 子分类按名称排序后合并，保证同名键冲突时的胜出者确定
    /// （HashMap 遍历顺序随机会导致多次加载结果不一致）
    pub fn 取分类映射(&self, 分类: 映射类别) -> HashMap<String, String> {
        self.映射表.get(&分类).map(摊平节表).unwrap_or_default()
    }

    /// 获取指定分类和子分类的映射表
    pub fn 取子映射(
        &self, 分类: 映射类别, 子分类: &str
    ) -> Option<&HashMap<String, String>> {
        self.映射表
            .get(&分类)
            .and_then(|分类映射| 分类映射.get(子分类))
    }

    /// 查询单个映射条目（按子分类名升序查找，同名键命中顺序确定）
    pub fn 检索(&self, 分类: 映射类别, 母语词: &str) -> Option<String> {
        if let Some(分类映射) = self.映射表.get(&分类) {
            let mut 子节名列表: Vec<&String> = 分类映射.keys().collect();
            子节名列表.sort();
            for 子节名 in 子节名列表 {
                if let Some(英文词) = 分类映射[子节名].get(母语词) {
                    return Some(英文词.clone());
                }
            }
        }
        None
    }

    /// 反向查询（从英文查中文；按子分类名升序遍历，结果确定）
    pub fn 逆向检索(&self, 分类: 映射类别, 英文词: &str) -> Option<String> {
        if let Some(分类映射) = self.映射表.get(&分类) {
            let mut 子节名列表: Vec<&String> = 分类映射.keys().collect();
            子节名列表.sort();
            for 子节名 in 子节名列表 {
                for (母语词, 值项) in &分类映射[子节名] {
                    if 值项 == 英文词 {
                        return Some(母语词.clone());
                    }
                }
            }
        }
        None
    }

    /// 获取所有子分类名称（排序后返回，输出确定）
    pub fn 取子分类(&self, 分类: 映射类别) -> Vec<String> {
        let mut 子节名列表 = self
            .映射表
            .get(&分类)
            .map(|子表| 子表.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        子节名列表.sort();
        子节名列表
    }

    /// 统计映射条目数（扁平化后）
    pub fn 条目数量(&self, 分类: 映射类别) -> usize {
        self.取分类映射(分类).len()
    }
}

/// 便捷函数：加载关键字映射
pub fn 加载关键词映射<路径参: AsRef<Path>>(
    语言包路径: 路径参,
) -> Result<HashMap<String, String>, 加载错误> {
    let mut 加载器 = 映射加载器::新建加载器(语言包路径);
    加载器.加载类别(映射类别::关键词表项)?;
    Ok(加载器.取分类映射(映射类别::关键词表项))
}

/// 便捷函数：加载标准库映射
pub fn 加载标准库映射<路径参: AsRef<Path>>(
    语言包路径: 路径参,
) -> Result<HashMap<String, String>, 加载错误> {
    let mut 加载器 = 映射加载器::新建加载器(语言包路径);
    加载器.加载类别(映射类别::标准库表项)?;
    Ok(加载器.取分类映射(映射类别::标准库表项))
}

/// 便捷函数：加载所有映射
pub fn 加载全部映射<路径参: AsRef<Path>>(
    语言包路径: 路径参,
) -> Result<HashMap<映射类别, HashMap<String, String>>, 加载错误> {
    let mut 加载器 = 映射加载器::新建加载器(语言包路径);
    加载器.加载全部()?;

    let mut 全表 = HashMap::new();
    全表.insert(
        映射类别::关键词表项,
        加载器.取分类映射(映射类别::关键词表项),
    );
    全表.insert(
        映射类别::标准库表项,
        加载器.取分类映射(映射类别::标准库表项),
    );
    全表.insert(
        映射类别::三方库表项,
        加载器.取分类映射(映射类别::三方库表项),
    );

    Ok(全表)
}

/// 创建默认的关键字映射（内置备用）
///
/// 当语言包文件不可用时，使用此内置映射作为回退，
/// 覆盖 Rust 基础语法关键字、类型、错误处理、异步等常用构造。
pub fn 创建内置关键词映射() -> HashMap<String, String> {
    let mut 映射表 = HashMap::new();

    // 声明关键字
    映射表.insert("函数".into(), "fn".into());
    映射表.insert("变量".into(), "let".into());
    映射表.insert("可变".into(), "mut".into());
    映射表.insert("常量".into(), "const".into());
    映射表.insert("结构体".into(), "struct".into());
    映射表.insert("枚举".into(), "enum".into());
    映射表.insert("实现".into(), "impl".into());
    映射表.insert("特征".into(), "trait".into());
    映射表.insert("类型".into(), "type".into());
    映射表.insert("模块".into(), "mod".into());
    映射表.insert("公开".into(), "pub".into());
    映射表.insert("使用".into(), "use".into());
    映射表.insert("作为".into(), "as".into());
    映射表.insert("包".into(), "crate".into());
    映射表.insert("超级".into(), "super".into());
    映射表.insert("外部".into(), "extern".into());
    映射表.insert("静态".into(), "static".into());

    // 控制流
    映射表.insert("如果".into(), "if".into());
    映射表.insert("否则".into(), "else".into());
    映射表.insert("匹配".into(), "match".into());
    映射表.insert("循环".into(), "loop".into());
    映射表.insert("当".into(), "while".into());
    映射表.insert("对于".into(), "for".into());
    映射表.insert("在".into(), "in".into());
    映射表.insert("中断".into(), "break".into());
    映射表.insert("继续".into(), "continue".into());
    映射表.insert("返回".into(), "return".into());

    // 基本类型
    映射表.insert("整数".into(), "i32".into());
    映射表.insert("长整数".into(), "i64".into());
    映射表.insert("浮点数".into(), "f64".into());
    映射表.insert("单精度浮点数".into(), "f32".into());
    映射表.insert("文本".into(), "str".into());
    映射表.insert("布尔".into(), "bool".into());
    映射表.insert("字符".into(), "char".into());
    映射表.insert("字节".into(), "u8".into());

    // 特殊值
    映射表.insert("真".into(), "true".into());
    映射表.insert("假".into(), "false".into());
    映射表.insert("空".into(), "()".into());
    映射表.insert("自我".into(), "self".into());
    映射表.insert("自身".into(), "Self".into());

    // 错误处理
    映射表.insert("结果".into(), "Result".into());
    映射表.insert("选项".into(), "Option".into());
    映射表.insert("有值".into(), "Some".into());
    映射表.insert("无".into(), "None".into());
    映射表.insert("成功".into(), "Ok".into());
    映射表.insert("错误".into(), "Err".into());

    // 内存
    映射表.insert("引用".into(), "&".into());
    映射表.insert("解引用".into(), "*".into());
    映射表.insert("移动".into(), "move".into());
    映射表.insert("盒子".into(), "Box".into());

    // 异步
    映射表.insert("异步".into(), "async".into());
    映射表.insert("等待".into(), "await".into());
    映射表.insert("不安全".into(), "unsafe".into());
    映射表.insert("动态".into(), "dyn".into());

    映射表
}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    #[test]
    fn 测试加载关键字映射() {
        // 独立临时目录：固定路径会在并行测试/多次运行间交叉污染
        let 临时 = tempfile::tempdir().unwrap();
        let 临时路径 = 临时.path();

        // 写入测试映射表
        let 测试内容 = r#"
["声明"]
"函数" = "fn"
"变量" = "let"

["控制流"]
"如果" = "if"
"否则" = "else"
"#;
        std::fs::write(临时路径.join("keywords.toml"), 测试内容).unwrap();

        // 测试加载
        let mut 加载器 = 映射加载器::新建加载器(临时路径);
        assert!(加载器.加载类别(映射类别::关键词表项).is_ok());

        // 测试查询
        assert_eq!(
            加载器.检索(映射类别::关键词表项, "函数"),
            Some("fn".to_string())
        );
        assert_eq!(
            加载器.检索(映射类别::关键词表项, "如果"),
            Some("if".to_string())
        );
        assert_eq!(加载器.检索(映射类别::关键词表项, "不存在"), None);

        // 测试反向查询
        assert_eq!(
            加载器.逆向检索(映射类别::关键词表项, "fn"),
            Some("函数".to_string())
        );

        // 测试子分类
        let 子节列表 = 加载器.取子分类(映射类别::关键词表项);
        assert!(子节列表.contains(&"声明".to_string()));
        assert!(子节列表.contains(&"控制流".to_string()));

        // 测试条目数
        assert_eq!(加载器.条目数量(映射类别::关键词表项), 4);
    }

    #[test]
    fn 测试获取扁平化映射() {
        let 临时 = tempfile::tempdir().unwrap();
        let 临时路径 = 临时.path();

        let 测试内容 = r#"
["分类A"]
"甲" = "alpha"
"乙" = "beta"

["分类B"]
"丙" = "gamma"
"#;
        std::fs::write(临时路径.join("stdlib.toml"), 测试内容).unwrap();

        let mut 加载器 = 映射加载器::新建加载器(临时路径);
        加载器.加载类别(映射类别::标准库表项).unwrap();

        let 映射结果 = 加载器.取分类映射(映射类别::标准库表项);
        assert_eq!(映射结果.len(), 3);
        assert_eq!(映射结果.get("甲"), Some(&"alpha".to_string()));
        assert_eq!(映射结果.get("丙"), Some(&"gamma".to_string()));
        // 子分类名排序后返回，输出确定
        assert_eq!(
            加载器.取子分类(映射类别::标准库表项),
            vec!["分类A", "分类B"]
        );
    }

    #[test]
    fn 测试内置关键字映射() {
        let 映射表 = 创建内置关键词映射();

        assert_eq!(映射表.get("函数"), Some(&"fn".to_string()));
        assert_eq!(映射表.get("如果"), Some(&"if".to_string()));
        assert_eq!(映射表.get("整数"), Some(&"i32".to_string()));
        assert!(映射表.len() > 30);
    }

    /// 加载全部 把 module_paths.toml 并入标准库分类的 ["模块路径"] 子节，
    /// 且同名键 stdlib 优先（与 映射管理器::自目录加载 的覆盖顺序语义一致）
    #[test]
    fn 测试全量加载并入模块路径且标准库优先() {
        let 临时 = tempfile::tempdir().unwrap();
        let 临时路径 = 临时.path();

        std::fs::write(
            临时路径.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        // stdlib 与 module_paths 含同名键，stdlib 必须优先
        std::fs::write(
            临时路径.join("stdlib.toml"),
            "[\"模块路径\"]\n\"标准库\" = \"std\"\n\"文件系统\" = \"std::fs\"\n",
        )
        .unwrap();
        std::fs::write(
            临时路径.join("module_paths.toml"),
            "[\"模块路径\"]\n\"文件系统\" = \"fs\"\n\"字符串\" = \"string\"\n",
        )
        .unwrap();

        let mut 加载器 = 映射加载器::新建加载器(临时路径);
        加载器.加载全部().unwrap();

        let 模块路径子节 = 加载器.取子映射(映射类别::标准库表项, "模块路径").unwrap();
        // module_paths 的独有键并入标准库分类
        assert_eq!(模块路径子节.get("字符串"), Some(&"string".to_string()));
        // 同名键 stdlib 优先（module_paths 仅补充缺失键）
        assert_eq!(模块路径子节.get("文件系统"), Some(&"std::fs".to_string()));
    }

    /// 仅 keywords.toml 时 加载全部 不应报错（stdlib/module_paths/crates 均可选）
    #[test]
    fn 测试全量加载标准库可选() {
        let 临时 = tempfile::tempdir().unwrap();
        let 临时路径 = 临时.path();
        std::fs::write(
            临时路径.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();

        let mut 加载器 = 映射加载器::新建加载器(临时路径);
        加载器.加载全部().unwrap();
        assert_eq!(
            加载器.检索(映射类别::关键词表项, "函数"),
            Some("fn".to_string())
        );
    }

    /// 非 UTF-8（GBK 编码）的文件名被正确转码为 UTF-8 分类键，
    /// 而不是被 lossy 替换或与其它文件碰撞
    #[test]
    #[cfg(unix)]
    fn 测试非utf8文件名转码为utf8() {
        use std::os::unix::ffi::OsStringExt;

        let 临时 = tempfile::tempdir().unwrap();
        let 临时路径 = 临时.path();
        let 库目录 = 临时路径.join("crates");
        std::fs::create_dir_all(&库目录).unwrap();
        // keywords.toml 为必需文件
        std::fs::write(
            临时路径.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();

        // “序列化”的 GBK 字节：序=d0f2 列=c1d0 化=bbaf
        let 编码名 = std::ffi::OsString::from_vec(vec![0xD0, 0xF2, 0xC1, 0xD0, 0xBB, 0xAF]);
        let 磁盘路径 = 库目录.join(编码名).with_extension("toml");
        std::fs::write(&磁盘路径, "[\"标识符\"]\n\"服务器\" = \"Server\"\n").unwrap();

        // 前置断言：该文件名的字节确实不是合法 UTF-8
        assert!(磁盘路径.file_name().unwrap().to_str().is_none());

        let mut 加载器 = 映射加载器::新建加载器(临时路径);
        加载器.加载全部().unwrap();

        // 分类键应为转码后的“序列化/标识符”（GB18030 解码），映射可正常查询
        let 子节 = 加载器
            .取子映射(映射类别::三方库表项, "序列化/标识符")
            .unwrap();
        assert_eq!(子节.get("服务器"), Some(&"Server".to_string()));
    }

    /// 加载错误分类：文件缺失 / TOML 解析失败；第三方目录不存在静默成功
    #[test]
    fn 测试加载器错误分支() {
        let 临时 = tempfile::tempdir().unwrap();
        let mut 加载器 = 映射加载器::新建加载器(临时.path());
        // 空目录加载关键字：FileMissing
        assert!(matches!(
            加载器.加载类别(映射类别::关键词表项),
            Err(加载错误::文件缺失 { .. })
        ));
        // crates/ 目录不存在：静默 成功
        加载器.加载类别(映射类别::三方库表项).unwrap();
        assert_eq!(加载器.条目数量(映射类别::三方库表项), 0);
        // 未加载的分类查询全为 无 / 空
        assert_eq!(加载器.取子分类(映射类别::标准库表项), Vec::<String>::new());
        assert!(加载器.取子映射(映射类别::标准库表项, "标识符").is_none());
        assert_eq!(加载器.检索(映射类别::标准库表项, "x"), None);
        assert_eq!(加载器.逆向检索(映射类别::标准库表项, "x"), None);

        // 损坏 TOML：ParseFailed
        std::fs::write(
            临时.path().join("keywords.toml"),
            "[\"声明\"]\n\"a\"=\"1\"\n\"a\"=\"2\"\n",
        )
        .unwrap();
        assert!(matches!(
            加载器.加载类别(映射类别::关键词表项),
            Err(加载错误::解析失败 { .. })
        ));
        // 解析配置节 直接校验同样报错
        assert!(解析配置节("not = = toml").is_err());
    }

    /// 第三方目录：多文件、多子节按 `文件名/节名` 归类，非 toml 忽略，
    /// 扁平化查询/反查可用
    #[test]
    fn 测试加载第三方多文件() {
        let 临时 = tempfile::tempdir().unwrap();
        let 目录 = 临时.path();
        std::fs::write(
            目录.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        let 库目录 = 目录.join("crates");
        std::fs::create_dir_all(&库目录).unwrap();
        std::fs::write(
            库目录.join("serde.toml"),
            "[\"模块路径\"]\n\"序列化\" = \"serde\"\n[\"标识符\"]\n\"序列\" = \"Serialize\"\n",
        )
        .unwrap();
        std::fs::write(
            库目录.join("tokio.toml"),
            "[\"模块路径\"]\n\"异步\" = \"tokio\"\n",
        )
        .unwrap();
        // 非 toml 文件必须忽略
        std::fs::write(库目录.join("README.md"), "ignore me").unwrap();

        let mut 加载器 = 映射加载器::新建加载器(目录);
        加载器.加载类别(映射类别::三方库表项).unwrap();

        let mut 子节列表 = 加载器.取子分类(映射类别::三方库表项);
        子节列表.sort();
        assert_eq!(
            子节列表,
            vec![
                "serde/标识符".to_string(),
                "serde/模块路径".to_string(),
                "tokio/模块路径".to_string(),
            ]
        );
        assert_eq!(
            加载器.检索(映射类别::三方库表项, "序列"),
            Some("Serialize".to_string())
        );
        assert_eq!(
            加载器.逆向检索(映射类别::三方库表项, "tokio"),
            Some("异步".to_string())
        );
        // 扁平化后共 3 条
        assert_eq!(加载器.条目数量(映射类别::三方库表项), 3);
    }

    /// 加载全部 全量链路：stdlib 存在则加载；module_paths 仅补充 stdlib
    /// 缺失的键（stdlib 优先）；crates 目录一并加载；
    /// 加载全部映射 便捷函数返回三个分类
    #[test]
    fn 测试全量加载并入模块路径优先级() {
        let 临时 = tempfile::tempdir().unwrap();
        let 目录 = 临时.path();
        std::fs::write(
            目录.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        std::fs::write(
            目录.join("stdlib.toml"),
            "[\"标识符\"]\n\"字符串\" = \"String\"\n[\"模块路径\"]\n\"格式化\" = \"fmt\"\n",
        )
        .unwrap();
        // module_paths：与 stdlib 同键（stdlib 胜）+ 独有键（补入）
        std::fs::write(
            目录.join("module_paths.toml"),
            "[\"模块路径\"]\n\"格式化\" = \"SHOULD_NOT_WIN\"\n\"输入输出\" = \"io\"\n",
        )
        .unwrap();
        let 库目录 = 目录.join("crates");
        std::fs::create_dir_all(&库目录).unwrap();
        std::fs::write(
            库目录.join("serde.toml"),
            "[\"标识符\"]\n\"序列\" = \"Serialize\"\n",
        )
        .unwrap();

        let 全表 = 加载全部映射(目录).expect("全量加载应成功");
        assert_eq!(全表.len(), 3, "三个分类都应存在");
        let mut 加载器 = 映射加载器::新建加载器(目录);
        加载器.加载全部().unwrap();
        let 模块路径子节 = 加载器.取子映射(映射类别::标准库表项, "模块路径").unwrap();
        assert_eq!(
            模块路径子节.get("格式化"),
            Some(&"fmt".to_string()),
            "stdlib 同键优先"
        );
        assert_eq!(
            模块路径子节.get("输入输出"),
            Some(&"io".to_string()),
            "独有键应补入"
        );
        assert_eq!(
            加载器.检索(映射类别::标准库表项, "字符串"),
            Some("String".to_string())
        );
        assert_eq!(加载器.条目数量(映射类别::三方库表项), 1);

        // 便捷函数
        assert_eq!(
            加载标准库映射(目录).unwrap().get("字符串"),
            Some(&"String".to_string())
        );
    }

    /// 分类元信息：默认文件名稳定，显示名非空且随分类变化
    #[test]
    fn 测试分类元信息() {
        assert_eq!(映射类别::关键词表项.默认文件名(), "keywords.toml");
        assert_eq!(映射类别::标准库表项.默认文件名(), "stdlib.toml");
        assert_eq!(映射类别::三方库表项.默认文件名(), "crates.toml");
        let 名字列表 = [
            映射类别::关键词表项.显示名称(),
            映射类别::标准库表项.显示名称(),
            映射类别::三方库表项.显示名称(),
        ];
        for 显示名 in &名字列表 {
            assert!(!显示名.is_empty());
        }
        assert_ne!(名字列表[0], 名字列表[2]);
        // UTF-8 文件名走快速路径原样返回
        let 待解码 = std::ffi::OsStr::new("普通名字");
        assert_eq!(解码系统名(待解码), "普通名字");
    }
}
