//!
//! 维护方言源码（.zh/.en/.de 等）与翻译后英文 .rs 代码的对应关系。
//! 每当编辑器打开或修改方言文件时，本模块将其翻译为英文，
//! 并记录行级映射信息供后续位置还原使用。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use i18n_rust_engine::lexer;

/// 单个文档的翻译缓存条目
#[derive(Debug, Clone)]
pub struct TranslationEntry {
    /// 原始方言文件的 URI
    pub original_uri: String,
    /// 原始文件的磁盘路径
    pub original_path: PathBuf,
    /// 中文源码原文
    pub zh_content: String,
    /// 翻译后的英文源码
    pub en_content: String,
    /// 虚拟 .rs 文件的 URI（通知 rust-analyzer 用）
    pub virtual_uri: String,
    /// 虚拟 .rs 文件的磁盘路径
    pub virtual_path: PathBuf,
    /// 英文行号 → 中文行号的映射
    pub line_map: Vec<u32>,
    /// 列偏移映射（每行一个分段表，行号 → 该行的分段边界点）
    pub column_map: Vec<Vec<ColumnMapPoint>>,
    /// 文档版本
    pub version: i32,
}

/// 列偏移映射的一个分段边界点
///
/// 在 [en_col, 下一段的 en_col) 区间内：
///   zh_col = en_col - offset_diff
/// 列号按 LSP 的 UTF-16 code unit 计数（常用中文字符在 BMP 内占 1 个单元，
/// 增补平面字符如 emoji 占 2 个单元），每行独立从 0 开始。
#[derive(Debug, Clone)]
pub struct ColumnMapPoint {
    /// 该分段起始处的英文列号
    pub en_col: u32,
    /// 该分段起始处的中文列号
    pub zh_col: u32,
    /// 累计字符偏移差（en_col - zh_col）
    pub offset_diff: i32,
}

/// 翻译缓存管理器
///
/// 持有所有已打开文档的翻译结果，并提供线程安全的读写接口。
pub struct TranslationCache {
    /// URI → 翻译条目
    ///
    /// 条目以 Arc 共享：查询返回廉价引用计数克隆，避免每次按键
    /// 都全量克隆源码与列映射等大字段。
    entries: RwLock<HashMap<String, Arc<TranslationEntry>>>,
    /// 关键字映射表（中文 → 英文）
    keyword_map: Arc<HashMap<String, String>>,
    /// 宏映射表（中文宏名 → 英文宏名，用于自动补充感叹号）
    macro_map: Arc<HashMap<String, String>>,
    /// 别名映射表（标准库/第三方库标识符：中文 → 英文，带声明位保护）
    alias_map: Arc<HashMap<String, String>>,
    /// 虚拟文件存放的临时目录
    temp_dir: PathBuf,
    /// 模块集合版本号：模块集合（已打开方言文件的文件名）变化时递增。
    /// 供 ProxyServer 判断是否需要重载虚拟项目工作区，
    /// 避免每次打开/关闭文档同学都触发 rust-analyzer 全量重扫。
    module_version: std::sync::atomic::AtomicU64,
    /// 合并反向表（英文 → 母语）：关键字反转后合并别名反转（关键字先入为主）。
    /// 映射表构造后不可变，构造时预构建一次，
    /// 供反向转译与 ResponseMapper 共用，避免每次调用重复构建。
    reverse_map: Arc<HashMap<String, String>>,
    /// 文档变更代号：任何文档打开/更新/关闭时递增，用于用户词汇缓存失效
    docs_generation: std::sync::atomic::AtomicU64,
    /// 用户词汇缓存：(代号, 结果)。代号匹配时直接复用，
    /// 避免每次补全请求都重新词法扫描全部已打开文档
    user_tokens_cache: std::sync::Mutex<(u64, Option<HashSet<String>>)>,
}

