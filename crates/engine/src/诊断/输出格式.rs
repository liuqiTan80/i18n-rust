//! JSON 解析与格式化输出、未解析导入检测（CLI / LSP 共用）。

use super::教学诊断记录::教学诊断;
use super::诊断模型::编译器诊断;

/// 解析 rustc/cargo 的 JSON 诊断输出（逐行 JSON 对象）
///
/// 支持两种格式：
/// - rustc 直接输出：每行一个完整的诊断 JSON 对象
/// - cargo --message-format=json 包装：诊断嵌套在顶层消息字段中
pub fn 解析诊断输出(输出文本: &str) -> Vec<编译器诊断> {
    let mut 结果集 = Vec::new();
    for 当前行 in 输出文本.lines() {
        let 当前行 = 当前行.trim();
        if 当前行.is_empty() || !当前行.starts_with('{') {
            continue;
        }
        if let Ok(诊断) = serde_json::from_str::<编译器诊断>(当前行) {
            结果集.push(诊断);
        } else if let Ok(值对象) = serde_json::from_str::<serde_json::Value>(当前行) {
            // cargo --message-format=json 的 compiler-message 包装行：
            // 完整诊断嵌套在顶层消息字段中（build-finished 等行无消息对象，自动跳过）
            if let Ok(诊断) = serde_json::from_value::<编译器诊断>(值对象["message"].clone())
            {
                结果集.push(诊断);
            }
        }
    }
    结果集
}

/// 格式化后的诊断信息（用于文本输出）
pub struct 诊断文本格式 {
    pub 级别文字: String,
    pub 码文字: String,
    pub 诊断消息: String,
    pub 位置描述集: Vec<String>,
    pub 提示集: Vec<String>,
}

impl 教学诊断 {
    /// 格式化为结构化诊断
    pub fn 取格式(&self) -> 诊断文本格式 {
        let 级别文字 = self.级别.显示文字().to_string();
        let 码文字 = self
            .错误码
            .as_ref()
            .map(|码| format!("[{}]", 码))
            .unwrap_or_default();

        let 位置描述集 = self
            .位置集
            .iter()
            .filter(|该位置| 该位置.是否主因)
            .map(|该位置| {
                let mut 说明 = format!(
                    "  --> {}:{}:{}",
                    该位置.文件名称, 该位置.起始行号, 该位置.起始列号
                );
                if let Some(标签) = &该位置.标签文本 {
                    说明 = format!("{}\n      {}", 说明, 标签);
                }
                说明
            })
            .collect();

        诊断文本格式 {
            级别文字,
            码文字,
            诊断消息: self.译文消息.clone(),
            位置描述集,
            提示集: self.提示集.clone(),
        }
    }

    /// 格式化为文本（可直接输出到终端）
    pub fn 作为文字输出(&self) -> String {
        let 格式化结果 = self.取格式();
        let mut 输出文本 = String::new();

        // 第一行：错误级别 + 错误码 + 消息
        输出文本.push_str(&format!(
            "{}{}: {}\n",
            格式化结果.级别文字, 格式化结果.码文字, 格式化结果.诊断消息
        ));

        // 位置信息（只显示第一个主要位置）
        if let Some(该位置) = self.位置集.iter().find(|该位置| 该位置.是否主因) {
            输出文本.push_str(&format!(
                "  --> {}:{}:{}\n",
                该位置.文件名称, 该位置.起始行号, 该位置.起始列号
            ));
            if let Some(源码) = &该位置.源码文本 {
                输出文本.push_str(&format!("   | {}\n", 源码));
            }
        }

        // 所有权错误叙事提示
        if let Some(详情) = &self.所有权详情 {
            输出文本.push_str(&format!("📌 {}\n", 详情.叙事文本()));
        }

        // 教学提示（全部输出；此前仅取首条，其余被静默丢弃）
        for 提示 in &self.提示集 {
            输出文本.push_str(&format!("💡 {}\n", 提示));
        }

        输出文本
    }

    /// 批量格式化为文本
    pub fn 批量作为文字输出(诊断集: &[教学诊断]) -> String {
        let mut 输出文本 = String::new();
        for (序号, 诊断) in 诊断集.iter().enumerate() {
            if 序号 > 0 {
                输出文本.push_str("\n---\n\n");
            }
            输出文本.push_str(&诊断.作为文字输出());
        }
        输出文本
    }
}

/// 判断诊断消息是否为未解析导入类错误（英文原文匹配）
///
/// 覆盖 rustc E0432（`unresolved import`）与 E0433 两种消息格式，
/// 后者原文为 `failed to resolve: use of undeclared crate or module`。
pub fn 是否未解析导入消息(全文: &str) -> bool {
    全文.contains("unresolved import") || 全文.contains("use of undeclared crate or module")
}

/// 提取消息中所有反引号包裹路径的首段（:: 分隔）
///
/// 仅取形如路径的内容（标识符字符与 ::），过滤含空格的自由文本；
/// 翻译后的诊断同样保留反引号内容，故母语/英文消息均可提取。
pub fn 取反引号首段(全文: &str) -> Vec<String> {
    let mut 段集 = Vec::new();
    let mut 剩余 = 全文;
    while let Some(首处) = 剩余.find('`') {
        let 后半 = &剩余[首处 + 1..];
        let Some(尾处) = 后半.find('`') else {
            break;
        };
        let 内部 = &后半[..尾处];
        剩余 = &后半[尾处 + 1..];
        if !内部.is_empty()
            && 内部
                .chars()
                .all(|每字符| 每字符.is_alphanumeric() || 每字符 == '_' || 每字符 == ':')
        {
            let 首段 = 内部.split("::").next().unwrap_or(内部);
            if !首段.is_empty() {
                段集.push(首段.to_string());
            }
        }
    }
    段集
}

/// 从未解析导入消息提取候选包名（去重，排除标准库与保留路径）
///
/// 非未解析导入消息返回空列表。供 CLI 编译诊断提示与 LSP
/// 快捷修复代码动作共用：提示用户通过 `rzc add <crate>` 添加依赖。
/// 包名必须是 ASCII（字母/数字/下划线/连字符）；母语标识符
/// （如漏写 `包::` 前缀的中文模块名 "数据模型"）不是包名，
/// 提示 `rzc add 数据模型` 只会误导用户。
pub fn 未解析包候选(全文: &str) -> Vec<String> {
    if !是否未解析导入消息(全文) {
        return Vec::new();
    }
    const 标准路径集: &[&str] = &[
        "std",
        "core",
        "alloc",
        "self",
        "super",
        "crate",
        "proc_macro",
    ];
    let mut 结果集 = Vec::new();
    for 段 in 取反引号首段(全文) {
        if 标准路径集.contains(&段.as_str())
            || 段
                .chars()
                .next()
                .is_some_and(|每字符| 每字符.is_ascii_digit())
            || !段.is_ascii()
        {
            continue;
        }
        if !结果集.contains(&段) {
            结果集.push(段);
        }
    }
    结果集
}
