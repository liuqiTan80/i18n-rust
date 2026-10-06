//! 全局语言与界面消息模块【zh 自举·批次1】
//! 本文件是源真相：由引导 rzc 转译为 语言.rs 后交 cargo 编译。
//! 再生成方式见 tools/zh-selfhost/regen.sh；请勿手改同目录下的 语言.rs。
//!
//! 引擎内所有用户可见消息（错误、诊断、日志、分类名等）经此模块
//! 按全局语言代码输出，彻底消除硬编码中文。
//!
//! - 语言由 CLI / LSP 启动时通过 [`设定语言`] 指定，默认 `zh`；
//! - 消息模板来自各语言包 `ui.toml` 的 `["界面消息"]` 节，编译期嵌入，
//!   运行期惰性解析（每语言一次），占位符 `{}` 按出现顺序替换；
//! - 缺失键依次回退：当前语言表 → 中文表 → 键名本身。
//!
//! 注：本模块是全项目公共底座，其导出 API（t/f/设定语言/当前语言/
//! builtin_* 等）被 engine/cli/lsp 数百处按名字调用，且与 build.rs 生成的
//! `BUILTIN_FILES`/`ui_table_for` 构成契约，故公开名保持英文（跨 包 稳定 ABI，
//! 属 rzc 官方支持的英文透传）；内部实现、私有函数、局部变量、测试均已中文化。

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
// 注：`互斥守卫` 必须从 use 导入后以裸名用于类型位——若写成 `标准库::同步::互斥守卫`
// 全路径，类型位置的 `同步` 会被同形词条解析成 标准库::Sync（大写，Sync auto-trait），
// 产生非法路径；use 语句位置的 `同步` 才正确解析为 标准库::同步模块。

/// 全局语言代码（默认 zh，由 CLI/LSP 启动时设置；可重复设置以便测试恢复）
static 当前语言存储: Mutex<String> = Mutex::new(String::new());

/// 获取语言锁；投毒时恢复而非 崩溃（锁内无关键不变量，
/// 仅存语言代码字符串，崩溃 传播会拖垮整个诊断输出链路）
fn 取语言锁() -> MutexGuard<'static, String> {
    当前语言存储.lock().unwrap_or_else(|投毒| 投毒.into_inner())
}

#[cfg(test)]
/// 测试级互斥锁：串行化所有修改全局语言或断言语言相关文本的测试，
/// 防止 test_set_language_de 等并行执行时把 CURRENT_LANG 污染为其他语言
pub(crate) static 语言测试锁: Mutex<()> = Mutex::new(());

/// 设置全局语言代码
pub fn 设定语言(代码: &str) {
    *取语言锁() = 代码.to_string();
}

/// RAII 语言作用域守卫：构造时设置语言，drop 时恢复进入前的值
///
/// 用于测试与临时切换场景，消除手工 `设定语言` + 末尾恢复
/// 的遗忘风险（忘记恢复会在并行测试间污染全局语言，是历史
/// flaky 的根因类别）。注意：守卫只保证恢复，不保证串行化；
/// 测试中仍应配合 [`语言测试锁`] 持锁使用。
pub struct 语言守卫 {
    先前: String,
}

impl 语言守卫 {
    /// 切换到指定语言，记录进入前的语言供 drop 时恢复
    pub fn 进入守卫(代码: &str) -> Self {
        let mut 语言守卫 = 取语言锁();
        let 先前 = 语言守卫.clone();
        *语言守卫 = 代码.to_string();
        Self { 先前 }
    }
}

impl Drop for 语言守卫 {
    fn drop(&mut self) {
        // 内存::取值 避免在 drop 中克隆；恢复后 previous 置空不再使用
        *取语言锁() = std::mem::take(&mut self.先前);
    }
}

/// 便捷入口：`let _g = 语言::附带语言("ru");` 作用域内生效，离开自动恢复
pub fn 附带语言(代码: &str) -> 语言守卫 {
    语言守卫::进入守卫(代码)
}