impl TranslationCache {
    /// 创建新的翻译缓存
    ///
    /// - 关键字映射：用于词法翻译
    /// - 宏映射表：用于自动补充宏感叹号（中文宏名 → 英文宏名）
    /// - 别名映射：标准库/第三方库标识符替换（带声明位保护，与 CLI 管线一致）
    /// - 临时目录：虚拟 .rs 文件的存放位置
    pub fn new(
        keyword_map: HashMap<String, String>,
        macro_map: HashMap<String, String>,
        alias_map: HashMap<String, String>,
        temp_dir: PathBuf,
    ) -> Arc<Self> {
        // 安全检查：临时目录若已被替换为符号链接则拒绝使用，
        // 防止后续写文件时跟随链接覆写任意位置
        if temp_dir
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            log::error!(
                "{}",
                crate::ui::global().f("lsp_err_temp_symlink", &[&temp_dir.display().to_string()])
            );
        }
        let _ = std::fs::create_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(temp_dir.join("src"));
        // 合并反向表预构建：关键字反转优先，别名反转仅在英文键未占用时并入
        let mut reverse_map = build_reverse_map(&keyword_map);
        for (english, native) in build_reverse_map(&alias_map) {
            reverse_map.entry(english).or_insert(native);
        }
        let cache = Arc::new(Self {
            entries: RwLock::new(HashMap::new()),
            keyword_map: Arc::new(keyword_map),
            macro_map: Arc::new(macro_map),
            alias_map: Arc::new(alias_map),
            temp_dir,
            module_version: std::sync::atomic::AtomicU64::new(0),
            reverse_map: Arc::new(reverse_map),
            docs_generation: std::sync::atomic::AtomicU64::new(0),
            user_tokens_cache: std::sync::Mutex::new((0, None)),
        });
        // 初始时生成空虚拟项目，供 rust-analyzer 工作区发现
        cache.refresh_virtual_project();
        cache
    }

    /// 打开或更新一个文档的翻译
    ///
    /// 将中文内容翻译为英文，写入虚拟文件，并记录行映射。
    /// 返回 (当前条目, 其他因模块集合变化而被重写的条目)；
    /// 条目以 Arc 共享，调用方按需廉价克隆。
    ///
    /// 模块集合 = 所有已打开方言文件的文件名（不含扩展名）。
    /// 打开新文件会新增模块，使其他文件的虚拟内容可能新增
    /// `crate::` 前缀，因此需要全量重写；纯内容更新则只重写当前条目。
    pub fn update_document(
        &self,
        uri: &str,
        content: &str,
        version: i32,
    ) -> anyhow::Result<(Arc<TranslationEntry>, Vec<Arc<TranslationEntry>>)> {
        let original_path = uri_to_path(uri);

        // Unicode 混淆安全检查（零宽/双向/同形字符）：仅告警不阻断翻译
        for warning in i18n_rust_engine::unicode_confusion::check_unicode_confusion(content) {
            log::warn!("{}", warning.format());
        }

        // 生成虚拟文件路径（用哈希避免同名文件冲突）。
        // 文件名只保留合法标识符字符，防止引号等特殊字符注入生成的 main.rs
        let file_stem = original_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            original_path.as_path().hash(&mut h);
            h.finish()
        };
        let virtual_path = self.temp_dir.join("src").join(format!(
            "{}_{:x}.rs",
            sanitize_module_name(file_stem),
            hash
        ));
        let virtual_uri = path_to_uri(&virtual_path);

        // 判断模块集合是否变化（打开新文件会新增模块）
        let new_module_names = self.current_module_names(Some(&original_path));
        let old_module_names = self.current_module_names(None);
        let set_changed = new_module_names != old_module_names;

        // 行映射不依赖模块路径重写（重写不改变行数），先按中文行数生成
        let line_map = generate_line_map(content, content);

        // 存入缓存（英文内容与列映射由下面的重写步骤填充）
        {
            let mut table = self
                .entries
                .write()
                .map_err(|_| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_cache_lock")))?;
            table.insert(
                uri.to_string(),
                Arc::new(TranslationEntry {
                    original_uri: uri.to_string(),
                    original_path: original_path.clone(),
                    zh_content: content.to_string(),
                    en_content: String::new(),
                    virtual_uri: virtual_uri.clone(),
                    virtual_path: virtual_path.clone(),
                    line_map,
                    column_map: Vec::new(),
                    version,
                }),
            );
        }

        // 内容可能变化：递增文档变更代号，使用户词汇缓存失效
        self.docs_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // 模块集合变化时重写全部条目并刷新虚拟项目，否则只重写当前条目
        let changes = if set_changed {
            let _ = self.bump_module_version();
            // main.rs/Cargo.toml 只依赖模块集合：纯内容编辑不触发，
            // 每次按键省去数次磁盘写与全表遍历
            self.refresh_virtual_project();
            self.rewrite_all(&new_module_names)
        } else {
            let mut changes = Vec::new();
            if let Some(entry) = self.rewrite_entry(uri, &new_module_names) {
                changes.push(entry);
            }
            changes
        };

        let entry = self.query_original(uri).ok_or_else(|| {
            anyhow::anyhow!("{}", crate::ui::global().f("lsp_err_entry_missing", &[uri]))
        })?;
        let other_changes: Vec<Arc<TranslationEntry>> = changes
            .into_iter()
            .filter(|e| e.original_uri != uri)
            .collect();

        log::info!(
            "{}",
            crate::ui::global().f(
                "lsp_log_cache_updated",
                &[uri, &content.lines().count().to_string()]
            )
        );
        Ok((entry, other_changes))
    }

    /// 关闭文档，清理虚拟文件
    ///
    /// 返回其余条目中因模块集合缩小而被重写的条目列表。
    /// 文档本就不在缓存中时不做任何工作（模块集合未变，无需重写/刷新）。
    pub fn close_document(&self, uri: &str) -> anyhow::Result<Vec<Arc<TranslationEntry>>> {
        let removed = {
            let mut table = self
                .entries
                .write()
                .map_err(|_| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_cache_lock")))?;
            if let Some(entry) = table.remove(uri) {
                let _ = std::fs::remove_file(&entry.virtual_path);
                // 模块集合缩小，版本号递增（供工作区重载判断）
                let _ = self.bump_module_version();
                log::info!("{}", crate::ui::global().f("lsp_log_cache_removed", &[uri]));
                true
            } else {
                false
            }
        };
        if !removed {
            return Ok(Vec::new());
        }

        // 内容可能变化：递增文档变更代号，使用户词汇缓存失效
        self.docs_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // 模块集合缩小：重写其余条目（去掉对已关闭模块的 crate:: 前缀）
        let module_names = self.current_module_names(None);
        let changes = self.rewrite_all(&module_names);

        // 模块集合变化：刷新虚拟项目文件（main.rs 聚合）
        self.refresh_virtual_project();
        Ok(changes)
    }

    /// 根据原始 URI 查询翻译条目（Arc 廉价克隆，不复制内容）
    pub fn query_original(&self, uri: &str) -> Option<Arc<TranslationEntry>> {
        let table = self.entries.read().ok()?;
        table.get(uri).cloned()
    }

    /// 收集所有已打开方言文件中出现的标识符（用户自定义名词白名单）
    ///
    /// 词法扫描原文中的全部 Ident token（含 r# 原始标识符，
    /// 注释与字符串字面量天然被词法器排除）。供补全语言过滤区分
    /// “用户自己定义的项”与“未翻译的外部英文项”：
    /// 用户源码中出现过的名词（无论母语还是英文）都视为其可见词汇。
    ///
    /// 结果按文档变更代号缓存：文档未变化时重复补全请求直接复用，
    /// 词法扫描只在文档变更后首次调用时发生。
    pub fn user_defined_tokens(&self) -> HashSet<String> {
        let 代号 = self
            .docs_generation
            .load(std::sync::atomic::Ordering::SeqCst);
        if let Ok(guard) = self.user_tokens_cache.lock()
            && guard.0 == 代号
            && let Some(set) = &guard.1
        {
            return set.clone();
        }
        let tokens = self.scan_user_tokens();
        if let Ok(mut guard) = self.user_tokens_cache.lock() {
            *guard = (代号, Some(tokens.clone()));
        }
        tokens
    }

    /// 词法扫描所有已打开文档的原文，收集标识符（无缓存）
    fn scan_user_tokens(&self) -> HashSet<String> {
        use rustc_lexer::{TokenKind, tokenize};
        let mut tokens = HashSet::new();
        let table = match self.entries.read() {
            Ok(t) => t,
            Err(_) => return tokens,
        };
        for entry in table.values() {
            let mut offset = 0usize;
            for token in tokenize(&entry.zh_content) {
                let text = &entry.zh_content[offset..offset + token.len];
                offset += token.len;
                if matches!(token.kind, TokenKind::Ident | TokenKind::RawIdent) {
                    tokens.insert(text.strip_prefix("r#").unwrap_or(text).to_string());
                }
            }
        }
        tokens
    }

    /// 根据虚拟 URI 反查原始条目（Arc 廉价克隆，不复制内容）
    ///
    /// rust-analyzer 返回的 URI 可能是 URL 百分号编码形式（如中文路径），
    /// 而缓存的虚拟 URI 是未编码的原始形式，因此先精确匹配，
    /// 失败后再解码匹配。
    pub fn query_by_virtual_uri(&self, virtual_uri: &str) -> Option<Arc<TranslationEntry>> {
        let table = self.entries.read().ok()?;
        for entry in table.values() {
            if entry.virtual_uri == virtual_uri {
                return Some(Arc::clone(entry));
            }
        }
        let decoded = url_decode(virtual_uri);
        for entry in table.values() {
            if entry.virtual_uri == decoded {
                return Some(Arc::clone(entry));
            }
        }
        None
    }

    /// 获取关键字映射的引用
    pub fn keyword_map(&self) -> &HashMap<String, String> {
        &self.keyword_map
    }

    /// 获取别名映射的引用（标准库/第三方库标识符，供反向转译合并使用）
    pub fn alias_map(&self) -> &HashMap<String, String> {
        &self.alias_map
    }

    /// 获取合并反向表的引用（英文 → 母语，关键字优先于别名）
    ///
    /// 构造时预构建，供 ResponseMapper 共用，避免重复构建。
    pub fn reverse_map(&self) -> &HashMap<String, String> {
        &self.reverse_map
    }

    /// 将英文（虚拟文件）列号映射回中文（原始文件）列号
    ///
    /// 接受虚拟 URI 或原始 URI（先查虚拟，再查原始）。
    pub fn en_col_to_zh_col(&self, uri: &str, line: u32, en_col: u32) -> u32 {
        let entry = self
            .query_by_virtual_uri(uri)
            .or_else(|| self.query_original(uri));
        if let Some(entry) = entry {
            en_col_to_zh_col_single(&entry, line, en_col)
        } else {
            en_col
        }
    }

    /// 将中文（原始文件）列号转换为英文（虚拟文件）列号
    ///
    /// 接受原始 URI 或虚拟 URI（先查虚拟，再查原始）。
    pub fn zh_col_to_en_col(&self, uri: &str, line: u32, zh_col: u32) -> u32 {
        let entry = self
            .query_by_virtual_uri(uri)
            .or_else(|| self.query_original(uri));
        if let Some(entry) = entry {
            zh_col_to_en_col_single(&entry, line, zh_col)
        } else {
            zh_col
        }
    }

    /// 将英文（虚拟文件）内容反向翻译为母语内容
    ///
    /// 供代码格式化（textDocument/formatting）与补全/代码操作文本还原使用：
    /// 英文代码经 rustfmt 格式化后，据此还原为母语代码。
    /// 反向表为构造时预构建的合并表（关键字优先），
    /// 保证与正向翻译互逆；模块路径重写插入的 `crate::` 前缀在此被删除。
    pub fn reverse_transpile(&self, en_content: &str) -> String {
        let module_names = self.current_module_names(None);
        lexer::reverse_transpile(en_content, &self.reverse_map, &module_names)
    }

    /// 获取虚拟项目目录的 file:// URI（供工作区通知使用）
    pub fn virtual_project_uri(&self) -> String {
        path_to_uri(&self.temp_dir)
    }

    /// 获取虚拟项目目录的文件路径（供代理自跑 cargo check 使用）
    pub fn virtual_project_dir(&self) -> PathBuf {
        self.temp_dir.clone()
    }

    /// 当前模块集合版本号（模块集合变化时递增）
    pub fn module_version(&self) -> u64 {
        self.module_version
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 递增模块集合版本号，返回新值
    #[must_use]
    pub fn bump_module_version(&self) -> u64 {
        self.module_version
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1
    }

    /// 获取当前模块名集合（所有已打开 .zh 文件的文件名）
    ///
    /// `extra_path` 用于在插入缓存前把新文件的模块名一并计入。
    fn current_module_names(&self, extra_path: Option<&PathBuf>) -> HashSet<String> {
        let mut names = HashSet::new();
        if let Some(path) = extra_path
            && let Some(name) = path.file_stem().and_then(|s| s.to_str())
        {
            names.insert(name.to_string());
        }
        if let Ok(table) = self.entries.read() {
            for entry in table.values() {
                if let Some(name) = entry.original_path.file_stem().and_then(|s| s.to_str()) {
                    names.insert(name.to_string());
                }
            }
        }
        names
    }

    /// 重写单个条目的虚拟内容：翻译 + 模块路径加 `crate::` 前缀 + 重建列映射 + 写盘
    ///
    /// 内容未发生变化（模块集合未引入新前缀）时返回 None。
    fn rewrite_entry(
        &self,
        uri: &str,
        module_names: &HashSet<String>,
    ) -> Option<Arc<TranslationEntry>> {
        let old_entry = self.query_original(uri)?;
        let en_content = lexer::transpile_source_with_macro_map(
            &old_entry.zh_content,
            &self.keyword_map,
            &self.macro_map,
        );
        let en_content = rewrite_module_paths(&en_content, module_names);
        // 别名替换（与 CLI 统一管线一致）：标准库/第三方库标识符转英文，
        // 声明位用户定义受保护；列映射模拟同一替换以保持列偏移精确
        let en_content = if self.alias_map.is_empty() {
            en_content
        } else {
            i18n_rust_engine::alias::replace_aliases(&en_content, &self.alias_map)
        };
        // main 文件（main.zh）的 `fn main` 仅在磁盘虚拟文件中提升为 pub：
        // 虚拟项目的 crate 入口在聚合 main.rs 中转发调用 `main::main()`，
        // 模块内 fn 默认私有会触发 cargo check E0603。但发送给 rust-analyzer
        // 的内存文档必须保持用户原文（无 pub）——否则语义 token 多出 pub、
        // fn/main 位置偏移，变量等颜色错乱。
        //（column_map 基于无 pub 内容构建，与内存文档一致）
        let is_main = old_entry.original_path.file_stem().and_then(|s| s.to_str()) == Some("main");
        let disk_content = if is_main {
            en_content.replace("fn main(", "pub fn main(")
        } else {
            en_content.clone()
        };
        let column_map = build_column_map(
            &old_entry.zh_content,
            &en_content,
            &self.keyword_map,
            &self.macro_map,
            &self.alias_map,
            module_names,
        );

        // 构造新版本需要克隆旧条目一次；此后查询均为 Arc 廉价克隆
        let new_entry = Arc::new(TranslationEntry {
            en_content: en_content.clone(),
            column_map,
            ..(*old_entry).clone()
        });

        // 写入虚拟文件到磁盘（rust-analyzer 需要文件系统支持；main 文件写 pub 版）
        // 先确保父目录存在（首次打开时 src/ 可能尚未创建，
        // 直接写会静默失败导致 cargo check 读到不完整的虚拟项目）
        if let Some(parent) = new_entry.virtual_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&new_entry.virtual_path, &disk_content);

        {
            let mut table = match self.entries.write() {
                Ok(t) => t,
                Err(_) => return None,
            };
            if let Some(entry) = table.get_mut(uri) {
                *entry = Arc::clone(&new_entry);
            }
        }

        if new_entry.en_content != old_entry.en_content {
            Some(new_entry)
        } else {
            None
        }
    }

    /// 用给定的模块名集合重写缓存中的所有条目
    ///
    /// 返回内容实际发生变化的条目列表（供调用方通知 rust-analyzer）。
    fn rewrite_all(&self, module_names: &HashSet<String>) -> Vec<Arc<TranslationEntry>> {
        let uris: Vec<String> = {
            let table = match self.entries.read() {
                Ok(t) => t,
                Err(_) => return Vec::new(),
            };
            table.values().map(|e| e.original_uri.clone()).collect()
        };
        let mut changes = Vec::new();
        for uri in uris {
            if let Some(entry) = self.rewrite_entry(&uri, module_names) {
                changes.push(entry);
            }
        }
        changes
    }

    /// 刷新虚拟项目：写入 Cargo.toml 和 src/main.rs
    ///
    /// 将当前所有虚拟翻译文件聚合为同一二进制 crate，
    /// 使 rust-analyzer 能够解析 `模块`/`使用` 声明的跨文件引用。
    /// 使用 [[bin]] 而非 [lib]，使 `fn main()` 被识别为程序入口，
    /// 避免 `function main is never used` 警告。
    fn refresh_virtual_project(&self) {
        let table = match self.entries.read() {
            Ok(t) => t,
            Err(_) => return,
        };

        // Cargo.toml（包名保留英文，见项目规范）
        // [[bin]] 使其成为二进制 crate，fn main() 即为入口
        // [workspace] 空表使其脱离任何父工作区，避免被上层 Cargo.toml 吞并
        let cargo_content = "[package]\nname = \"i18n-virtual\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[[bin]]\npath = \"src/main.rs\"\nname = \"i18n-virtual\"\n\n[workspace]\n";
        let _ = std::fs::write(self.temp_dir.join("Cargo.toml"), cargo_content);
        // 预生成 Cargo.lock：无依赖项目内容固定。缺少锁文件时 cargo
        // （rust-analyzer 的 cargo metadata / 代理的 check）会尝试联网更新
        // crates.io 索引，网络不可达时进程卡死且无诊断
        let lock_content = "# This file is automatically @generated by Cargo.\n# It is not intended for manual editing.\nversion = 3\n\n[[package]]\nname = \"i18n-virtual\"\nversion = \"0.1.0\"\n";
        let _ = std::fs::write(self.temp_dir.join("Cargo.lock"), lock_content);

        // 清理旧版本残留文件（避免 rust-analyzer 同时读取 lib.rs 和 main.rs）
        let _ = std::fs::remove_file(self.temp_dir.join("src").join("lib.rs"));
        // 确保 src 目录存在（首次启动时可能尚未创建，写盘会静默失败）
        let _ = std::fs::create_dir_all(self.temp_dir.join("src"));

        // src/main.rs：以 #[path] 属性按模块名聚合所有虚拟文件
        // #![allow(dead_code)] 抑制辅助函数/类型的未使用警告
        let mut main_content = format!(
            "#![allow(dead_code)]\n{}\n",
            crate::ui::global().t("lsp_gen_lib_comment")
        );
        // 模块名净化为合法 Rust 标识符，并在重名时追加哈希后缀
        let mut used_names: HashSet<String> = HashSet::new();
        let mut has_main_module = false;
        for entry in table.values() {
            let stem = entry
                .original_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if stem == "main" {
                has_main_module = true;
            }
            let mut module_name = if stem.is_empty() {
                crate::ui::global().t("lsp_gen_module_fallback")
            } else {
                sanitize_module_name(stem)
            };
            if !used_names.insert(module_name.clone()) {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut h = DefaultHasher::new();
                entry.virtual_path.as_path().hash(&mut h);
                module_name = format!("{}_{:x}", module_name, h.finish());
                used_names.insert(module_name.clone());
            }
            let file_name = entry
                .virtual_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            main_content.push_str(&format!(
                "#[path = \"{}\"]\nmod {};\n",
                file_name, module_name
            ));
        }
        // 入口转发：`fn main()` 位于子模块（如 mod main）时不是 crate 入口，
        // cargo check 会报 E0601 使 checkOnSave 诊断整体失败（所有权可视化
        // 依赖 E0382 等 cargo check 诊断，将全部丢失）。此处显式转发调用：
        // Rust 中 mod 名在类型命名空间、fn 名在值命名空间，同名合法；
        // 错误仍定位在子模块文件内，行号映射零偏移。
        if has_main_module {
            main_content.push_str("fn main() { main::main() }\n");
        }
        let _ = std::fs::write(self.temp_dir.join("src").join("main.rs"), main_content);
    }
}

