//!
//! 维护方言源码（.zh/.en/.de 等）与翻译后英文 .rs 代码的对应关系。
//! 每当编辑器打开或修改方言文件时，本模块将其翻译为英文，
//! 并记录行级映射信息供后续位置还原使用。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use i18n_rust_engine::cache::SourceMapEntry;
use i18n_rust_engine::lexer;
use i18n_rust_engine::mapping_manager::MappingManager;

/// 虚拟项目磁盘操作：失败必须记录日志——虚拟项目是 rust-analyzer 的分析基础，
/// 写失败会导致补全/诊断静默失效且无任何线索可查
fn log_io_err(context: &str, path: &Path, result: std::io::Result<()>) {
    if let Err(e) = result {
        log::warn!("虚拟项目 IO 失败：{context} {}：{e}", path.display());
    }
}

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
    /// 净化后的英文源码：抹除文件式 `mod 名字;` 声明（含前置属性/可见性）
    ///
    /// LSP 虚拟项目按哈希名托管文件并以 `#[path]` 聚合，用户原文的文件式
    /// 声明指向不存在的文件，会触发 cargo check E0583/E0754 误报。净化以
    /// 1:1 字符替换实现，行号/列号与 `en_content` 严格一致（列映射继续有效）；
    /// 发送给 rust-analyzer 的内存文档使用本字段，格式化与反向转译仍用
    /// `en_content`（用户原文的 `模块 名字;` 行不能丢失）。
    pub ra_content: String,
    /// 虚拟 .rs 文件的 URI（通知 rust-analyzer 用）
    pub virtual_uri: String,
    /// 虚拟 .rs 文件的磁盘路径
    pub virtual_path: PathBuf,
    /// 英文行号 → 中文行号的映射
    pub line_map: Vec<u32>,
    /// 列偏移映射（每行一个分段表，行号 → 该行的分段边界点）
    pub column_map: Vec<Vec<ColumnMapPoint>>,
    /// 代理添加的 `crate::` 前缀在英文输出中的非空白 token 序号
    ///（仅 LSP 虚拟项目跨文件引用重写产生；供反向转译精确删除，
    /// 避免误删用户显式书写的 `crate::` 前缀）
    pub added_crate_tokens: HashSet<usize>,
    /// 文档版本
    pub version: i32,
    /// 是否为编辑器打开的文档（false = 同目录自动聚合的兄弟模块）
    ///
    /// 打开文档以编辑器缓冲区内容为准，代理用 didOpen/didChange 向
    /// rust-analyzer 同步；兄弟模块以磁盘内容为准，内容刷新用
    /// didChangeWatchedFiles 通知（不得对未打开的文档发 didChange）。
    pub is_open: bool,
    /// 磁盘源文件的 (修改时间, 字节数)：兄弟模块增量同步的变更判定
    pub source_meta: Option<(SystemTime, u64)>,
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
    /// 虚拟 URI → 翻译条目索引：RA 响应映射热路径的 O(1) 反查。
    /// 直存 Arc 引用：命中时一次锁完成查询（存原始 URI 还需二次查 entries）；
    /// 无索引时每次语义 token/诊断/高亮/引用映射都线性扫描全表
    ///（语义 token 每个 token 查 2 次，O(n) 放大到 O(n×token数)），
    /// 条目插入/替换/移除时与 entries 同步维护。
    virtual_index: RwLock<HashMap<String, Arc<TranslationEntry>>>,
    /// 统一映射管理器（关键字/宏/派生/模块路径/别名，与 CLI 管线完全同源）。
    /// 转译与列映射统一走引擎完整管线，规则唯一来源为引擎。
    manager: Arc<MappingManager>,
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
    /// 项目声明名指纹（[`i18n_rust_engine::alias::ProjectContext::fingerprint`]）：
    /// 声明集合变化（新增/删除项、结构体字段）时触发全量重写——
    /// 跨文件声明豁免会影响其他文件的虚拟内容（#8）
    project_fingerprint: std::sync::atomic::AtomicU64,
    /// 用户词汇缓存：(代号, 结果)。Arc 共享避免每次补全请求克隆整个集合，
    /// 代号匹配时直接复用，避免重复词法扫描全部已打开文档
    user_tokens_cache: std::sync::Mutex<(u64, Option<Arc<HashSet<String>>>)>,
}

