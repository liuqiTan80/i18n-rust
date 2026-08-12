//! 翻译缓存模块
//!
//! 维护中文 .zh 源码与翻译后英文 .rs 代码的对应关系。
//! 每当编辑器打开或修改 .zh 文件时，本模块将其翻译为英文，
//! 并记录行级映射信息供后续位置还原使用。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use i18n_rust_engine::词法处理;

/// 单个文档的翻译缓存条目
#[derive(Debug, Clone)]
pub struct 翻译条目 {
    /// 原始 .zh 文件的 URI
    pub 原始URI: String,
    /// 原始文件的磁盘路径
    pub 原始路径: PathBuf,
    /// 中文源码原文
    pub 中文内容: String,
    /// 翻译后的英文源码
    pub 英文内容: String,
    /// 虚拟 .rs 文件的 URI（通知 rust-analyzer 用）
    pub 虚拟URI: String,
    /// 虚拟 .rs 文件的磁盘路径
    pub 虚拟路径: PathBuf,
    /// 英文行号 → 中文行号的映射
    pub 行映射: Vec<u32>,
    /// 列偏移映射（用于精确还原列号）
    pub 列映射: Vec<列映射点>,
    /// 文档版本
    pub 版本: i32,
}

/// 列偏移映射的一个分段边界点
///
/// 在 [英文列, 下一段的英文列) 区间内：
///   中文列 = 英文列 - 偏移差
#[derive(Debug, Clone)]
pub struct 列映射点 {
    /// 该分段起始处的英文列号
    pub 英文列: u32,
    /// 该分段起始处的中文列号
    pub 中文列: u32,
    /// 累计字符偏移差（英文列 - 中文列）
    pub 偏移差: i32,
}

/// 翻译缓存管理器
///
/// 持有所有已打开文档的翻译结果，并提供线程安全的读写接口。
pub struct 翻译缓存 {
    /// URI → 翻译条目
    条目表: RwLock<HashMap<String, 翻译条目>>,
    /// 关键字映射表（中文 → 英文）
    关键字映射: Arc<HashMap<String, String>>,
    /// 宏名称集合（用于自动补充感叹号）
    宏名称集合: Arc<HashSet<String>>,
    /// 虚拟文件存放的临时目录
    临时目录: PathBuf,
}