/// 测试语言守卫：持 [`语言测试锁`] 串行化并切换到指定语言，
/// drop 时先恢复原语言后释放锁（字段声明顺序决定 drop 顺序，
/// 断言 崩溃 也不会泄漏语言或锁）。
///
/// 加锁抗毒化：即使先前有测试持锁 崩溃 导致锁中毒，也接管继续
/// 执行，避免一次偶发断言失败沿锁毒化放大为整批测试连环失败。
/// 所有修改或断言全局语言的测试应统一使用本守卫。
#[cfg(test)]
pub(crate) struct 语言测试守卫 {
    _语言守卫: 语言守卫,
    _锁: MutexGuard<'static, ()>,
}

/// 测试入口：`let _g = 语言::测试语言("zh");` 串行化 + 语言钉住，离开自动恢复
#[cfg(test)]
pub(crate) fn 测试语言(代码: &str) -> 语言测试守卫 {
    let 锁 = 语言测试锁
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    语言测试守卫 {
        _语言守卫: 语言守卫::进入守卫(代码),
        _锁: 锁,
    }
}

/// 当前全局语言代码（未设置时为 zh）
pub fn 当前语言() -> String {
    let 语言守卫 = 取语言锁();
    if 语言守卫.is_empty() {
        "zh".to_string()
    } else {
        语言守卫.clone()
    }
}

/// 解析 ui.toml 内容为消息表（键 → 模板）
///
/// 函数名 `解析界面表` 是 build.rs 代码生成器契约符号——生成的
/// `builtin_generated.rs` 按英文名调用它初始化各语言消息表，故不可中文化。
///
/// 使用自有 字符串值 存储而非 `&'static str`：每语言表仅解析一次，
/// `Box::leak` 会永久泄漏全部消息文本，无必要。
fn 解析界面表(内容: &str) -> HashMap<String, String> {
    let mut 消息表 = HashMap::new();
    if let Ok(解析值) = toml::from_str::<toml::Value>(内容)
        && let Some(表) = 解析值.get("界面消息").and_then(|值| 值.as_table())
    {
        for (键, 值项) in 表 {
            if let Some(消息内容) = 值项.as_str() {
                消息表.insert(键.clone(), 消息内容.to_string());
            }
        }
    }
    消息表
}

// build.rs 扫描 lang-packs/ 自动生成的内嵌清单（勿手工编辑）：
// - BUILTIN_FILES：(语言代码, 相对路径, 内容) 全量清单
// - UI_TABLE_*：每语言 ui.toml 消息表静态实例 + ui_table_for 路由
// 新增语言包文件无需修改任何 Rust 代码，重新编译即自动纳入。
include!(concat!(env!("OUT_DIR"), "/builtin_generated.rs"));

/// 按语言代码取内置语言包文件的编译期内容（供 CLI / LSP 嵌入回退数据）
///
/// `file` 为语言包目录内的相对路径，如 `"keywords.toml"`、
/// `"crates/序列化.toml"`；未知语言或文件返回 `无`。
/// 清单由 build.rs 扫描 lang-packs/ 自动生成，覆盖全部语言与文件。
pub fn 内置文件(语言代码: &str, 文件名: &str) -> Option<&'static str> {
    BUILTIN_FILES
        .iter()
        .find(|(语言位, 文件位, _)| *语言位 == 语言代码 && *文件位 == 文件名)
        .map(|(_, _, 内容)| *内容)
}

/// 列出某语言的全部内嵌语言包文件（相对路径 → 内容）
///
/// 供 CLI / LSP 在无语言包目录时物化完整内置包：与磁盘语言包
/// 完全同源（关键字/宏/别名全量），避免硬编码旧表随语言包演进
/// 而残缺（如缺宏表、缺新关键字）导致转译不完整。
pub fn 内置语言文件(语言代码: &str) -> Vec<(&'static str, &'static str)> {
    BUILTIN_FILES
        .iter()
        .filter(|(语言位, _, _)| *语言位 == 语言代码)
        .map(|(_, 文件位, 内容位)| (*文件位, *内容位))
        .collect()
}

