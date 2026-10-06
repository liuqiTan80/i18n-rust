// JSON 诊断解析、格式化输出与未解析导入检测

use super::教学::教学诊断;
use super::诊断模型::编译器诊断;

/// 解析 rustc/cargo 的 JSON 诊断输出（逐行 JSON 对象）
///
/// 支持两种格式：
/// - rustc 直接输出：每行一个完整的诊断 JSON 对象
/// - cargo --message-format=json 包装：诊断嵌套在 `message` 字段中
pub fn 解析诊断输出(输出: &str) -> Vec<编译器诊断> {
    let mut 诊断结果 = Vec::new();
    for 每行 in 输出.lines() {
        let 每行 = 每行.trim();
        if 每行.is_empty() || !每行.starts_with('{') {
            continue;
        }
        if let Ok(诊断) = serde_json::from_str::<编译器诊断>(每行) {
            诊断结果.push(诊断);
        } else if let Ok(包装值) = serde_json::from_str::<serde_json::Value>(每行) {
            // cargo --message-format=json 的 compiler-message 包装行：
            // 完整诊断嵌套在顶层 message 字段中（build-finished 等行无 message 对象，自动跳过）
            if let Ok(诊断) = serde_json::from_value::<编译器诊断>(包装值["message"].clone())
            {
                诊断结果.push(诊断);
            }
        }
    }
    诊断结果
}

/// 格式化后的诊断信息（用于文本输出）
pub struct 格式化诊断 {
    pub 级别文字: String,
    pub 码文字: String,
    pub 消息: String,
    pub 位置描述: Vec<String>,
    pub 教学提示: Vec<String>,
}

impl 教学诊断 {
    /// 格式化为结构化诊断
    pub fn 格式化为结构(&self) -> 格式化诊断 {
        let 级别文字 = self.严重级别.显示文字().to_string();
        let 码文字 = self
            .错误码
            .as_ref()
            .map(|码| format!("[{}]", 码))
            .unwrap_or_default();

        let 位置描述 = self
            .位置列表
            .iter()
            .filter(|跨度项| 跨度项.是主跨度)
            .map(|跨度项| {
                let mut 描述 = format!(
                    "  --> {}:{}:{}",
                    跨度项.源文件名, 跨度项.起始行, 跨度项.起始列
                );
                if let Some(标签) = &跨度项.标签 {
                    描述 = format!("{}\n      {}", 描述, 标签);
                }
                描述
            })
            .collect();

        格式化诊断 {
            级别文字,
            码文字,
            消息: self.翻译消息.clone(),
            位置描述,
            教学提示: self.教学提示.clone(),
        }
    }

    /// 格式化为文本（可直接输出到终端）
    pub fn 格式化为文本(&self) -> String {
        let 已格式化 = self.格式化为结构();
        let mut 输出 = String::new();

        // 第一行：错误级别 + 错误码 + 消息
        输出.push_str(&format!(
            "{}{}: {}\n",
            已格式化.级别文字, 已格式化.码文字, 已格式化.消息
        ));

        // 位置信息（只显示第一个主要位置）
        if let Some(位置) = self.位置列表.iter().find(|跨度项| 跨度项.是主跨度) {
            输出.push_str(&format!(
                "  --> {}:{}:{}\n",
                位置.源文件名, 位置.起始行, 位置.起始列
            ));
            if let Some(源码) = &位置.源码文本 {
                输出.push_str(&format!("   | {}\n", 源码));
            }
        }

        // 所有权错误叙事提示
        if let Some(详情) = &self.所有权详情 {
            输出.push_str(&format!("📌 {}\n", 详情.叙事文本()));
        }

        // 教学提示（全部输出；此前仅取首条，其余被静默丢弃）
        for 提示 in &self.教学提示 {
            输出.push_str(&format!("💡 {}\n", 提示));
        }

        输出
    }

    /// 批量格式化为文本
    pub fn 批量格式化为文本(诊断列表: &[教学诊断]) -> String {
        let mut 输出 = String::new();
        for (序号, 诊断) in 诊断列表.iter().enumerate() {
            if 序号 > 0 {
                输出.push_str("\n---\n\n");
            }
            输出.push_str(&诊断.格式化为文本());
        }
        输出
    }
}

/// 判断诊断消息是否为未解析导入类错误（英文原文匹配）
///
/// 覆盖 rustc E0432（unresolved import）与 E0433（failed to resolve:
/// use of undeclared crate or module）两种消息格式。
pub fn 是未解析导入消息(消息: &str) -> bool {
    消息.contains("unresolved import") || 消息.contains("use of undeclared crate or module")
}

/// 提取消息中所有反引号包裹路径的首段（:: 分隔）
///
/// 仅取形如路径的内容（标识符字符与 ::），过滤含空格的自由文本；
/// 翻译后的诊断同样保留反引号内容，故母语/英文消息均可提取。
pub fn 抽取反引号首段(待扫描文本: &str) -> Vec<String> {
    let mut 段列表 = Vec::new();
    let mut 剩余 = 待扫描文本;
    while let Some(起点) = 剩余.find('`') {
        let 之后 = &剩余[起点 + 1..];
        let Some(终点) = 之后.find('`') else {
            break;
        };
        let 内部 = &之后[..终点];
        剩余 = &之后[终点 + 1..];
        if !内部.is_empty()
            && 内部
                .chars()
                .all(|字符项| 字符项.is_alphanumeric() || 字符项 == '_' || 字符项 == ':')
        {
            let 首段 = 内部.split("::").next().unwrap_or(内部);
            if !首段.is_empty() {
                段列表.push(首段.to_string());
            }
        }
    }
    段列表
}

/// 从未解析导入消息提取候选 crate 名（去重，排除标准库与保留路径）
///
/// 非未解析导入消息返回空列表。供 CLI 编译诊断提示与 LSP
/// 快捷修复代码动作共用：提示用户通过 `rzc add <crate>` 添加依赖。
/// crate 名必须是 ASCII（字母/数字/下划线/连字符）；母语标识符
/// （如漏写 `包::` 前缀的中文模块名 "数据模型"）不是 crate，
/// 提示 `rzc add 数据模型` 只会误导用户。
pub fn 未解析包候选(消息: &str) -> Vec<String> {
    if !是未解析导入消息(消息) {
        return Vec::new();
    }
    let mut 候选列表 = Vec::new();
    for 段 in 抽取反引号首段(消息) {
        if matches!(
            段.as_str(),
            "std" | "core" | "alloc" | "self" | "super" | "crate" | "proc_macro"
        ) || 段
            .chars()
            .next()
            .is_some_and(|字符项| 字符项.is_ascii_digit())
            || !段.is_ascii()
        {
            continue;
        }
        if !候选列表.contains(&段) {
            候选列表.push(段);
        }
    }
    候选列表
}