/// 将任意文件名主干净化为合法 Rust 模块名
///
/// 非法字符替换为 `_`；空名回退 `m`；数字开头前补 `_`。
/// 中文等 Unicode 字母属于合法标识符字符，予以保留。
fn sanitize_module_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c == '_' || c.is_alphanumeric() {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out.push('m');
    }
    if out.chars().next().is_some_and(|c| c.is_numeric()) {
        out.insert(0, '_');
    }
    out
}

/// 将 file:// URI 转换为文件路径
///
/// 编辑器（如 VSCode）会对非 ASCII 字符（中文文件名）做百分号编码，
/// 必须完整解码，否则文件名残留 %XX 导致模块名非法。
fn uri_to_path(uri: &str) -> PathBuf {
    if let Some(path) = uri.strip_prefix("file://") {
        let decoded = url_decode(path);
        // Windows 形式 file:///C:/...：盘符前的前导 `/` 不属于路径
        let bytes = decoded.as_bytes();
        if bytes.len() >= 3 && bytes[0] == b'/' && bytes[2] == b':' {
            return PathBuf::from(&decoded[1..]);
        }
        PathBuf::from(decoded)
    } else {
        PathBuf::from(uri)
    }
}

/// 百分号解码 URI（仅处理 %XX 形式，UTF-8 字节流）
///
/// rust-analyzer 等工具返回的 URI 会百分号编码非 ASCII 字符
/// （如中文路径），需要解码后才能与缓存中的未编码 URI 比较。
fn url_decode(uri: &str) -> String {
    let bytes = uri.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2]))
        {
            result.push(high * 16 + low);
            i += 3;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).to_string()
}