/// 按语言代码取消息表（未知语言回退中文表）；路由由 build.rs 生成
fn 取消息表(代码: &str) -> &'static HashMap<String, String> {
    ui_table_for(代码)
}

/// 内置语言包代码列表（从 lang-packs/ 目录自动生成，单一事实源）
///
/// 由 build.rs 扫描目录生成 [`BUILTIN_FILES`] 后在此去重排序；
/// 新增/删除语言包无需修改任何 Rust 代码，重新编译即自动纳入。
/// CLI / LSP 的语言清单（rzc lang、扩展名推断、系统语言检测）均以本函数为源。
pub fn 内置语言代码() -> Vec<&'static str> {
    let mut 代码列表: Vec<&'static str> =
        BUILTIN_FILES.iter().map(|(语言位, _, _)| *语言位).collect();
    代码列表.sort_unstable();
    代码列表.dedup();
    代码列表
}

/// 判断语言代码是否使用西里尔字母文字系统
///
/// 覆盖俄语 (ru)、乌克兰语 (uk)、白俄罗斯语、保加利亚语 (bg)、
/// 塞尔维亚语 (sr)、马其顿语等使用西里尔字母的语言。
/// 供 Unicode 混淆检测判断形似拉丁字母的西里尔字符是否为合法字符。
pub fn 使用西里尔文字(语言代码: &str) -> bool {
    matches!(语言代码, "ru" | "uk" | "bg" | "sr" | "mk" | "be")
}

/// 判断语言代码是否有对应的内置语言包
pub fn 拥有所属语言(代码: &str) -> bool {
    BUILTIN_FILES.iter().any(|(语言位, _, _)| *语言位 == 代码)
}

/// 内置语言包扩展名列表（来自各语言包 lang_info.toml 的 `"扩展名"` 字段）
///
/// 与 [`内置语言代码`] 同源（代码与扩展名一一对应），
/// 供 CLI / LSP 在缺省时推断方言文件扩展名，避免在各 包 中
/// 手工维护扩展名清单（历史上曾与语言包目录漂移）。
pub fn 内置语言扩展() -> Vec<String> {
    内置语言代码()
        .into_iter()
        .filter_map(语言包扩展名)
        .collect()
}

/// 解析语言包 lang_info.toml 的扩展名字段
fn 语言包扩展名(语言代码: &str) -> Option<String> {
    let 内容 = 内置文件(语言代码, "lang_info.toml")?;
    let 解析值: toml::Value = toml::from_str(内容).ok()?;
    解析值
        .get("语言包")?
        .get("扩展名")?
        .as_str()
        .map(String::from)
}

/// 取指定语言的消息模板；缺失时回退中文表，再缺失回退键名本身
fn 取消息于(代码: &str, 键: &str) -> String {
    if 代码 != "zh"
        && let Some(消息内容) = 取消息表(代码).get(键)
    {
        return 消息内容.to_string();
    }
    ui_table_for("zh")
        .get(键)
        .map(|串| 串.to_string())
        .unwrap_or_else(|| 键.to_string())
}

/// 取当前语言的消息模板；缺失时回退中文表，再缺失回退键名本身
pub fn 查句(键: &str) -> String {
    取消息于(&当前语言(), 键)
}

/// 按指定语言取消息模板并替换 `{}` 占位符（纯函数，测试与内部使用）
fn 格式化消息于(代码: &str, 键: &str, 参数列表: &[&str]) -> String {
    替换占位符(&取消息于(代码, 键), 参数列表)
}

/// 取消息模板并替换 `{}` 占位符（按出现顺序）
pub fn 查译(键: &str, 参数列表: &[&str]) -> String {
    格式化消息于(&当前语言(), 键, 参数列表)
}

