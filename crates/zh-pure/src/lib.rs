//! 纯函数样例：判色 / 拆分词表 / 播报解析
//!
//! 三个函数都是「输入 → 输出」的纯函数：不读文件、不联网、不读环境变量、
//! 不使用全局状态。配套的方言测试写在 `tests-zh/纯函数.zh`（`#[测试]`），
//! 由 `build.zh` 转译后随 `cargo test` 自动执行。

/// 词形冻结：引擎 `转译输出` 的公开字段 `输出` 必须保持中文。
///
/// `build.zh` 跨包读取该字段；转译器按「同目录声明冻结」规则，
/// 需要本包也出现一次同名声明，否则会把 `输出` 按标准库词形译成英文。
#[derive(Debug)]
pub struct 转译输出契约 {
    pub 输出: String,
}

/// 可判定的颜色
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 颜色 {
    红,
    绿,
    蓝,
    黄,
}

/// 判色失败原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 判色故障 {
    /// 输入为空或仅含空白
    空输入,
    /// 不在可判定范围内的颜色名
    未知颜色,
}

/// 判色：颜色名（中/英，大小写不敏感，忽略前后空白）→ 颜色
pub fn 判色(颜色名: &str) -> Result<颜色, 判色故障> {
    let 颜色名 = 颜色名.trim();
    if 颜色名.is_empty() {
        return Err(判色故障::空输入);
    }
    let 判定值 = match 颜色名.to_lowercase().as_str() {
        "红" | "red" => 颜色::红,
        "绿" | "green" => 颜色::绿,
        "蓝" | "blue" => 颜色::蓝,
        "黄" | "yellow" => 颜色::黄,
        _ => return Err(判色故障::未知颜色),
    };
    Ok(判定值)
}

/// 拆分词表：按中英文逗号/顿号/分号/竖线/空白切分，丢弃空项
///
/// 空项一律丢弃（连续分隔符、首尾分隔符都不会产生空词条），
/// 因此空串与纯分隔符输入返回空表而不是报错。
pub fn 拆分词表(词表文本: &str) -> Vec<String> {
    let mut 词条们 = Vec::new();
    for 片段值 in 词表文本.split(|单个字符: char| match 单个字符 {
        ',' | '，' | '、' | ';' | '；' | '|' | '\t' | '\n' | '\r' => true,
        _ => 单个字符.is_whitespace(),
    }) {
        let 词条 = 片段值.trim();
        if !词条.is_empty() {
            词条们.push(词条.to_string());
        }
    }
    词条们
}

/// 播报级别
///
/// 变体名刻意避开 `信息`/`警告`：词库中它们已被日志库映射为日志级别，
/// 用作标识符会在转译时被改写。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 播报级别 {
    提示,
    警示,
    故障,
}

/// 一条播报（级别 + 内容）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct 播报 {
    pub 级别: 播报级别,
    pub 内容: String,
}

/// 播报解析失败原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 播报故障 {
    /// 输入为空或仅含空白
    空输入,
    /// 缺少 `级别|内容` 的竖线分隔符
    缺少分隔符,
    /// 级别不是 信息/警告/故障（或 info/warn/error）
    未知级别,
    /// 竖线之后的内容为空
    内容为空,
}

/// 播报解析：`级别|内容` → 播报
///
/// 只按**首个**竖线切分，内容中的竖线原样保留；级别与内容都忽略前后空白。
pub fn 播报解析(播报文本: &str) -> Result<播报, 播报故障> {
    let 播报文本 = 播报文本.trim();
    if 播报文本.is_empty() {
        return Err(播报故障::空输入);
    }
    let Some((级别值, 内容值)) = 播报文本.split_once('|') else {
        return Err(播报故障::缺少分隔符);
    };
    // 入参接受最自然的 信息/警告 写法，也接受与变体同名的 提示/警示；
    // 变体名避开 信息/警告，是为了不被词库改写（见枚举定义处的注释）
    let 级别值 = match 级别值.trim().to_lowercase().as_str() {
        "信息" | "提示" | "info" => 播报级别::提示,
        "警告" | "警示" | "warn" | "warning" => 播报级别::警示,
        "故障" | "error" => 播报级别::故障,
        _ => return Err(播报故障::未知级别),
    };
    let 内容值 = 内容值.trim();
    if 内容值.is_empty() {
        return Err(播报故障::内容为空);
    }
    Ok(播报 {
        级别: 级别值,
        内容: 内容值.to_string(),
    })
}