impl 翻译缓存 {
    /// 创建新的翻译缓存
    ///
    /// - 关键字映射：用于词法翻译
    /// - 宏名称集合：用于自动补充宏感叹号
    /// - 临时目录：虚拟 .rs 文件的存放位置
    pub fn 新建(关键字映射: HashMap<String, String>, 宏名称集合: HashSet<String>, 临时目录: PathBuf) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&临时目录);
        Arc::new(Self {
            条目表: RwLock::new(HashMap::new()),
            关键字映射: Arc::new(关键字映射),
            宏名称集合: Arc::new(宏名称集合),
            临时目录,
        })
    }

    /// 打开或更新一个文档的翻译
    ///
    /// 将中文内容翻译为英文，写入虚拟文件，并记录行映射。
    pub fn 更新文档(&self, URI: &str, 内容: &str, 版本: i32) -> anyhow::Result<翻译条目> {
        let 原始路径 = URI转路径(URI);

        // 翻译源码
        let 英文内容 = 词法处理::转译源码带宏集合(内容, &self.关键字映射, &self.宏名称集合);

        // 生成虚拟文件路径（用哈希避免同名文件冲突）
        let 文件stem = 原始路径
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let 哈希 = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            原始路径.as_path().hash(&mut h);
            h.finish()
        };
        let 虚拟路径 = self.临时目录.join(format!("{}_{:x}.rs", 文件stem, 哈希));
        let 虚拟URI = 路径转URI(&虚拟路径);

        // 写入虚拟文件到磁盘（rust-analyzer 需要文件系统支持）
        std::fs::write(&虚拟路径, &英文内容)?;

        // 生成行映射和列映射
        let 行映射 = 生成行映射(内容, &英文内容);
        let 列映射 = 构建列映射(内容, &英文内容, &self.关键字映射, &self.宏名称集合);

        let 条目 = 翻译条目 {
            原始URI: URI.to_string(),
            原始路径,
            中文内容: 内容.to_string(),
            英文内容,
            虚拟URI,
            虚拟路径,
            行映射,
            列映射,
            版本,
        };

        // 存入缓存
        let mut 表 = self.条目表.write()
            .map_err(|_| anyhow::anyhow!("翻译缓存写锁获取失败"))?;
        表.insert(URI.to_string(), 条目.clone());

        log::info!("翻译缓存已更新: {} ({} 行)", URI, 内容.lines().count());
        Ok(条目)
    }

    /// 关闭文档，清理虚拟文件
    pub fn 关闭文档(&self, URI: &str) -> anyhow::Result<()> {
        let mut 表 = self.条目表.write()
            .map_err(|_| anyhow::anyhow!("翻译缓存写锁获取失败"))?;
        if let Some(条目) = 表.remove(URI) {
            let _ = std::fs::remove_file(&条目.虚拟路径);
            log::info!("翻译缓存已移除: {}", URI);
        }
        Ok(())
    }

    /// 根据原始 URI 查询翻译条目
    pub fn 查询原始(&self, URI: &str) -> Option<翻译条目> {
        let 表 = self.条目表.read().ok()?;
        表.get(URI).cloned()
    }

    /// 根据虚拟 URI 反查原始条目
    pub fn 从虚拟URI查询(&self, 虚拟URI: &str) -> Option<翻译条目> {
        let 表 = self.条目表.read().ok()?;
        for 条目 in 表.values() {
            if 条目.虚拟URI == 虚拟URI {
                return Some(条目.clone());
            }
        }
        None
    }

    /// 根据虚拟路径反查原始条目
    pub fn 从虚拟路径查询(&self, 虚拟路径: &Path) -> Option<翻译条目> {
        let 表 = self.条目表.read().ok()?;
        for 条目 in 表.values() {
            if 条目.虚拟路径 == 虚拟路径 {
                return Some(条目.clone());
            }
        }
        None
    }

    /// 获取所有虚拟文件路径
    pub fn 所有虚拟路径(&self) -> Vec<PathBuf> {
        let 表 = match self.条目表.read() {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };
        表.values().map(|e| e.虚拟路径.clone()).collect()
    }

    /// 获取关键字映射的引用
    pub fn 关键字映射(&self) -> &HashMap<String, String> {
        &self.关键字映射
    }

    /// 将英文（虚拟文件）列号映射回中文（原始文件）列号
    pub fn 英文列转中文列(&self, URI: &str, 行: u32, 英文列: u32) -> u32 {
        if let Some(条目) = self.从虚拟URI查询(URI) {
            英文列转中文列_单条(&条目, 行, 英文列)
        } else {
            英文列
        }
    }

    /// 将中文（原始文件）列号转换为英文（虚拟文件）列号
    pub fn 中文列转英文列(&self, URI: &str, 行: u32, 中文列: u32) -> u32 {
        if let Some(条目) = self.从虚拟URI查询(URI) {
            中文列转英文列_单条(&条目, 行, 中文列)
        } else {
            中文列
        }
    }
}

/// 将 file:// URI 转换为文件路径
fn URI转路径(URI: &str) -> PathBuf {
    if let Some(路径) = URI.strip_prefix("file://") {
        // 简单处理 URL 编码的常见字符
        let 路径 = 路径.replace("%20", " ").replace("%23", "#");
        PathBuf::from(路径)
    } else {
        PathBuf::from(URI)
    }
}

/// 将文件路径转换为 file:// URI
fn 路径转URI(路径: &Path) -> String {
    format!("file://{}", 路径.display())
}