/// 按出现顺序把 `{}` 替换为参数；参数多于占位符时忽略多余参数，
/// 参数不足时保留未替换的 `{}`（模板兜底，避免 崩溃）。
fn 替换占位符(模板: &str, 参数列表: &[&str]) -> String {
    let mut 结果串 = String::with_capacity(模板.len());
    let mut 剩余 = 模板;
    for 参数 in 参数列表 {
        match 剩余.find("{}") {
            Some(起始位) => {
                结果串.push_str(&剩余[..起始位]);
                结果串.push_str(参数);
                剩余 = &剩余[起始位 + 2..];
            }
            None => {
                // 占位符已耗尽：剩余模板原样保留
                结果串.push_str(剩余);
                return 结果串;
            }
        }
    }
    结果串.push_str(剩余);
    结果串
}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    #[test]
    fn 默认语言为中文() {
        let _守卫 = 测试语言("zh");
        assert_eq!(当前语言(), "zh");
        assert_eq!(取消息于("zh", "err_line_col"), "第 {} 行第 {} 列");
        assert_eq!(取消息于("zh", "cli_about"), "多语言 Rust 教学方言编译器");
    }

    #[test]
    fn 设置语言为德语() {
        let _守卫 = 测试语言("de");
        assert_eq!(当前语言(), "de");
        // 纯函数按语言取模板，不受全局状态干扰
        assert_eq!(取消息于("de", "err_line_col"), "Zeile {}, Spalte {}");
        assert_eq!(
            格式化消息于("de", "err_line_col", &["1", "2"]),
            "Zeile 1, Spalte 2"
        );
    }

    #[test]
    fn 语言守卫恢复先前值() {
        let _守卫 = 测试语言("zh");
        {
            let _语言 = 附带语言("ru");
            assert_eq!(当前语言(), "ru");
            // 嵌套守卫逐层恢复
            let _内层 = 附带语言("de");
            assert_eq!(当前语言(), "de");
        }
        assert_eq!(当前语言(), "zh");
    }

    #[test]
    fn 回退链路() {
        let _守卫 = 测试语言("de");
        // unicode_name_* 已同步到全部语言包（英文 Unicode 标准名），直接命中德语表
        assert_eq!(
            取消息于("de", "unicode_name_200B"),
            "\u{200b} (Zero Width Space)"
        );
        // 完全缺失的键回退键名
        assert_eq!(取消息于("de", "no_such_key"), "no_such_key");
    }

    #[test]
    fn 所有语言含公共键() {
        // 语言集合来自 lang-packs 目录（单一事实源），随语言包增删自动适应
        let 代码 = 内置语言代码();
        assert!(代码.len() >= 10, "内置语言包数量异常: {}", 代码.len());
        for 语言 in 代码 {
            let 表 = 取消息表(语言);
            for 键 in ["err_line_col", "diag_kind_error", "mapping_cat_keywords"] {
                assert!(表.contains_key(键), "{语言} 缺少 {键}");
            }
        }
    }

    #[test]
    fn 内置语言单一事实源() {
        // 代码列表与扩展名一一对应，且每语言都有 lang_info.toml 元数据
        let 代码 = 内置语言代码();
        let 扩展名 = 内置语言扩展();
        assert_eq!(代码.len(), 扩展名.len());
        for 语言 in &代码 {
            assert!(拥有所属语言(语言), "{语言} 应被识别为内置语言");
        }
        // 扩展名去重（任一语言有且仅有一个扩展名）
        let mut 已排序 = 扩展名.clone();
        已排序.sort();
        已排序.dedup();
        assert_eq!(已排序.len(), 扩展名.len());
    }

    #[test]
    fn 按序替换占位符() {
        assert_eq!(替换占位符("已导出到 {}", &["a.rs"]), "已导出到 a.rs");
        assert_eq!(
            替换占位符("项目根: {}，错误: {}", &["/p", "boom"]),
            "项目根: /p，错误: boom"
        );
        assert_eq!(替换占位符("只有 {}", &["a", "b"]), "只有 a");
        assert_eq!(替换占位符("{} 和 {}", &["a"]), "a 和 {}");
    }
}