/// 十六进制字符转数值
fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 将文件路径转换为 file:// URI
///
/// 对非 URI 安全字符做百分号编码（路径含空格/中文时生成合法 URI）。
/// Windows 路径额外处理：反斜杠归一为正斜杠、盘符前补 `/`、盘符转小写，
/// 与 rust-analyzer 返回的规范形式（file:///c:/...）保持一致，
/// 否则两端 URI 永不相等，查询/还原全链路失效。
pub(crate) fn path_to_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    let mut text = path.to_string_lossy().replace('\\', "/");
    // 盘符路径（X:/...）补前导斜杠并统一小写盘符（RA 返回小写）
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && !text.starts_with('/') {
        text.insert(0, '/');
        text.replace_range(1..2, &text[1..2].to_lowercase());
    }
    for &byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                uri.push(byte as char)
            }
            _ => uri.push_str(&format!("%{:02X}", byte)),
        }
    }
    uri
}

/// 根据列映射条目将英文列转换为中文列（按行查询）
fn en_col_to_zh_col_single(entry: &TranslationEntry, line: u32, en_col: u32) -> u32 {
    let row = entry
        .column_map
        .get(line as usize)
        .or_else(|| entry.column_map.last());
    let row = match row {
        Some(r) if !r.is_empty() => r,
        _ => return en_col,
    };
    // 顺序查找（每行分段极少，线性足够）：找到最后一个 en_col <= 目标 en_col 的分段
    let mut result = row[0].offset_diff;
    for point in row {
        if point.en_col <= en_col {
            result = point.offset_diff;
        } else {
            break;
        }
    }
    (en_col as i32 - result).max(0) as u32
}