impl TranslationCache {
    /// 创建新的翻译缓存
    ///
    /// - 映射管理器：统一持有关键字/宏/派生/模块路径/别名映射（与 CLI 管线同源）
    /// - 临时目录：虚拟 .rs 文件的存放位置
    pub fn new(manager: MappingManager, temp_dir: PathBuf) -> Arc<Self> {
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
        log_io_err(
            "创建临时目录",
            &temp_dir,
            std::fs::create_dir_all(&temp_dir),
        );
        log_io_err(
            "创建 src 目录",
            &temp_dir.join("src"),
            std::fs::create_dir_all(temp_dir.join("src")),
        );
        // 合并反向表预构建：关键字反转优先，别名反转仅在英文键未占用时并入
        let mut reverse_map = build_reverse_map(&manager.keyword_map);
        for (english, native) in build_reverse_map(&manager.alias_map) {
            reverse_map.entry(english).or_insert(native);
        }
        let manager = Arc::new(manager);
        let cache = Arc::new(Self {
            entries: RwLock::new(HashMap::new()),
            virtual_index: RwLock::new(HashMap::new()),
            manager,
            temp_dir,
            module_version: std::sync::atomic::AtomicU64::new(0),
            reverse_map: Arc::new(reverse_map),
            docs_generation: std::sync::atomic::AtomicU64::new(0),
            project_fingerprint: std::sync::atomic::AtomicU64::new(0),
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
        let virtual_path = self.virtual_path_for(&original_path);
        let virtual_uri = path_to_uri(&virtual_path);

        // 同步前的旧模块集合：同步后按新旧集合差异判定变化
        let old_names = self.current_module_names(None);

        // 同目录兄弟模块同步（虚拟项目高保真化）：未打开的同目录方言
        // 文件也纳入虚拟项目，使 crate::模块 跨文件引用在兄弟文件从未
        // 打开时也能解析（E0432/E0433 误报的根治手段，过滤链仅兜底）
        let updated_siblings = self.sync_sibling_modules(&original_path);

        // 行映射不依赖模块路径重写（重写不改变行数），先按中文行数生成
        let line_map = generate_line_map(content, content);

        // 存入缓存（英文内容与列映射由下面的重写步骤填充）
        {
            let mut table = self
                .entries
                .write()
                .map_err(|_| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_cache_lock")))?;
            let new_entry = Arc::new(TranslationEntry {
                original_uri: uri.to_string(),
                original_path: original_path.clone(),
                zh_content: content.to_string(),
                en_content: String::new(),
                ra_content: String::new(),
                virtual_uri: virtual_uri.clone(),
                virtual_path: virtual_path.clone(),
                line_map,
                column_map: Vec::new(),
                added_crate_tokens: HashSet::new(),
                version,
                is_open: true,
                source_meta: disk_meta(&original_path),
            });
            table.insert(uri.to_string(), Arc::clone(&new_entry));
            // 同步虚拟 URI 索引（同一 uri 的 virtual_uri 恒定，插入一次即可）
            if let Ok(mut index) = self.virtual_index.write() {
                index.insert(virtual_uri.clone(), Arc::clone(&new_entry));
            }
        }

        // 内容可能变化：递增文档变更代号，使用户词汇缓存失效
        self.docs_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // 模块集合 = 全部条目（已打开文档 + 同目录兄弟模块）的词干；
        // 新增/删除必然体现为新旧集合差异（首开文件旧集合为空必然变化）
        let new_module_names = self.current_module_names(None);
        let set_changed = new_module_names != old_names;

        // 项目级声明上下文（跨文件声明豁免，#8）：项目声明名（项/结构体
        // 字段）变化会改变其他文件的豁免结果，须与模块集合变化同样触发
        // 全量重写；声明名指纹无变化（纯函数体编辑）时只重写当前条目
        let project = self.current_project_context(&new_module_names);
        let project_fp = project.fingerprint();
        let names_changed = self
            .project_fingerprint
            .swap(project_fp, std::sync::atomic::Ordering::SeqCst)
            != project_fp;
        // 模块集合变化时重写全部条目并刷新虚拟项目；声明名变化时
        // 仅全量重写（main.rs 聚合只依赖模块集合，无需刷新/重载）
        let changes = if set_changed || names_changed {
            if set_changed {
                let _ = self.bump_module_version();
                // main.rs/Cargo.toml 只依赖模块集合：纯内容编辑不触发，
                // 每次按键省去数次磁盘写与全表遍历
                self.refresh_virtual_project();
            }
            self.rewrite_all(&new_module_names, &project)
        } else {
            let mut changes = Vec::new();
            if let Some(entry) = self.rewrite_entry(uri, &new_module_names, &project) {
                changes.push(entry);
            }
            // 兄弟模块磁盘内容变更（集合不变时的增量路径）：逐条重译
            for sibling_uri in &updated_siblings {
                if sibling_uri == uri {
                    continue;
                }
                if let Some(entry) = self.rewrite_entry(sibling_uri, &new_module_names, &project) {
                    changes.push(entry);
                }
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

    /// 计算原始路径对应的虚拟文件路径（哈希后缀避免同名文件冲突）
    ///
    /// 文件名只保留合法标识符字符，防止引号等特殊字符注入生成的 main.rs。
    fn virtual_path_for(&self, original_path: &Path) -> PathBuf {
        let file_stem = original_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            original_path.hash(&mut h);
            h.finish()
        };
        self.temp_dir.join("src").join(format!(
            "{}_{:x}.rs",
            sanitize_module_name(file_stem),
            hash
        ))
    }

    /// 同步同目录兄弟方言模块（虚拟项目高保真化）
    ///
    /// 虚拟项目只聚合已打开文件时，被引用模块未打开就无法解析
    /// （E0432/E0433 误报的根源之一，过滤链只是兜底）。本方法把当前
    /// 文档同目录下的全部兄弟方言文件（同扩展名）纳入缓存与虚拟项目：
    /// - 新文件按磁盘内容登记为模块条目（is_open=false，翻译在重写步骤）；
    /// - 磁盘内容变化的兄弟条目重新登记（以 (mtime, 大小) 判定增量）；
    /// - 磁盘文件已删除的兄弟条目连同虚拟文件一并移除（模块集合随之
    ///   缩小，由调用方按新旧集合差异触发刷新）。
    ///
    /// 已打开文档（is_open=true）一律跳过：其内容以编辑器缓冲区为准。
    /// 返回磁盘内容发生变化、需要重新翻译的条目 uri 列表。
    fn sync_sibling_modules(&self, current_path: &Path) -> Vec<String> {
        let (Some(dir), Some(ext)) = (current_path.parent(), current_path.extension()) else {
            return Vec::new();
        };
        let Ok(read_dir) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut on_disk: HashSet<PathBuf> = HashSet::new();
        let mut updated: Vec<String> = Vec::new();
        for dent in read_dir.flatten() {
            let path = dent.path();
            if path == current_path || path.extension() != Some(ext) || !path.is_file() {
                continue;
            }
            let Some(meta) = disk_meta(&path) else { continue };
            on_disk.insert(path.clone());
            let uri = path_to_uri(&path);
            match self.query_original(&uri).as_ref() {
                // 已打开：缓冲区内容为准，不参与磁盘同步
                Some(e) if e.is_open => continue,
                // 磁盘未变化：跳过重译
                Some(e) if e.source_meta == Some(meta) => continue,
                _ => {}
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let virtual_path = self.virtual_path_for(&path);
            let virtual_uri = path_to_uri(&virtual_path);
            let line_map = generate_line_map(&content, &content);
            if let Ok(mut table) = self.entries.write() {
                let entry = Arc::new(TranslationEntry {
                    original_uri: uri.clone(),
                    original_path: path.clone(),
                    zh_content: content,
                    en_content: String::new(),
                    ra_content: String::new(),
                    virtual_uri: virtual_uri.clone(),
                    virtual_path,
                    line_map,
                    column_map: Vec::new(),
                    added_crate_tokens: HashSet::new(),
                    version: 0,
                    is_open: false,
                    source_meta: Some(meta),
                });
                table.insert(uri.clone(), Arc::clone(&entry));
                if let Ok(mut index) = self.virtual_index.write() {
                    index.insert(virtual_uri, entry);
                }
            }
            self.docs_generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            updated.push(uri);
        }

        // 清理：同目录下已不在磁盘的兄弟模块条目（文件删除/改名）
        let stale: Vec<String> = match self.entries.read() {
            Ok(table) => table
                .values()
                .filter(|e| {
                    !e.is_open
                        && e.original_path.parent() == Some(dir)
                        && !on_disk.contains(&e.original_path)
                })
                .map(|e| e.original_uri.clone())
                .collect(),
            Err(_) => Vec::new(),
        };
        for uri in stale {
            let removed = self
                .entries
                .write()
                .ok()
                .and_then(|mut table| table.remove(&uri));
            if let Some(entry) = removed {
                log_io_err(
                    "删除虚拟文件",
                    &entry.virtual_path,
                    std::fs::remove_file(&entry.virtual_path),
                );
                if let Ok(mut index) = self.virtual_index.write() {
                    index.remove(&entry.virtual_uri);
                }
                self.docs_generation
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                log::info!("{}", crate::ui::global().f("lsp_log_cache_removed", &[&uri]));
            }
        }
        updated
    }

    /// 关闭文档：编辑器侧关闭后条目仍保留为磁盘模块
    ///
    /// 虚拟项目聚合的是“同目录全部方言模块”，不因编辑器关闭而移除；
    /// 磁盘文件仍存在时把条目降级为兄弟模块（内容以磁盘为准，虚拟文件
    /// 保留）；磁盘文件已删除时才移除条目与虚拟文件。
    /// 返回其他条目中因模块/声明集合变化而被重写的条目列表。
    pub fn close_document(&self, uri: &str) -> anyhow::Result<Vec<Arc<TranslationEntry>>> {
        let removed_entry = {
            let mut table = self
                .entries
                .write()
                .map_err(|_| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_cache_lock")))?;
            match table.remove(uri) {
                Some(entry) => entry,
                None => return Ok(Vec::new()),
            }
        };
        // 先移除虚拟 URI 索引（降级为模块时会重新插入）
        if let Ok(mut index) = self.virtual_index.write() {
            index.remove(&removed_entry.virtual_uri);
        }
        // 内容可能变化：递增文档变更代号，使用户词汇缓存失效
        self.docs_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        log::info!("{}", crate::ui::global().f("lsp_log_cache_removed", &[uri]));

        // 磁盘文件仍存在：降级为兄弟模块（缓冲区 → 磁盘内容），
        // 模块集合不变（名字仍在），虚拟项目无需刷新/重载
        if let Some(meta) = disk_meta(&removed_entry.original_path)
            && let Ok(content) = std::fs::read_to_string(&removed_entry.original_path)
        {
            let mut demoted = (*removed_entry).clone();
            demoted.is_open = false;
            demoted.zh_content = content;
            demoted.en_content = String::new();
            demoted.ra_content = String::new();
            demoted.source_meta = Some(meta);
            demoted.line_map = generate_line_map(&demoted.zh_content, &demoted.zh_content);
            demoted.column_map = Vec::new();
            demoted.added_crate_tokens = HashSet::new();
            let demoted = Arc::new(demoted);
            {
                let mut table = self.entries.write().map_err(|_| {
                    anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_cache_lock"))
                })?;
                table.insert(uri.to_string(), Arc::clone(&demoted));
                if let Ok(mut index) = self.virtual_index.write() {
                    index.insert(demoted.virtual_uri.clone(), Arc::clone(&demoted));
                }
            }
            let module_names = self.current_module_names(None);
            let project = self.current_project_context(&module_names);
            let project_fp = project.fingerprint();
            let names_changed = self
                .project_fingerprint
                .swap(project_fp, std::sync::atomic::Ordering::SeqCst)
                != project_fp;
            return Ok(if names_changed {
                // 声明名集合变化（缓冲区与磁盘不一致）：全量重写
                self.rewrite_all(&module_names, &project)
            } else {
                // 仅本条目内容变化（缓冲区 → 磁盘）：单条重译
                let mut changes = Vec::new();
                if let Some(entry) = self.rewrite_entry(uri, &module_names, &project) {
                    changes.push(entry);
                }
                changes
            });
        }

        // 磁盘文件已删除：移除虚拟文件，模块集合缩小
        log_io_err(
            "删除虚拟文件",
            &removed_entry.virtual_path,
            std::fs::remove_file(&removed_entry.virtual_path),
        );
        let _ = self.bump_module_version();
        let module_names = self.current_module_names(None);
        let project = self.current_project_context(&module_names);
        self.project_fingerprint
            .store(project.fingerprint(), std::sync::atomic::Ordering::SeqCst);
        let changes = self.rewrite_all(&module_names, &project);

        // 模块集合变化：刷新虚拟项目文件（main.rs 聚合）
        self.refresh_virtual_project();
        Ok(changes)
    }

    /// 根据原始 URI 查询翻译条目（Arc 廉价克隆，不复制内容）
    pub fn query_original(&self, uri: &str) -> Option<Arc<TranslationEntry>> {
        let table = self.entries.read().ok()?;
        table.get(uri).cloned()
    }

    /// 返回所有已打开文档的翻译条目
    ///
    /// rust-analyzer 崩溃自动重启后，代理用它重发 didOpen 同步全部文档。
    pub fn all_entries(&self) -> Vec<Arc<TranslationEntry>> {
        self.entries
            .read()
            .map(|table| table.values().cloned().collect())
            .unwrap_or_default()
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
    pub fn user_defined_tokens(&self) -> Arc<HashSet<String>> {
        let 代号 = self
            .docs_generation
            .load(std::sync::atomic::Ordering::SeqCst);
        if let Ok(guard) = self.user_tokens_cache.lock()
            && guard.0 == 代号
            && let Some(set) = &guard.1
        {
            return Arc::clone(set);
        }
        let tokens = Arc::new(self.scan_user_tokens());
        if let Ok(mut guard) = self.user_tokens_cache.lock() {
            *guard = (代号, Some(Arc::clone(&tokens)));
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
        // 索引 O(1) 反查：直存 Arc 引用，命中时一次锁完成
        //（RA 响应映射热路径：语义 token 每个 token 查 2 次、
        // 诊断/高亮/引用/定义每处位置查 2-5 次；无索引时
        // 这些放大到 O(n×次数)）
        if let Ok(index) = self.virtual_index.read()
            && let Some(entry) = index.get(virtual_uri)
        {
            return Some(Arc::clone(entry));
        }
        // 兜底：索引未命中时回退线性扫描（含 URL 解码匹配）
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
        &self.manager.keyword_map
    }

    /// 获取别名映射的引用（标准库/第三方库标识符，供反向转译合并使用）
    pub fn alias_map(&self) -> &HashMap<String, String> {
        &self.manager.alias_map
    }

    /// 获取教学 lint 已知词表的引用（全部映射表键的并集，
    /// 供易混方法名提示判定；manager 内惰性缓存一次）
    pub fn lint_words(&self) -> &HashSet<String> {
        self.manager.get_lint_words()
    }

    /// 获取合并反向表的引用（英文 → 母语，关键字优先于别名）
    ///
    /// 构造时预构建，供 ResponseMapper 共用，避免重复构建。
    pub fn reverse_map(&self) -> &HashMap<String, String> {
        &self.reverse_map
    }

    /// 将中文（原始文件）列号转换为英文（虚拟文件）列号
    ///
    /// 由调用方先经 query_by_virtual_uri/query_original 预取条目后，
    /// 调用无锁纯函数 zh_col_to_en_col_single（请求方向位置转换热路径）。
    /// 将英文（虚拟文件）内容反向翻译为母语内容
    ///
    /// 供代码格式化（textDocument/formatting）与补全/代码操作文本还原使用：
    /// 英文代码经 rustfmt 格式化后，据此还原为母语代码。
    /// 反向表为构造时预构建的合并表（关键字优先），
    /// 保证与正向翻译互逆。
    ///
    /// `uri` 为文档原始 URI：传入时按该文档的编辑地图精确删除代理添加的
    /// `crate::` 前缀（用户手写的前缀保留）；为 None（补全片段等无文档
    /// 上下文场景）时不删除任何前缀，宁可保留代理前缀也不误删用户手写。
    pub fn reverse_transpile(&self, uri: Option<&str>, en_content: &str) -> String {
        // 无文档上下文或条目无代理前缀时，module_names 完全不被使用
        //（lexer 仅在 added_crate_tokens 命中后才查模块名集合）——
        // 跳过全表扫描：补全/代码操作响应的每个片段都走这里，
        // 每次省一次 O(文档数) 的读锁遍历
        let added_crate_tokens = uri
            .and_then(|u| self.query_original(u))
            .map(|e| e.added_crate_tokens.clone())
            .unwrap_or_default();
        let module_names = if added_crate_tokens.is_empty() {
            HashSet::new()
        } else {
            self.current_module_names(None)
        };
        lexer::reverse_transpile(
            en_content,
            &self.reverse_map,
            &module_names,
            &added_crate_tokens,
        )
    }

    /// 获取虚拟项目目录的 file:// URI（供工作区通知使用）
    pub fn virtual_project_uri(&self) -> String {
        path_to_uri(&self.temp_dir)
    }

    /// 获取虚拟项目目录的文件路径（供代理自跑 cargo check 使用）
    pub fn virtual_project_dir(&self) -> PathBuf {
        self.temp_dir.clone()
    }

    /// 映射管理器（镜像检查用同一语言包转译，规则零重复）
    pub(crate) fn manager(&self) -> &MappingManager {
        &self.manager
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

    /// 项目级声明上下文（跨文件声明豁免，#8）
    ///
    /// 模块名 = 全部已打开方言文件的词干（同 [`current_module_names`]）；
    /// 声明名 = 各文件母语原文的项名与结构体字段并集。声明收集在关键字
    /// 转译后的文本上进行，由引擎 [`i18n_rust_engine::alias::ProjectContext::from_sources`]
    /// 统一实现（与 CLI 同一规则来源，杜绝平行实现漂移）。
    /// 调用方在文档内容入库后调用，保证上下文包含最新内容。
    fn current_project_context(
        &self,
        module_names: &HashSet<String>,
    ) -> i18n_rust_engine::alias::ProjectContext {
        let sources: Vec<String> = match self.entries.read() {
            Ok(table) => table.values().map(|e| e.zh_content.clone()).collect(),
            Err(_) => Vec::new(),
        };
        i18n_rust_engine::alias::ProjectContext::from_sources(
            module_names.clone(),
            sources.iter().map(String::as_str),
            &self.manager,
        )
    }

    /// 重写单个条目的虚拟内容：翻译 + 模块路径加 `crate::` 前缀 + 重建列映射 + 写盘
    ///
    /// 内容未发生变化（模块集合/项目声明名集合未引入新豁免或前缀）时返回 None。
    ///
    /// 转译与列映射统一走引擎完整管线（`transpile_pipeline_with_map_and_project`）：
    /// 词法 → use 路径 → `crate::` 前缀（跨文件引用）→ 别名（含项目级
    /// 声明豁免，#8），列映射基于引擎实测的 `pipeline_map` 回放（[`replay_column_map`]），
    /// 不复刻任何转译规则——规则唯一来源是引擎，杜绝平行实现漂移。
    fn rewrite_entry(
        &self,
        uri: &str,
        module_names: &HashSet<String>,
        project: &i18n_rust_engine::alias::ProjectContext,
    ) -> Option<Arc<TranslationEntry>> {
        let old_entry = self.query_original(uri)?;
        let output = i18n_rust_engine::transpile_pipeline_with_map_and_project(
            &old_entry.zh_content,
            &self.manager,
            Some(module_names),
            Some(project),
        );
        let en_content = output.output;
        // 净化：抹除文件式 `mod 名字;` 声明（含前置属性/可见性）——
        // LSP 虚拟项目按哈希名托管文件并以 `#[path]` 聚合，用户原文的
        // 文件式声明指向不存在的文件，cargo check / rust-analyzer 会报
        // E0583/E0754 误报。净化以 1:1 字符替换实现，行列号与 en_content
        // 严格一致（column_map 无需调整）；发送给 rust-analyzer 的内存
        // 文档使用净化版，格式化与反向转译仍用 en_content
        //（用户原文的 `模块 名字;` 行不能丢失）。
        let ra_content = i18n_rust_engine::module_path::strip_file_module_decls(&en_content);
        // 虚拟项目的 crate 入口在聚合 main.rs 中转发调用 `main::main()`，
        // 模块内 fn 默认私有会触发 cargo check E0603。但发送给 rust-analyzer
        // 的内存文档必须保持无 pub——否则语义 token 多出 pub、
        // fn/main 位置偏移，变量等颜色错乱。
        //（column_map 基于无 pub 内容构建，与内存文档一致）
        let is_main = old_entry.original_path.file_stem().and_then(|s| s.to_str()) == Some("main");
        let disk_content = if is_main {
            // 逐行查找函数声明（行首空白后紧跟 `fn main(`），
            // 避免朴素子串替换误命中注释或字符串字面量中的 `fn main(`
            let mut result = String::with_capacity(ra_content.len() + 8);
            for line in ra_content.lines() {
                let trimmed = line.trim_start();
                let indent_len = line.len() - trimmed.len();
                if trimmed.starts_with("fn main(") {
                    result.push_str(&line[..indent_len]);
                    result.push_str("pub ");
                    result.push_str(trimmed);
                } else {
                    result.push_str(line);
                }
                result.push('\n');
            }
            // 保留原文末尾是否有换行的精确性
            if !ra_content.ends_with('\n') && result.ends_with('\n') {
                result.pop();
            }
            result
        } else {
            ra_content.clone()
        };
        let column_map = replay_column_map(&old_entry.zh_content, &output.pipeline_map);
        // 代理添加的 crate:: 前缀记录（token 序号）：反向转译时只删这些前缀
        let added_crate_tokens =
            lexer::added_crate_token_indices(&en_content, &output.pipeline_map);

        // 构造新版本需要克隆旧条目一次；此后查询均为 Arc 廉价克隆
        let new_entry = Arc::new(TranslationEntry {
            en_content: en_content.clone(),
            ra_content: ra_content.clone(),
            column_map,
            added_crate_tokens,
            ..(*old_entry).clone()
        });

        // 写入虚拟文件到磁盘（rust-analyzer 需要文件系统支持；main 文件写 pub 版）
        // 先确保父目录存在（首次打开时 src/ 可能尚未创建，
        // 直接写会静默失败导致 cargo check 读到不完整的虚拟项目）
        if let Some(parent) = new_entry.virtual_path.parent() {
            log_io_err("创建父目录", parent, std::fs::create_dir_all(parent));
        }
        log_io_err(
            "写入虚拟文件",
            &new_entry.virtual_path,
            std::fs::write(&new_entry.virtual_path, &disk_content),
        );

        // include_str!/include_bytes! 资源复制（虚拟项目高保真化）：
        // 资源按与源文件相同的相对位置复制到虚拟项目，使 rust-analyzer
        // 与 cargo check 在虚拟项目中也能读取 include 资源（如 `包含字符串!
        // ("界面.html")` 转译后为 include_str!("界面.html")），消除
        // "couldn't read" 类误报的根源（过滤链仅兜底）
        if let (Some(original_dir), Some(virtual_dir)) = (
            new_entry.original_path.parent(),
            new_entry.virtual_path.parent(),
        ) {
            copy_include_assets(&disk_content, original_dir, virtual_dir, &self.temp_dir);
        }

        {
            let mut table = match self.entries.write() {
                Ok(t) => t,
                Err(_) => return None,
            };
            if let Some(entry) = table.get_mut(uri) {
                *entry = Arc::clone(&new_entry);
            }
            // 条目已替换：同步索引指向最新 Arc（语义 token/诊断等经索引
            // 查询若拿到旧版本，列映射与虚拟内容会错位）
            if let Ok(mut index) = self.virtual_index.write() {
                index.insert(new_entry.virtual_uri.clone(), Arc::clone(&new_entry));
            }
        }

        // 内容是否变化以净化版（rust-analyzer 实际感知的内容）为准：
        // 仅在模块声明区域内改动（净化后为等宽空格）不需要重新通知
        if new_entry.ra_content != old_entry.ra_content {
            Some(new_entry)
        } else {
            None
        }
    }

    /// 用给定的模块名集合重写缓存中的所有条目
    ///
    /// 返回内容实际发生变化的条目列表（供调用方通知 rust-analyzer）。
    fn rewrite_all(
        &self,
        module_names: &HashSet<String>,
        project: &i18n_rust_engine::alias::ProjectContext,
    ) -> Vec<Arc<TranslationEntry>> {
        let uris: Vec<String> = {
            let table = match self.entries.read() {
                Ok(t) => t,
                Err(_) => return Vec::new(),
            };
            table.values().map(|e| e.original_uri.clone()).collect()
        };
        let mut changes = Vec::new();
        for uri in uris {
            if let Some(entry) = self.rewrite_entry(&uri, module_names, project) {
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
        // 先在锁内收集所需数据、释放读锁，再执行磁盘 I/O：
        // 持读锁期间做文件系统操作会阻塞所有写入者（rewrite_entry 等）
        let modules: Vec<(String, PathBuf)> = {
            let table = match self.entries.read() {
                Ok(t) => t,
                Err(_) => return,
            };
            table
                .values()
                .map(|e| {
                    (
                        e.original_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or_default()
                            .to_string(),
                        e.virtual_path.clone(),
                    )
                })
                .collect()
        };

        // Cargo.toml（包名保留英文，见项目规范）
        // [[bin]] 使其成为二进制 crate，fn main() 即为入口
        // [workspace] 空表使其脱离任何父工作区，避免被上层 Cargo.toml 吞并
        let cargo_content = "[package]\nname = \"i18n-virtual\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[[bin]]\npath = \"src/main.rs\"\nname = \"i18n-virtual\"\n\n[workspace]\n";
        log_io_err(
            "写入 Cargo.toml",
            &self.temp_dir.join("Cargo.toml"),
            std::fs::write(self.temp_dir.join("Cargo.toml"), cargo_content),
        );
        // 预生成 Cargo.lock：无依赖项目内容固定。缺少锁文件时 cargo
        // （rust-analyzer 的 cargo metadata / 代理的 check）会尝试联网更新
        // crates.io 索引，网络不可达时进程卡死且无诊断
        let lock_content = "# This file is automatically @generated by Cargo.\n# It is not intended for manual editing.\nversion = 3\n\n[[package]]\nname = \"i18n-virtual\"\nversion = \"0.1.0\"\n";
        log_io_err(
            "写入 Cargo.lock",
            &self.temp_dir.join("Cargo.lock"),
            std::fs::write(self.temp_dir.join("Cargo.lock"), lock_content),
        );

        // 清理旧版本残留文件（避免 rust-analyzer 同时读取 lib.rs 和 main.rs）
        log_io_err(
            "清理残留 lib.rs",
            &self.temp_dir.join("src").join("lib.rs"),
            std::fs::remove_file(self.temp_dir.join("src").join("lib.rs")),
        );
        // 确保 src 目录存在（首次启动时可能尚未创建，写盘会静默失败）
        log_io_err(
            "创建 src 目录",
            &self.temp_dir.join("src"),
            std::fs::create_dir_all(self.temp_dir.join("src")),
        );

        // src/main.rs：以 #[path] 属性按模块名聚合所有虚拟文件
        // #![allow(dead_code)] 抑制辅助函数/类型的未使用警告
        let mut main_content = format!(
            "#![allow(dead_code)]\n{}\n",
            crate::ui::global().t("lsp_gen_lib_comment")
        );
        // 模块名净化为合法 Rust 标识符，并在重名时追加哈希后缀
        let mut used_names: HashSet<String> = HashSet::new();
        let mut has_main_module = false;
        for (stem, virtual_path) in &modules {
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
                virtual_path.as_path().hash(&mut h);
                module_name = format!("{}_{:x}", module_name, h.finish());
                used_names.insert(module_name.clone());
            }
            let file_name = virtual_path
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
        log_io_err(
            "写入聚合 main.rs",
            &self.temp_dir.join("src").join("main.rs"),
            std::fs::write(self.temp_dir.join("src").join("main.rs"), main_content),
        );
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

/// 读取磁盘文件的 (修改时间, 字节数)，失败返回 None
fn disk_meta(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// 词法归一化路径（解析 `.`/`..`，不访问文件系统）并判断是否位于根内
///
/// 供 include 资源复制防护 `..` 逃逸虚拟项目根（写入任意位置）。
fn path_within(path: &Path, root: &Path) -> bool {
    fn normalize(p: &Path) -> PathBuf {
        let mut out = PathBuf::new();
        for c in p.components() {
            match c {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                std::path::Component::CurDir => {}
                other => out.push(other),
            }
        }
        out
    }
    normalize(path).starts_with(normalize(root))
}

/// 从（已转译的）源码中提取 include_str!/include_bytes! 的路径字面量
///
/// 以词法扫描实现：仅识别标识符后紧跟 `!` `(` 字符串字面量 `)` 的形式，
/// 注释与字符串内的伪调用天然排除。
fn collect_include_asset_paths(content: &str) -> Vec<String> {
    use rustc_lexer::{LiteralKind, TokenKind, tokenize};
    let mut spans: Vec<(TokenKind, usize, usize)> = Vec::new();
    let mut offset = 0usize;
    for token in tokenize(content) {
        let start = offset;
        offset += token.len;
        spans.push((token.kind, start, offset));
    }
    let mut result = Vec::new();
    let mut i = 0usize;
    while i < spans.len() {
        let (kind, start, end) = spans[i];
        i += 1;
        if kind != TokenKind::Ident || !matches!(&content[start..end], "include_str" | "include_bytes") {
            continue;
        }
        // 依次找到 `!` `(` 后的字符串字面量（允许空白/注释分隔）
        let (mut j, mut step, mut path) = (i, 0u8, None);
        while j < spans.len() {
            let (k, s, e) = spans[j];
            if matches!(
                k,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
            ) {
                j += 1;
                continue;
            }
            match step {
                0 if k == TokenKind::Not => step = 1,
                1 if k == TokenKind::OpenParen => step = 2,
                2 => {
                    if matches!(k, TokenKind::Literal { kind: LiteralKind::Str { .. }, .. }) {
                        let text = &content[s..e];
                        path = Some(
                            text.strip_prefix('"')
                                .and_then(|t| t.strip_suffix('"'))
                                .unwrap_or(text)
                                .to_string(),
                        );
                    }
                    break;
                }
                _ => break,
            }
            j += 1;
        }
        if let Some(p) = path {
            result.push(p);
        }
    }
    result
}

/// 复制 include_str!/include_bytes! 引用的资源到虚拟项目对应相对位置
///
/// 源文件与其虚拟文件处于相同的相对位置（都位于 src/ 下），以相同的
/// 相对路径拷贝即可让 include 宏在虚拟项目中命中（支持 `../`，如
/// `include_str!("../配置.toml")`）。目标路径词法归一化后必须仍落在
/// 虚拟项目根内（防 `..` 逃逸）；源文件不存在时跳过，交由诊断过滤链兜底。
fn copy_include_assets(content: &str, original_dir: &Path, virtual_dir: &Path, temp_dir: &Path) {
    for rel in collect_include_asset_paths(content) {
        let rel = Path::new(&rel);
        if rel.is_absolute() {
            continue;
        }
        let source = original_dir.join(rel);
        if !source.is_file() {
            continue;
        }
        let dest = virtual_dir.join(rel);
        if !path_within(&dest, temp_dir) {
            continue;
        }
        if let Some(parent) = dest.parent() {
            log_io_err("创建资源目录", parent, std::fs::create_dir_all(parent));
        }
        log_io_err(
            "复制 include 资源",
            &dest,
            std::fs::copy(&source, &dest).map(|_| ()),
        );
    }
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
///
/// pub(crate)：供 ResponseMapper 的语义 token 预取条目路径直接调用
///（避免每 token 重复锁与表扫描）。
pub(crate) fn en_col_to_zh_col_single(entry: &TranslationEntry, line: u32, en_col: u32) -> u32 {
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
///
/// pub(crate)：供 server.rs 请求方向位置转换直接调用
///（调用方已持有预取的条目，避免每处位置重复锁与表查询）。
pub(crate) fn zh_col_to_en_col_single(entry: &TranslationEntry, line: u32, zh_col: u32) -> u32 {
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

/// 由引擎全管线编辑地图重建列偏移映射
///
/// 对母语源逐 token 推进：命中地图条目的 token 输出长度取 replacement 的
/// UTF-16 长度（与真实转译输出一致，含宏自动补的 `!` 与 `crate::` 前缀），
/// 未命中的 token 原样长度；累计偏移差变化处记录分段点。
///
/// 不包含任何转译判定——规则的唯一来源是引擎（`transpile_pipeline_with_map`），
/// 此处仅做纯算术回放，杜绝与引擎转译逻辑的平行实现漂移。
fn replay_column_map(
    zh_content: &str,
    pipeline_map: &[SourceMapEntry],
) -> Vec<Vec<ColumnMapPoint>> {
    use rustc_lexer::{TokenKind, tokenize};

    // 索引：源偏移 → 条目（token 级替换，一个 token 至多一条）
    let by_offset: HashMap<usize, &SourceMapEntry> =
        pipeline_map.iter().map(|e| (e.source_offset, e)).collect();

    let mut per_line_map: Vec<Vec<ColumnMapPoint>> = vec![vec![ColumnMapPoint {
        en_col: 0,
        zh_col: 0,
        offset_diff: 0,
    }]];
    let mut zh_col = 0u32;
    let mut en_col = 0u32;
    let mut cumulative_diff = 0i32; // 当前行内 en_col - zh_col
    let mut current_offset = 0usize;
    let is_whitespace = |k: TokenKind| {
        matches!(
            k,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
        )
    };

    for token in tokenize(zh_content) {
        let token_start = current_offset;
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

        // 输出长度：命中地图条目取 replacement 的 UTF-16 长度，否则原样
        let zh_len: u32 = token_text.chars().map(|c| c.len_utf16() as u32).sum();
        let en_len: u32 = match by_offset.get(&token_start) {
            Some(e) => e.replacement.chars().map(|c| c.len_utf16() as u32).sum(),
            None => zh_len,
        };

        cumulative_diff += en_len as i32 - zh_len as i32;
        zh_col += zh_len;
        en_col += en_len;

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

    /// 构造测试用映射管理器（关键字表 + 可选别名表，模块路径/宏/派生表为空）
    fn test_manager(alias_map: HashMap<String, String>) -> MappingManager {
        MappingManager::from_flat_maps(test_map(), HashMap::new(), alias_map)
    }

    #[test]
    fn test_update_document() {
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(test_manager(HashMap::new()), temp.path().to_path_buf());

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
        let cache = TranslationCache::new(test_manager(alias_map), temp.path().to_path_buf());

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
        let cache = TranslationCache::new(test_manager(alias_map), temp.path().to_path_buf());

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
        let cache = TranslationCache::new(test_manager(alias_map), temp.path().to_path_buf());
        let uri = "file:///test/main.zh";
        let (entry, _) = cache.update_document(uri, "让 s: 字符串 = x;", 1).unwrap();

        // 中文列 5（字符串 起点）→ 英文列 7（String 起点），反向亦然
        assert_eq!(zh_col_to_en_col_single(&entry, 0, 5), 7);
        assert_eq!(en_col_to_zh_col_single(&entry, 0, 7), 5);
        // 替换点之后的列（= 号：中文列 9 / 英文列 14）仍精确
        assert_eq!(zh_col_to_en_col_single(&entry, 0, 9), 14);
        assert_eq!(en_col_to_zh_col_single(&entry, 0, 14), 9);
    }

    #[test]
    fn test_close_document() {
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(test_manager(HashMap::new()), temp.path().to_path_buf());

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
        let cache = TranslationCache::new(test_manager(HashMap::new()), temp.path().to_path_buf());
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
        let cache = TranslationCache::new(test_manager(HashMap::new()), temp.path().to_path_buf());
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
        let cache = TranslationCache::new(test_manager(HashMap::new()), temp.path().to_path_buf());

        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();
        let found = cache.query_by_virtual_uri(&entry.virtual_uri).unwrap();
        assert_eq!(found.original_uri, "file:///test/main.zh");
        // 解码形式（索引未命中时回退线性扫描）仍可查到
        let decoded = url_decode(&entry.virtual_uri);
        let found = cache.query_by_virtual_uri(&decoded).unwrap();
        assert_eq!(found.original_uri, "file:///test/main.zh");
        // 中文文件名：编码 URI 经索引命中
        let (entry_cn, _) = cache
            .update_document("file:///test/%E6%B5%8B%E8%AF%95.zh", "让 x = 1;", 1)
            .unwrap();
        assert!(
            cache.query_by_virtual_uri(&entry_cn.virtual_uri).is_some(),
            "编码 URI 应经索引命中"
        );
        // 内容更新（rewrite_entry 替换条目）后，索引应指向最新版本
        let (entry_v2, _) = cache
            .update_document("file:///test/main.zh", "让 x = 2;", 2)
            .unwrap();
        let found = cache.query_by_virtual_uri(&entry_v2.virtual_uri).unwrap();
        assert_eq!(found.en_content, "let x = 2;");
        // 关闭文档后索引同步清理，查询不再命中
        cache.close_document("file:///test/main.zh").unwrap();
        assert!(cache.query_by_virtual_uri(&entry.virtual_uri).is_none());
    }

    #[test]
    fn test_module_path_column_map() {
        // crate:: 前缀重写逻辑已迁入引擎（module_path::qualify_module_paths_with_map，
        // 其行为测试在引擎侧）；此处验证列映射与虚拟内容经由完整管线正确对齐
        let manager = MappingManager::from_flat_maps(
            HashMap::from([
                ("函数".into(), "fn".into()),
                ("让".into(), "let".into()),
                ("公开".into(), "pub".into()),
            ]),
            HashMap::new(),
            HashMap::new(),
        );
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(manager, temp.path().to_path_buf());

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
        assert_eq!(zh_col_to_en_col_single(&entry, 1, 8), 15);
        // 英文列 15 → 中文列 8
        assert_eq!(en_col_to_zh_col_single(&entry, 1, 15), 8);
        // 英文列 19（辅助函数末尾）→ 中文列 12
        assert_eq!(en_col_to_zh_col_single(&entry, 1, 19), 12);
        // 辅助.zh 未引用任何模块，内容不变，不进入变更列表
        assert!(others.is_empty());
    }

    /// use 语句路径段中文化（LSP 此前缺失的环节，现经引擎完整管线修复）
    #[test]
    fn test_use_stmt_module_path_translated() {
        let manager = MappingManager::load_from_builtin(
            r#"
["声明"]
"函数" = "fn"
"让" = "let"
"使用" = "use"
"#,
            r#"
["模块路径"]
"标准集合" = "std::collections"
"#,
            r#"
["标识符"]
"哈希映射" = "HashMap"
"#,
            &[],
        )
        .expect("管理器创建失败");
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(manager, temp.path().to_path_buf());
        let (entry, _) = cache
            .update_document("file:///test/main.zh", "使用 标准集合::哈希映射;", 1)
            .unwrap();
        assert_eq!(entry.en_content, "use std::collections::HashMap;");
        // 列映射与虚拟内容对齐：行首位置 zh↔en 恒等
        assert_eq!(zh_col_to_en_col_single(&entry, 0, 0), 0);
        assert_eq!(en_col_to_zh_col_single(&entry, 0, 0), 0);
    }

    /// #3 症状一回归：crate 名连字符在 LSP 转译路径上规范化为下划线
    ///
    /// weix-1 实测：`使用 日志订阅 as 日志框架;` 曾转译为
    /// `use tracing-subscriber as 日志框架;`（含连字符，非法路径），
    /// rust-analyzer 报 "expected one of `::`, `;`, or `as`, found `-`"。
    /// crate 段规范化后虚拟文件语法合法，误报消失（CLI 走同一引擎管线）。
    #[test]
    fn test_use_stmt_hyphenated_crate_normalized() {
        let manager = MappingManager::load_from_builtin(
            r#"
["声明"]
"使用" = "use"
"#,
            r#"
["模块路径"]
"日志订阅" = "tracing-subscriber"
"#,
            "",
            &[],
        )
        .expect("管理器创建失败");
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(manager, temp.path().to_path_buf());
        let (entry, _) = cache
            .update_document("file:///test/日志设置.zh", "使用 日志订阅 as 日志框架;", 1)
            .unwrap();
        assert_eq!(entry.en_content, "use tracing_subscriber as 日志框架;");
    }

    /// 跨文件声明豁免（#8）：其他文件声明的成员名在调用侧同样豁免
    ///
    /// weix-1 场景：A 文件声明 `函数 新建()`（撞 `新建`=new 映射），
    /// B 文件跨文件调用 `平台Linux::新建()`——模块路径链（`crate::` 前缀
    /// 由虚拟项目重写补全）内成员与声明侧一致；库 API 路径（盒子::新建）
    /// 照常替换。
    #[test]
    fn test_project_context_cross_file_declaration() {
        let temp = tempfile::tempdir().unwrap();
        let alias_map =
            HashMap::from([("新建".into(), "new".into()), ("盒子".into(), "Box".into())]);
        let cache =
            TranslationCache::new(test_manager(alias_map.clone()), temp.path().to_path_buf());

        // A 文件：声明 `新建`（同名用户函数）
        let (_, _) = cache
            .update_document("file:///test/平台Linux.zh", "函数 新建() {}", 1)
            .unwrap();
        // B 文件：跨文件调用 A 的成员 + 库 API 调用
        let (entry, _) = cache
            .update_document(
                "file:///test/平台接口.zh",
                "让 e = 平台Linux::新建();\n让 b = 盒子::新建();",
                1,
            )
            .unwrap();
        assert!(
            entry.en_content.contains("crate::平台Linux::新建()"),
            "跨文件调用位应与声明侧一致：{}",
            entry.en_content
        );
        assert!(
            entry.en_content.contains("Box::new()"),
            "库 API 路径段照常替换：{}",
            entry.en_content
        );

        // 对照：无 A 声明（单独打开 B）时保持旧行为——`新建` 被替换出 `new`
        let temp2 = tempfile::tempdir().unwrap();
        let cache2 = TranslationCache::new(test_manager(alias_map), temp2.path().to_path_buf());
        let (entry2, _) = cache2
            .update_document(
                "file:///test/平台接口.zh",
                "让 e = 平台Linux::新建();\n让 b = 盒子::新建();",
                1,
            )
            .unwrap();
        assert!(
            entry2.en_content.contains("平台Linux::new()"),
            "无跨文件声明时保持旧行为：{}",
            entry2.en_content
        );
    }

    #[test]
    fn test_line_map() {
        let map = generate_line_map("行0\n行1\n行2", "line0\nline1\nline2");
        assert_eq!(map, vec![0, 1, 2]);
    }

    #[test]
    fn test_reverse_transpile() {
        let manager = MappingManager::from_flat_maps(
            HashMap::from([
                ("函数".into(), "fn".into()),
                ("让".into(), "let".into()),
                ("打印行".into(), "println".into()),
                ("整数".into(), "i32".into()),
            ]),
            HashMap::new(),
            HashMap::new(),
        );
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(manager, temp.path().to_path_buf());
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
        let restored = cache.reverse_transpile(Some("file:///test/main.zh"), formatted_en);
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
        let cache = TranslationCache::new(test_manager(HashMap::new()), temp.path().to_path_buf());
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

    /// 含 `公开`/`包含字符串` 映射的测试管理器（C2 系列测试用）
    fn test_manager_extended() -> MappingManager {
        let mut map = test_map();
        map.insert("公开".into(), "pub".into());
        map.insert("包含字符串".into(), "include_str".into());
        MappingManager::from_flat_maps(map, HashMap::new(), HashMap::new())
    }

    /// C2a：同目录未打开的兄弟模块被聚合（虚拟项目高保真化）
    ///
    /// weix-1 场景：主文件引用 工具.zh 的函数而 工具.zh 从未在编辑器中
    /// 打开——此前虚拟项目不含该模块，rust-analyzer 报 E0433/E0432 误报。
    #[test]
    fn test_sibling_module_aggregated() {
        let proj = tempfile::tempdir().unwrap();
        let virt = tempfile::tempdir().unwrap();
        let src = proj.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("工具.zh"),
            "公开 函数 加一(数: i32) -> i32 {\n    数 + 1\n}\n",
        )
        .unwrap();

        let cache = TranslationCache::new(test_manager_extended(), virt.path().to_path_buf());
        let main_uri = path_to_uri(&src.join("main.zh"));
        let (entry, _) = cache
            .update_document(&main_uri, "函数 主函数() {\n    工具::加一(1);\n}\n", 1)
            .unwrap();

        // 主文件跨文件引用补 crate:: 前缀（模块集合已含新聚合的 工具）
        assert!(
            entry.en_content.contains("crate::工具::加一(1)"),
            "跨文件引用应补 crate:: 前缀：{}",
            entry.en_content
        );
        // 兄弟模块已登记（is_open=false）并转译写盘
        let tool = cache
            .query_original(&path_to_uri(&src.join("工具.zh")))
            .expect("同目录兄弟模块应被聚合");
        assert!(!tool.is_open, "磁盘聚合的模块不应标记为打开");
        assert!(
            tool.en_content.contains("pub fn 加一"),
            "兄弟模块应已转译：{}",
            tool.en_content
        );
        assert!(tool.virtual_path.exists(), "兄弟模块虚拟文件应写盘");
        // 聚合 main.rs 同时声明两个模块
        let agg = std::fs::read_to_string(virt.path().join("src").join("main.rs")).unwrap();
        assert!(
            agg.contains("mod 工具;") && agg.contains("mod main;"),
            "聚合 main.rs：{agg}"
        );
    }

    /// C2a：兄弟模块磁盘内容变化被增量同步（未打开文件以磁盘为准）
    #[test]
    fn test_sibling_module_disk_update_synced() {
        let proj = tempfile::tempdir().unwrap();
        let virt = tempfile::tempdir().unwrap();
        let src = proj.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let tool_path = src.join("工具.zh");
        std::fs::write(&tool_path, "公开 函数 加一(数: i32) -> i32 {\n    数 + 1\n}\n").unwrap();

        let cache = TranslationCache::new(test_manager_extended(), virt.path().to_path_buf());
        let main_uri = path_to_uri(&src.join("main.zh"));
        cache
            .update_document(&main_uri, "函数 主函数() {\n    工具::加一(1);\n}\n", 1)
            .unwrap();
        let tool_uri = path_to_uri(&tool_path);
        assert!(
            cache
                .query_original(&tool_uri)
                .unwrap()
                .en_content
                .contains("加一")
        );

        // 修改磁盘上的兄弟模块（长度变化确保增量判定命中）
        std::fs::write(
            &tool_path,
            "公开 函数 加二(数: i32) -> i32 {\n    数 + 2\n}\n// 变更标记\n",
        )
        .unwrap();
        let (_, others) = cache
            .update_document(&main_uri, "函数 主函数() {\n    工具::加一(1);\n}\n", 2)
            .unwrap();

        let tool = cache.query_original(&tool_uri).unwrap();
        assert!(
            tool.en_content.contains("加二"),
            "磁盘变更应增量同步：{}",
            tool.en_content
        );
        assert!(
            others.iter().any(|e| e.original_uri == tool_uri),
            "变更的兄弟模块应进入通知列表"
        );
    }

    /// C2a：磁盘上删除的兄弟模块被清理（条目与虚拟文件一并移除）
    #[test]
    fn test_sibling_module_removed_on_disk_delete() {
        let proj = tempfile::tempdir().unwrap();
        let virt = tempfile::tempdir().unwrap();
        let src = proj.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let tool_path = src.join("工具.zh");
        std::fs::write(&tool_path, "公开 函数 加一() {}\n").unwrap();

        let cache = TranslationCache::new(test_manager_extended(), virt.path().to_path_buf());
        let main_uri = path_to_uri(&src.join("main.zh"));
        cache
            .update_document(&main_uri, "函数 主函数() {}\n", 1)
            .unwrap();
        let tool_uri = path_to_uri(&tool_path);
        let tool_virtual = cache.query_original(&tool_uri).unwrap().virtual_path.clone();
        assert!(tool_virtual.exists());
        let version_before = cache.module_version();

        // 磁盘删除后再次更新：条目与虚拟文件应被清理，模块集合版本递增
        std::fs::remove_file(&tool_path).unwrap();
        cache
            .update_document(&main_uri, "函数 主函数() {}\n", 2)
            .unwrap();

        assert!(
            cache.query_original(&tool_uri).is_none(),
            "删除的兄弟模块应移除条目"
        );
        assert!(!tool_virtual.exists(), "虚拟文件应一并移除");
        assert!(
            cache.module_version() > version_before,
            "模块集合变化应递增版本"
        );
    }

    /// C2b：include_str! 资源复制进虚拟项目（couldn't read 误报根治）
    #[test]
    fn test_include_asset_copied_to_virtual_project() {
        let proj = tempfile::tempdir().unwrap();
        let virt = tempfile::tempdir().unwrap();
        let src = proj.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("数据.txt"), "占位内容").unwrap();

        let cache = TranslationCache::new(test_manager_extended(), virt.path().to_path_buf());
        let main_uri = path_to_uri(&src.join("main.zh"));
        let (entry, _) = cache
            .update_document(
                &main_uri,
                "函数 主函数() {\n    让 页面 = 包含字符串!(\"数据.txt\");\n}\n",
                1,
            )
            .unwrap();
        assert!(
            entry.en_content.contains("include_str!(\"数据.txt\")"),
            "宏名应转译且保留感叹号：{}",
            entry.en_content
        );
        // 资源按相对位置复制到虚拟项目（与虚拟 .rs 同目录）
        let copied = virt.path().join("src").join("数据.txt");
        assert!(copied.is_file(), "include 资源应复制到虚拟项目");
        assert_eq!(std::fs::read_to_string(copied).unwrap(), "占位内容");
    }

    /// 关闭文档：磁盘文件仍存在时降级为兄弟模块（不删除条目/虚拟文件）
    #[test]
    fn test_close_document_demotes_when_file_on_disk() {
        let proj = tempfile::tempdir().unwrap();
        let virt = tempfile::tempdir().unwrap();
        let src = proj.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        // 磁盘内容与缓冲区不同：关闭后应以磁盘为准
        std::fs::write(src.join("main.zh"), "让 磁盘 = 1;\n").unwrap();

        let cache = TranslationCache::new(test_manager(HashMap::new()), virt.path().to_path_buf());
        let main_uri = path_to_uri(&src.join("main.zh"));
        cache
            .update_document(&main_uri, "让 缓冲 = 2;\n", 1)
            .unwrap();

        cache.close_document(&main_uri).unwrap();
        let entry = cache
            .query_original(&main_uri)
            .expect("磁盘文件存在时应降级保留，而非移除");
        assert!(!entry.is_open, "关闭后应降级为兄弟模块");
        assert_eq!(entry.zh_content, "让 磁盘 = 1;\n", "降级后内容以磁盘为准");
        assert!(entry.virtual_path.exists(), "虚拟文件保留");
        assert!(
            entry.en_content.contains("let 磁盘 = 1;"),
            "应重译磁盘内容：{}",
            entry.en_content
        );
    }
}