/// 根据列映射条目将英文列转换为中文列
fn 英文列转中文列_单条(条目: &翻译条目, _行: u32, 英文列: u32) -> u32 {
    if 条目.列映射.is_empty() { return 英文列; }
    // 二分查找：找到最后一个 英文列 <= 目标英文列 的分段
    let mut 结果 = 条目.列映射[0].偏移差;
    for 点 in &条目.列映射 {
        if 点.英文列 <= 英文列 {
            结果 = 点.偏移差;
        } else {
            break;
        }
    }
    (英文列 as i32 - 结果).max(0) as u32
}

/// 根据列映射条目将中文列转换为英文列
fn 中文列转英文列_单条(条目: &翻译条目, _行: u32, 中文列: u32) -> u32 {
    if 条目.列映射.is_empty() { return 中文列; }
    let mut 结果 = 条目.列映射[0].偏移差;
    for 点 in &条目.列映射 {
        if 点.中文列 <= 中文列 {
            结果 = 点.偏移差;
        } else {
            break;
        }
    }
    (中文列 as i32 + 结果).max(0) as u32
}

/// 构建列偏移映射
///
/// 扫描中文 token 流，模拟翻译过程，记录每次替换导致的列偏移变化。
/// 不依赖英文 token 流，避免宏感叹号插入导致的 token 不对齐问题。
fn 构建列映射(
    中文内容: &str,
    _英文内容: &str,
    关键字映射: &HashMap<String, String>,
    宏名称集合: &HashSet<String>,
) -> Vec<列映射点> {
    use rustc_lexer::tokenize;

    let 中文令牌: Vec<_> = tokenize(中文内容).collect();
    let mut 映射点 = Vec::new();
    let mut 中文列 = 0u32;
    let mut 英文列 = 0u32;
    let mut 累计差 = 0i32; // 英文列 - 中文列
    let mut 当前偏移 = 0usize;

    映射点.push(列映射点 { 英文列: 0, 中文列: 0, 偏移差: 0 });

    use rustc_lexer::TokenKind;
    let 是空白 = |k: TokenKind| matches!(k, TokenKind::Whitespace | TokenKind::LineComment { .. } | TokenKind::BlockComment { .. });
    let 是标识符 = |k: TokenKind| matches!(k, TokenKind::Ident { .. } | TokenKind::RawIdent { .. });

    for i in 0..中文令牌.len() {
        let token = &中文令牌[i];
        let token文本 = &中文内容[当前偏移..][..token.len];
        当前偏移 += token.len;

        // 空白 token：两列同步前进
        if 是空白(token.kind) {
            let 长 = token文本.chars().count() as u32;
            中文列 += 长;
            英文列 += 长;
            continue;
        }

        if 是标识符(token.kind) {
            let 原始名 = if token文本.starts_with("r#") { &token文本[2..] } else { token文本 };
            let 中文长 = token文本.chars().count() as u32;

            // 检查是否为宏名（后跟开括号）
            let 是宏调用 = 宏名称集合.contains(原始名) && 后面是开括号(&中文令牌, i);

            if 是宏调用 {
                let 英文名 = 关键字映射.get(原始名).map(|s| s.as_str()).unwrap_or(原始名);
                let 英文名长 = 英文名.chars().count() as u32;
                let 是否翻译了宏名 = 关键字映射.contains_key(原始名);

                // 翻译后的英文输出：英文名 + 可选的 !
                // 仅当宏名被翻译时才补 !（未翻译时 ! 已在中文源码中作为独立 token）
                let 英文输出长 = if 是否翻译了宏名 {
                    英文名长 + 1 // +1 for inserted !
                } else {
                    中文长 // 保持原样（! 作为独立 token 会在后续迭代中处理）
                };

                累计差 += 英文输出长 as i32 - 中文长 as i32;
                中文列 += 中文长;
                英文列 += 英文输出长;
            } else {
                // 普通标识符：检查是否被关键字映射替换
                let 英文输出长 = if let Some(英文名) = 关键字映射.get(原始名) {
                    英文名.chars().count() as u32
                } else {
                    中文长
                };

                累计差 += 英文输出长 as i32 - 中文长 as i32;
                中文列 += 中文长;
                英文列 += 英文输出长;
            }
        } else {
            // 非标识符、非空白 token：原样输出
            let 长 = token文本.chars().count() as u32;
            中文列 += 长;
            英文列 += 长;
        }

        // 如果偏移差变化了，记录新的分段边界
        if 累计差 != 映射点.last().map(|p| p.偏移差).unwrap_or(0) {
            映射点.push(列映射点 { 英文列, 中文列, 偏移差: 累计差 });
        }
    }

    映射点
}