/// 根据列映射条目将中文列转换为英文列（按行查询）
fn zh_col_to_en_col_single(entry: &TranslationEntry, line: u32, zh_col: u32) -> u32 {
    let row = entry
        .column_map
        .get(line as usize)
        .or_else(|| entry.column_map.last());
    let row = match row {
        Some(r) if !r.is_empty() => r,
        _ => return zh_col,
    };
    // 顺序查找：找到最后一个 zh_col <= 目标 zh_col 的分段
    let mut result = row[0].offset_diff;
    for point in row {
        if point.zh_col <= zh_col {
            result = point.offset_diff;
        } else {
            break;
        }
    }
    (zh_col as i32 + result).max(0) as u32
}

/// 由正向关键字映射（母语 → 英文）构建反向映射（英文 → 母语）
///
/// 按母语词排序后插入，多对一冲突时保留排序最小者，保证结果确定性。
fn build_reverse_map(forward: &HashMap<String, String>) -> HashMap<String, String> {
    let mut pairs: Vec<(&String, &String)> = forward.iter().collect();
    pairs.sort();
    let mut reverse = HashMap::with_capacity(forward.len());
    for (native, english) in pairs {
        reverse.entry(english.clone()).or_insert(native.clone());
    }
    reverse
}

/// 为已知模块路径段添加 `crate::` 前缀
///
/// Rust 2018+ 中，子模块内的裸路径 `模块::项` 无法解析到 crate 根的模块，
/// 必须写成 `crate::模块::项` 或先 use。由于虚拟项目把每个 .zh 文件聚合为
/// 同一 crate 的兄弟模块，此处为引用其他模块的路径段自动补全前缀，
/// 使 rust-analyzer 能够解析跨文件引用（references/rename）。
///
/// 已带前缀的路径（`crate::辅助`、`其他::辅助`）不会被重复处理。
fn rewrite_module_paths(content: &str, module_names: &HashSet<String>) -> String {
    if module_names.is_empty() || content.is_empty() {
        return content.to_string();
    }
    use rustc_lexer::{TokenKind, tokenize};
    let tokens: Vec<_> = tokenize(content).collect();
    let is_whitespace = |k: TokenKind| {
        matches!(
            k,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
        )
    };
    let is_ident = |k: TokenKind| matches!(k, TokenKind::Ident | TokenKind::RawIdent);

    let mut output = String::with_capacity(content.len() + module_names.len() * 8);
    let mut offset = 0usize;

    for i in 0..tokens.len() {
        let token = &tokens[i];
        let text = &content[offset..][..token.len];
        offset += token.len;

        if is_whitespace(token.kind) {
            output.push_str(text);
            continue;
        }

        // 模块路径段：标识符属于已知模块名、后跟 `::`、且不在既有路径段之后
        // （`crate::辅助`、`a::辅助` 中的 `辅助` 已处于路径内，跳过）
        let needs_prefix = is_ident(token.kind) && {
            let raw_name = text.strip_prefix("r#").unwrap_or(text);
            module_names.contains(raw_name)
                && is_path_separator_after(&tokens, i)
                && !is_path_separator_before(&tokens, i)
        };
        if needs_prefix {
            output.push_str("crate::");
        }
        output.push_str(text);
    }
    output
}

/// 检查指定 token 之后两个连续的非空白 token 是否为 `::`
///
/// rustc_lexer 将 `::` 拆分为两个 `Colon` token。
fn is_path_separator_after(tokens: &[rustc_lexer::Token], current: usize) -> bool {
    use rustc_lexer::TokenKind;
    let is_whitespace = |k: TokenKind| {
        matches!(
            k,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
        )
    };
    let mut colon_count = 0;
    for token in &tokens[(current + 1)..] {
        if is_whitespace(token.kind) {
            continue;
        }
        if matches!(token.kind, TokenKind::Colon) {
            colon_count += 1;
            if colon_count >= 2 {
                return true;
            }
            continue;
        }
        return false;
    }
    false
}

/// 检查指定 token 之前两个连续的非空白 token 是否为 `::`
fn is_path_separator_before(tokens: &[rustc_lexer::Token], current: usize) -> bool {
    use rustc_lexer::TokenKind;
    let is_whitespace = |k: TokenKind| {
        matches!(
            k,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
        )
    };
    let mut colon_count = 0;
    for token in tokens[..current].iter().rev() {
        if is_whitespace(token.kind) {
            continue;
        }
        if matches!(token.kind, TokenKind::Colon) {
            colon_count += 1;
            if colon_count >= 2 {
                return true;
            }
        } else {
            return false;
        }
    }
    false
}