/// 检查指定 token 之后下一个非空白 token 是否是开括号（( [ {）
fn 后面是开括号(令牌: &[rustc_lexer::Token], 当前: usize) -> bool {
    use rustc_lexer::TokenKind;
    let 是空白 = |k: TokenKind| matches!(k, TokenKind::Whitespace | TokenKind::LineComment { .. } | TokenKind::BlockComment { .. });
    for j in (当前 + 1)..令牌.len() {
        if 是空白(令牌[j].kind) { continue; }
        return matches!(
            令牌[j].kind,
            TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::OpenBrace
        );
    }
    false
}

/// 生成英文行号到中文行号的映射
///
/// 由于当前翻译是逐行替换关键字，行数保持一致，
/// 因此映射为 0→0, 1→1, 2→2, ...
/// 未来若支持多行展开/折叠，此处需要更复杂的算法。
fn 生成行映射(中文内容: &str, 英文内容: &str) -> Vec<u32> {
    let 英文行数 = 英文内容.lines().count() as u32;
    let 中文行数 = 中文内容.lines().count() as u32;
    let 最小行数 = 英文行数.min(中文行数);

    // 基础 1:1 映射
    let mut 映射: Vec<u32> = (0..英文行数).collect();

    // 对于超出中文行数的英文行，映射到最后一行
    for i in 最小行数..英文行数 {
        映射[i as usize] = 中文行数.saturating_sub(1);
    }

    映射
}

#[cfg(test)]
mod 测试 {
    use super::*;

    fn 测试映射() -> HashMap<String, String> {
        HashMap::from([
            ("函数".into(), "fn".into()),
            ("让".into(), "let".into()),
            ("可变".into(), "mut".into()),
            ("如果".into(), "if".into()),
            ("否则".into(), "else".into()),
        ])
    }

    #[test]
    fn 测试_更新文档() {
        let 临时 = tempfile::tempdir().unwrap();
        let 缓存 = 翻译缓存::新建(测试映射(), HashSet::new(), 临时.path().to_path_buf());

        let 条目 = 缓存.更新文档("file:///test/main.zh", "让 可变 x = 5;", 1).unwrap();
        assert_eq!(条目.英文内容, "let mut x = 5;");
        assert!(条目.虚拟路径.exists());
    }

    #[test]
    fn 测试_关闭文档() {
        let 临时 = tempfile::tempdir().unwrap();
        let 缓存 = 翻译缓存::新建(测试映射(), HashSet::new(), 临时.path().to_path_buf());

        let 条目 = 缓存.更新文档("file:///test/main.zh", "让 x = 1;", 1).unwrap();
        assert!(条目.虚拟路径.exists());

        缓存.关闭文档("file:///test/main.zh").unwrap();
        assert!(!条目.虚拟路径.exists());
        assert!(缓存.查询原始("file:///test/main.zh").is_none());
    }

    #[test]
    fn 测试_从虚拟URI查询() {
        let 临时 = tempfile::tempdir().unwrap();
        let 缓存 = 翻译缓存::新建(测试映射(), HashSet::new(), 临时.path().to_path_buf());

        let 条目 = 缓存.更新文档("file:///test/main.zh", "让 x = 1;", 1).unwrap();
        let 查到 = 缓存.从虚拟URI查询(&条目.虚拟URI).unwrap();
        assert_eq!(查到.原始URI, "file:///test/main.zh");
    }

    #[test]
    fn 测试_行映射() {
        let 映射 = 生成行映射("行0\n行1\n行2", "line0\nline1\nline2");
        assert_eq!(映射, vec![0, 1, 2]);
    }
}