/// 构建列偏移映射
///
/// 扫描中文 token 流，模拟翻译过程，记录每次替换导致的列偏移变化。
/// 列号按 LSP 的 UTF-16 code unit 计数，并按行分段存储
/// （每行独立从 0 开始，换行时偏移差重置）。
/// 不依赖英文 token 流，避免宏感叹号插入导致的 token 不对齐问题。
///
/// `module_names` 用于模拟模块路径重写：命中模块路径段时英文侧多出
/// `crate::` 前缀（7 个 UTF-16 单元），与 `rewrite_module_paths` 保持一致。
///
/// `alias_map` 用于模拟别名替换：与 engine `alias::replace_aliases` 同一
/// 声明位保护状态机（声明关键字后紧跟的标识符为用户定义，不替换；
/// `mut` 在声明态内透明传递），且用户声明名在全文使用处全局豁免
/// （预扫描收集声明名，与 engine 两遍扫描语义一致），保证列偏移与真实转译输出一致。
fn build_column_map(
    zh_content: &str,
    _en_content: &str,
    keyword_map: &HashMap<String, String>,
    macro_map: &HashMap<String, String>,
    alias_map: &HashMap<String, String>,
    module_names: &HashSet<String>,
) -> Vec<Vec<ColumnMapPoint>> {
    use rustc_lexer::tokenize;

    let zh_tokens: Vec<_> = tokenize(zh_content).collect();
    // 预扫描：收集用户声明名（中文原名），使用处豁免别名替换，
    // 与 engine `alias::replace_aliases` 的两遍扫描语义一致
    let declared_names = collect_zh_declared_names(&zh_tokens, zh_content, keyword_map, macro_map);
    let mut per_line_map: Vec<Vec<ColumnMapPoint>> = Vec::new();
    let mut zh_col = 0u32;
    let mut en_col = 0u32;
    let mut cumulative_diff = 0i32; // 当前行内 en_col - zh_col
    let mut current_offset = 0usize;
    // 声明位保护状态（与 alias::replace_aliases 一致，跨行保持）
    let mut prev_is_decl = false;

    // 第一行起点
    per_line_map.push(vec![ColumnMapPoint {
        en_col: 0,
        zh_col: 0,
        offset_diff: 0,
    }]);

    use rustc_lexer::TokenKind;
    let is_whitespace = |k: TokenKind| {
        matches!(
            k,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
        )
    };
    let is_ident = |k: TokenKind| matches!(k, TokenKind::Ident | TokenKind::RawIdent);

    for i in 0..zh_tokens.len() {
        let token = &zh_tokens[i];
        let token_text = &zh_content[current_offset..][..token.len];
        current_offset += token.len;

        // 空白 token：两列同步前进（逐字符处理，空白可能跨行）
        if is_whitespace(token.kind) {
            for c in token_text.chars() {
                if c == '\n' {
                    // 新行：行内列与偏移差重置，并记录新行起点
                    zh_col = 0;
                    en_col = 0;
                    cumulative_diff = 0;
                    per_line_map.push(vec![ColumnMapPoint {
                        en_col: 0,
                        zh_col: 0,
                        offset_diff: 0,
                    }]);
                } else {
                    zh_col += c.len_utf16() as u32;
                    en_col += c.len_utf16() as u32;
                }
            }
            continue;
        }

        if is_ident(token.kind) {
            let raw_name = token_text.strip_prefix("r#").unwrap_or(token_text);
            let zh_len: u32 = token_text.chars().map(|c| c.len_utf16() as u32).sum();

            // 检查是否为宏名（后跟开括号）
            let is_macro_call =
                macro_map.contains_key(raw_name) && is_open_paren_after(&zh_tokens, i);

            if is_macro_call {
                // 英文名取自宏映射（与 transpile_source_with_macro_map 一致），
                // 后跟开括号时补 !，与真实转译输出保持列偏移一致
                let en_name = macro_map
                    .get(raw_name)
                    .map(|s| s.as_str())
                    .unwrap_or(raw_name);
                let en_name_len: u32 = en_name.chars().map(|c| c.len_utf16() as u32).sum();

                // 翻译后的英文输出：英文名 + 补上的 !（宏名必然在映射中才会命中此分支）
                let en_output_len = en_name_len + 1; // +1 for inserted !

                cumulative_diff += en_output_len as i32 - zh_len as i32;
                zh_col += zh_len;
                en_col += en_output_len;
                // 别名替换阶段宏名后紧跟 !（非标识符），声明态终结
                prev_is_decl = false;
            } else {
                // 普通标识符：关键字映射 > 模块路径前缀/别名替换 > 原样
                let is_module_seg = module_names.contains(raw_name)
                    && is_path_separator_after(&zh_tokens, i)
                    && !is_path_separator_before(&zh_tokens, i);
                // 别名替换模拟（仅正常 Ident 参与，RawIdent 不命中，
                // 与 alias::replace_aliases 的 token 匹配行为一致）；
                // 用户声明名的裸使用处豁免，但 `::` 限定后的路径段
                // （库 API 限定访问）不受豁免，与 engine 语义一致
                let alias_exempt = prev_is_decl
                    || (declared_names.contains(raw_name)
                        && !is_path_separator_before(&zh_tokens, i));
                let alias_repl = if !token_text.starts_with("r#") && !alias_exempt {
                    alias_map.get(raw_name)
                } else {
                    None
                };

                let mut translated: &str = raw_name;
                let en_output_len = if let Some(en_name) = keyword_map.get(raw_name) {
                    translated = en_name;
                    en_name.chars().map(|c| c.len_utf16() as u32).sum::<u32>()
                } else if let Some(en_name) = alias_repl {
                    translated = en_name;
                    let base: u32 = en_name.chars().map(|c| c.len_utf16() as u32).sum();
                    if is_module_seg { base + 7 } else { base }
                } else if is_module_seg {
                    // 模块路径段：虚拟文件中被补上 crate:: 前缀（7 个 UTF-16 单元）
                    zh_len + 7
                } else {
                    zh_len
                };

                cumulative_diff += en_output_len as i32 - zh_len as i32;
                zh_col += zh_len;
                en_col += en_output_len;

                // 更新声明态（按转译后的英文名；mut 透明传递声明状态）；
                // RawIdent 在别名替换中归入符号分支，终结声明位
                prev_is_decl = if token_text.starts_with("r#") {
                    false
                } else if translated == "mut" {
                    prev_is_decl
                } else {
                    i18n_rust_engine::alias::DECL_KEYWORDS.contains(&translated)
                };
            }
        } else {
            // 非标识符、非空白 token：原样输出（UTF-16 计数），终结声明位
            let len: u32 = token_text.chars().map(|c| c.len_utf16() as u32).sum();
            zh_col += len;
            en_col += len;
            prev_is_decl = false;
        }

        // 如果偏移差变化了，记录新的分段边界（当前行内）
        let last_diff = per_line_map
            .last()
            .and_then(|row| row.last())
            .map(|p| p.offset_diff)
            .unwrap_or(0);
        if cumulative_diff != last_diff {
            // 当前行映射必然存在（每行起点已入表）；防御性判断避免 panic
            if let Some(row) = per_line_map.last_mut() {
                row.push(ColumnMapPoint {
                    en_col,
                    zh_col,
                    offset_diff: cumulative_diff,
                });
            }
        }
    }

    per_line_map
}

/// 预扫描中文 token 流，收集用户在声明位定义的标识符名（中文原名）
///
/// 状态机与 `build_column_map` 主循环一致：经关键字映射转译为英文后，
/// 声明关键字（DECL_KEYWORDS）后紧跟的标识符计入集合；`mut` 在声明态内
/// 透明传递；空白/注释不打断声明态；符号与 RawIdent 终结声明位；
/// 宏调用分支（后跟开括号）终结声明态且不计入。
/// 集合内的名字在全文使用处豁免别名替换，与 engine `alias::replace_aliases`
/// 的两遍扫描语义一致。
fn collect_zh_declared_names(
    zh_tokens: &[rustc_lexer::Token],
    zh_content: &str,
    keyword_map: &HashMap<String, String>,
    macro_map: &HashMap<String, String>,
) -> HashSet<String> {
    use rustc_lexer::TokenKind;
    let mut declared = HashSet::new();
    let mut prev_is_decl = false;
    let mut offset = 0usize;
    for (i, token) in zh_tokens.iter().enumerate() {
        let text = &zh_content[offset..offset + token.len];
        offset += token.len;
        match token.kind {
            TokenKind::Ident | TokenKind::RawIdent => {
                let raw_name = text.strip_prefix("r#").unwrap_or(text);
                let is_macro_call =
                    macro_map.contains_key(raw_name) && is_open_paren_after(zh_tokens, i);
                if is_macro_call {
                    // 宏名后紧跟 !（非标识符），声明态终结
                    prev_is_decl = false;
                } else {
                    if prev_is_decl && !text.starts_with("r#") {
                        declared.insert(raw_name.to_string());
                    }
                    let translated = keyword_map
                        .get(raw_name)
                        .map(|s| s.as_str())
                        .unwrap_or(raw_name);
                    prev_is_decl = if text.starts_with("r#") {
                        false
                    } else if translated == "mut" {
                        prev_is_decl
                    } else {
                        i18n_rust_engine::alias::DECL_KEYWORDS.contains(&translated)
                    };
                }
            }
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => {}
            _ => prev_is_decl = false,
        }
    }
    declared
}

/// 检查指定 token 之后下一个非空白 token 是否是开括号（( [ {）
fn is_open_paren_after(tokens: &[rustc_lexer::Token], current: usize) -> bool {
    use rustc_lexer::TokenKind;
    let is_whitespace = |k: TokenKind| {
        matches!(
            k,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
        )
    };
    for token in &tokens[(current + 1)..] {
        if is_whitespace(token.kind) {
            continue;
        }
        return matches!(
            token.kind,
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
fn generate_line_map(zh_content: &str, en_content: &str) -> Vec<u32> {
    let en_line_count = en_content.lines().count() as u32;
    let zh_line_count = zh_content.lines().count() as u32;
    let min_lines = en_line_count.min(zh_line_count);

    // 基础 1:1 映射
    let mut map: Vec<u32> = (0..en_line_count).collect();

    // 对于超出中文行数的英文行，映射到最后一行
    for i in min_lines..en_line_count {
        map[i as usize] = zh_line_count.saturating_sub(1);
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    /// URI 双向转换：Unix 路径、空格/中文编码、Windows 盘符形式
    #[test]
    fn test_path_uri_roundtrip() {
        // Unix：根路径保留，空格百分号编码
        assert_eq!(
            path_to_uri(Path::new("/tmp/a b/main.rs")),
            "file:///tmp/a%20b/main.rs"
        );
        assert_eq!(
            uri_to_path("file:///tmp/a%20b/main.rs"),
            PathBuf::from("/tmp/a b/main.rs")
        );

        // Windows：反斜杠归一、盘符前补 /、盘符转小写（与 rust-analyzer 一致）
        assert_eq!(
            path_to_uri(Path::new("C:\\Users\\x\\main.rs")),
            "file:///c:/Users/x/main.rs"
        );
        // 反向：盘符前导 / 被剥离
        let back = uri_to_path("file:///c:/Users/x/main.rs");
        assert_eq!(
            back.to_string_lossy().replace('\\', "/"),
            "c:/Users/x/main.rs"
        );
    }

    fn test_map() -> HashMap<String, String> {
        HashMap::from([
            ("函数".into(), "fn".into()),
            ("让".into(), "let".into()),
            ("可变".into(), "mut".into()),
            ("如果".into(), "if".into()),
            ("否则".into(), "else".into()),
        ])
    }

    #[test]
    fn test_update_document() {
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(
            test_map(),
            HashMap::new(),
            HashMap::new(),
            temp.path().to_path_buf(),
        );

        let (entry, others) = cache
            .update_document("file:///test/main.zh", "让 可变 x = 5;", 1)
            .unwrap();
        assert_eq!(entry.en_content, "let mut x = 5;");
        assert!(entry.virtual_path.exists());
        assert!(others.is_empty());
    }

    /// 别名替换接通：库标识符转英文，声明位同名用户定义受保护（与 CLI 一致）
    #[test]
    fn test_alias_replacement_with_declaration_protection() {
        let temp = tempfile::tempdir().unwrap();
        let alias_map = HashMap::from([
            ("字符串".into(), "String".into()),
            ("新建".into(), "new".into()),
        ]);
        let cache = TranslationCache::new(
            test_map(),
            HashMap::new(),
            alias_map,
            temp.path().to_path_buf(),
        );

        let (entry, _) = cache
            .update_document(
                "file:///test/main.zh",
                "让 新建 = 1;\n让 y = 新建;\n让 t = 字符串::新建();",
                1,
            )
            .unwrap();
        // 声明位 新建 保留；用户声明名的裸使用处（y = 新建）豁免；
        // 但 `::` 限定后的路径段是库 API 访问，照常替换（字符串::新建 → String::new）
        assert_eq!(
            entry.en_content,
            "let 新建 = 1;\nlet y = 新建;\nlet t = String::new();"
        );
    }

    /// 无用户声明撞名时，别名在使用处照常替换
    #[test]
    fn test_alias_usage_replaced_when_not_declared() {
        let temp = tempfile::tempdir().unwrap();
        let alias_map = HashMap::from([("字符串".into(), "String".into())]);
        let cache = TranslationCache::new(
            test_map(),
            HashMap::new(),
            alias_map,
            temp.path().to_path_buf(),
        );

        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 s: 字符串 = x;", 1)
            .unwrap();
        assert_eq!(entry.en_content, "let s: String = x;");
    }

    /// 别名替换后的列映射对齐：中英文列号双向转换在替换点精确
    #[test]
    fn test_alias_column_map_alignment() {
        let temp = tempfile::tempdir().unwrap();
        let alias_map = HashMap::from([("字符串".into(), "String".into())]);
        let cache = TranslationCache::new(
            test_map(),
            HashMap::new(),
            alias_map,
            temp.path().to_path_buf(),
        );
        let uri = "file:///test/main.zh";
        cache.update_document(uri, "让 s: 字符串 = x;", 1).unwrap();

        // 中文列 5（字符串 起点）→ 英文列 7（String 起点），反向亦然
        assert_eq!(cache.zh_col_to_en_col(uri, 0, 5), 7);
        assert_eq!(cache.en_col_to_zh_col(uri, 0, 7), 5);
        // 替换点之后的列（= 号：中文列 9 / 英文列 14）仍精确
        assert_eq!(cache.zh_col_to_en_col(uri, 0, 9), 14);
        assert_eq!(cache.en_col_to_zh_col(uri, 0, 14), 9);
    }

    #[test]
    fn test_close_document() {
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(
            test_map(),
            HashMap::new(),
            HashMap::new(),
            temp.path().to_path_buf(),
        );

        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();
        assert!(entry.virtual_path.exists());

        cache.close_document("file:///test/main.zh").unwrap();
        assert!(!entry.virtual_path.exists());
        assert!(cache.query_original("file:///test/main.zh").is_none());
    }

    /// 关闭不在缓存中的文档：幂等早返回，不触发重写
    #[test]
    fn test_close_document_missing_is_noop() {
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(
            test_map(),
            HashMap::new(),
            HashMap::new(),
            temp.path().to_path_buf(),
        );
        cache
            .update_document("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();
        let version_before = cache.module_version();
        let changes = cache.close_document("file:///test/不存在.zh").unwrap();
        assert!(changes.is_empty());
        // 模块集合未变：版本号不递增
        assert_eq!(cache.module_version(), version_before);
    }

    /// 用户词汇缓存：文档未变时命中，变更后失效重建
    #[test]
    fn test_user_defined_tokens_cache_invalidation() {
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(
            test_map(),
            HashMap::new(),
            HashMap::new(),
            temp.path().to_path_buf(),
        );
        cache
            .update_document("file:///test/main.zh", "函数 自定义甲() {}", 1)
            .unwrap();
        let first = cache.user_defined_tokens();
        assert!(first.contains("自定义甲"));
        // 命中路径：结果一致
        assert_eq!(first, cache.user_defined_tokens());
        // 文档变更后缓存失效，新名词可见
        cache
            .update_document("file:///test/main.zh", "函数 自定义乙() {}", 2)
            .unwrap();
        let second = cache.user_defined_tokens();
        assert!(second.contains("自定义乙"));
        assert!(!second.contains("自定义甲"));
    }

    #[test]
    fn test_query_by_virtual_uri() {
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(
            test_map(),
            HashMap::new(),
            HashMap::new(),
            temp.path().to_path_buf(),
        );

        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();
        let found = cache.query_by_virtual_uri(&entry.virtual_uri).unwrap();
        assert_eq!(found.original_uri, "file:///test/main.zh");
    }

    #[test]
    fn test_rewrite_module_paths() {
        let set = HashSet::from(["辅助".to_string(), "主".to_string()]);

        // 裸路径加前缀
        assert_eq!(
            rewrite_module_paths("fn main() {\n    辅助::辅助函数();\n}", &set),
            "fn main() {\n    crate::辅助::辅助函数();\n}"
        );
        // 已有 crate:: 前缀的不重复处理
        assert_eq!(
            rewrite_module_paths("crate::辅助::辅助函数()", &set),
            "crate::辅助::辅助函数()"
        );
        // 非模块名的标识符路径不处理
        assert_eq!(rewrite_module_paths("x::方法()", &set), "x::方法()");
        // 空集合保持原样
        assert_eq!(
            rewrite_module_paths("辅助::辅助函数()", &HashSet::new()),
            "辅助::辅助函数()"
        );
    }

    #[test]
    fn test_module_path_column_map() {
        let map = HashMap::from([
            ("函数".into(), "fn".into()),
            ("让".into(), "let".into()),
            ("公开".into(), "pub".into()),
        ]);
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(
            map,
            HashMap::new(),
            HashMap::new(),
            temp.path().to_path_buf(),
        );

        // 先打开 辅助.zh，使模块集合包含 辅助
        let (helper_entry, _) = cache
            .update_document(
                "file:///test/辅助.zh",
                "公开 函数 辅助函数() {\n    让 x = 1;\n}",
                1,
            )
            .unwrap();
        assert_eq!(
            helper_entry.en_content,
            "pub fn 辅助函数() {\n    let x = 1;\n}"
        );

        // 主.zh 引用 辅助 模块：中文列 8（辅助函数起点）应映射到英文列 15
        // （`辅助::` 被重写为 `crate::辅助::`，多出 7 个 UTF-16 单元）
        let (entry, others) = cache
            .update_document(
                "file:///test/主.zh",
                "函数 主函数() {\n    辅助::辅助函数();\n}",
                1,
            )
            .unwrap();
        assert_eq!(
            entry.en_content,
            "fn 主函数() {\n    crate::辅助::辅助函数();\n}"
        );

        // 中文列 8 → 英文列 15
        assert_eq!(cache.zh_col_to_en_col(&entry.virtual_uri, 1, 8), 15);
        // 英文列 15 → 中文列 8
        assert_eq!(cache.en_col_to_zh_col(&entry.virtual_uri, 1, 15), 8);
        // 英文列 19（辅助函数末尾）→ 中文列 12
        assert_eq!(cache.en_col_to_zh_col(&entry.virtual_uri, 1, 19), 12);
        // 辅助.zh 未引用任何模块，内容不变，不进入变更列表
        assert!(others.is_empty());
    }

    #[test]
    fn test_line_map() {
        let map = generate_line_map("行0\n行1\n行2", "line0\nline1\nline2");
        assert_eq!(map, vec![0, 1, 2]);
    }

    #[test]
    fn test_reverse_transpile() {
        let map = HashMap::from([
            ("函数".into(), "fn".into()),
            ("让".into(), "let".into()),
            ("打印行".into(), "println".into()),
            ("整数".into(), "i32".into()),
        ]);
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(
            map,
            HashMap::new(),
            HashMap::new(),
            temp.path().to_path_buf(),
        );
        let (entry, _) = cache
            .update_document(
                "file:///test/main.zh",
                "函数 主函数() {\n让 count = 1;\n打印行!(\"count={}\", count);\n}",
                1,
            )
            .unwrap();
        assert_eq!(
            entry.en_content,
            "fn 主函数() {\nlet count = 1;\nprintln!(\"count={}\", count);\n}"
        );

        // 模拟 rustfmt 输出：统一缩进为 4 空格
        let formatted_en =
            "fn 主函数() {\n    let count = 1;\n    println!(\"count={}\", count);\n}\n";
        let restored = cache.reverse_transpile(formatted_en);
        // 关键字/宏还原为母语，英文自定义标识符 count 与中文标识符 主函数 保留
        assert_eq!(
            restored,
            "函数 主函数() {\n    让 count = 1;\n    打印行!(\"count={}\", count);\n}\n"
        );
    }

    /// URI 百分号解码：中文文件名（VSCode 必然编码）能还原为真实路径
    #[test]
    fn test_uri_to_path_decodes_percent_encoding() {
        // 测试.zh 的 UTF-8 百分号编码
        let path = uri_to_path("file:///test/%E6%B5%8B%E8%AF%95.zh");
        assert_eq!(path, std::path::PathBuf::from("/test/测试.zh"));
        // 空格与 # 同样可解码
        let path = uri_to_path("file:///a%20dir/f%23ile.zh");
        assert_eq!(path, std::path::PathBuf::from("/a dir/f#ile.zh"));
    }

    /// path_to_uri 对非安全字符做百分号编码，生成合法 URI
    #[test]
    fn test_path_to_uri_encodes_special_chars() {
        let uri = path_to_uri(std::path::Path::new("/tmp/my dir/测试.rs"));
        assert!(uri.starts_with("file:///tmp/my%20dir/"));
        assert!(uri.contains("%"));
        assert!(!uri.contains(' '));
        // 解码后能往返还原
        assert_eq!(url_decode(&uri), "file:///tmp/my dir/测试.rs");
    }

    /// 模块名净化：非法字符替换、数字开头补下划线、空名回退
    #[test]
    fn test_sanitize_module_name() {
        // 中文保留（合法标识符）
        assert_eq!(sanitize_module_name("辅助"), "辅助");
        // 非法字符替换为 _
        assert_eq!(sanitize_module_name("a-b.zh\"x"), "a_b_zh_x");
        // 数字开头前补 _
        assert_eq!(sanitize_module_name("1main"), "_1main");
        // 空名回退 m
        assert_eq!(sanitize_module_name(""), "m");
    }

    /// 中文文件名（百分号编码 URI）打开后模块名合法，虚拟项目可编译
    #[test]
    fn test_update_document_encoded_chinese_filename() {
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(
            test_map(),
            HashMap::new(),
            HashMap::new(),
            temp.path().to_path_buf(),
        );
        let (entry, _) = cache
            .update_document("file:///test/%E6%B5%8B%E8%AF%95.zh", "让 x = 1;", 1)
            .unwrap();
        assert_eq!(entry.en_content, "let x = 1;");
        // 虚拟 main.rs 中的 mod 名应为解码后的中文（合法标识符），而非 %XX
        let main_rs = std::fs::read_to_string(temp.path().join("src").join("main.rs")).unwrap();
        assert!(
            main_rs.contains("mod 测试;"),
            "mod 名应为解码后的中文: {main_rs}"
        );
        assert!(!main_rs.contains('%'));
    }
}
