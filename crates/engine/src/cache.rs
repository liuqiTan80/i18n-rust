//! 翻译缓存模块
//! 基于内容哈希的增量翻译缓存。
//! 内容未变化且语言包映射未变化时直接复用翻译结果，避免重复翻译；
//! 同时缓存源映射（被替换标识符的源偏移与替换文本），供 LSP/调试使用。

use crate::error::TranspileError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 源映射条目：输入文本中一个被替换的标识符 token
///
/// 记录输入侧信息（字节偏移、长度）与翻译前后文本；
/// 不提供目标偏移——完整管线后续的模块路径替换/别名替换会改变输出偏移，
/// 输入侧信息始终保持精确。
/// 语义按使用场景分两级：
/// - 词法阶段 `source_map`：偏移为母语源坐标，replacement 为词法阶段文本；
/// - 全管线 `pipeline_map`：偏移为母语源坐标，replacement 为**最终输出文本**；
/// - 各中间阶段 `_with_map` 的 edits：偏移为该阶段输入文本坐标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMapEntry {
    /// 源文件中的字节偏移（token 起点）
    pub source_offset: usize,
    /// token 字节长度
    pub length: usize,
    /// 源 token 文本（如 `函数`）
    pub original: String,
    /// 翻译后文本（如 `fn`）
    pub replacement: String,
}

impl SourceMapEntry {
    pub fn new(source_offset: usize, length: usize, original: &str, replacement: &str) -> Self {
        Self {
            source_offset,
            length,
            original: original.to_string(),
            replacement: replacement.to_string(),
        }
    }
}

/// 翻译产物：翻译后的代码、词法阶段源映射与全管线编辑地图
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranspileOutput {
    /// 翻译后的代码文本
    pub output: String,
    /// 词法阶段源映射（向后兼容：仅含词法阶段的替换，replacement 为词法阶段文本）
    pub source_map: Vec<SourceMapEntry>,
    /// 全管线编辑地图：以母语源偏移升序记录，replacement 为**最终输出文本**
    ///（宏调用自动补充的 `!` 已计入 replacement）。
    /// 列映射等消费方只需回放此表，无需复刻任何转译规则。
    pub pipeline_map: Vec<SourceMapEntry>,
}

impl TranspileOutput {
    /// 仅创建输出（无源映射）
    pub fn new(output: String) -> Self {
        Self {
            output,
            source_map: Vec::new(),
            pipeline_map: Vec::new(),
        }
    }

    /// 创建输出并附带词法阶段源映射
    pub fn with_map(output: String, source_map: Vec<SourceMapEntry>) -> Self {
        Self {
            output,
            source_map,
            pipeline_map: Vec::new(),
        }
    }

    /// 创建输出并附带词法阶段源映射与全管线编辑地图
    pub fn with_full_map(
        output: String,
        source_map: Vec<SourceMapEntry>,
        pipeline_map: Vec<SourceMapEntry>,
    ) -> Self {
        Self {
            output,
            source_map,
            pipeline_map,
        }
    }
}

/// 缓存条目：内容哈希 → 条目
#[derive(Debug, Clone)]
struct CacheEntry {
    /// 源内容字节长度（哈希冲突时的廉价校验）
    content_length: usize,
    /// 翻译语境指纹（语言包映射内容变化时指纹变化，缓存自动失效）
    context_fingerprint: u64,
    /// 缓存的翻译产物
    output: TranspileOutput,
}

/// 基于内容哈希的 LRU 翻译缓存
///
/// - 键：内容 FNV-1a 64 位哈希（附长度校验，降低冲突风险）
/// - 值：翻译产物（输出 + 源映射）
/// - 语境指纹：关键字/模块路径/标识符别名映射内容的哈希，
///   语言包更新后旧缓存自动失效，保证翻译结果与映射一致
/// - 淘汰策略：LRU（最近最少使用），容量可配置
///
/// 统计计数用 [`AtomicU64`]，类型满足 `Send + Sync`，
/// 可置于 `Mutex`/`RwLock` 后在多线程间共享（如并行转译项目文件）。
pub struct TranslationCache {
    entries: HashMap<u64, CacheEntry>,
    /// LRU 顺序：队首最旧、队尾最新；每个条目附带代际计数器，
    /// `mark_hit` 递增代际并在队尾添加新条目，旧代际条目在淘汰时惰性跳过
    order: VecDeque<(u64, u64)>,
    /// 每个哈希的当前代际：与队列条目的代际匹配时为有效条目
    generations: HashMap<u64, u64>,
    capacity: usize,
    hits: AtomicU64,
    misses: AtomicU64,
    /// 磁盘持久化路径：Some 时由 [`TranslationCache::flush`] 原子写盘
    ///（跨进程复用：CLI 短命进程把上次运行的翻译结果留给下次）
    persistence: Option<PathBuf>,
    /// 待写盘标记：变更后置位，[`TranslationCache::flush`] 成功写盘后清除。
    ///
    /// 不每次变更都写盘的原因：`save` 会把**整个**缓存序列化为 JSON，
    /// 批量转译 N 个文件（各自 miss 后插入）会产生 N 次全量写，
    /// 且这些写发生在调用方持有缓存的临界区内，把并行转译串行化。
    /// 改为累积变更、批量一次写盘（进程退出由 `Drop` 兜底，不丢数据）。
    dirty: bool,
}

/// 退出兜底：保证累积的未写盘变更不丢失（正常路径无需显式 `flush`）
impl Drop for TranslationCache {
    fn drop(&mut self) {
        self.flush();
    }
}

/// 磁盘持久化文件格式（版本不匹配/解析失败时静默丢弃，回退内存缓存）
#[derive(Serialize, Deserialize)]
struct PersistentCacheFile {
    /// 格式版本：结构变更时递增，旧版本文件直接丢弃
    version: u32,
    /// LRU 顺序（旧→新）；加载时按序重建代际队列
    entries: Vec<PersistentEntry>,
}

/// 磁盘上的单个缓存条目（结构与内存条目对应）
#[derive(Serialize, Deserialize)]
struct PersistentEntry {
    hash: u64,
    content_length: usize,
    context_fingerprint: u64,
    output: TranspileOutput,
}

/// 当前持久化格式版本
const PERSISTENT_FORMAT_VERSION: u32 = 1;

/// 持久化缓存的默认容量（磁盘缓存面向多项目，比内存默认稍大）
const PERSISTENT_CAPACITY: usize = 512;

impl TranslationCache {
    /// 新建缓存（容量至少为 1）
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            generations: HashMap::new(),
            capacity: capacity.max(1),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            persistence: None,
            dirty: false,
        }
    }

    /// 默认容量：256 个文件条目
    pub fn with_default_capacity() -> Self {
        Self::new(256)
    }

    /// 带磁盘持久化的缓存：从 `path` 加载既有条目（尽力而为：文件不存在、
    /// 版本不匹配、JSON 损坏时静默回退为空缓存），此后变更累积、由
    /// [`Self::flush`] 批量写盘（进程退出由 `Drop` 兜底）。
    ///
    /// 跨进程增量复用场景（如 CLI 每次运行都是新进程）：上次运行转译过的
    /// 文件内容未变时直接命中，省去整条转译管线（Unicode/全角/lint 检查 + 词法）。
    pub fn persistent(path: &Path) -> Self {
        let mut cache = Self::new(PERSISTENT_CAPACITY);
        if let Ok(bytes) = std::fs::read(path)
            && let Ok(file) = serde_json::from_slice::<PersistentCacheFile>(&bytes)
        {
            if file.version == PERSISTENT_FORMAT_VERSION {
                cache.restore(file.entries);
            } else {
                // 版本不匹配或结构损坏：丢弃旧文件（下次插入时重建）
                crate::log_warn!(
                    "translation_cache",
                    "{}（{}）",
                    crate::语言::t("log_cache_disk_discard"),
                    path.display()
                );
            }
        }
        // 文件不存在：首次使用，属正常路径，保持空缓存
        cache.persistence = Some(path.to_path_buf());
        cache
    }

    /// 默认位置的持久化缓存：`~/.rz/cache/transpile-v1.json`
    ///（与工具链/语言包同根，见 [`crate::toolchain::rz_home`]）
    pub fn persistent_default() -> Self {
        let path = crate::toolchain::rz_home()
            .join("cache")
            .join("transpile-v1.json");
        Self::persistent(&path)
    }

    /// 显式写盘（幂等；无持久化路径时为空操作）
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let file = PersistentCacheFile {
            version: PERSISTENT_FORMAT_VERSION,
            entries: self.export_entries(),
        };
        let bytes = serde_json::to_vec(&file)?;
        // 父目录可能不存在（首次运行 ~/.rz/cache/ 未创建）：先建目录再写
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // 原子写：临时文件 + rename，避免进程中断留下半截 JSON
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// 批量写盘：有待写变更且绑定了持久化路径时写盘一次（幂等，无变更时空操作）。
    ///
    /// 批量转译场景应在全部文件处理完后调用一次（而非每个文件一次）：
    /// [`Self::save`] 会序列化整个缓存，逐次写盘会把并行转译串行化。
    /// 进程退出时由 `Drop` 兜底调用，正常路径无需显式调用。
    pub fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        let Some(path) = self.persistence.clone() else {
            // 无持久化路径：变更无需落盘，标记清零避免无谓重试
            self.dirty = false;
            return;
        };
        match self.save(&path) {
            Ok(()) => self.dirty = false,
            // 写失败：保留 dirty 以便后续（含 Drop）重试；缓存丢失可接受，不阻断转译
            Err(e) => crate::log_warn!(
                "translation_cache",
                "{}（{}）",
                crate::语言::t("log_cache_disk_save_failed"),
                e
            ),
        }
    }

    /// 从磁盘条目重建内存 LRU 结构（条目按旧→新顺序导入，代际队列保持一致）
    fn restore(&mut self, entries: Vec<PersistentEntry>) {
        for entry in entries {
            let hash = entry.hash;
            let generation = self.generations.entry(hash).or_insert(0);
            self.order.push_back((hash, *generation));
            self.entries.insert(
                hash,
                CacheEntry {
                    content_length: entry.content_length,
                    context_fingerprint: entry.context_fingerprint,
                    output: entry.output,
                },
            );
        }
        crate::log_info!(
            "translation_cache",
            "{}",
            crate::语言::f("log_cache_disk_loaded", &[&self.entries.len().to_string()])
        );
    }

    /// 按 LRU 顺序（旧→新）导出全部有效条目
    fn export_entries(&self) -> Vec<PersistentEntry> {
        let mut out = Vec::with_capacity(self.entries.len());
        for (hash, generation) in &self.order {
            if self.generations.get(hash) != Some(generation) {
                continue; // 过时代际条目：惰性跳过
            }
            if let Some(entry) = self.entries.get(hash) {
                out.push(PersistentEntry {
                    hash: *hash,
                    content_length: entry.content_length,
                    context_fingerprint: entry.context_fingerprint,
                    output: entry.output.clone(),
                });
            }
        }
        out
    }

    /// FNV-1a 64 位哈希（无第三方依赖，速度快，适合缓存键）
    pub fn compute_content_hash(content: &str) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in content.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// 生成翻译语境指纹：任一映射表内容变化或引擎源码变化时指纹变化
    ///
    /// 基于排序后的键值对拼接哈希，与映射表的插入顺序无关。
    ///
    /// 四张表全部参与：关键字、模块路径、别名、派生特征。
    /// 派生特征表由 `MappingManager` 从「派生特征」节单独存放（不并入
    /// keyword_map，避免与方法名别名冲突），若漏算会导致只改派生表时
    /// 指纹不变、缓存返回旧转译产物。
    /// 另混入引擎源码指纹（build.rs 对 src/ 全量生成）：缓存不仅须跟随
    /// 语言包映射变化，也须跟随转译算法变化——否则升级 rzc 后（算法修复/
    /// 调整），源文件未变时旧缓存仍命中、修复不生效（真实事故：别名替换
    /// 细则修复后，旧转译产物继续被复用）。
    pub fn generate_context_fingerprint(
        keyword_map: &HashMap<String, String>,
        module_path_map: &HashMap<String, String>,
        alias_map: &HashMap<String, String>,
        derive_map: &HashMap<String, String>,
    ) -> u64 {
        let mut pairs: Vec<String> = Vec::new();
        // 引擎源码指纹：转译算法自身的身份，算法代码变化即失效全部缓存
        pairs.push(format!(
            "engine-source:{:#x}",
            crate::语言::ENGINE_SOURCE_FINGERPRINT
        ));
        for map in [keyword_map, module_path_map, alias_map, derive_map] {
            for (key, value) in map {
                // 长度前缀 + NUL 定界：键/值中出现任意字符（含 `=`、`\0`）
                // 都不会产生歧义（`"a=b"/"c"` 与 `"a"/"b=c"` 若用 `=`
                // 拼接会生成相同指纹，缓存语境误判为未变化）
                pairs.push(format!("{}:{}\0{}", key.len(), key, value));
            }
        }
        pairs.sort();
        Self::compute_content_hash(&pairs.join("\n"))
    }

    /// 组合语言包语境指纹与项目上下文指纹为最终缓存指纹
    ///
    /// 不用异或：异或满足交换律（`(a, b)` 与 `(b, a)` 不可区分），且
    /// `None` 与「项目指纹恰为 0」的组合结果相同，会把「无项目上下文」与
    /// 「项目上下文指纹为 0」混为一谈而错误复用缓存。此处对 `None`/`Some`
    /// 使用显式标签，并用无歧义定界拼接后走稳定哈希（FNV-1a，跨进程一致，
    /// 与 [`Self::generate_context_fingerprint`] 同一算法，便于磁盘缓存比对）。
    pub fn combine_fingerprint(manager_fingerprint: u64, project_fingerprint: Option<u64>) -> u64 {
        let project_part = match project_fingerprint {
            Some(fp) => format!("project:{fp:#x}"),
            None => "project:none".to_string(),
        };
        Self::compute_content_hash(&format!("{manager_fingerprint:#x}\n{project_part}"))
    }

    /// 查询缓存（计数命中/未命中；不更新 LRU 顺序）
    ///
    /// 命中条件：内容哈希一致、内容长度一致、语境指纹一致。
    pub fn query(&self, content: &str, context_fingerprint: u64) -> Option<&TranspileOutput> {
        let hash = Self::compute_content_hash(content);
        match self.entries.get(&hash) {
            Some(entry)
                if entry.content_length == content.len()
                    && entry.context_fingerprint == context_fingerprint =>
            {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(&entry.output)
            }
            _ => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// 插入缓存条目（同哈希存在时覆盖并视为最近使用；超容量淘汰最旧条目）
    pub fn insert(&mut self, content: &str, context_fingerprint: u64, output: TranspileOutput) {
        let hash = Self::compute_content_hash(content);
        self.insert_precomputed(hash, content.len(), context_fingerprint, output);
    }

    /// 获取或翻译：命中则直接返回缓存产物，未命中则执行翻译闭包并写入缓存
    ///
    /// 翻译闭包返回 `Result`，失败时错误透传、不写入缓存。
    pub fn get_or_transpile<F>(
        &mut self,
        content: &str,
        context_fingerprint: u64,
        transpile_fn: F,
    ) -> Result<TranspileOutput, TranspileError>
    where
        F: FnOnce() -> Result<TranspileOutput, TranspileError>,
    {
        // 预计算哈希一次（旧实现 miss 路径计算两次）
        let hash = Self::compute_content_hash(content);
        // 内联查询逻辑，避免 `query()` 重复计算哈希
        let cached = match self.entries.get(&hash) {
            Some(entry)
                if entry.content_length == content.len()
                    && entry.context_fingerprint == context_fingerprint =>
            {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(entry.output.clone())
            }
            _ => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        };
        if let Some(output) = cached {
            crate::log_info!(
                "translation_cache",
                "{}",
                crate::语言::f(
                    "log_cache_hit",
                    &[&content.len().to_string(), &context_fingerprint.to_string()]
                )
            );
            self.mark_hit(hash);
            return Ok(output);
        }
        crate::log_info!(
            "translation_cache",
            "{}",
            crate::语言::f(
                "log_cache_miss",
                &[&content.len().to_string(), &context_fingerprint.to_string()]
            )
        );
        let output = transpile_fn()?;
        self.insert_precomputed(hash, content.len(), context_fingerprint, output.clone());
        Ok(output)
    }

    /// 清空全部条目（统计计数保留；绑定持久化路径时同步清空磁盘文件）
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.generations.clear();
        // 清空是显式破坏性操作，立即落盘（不等批量写盘）
        self.dirty = true;
        self.flush();
    }

    /// 当前缓存条目数
    pub fn current_count(&self) -> usize {
        self.entries.len()
    }

    /// 缓存容量值
    pub fn capacity_value(&self) -> usize {
        self.capacity
    }

    /// 累计命中次数
    pub fn hit_count(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// 累计未命中次数
    pub fn miss_count(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// 命中率（0.0 ~ 1.0；无查询记录时为 0.0）
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let total = hits + self.misses.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    // ===== 内部实现 =====

    fn insert_precomputed(
        &mut self,
        hash: u64,
        content_length: usize,
        context_fingerprint: u64,
        output: TranspileOutput,
    ) {
        if let Some(entry) = self.entries.get_mut(&hash) {
            // 同内容（含语境变化）覆盖，保持 LRU 位置为最新
            entry.content_length = content_length;
            entry.context_fingerprint = context_fingerprint;
            entry.output = output;
            self.mark_hit(hash);
            // 覆盖也是内容变更：置脏（此前该路径不回写磁盘，磁盘副本会残留旧产物）
            self.dirty = true;
            return;
        }
        self.entries.insert(
            hash,
            CacheEntry {
                content_length,
                context_fingerprint,
                output,
            },
        );
        let generation = self.generations.entry(hash).or_insert(0);
        self.order.push_back((hash, *generation));
        // 淘汰最旧有效条目（跳过代际不匹配的过时条目）
        while self.entries.len() > self.capacity {
            while let Some((front_hash, front_gen)) = self.order.pop_front() {
                if self.generations.get(&front_hash) == Some(&front_gen) {
                    // 代际匹配：这是有效条目，执行淘汰
                    self.generations.remove(&front_hash);
                    self.entries.remove(&front_hash);
                    crate::log_debug!(
                        "translation_cache",
                        "{}",
                        crate::语言::f("log_cache_evict", &[&front_hash.to_string()])
                    );
                    break;
                }
                // 代际不匹配：过时条目，惰性跳过
            }
        }
        // 标记待写盘：实际写盘由 `flush()` 批量执行（详见 `dirty` 字段说明）
        self.dirty = true;
    }

    /// O(1) LRU 命中更新：递增代际并在队尾添加新条目，
    /// 旧代际条目留在队列中，淘汰时因代际不匹配被惰性跳过；
    /// 队列膨胀超过阈值（容量 4 倍，最少 64）时压缩，
    /// 防止长期运行（如 LSP 会话）下过时条目无限累积
    fn mark_hit(&mut self, hash: u64) {
        let generation = self.generations.entry(hash).or_insert(0);
        *generation += 1;
        self.order.push_back((hash, *generation));
        if self.order.len() > self.capacity.saturating_mul(4).max(64) {
            self.compact_order();
        }
    }

    /// 压缩 LRU 队列：从队尾（最新）向队首扫描，每个哈希只保留
    /// 最后一次访问的有效条目，其余过时条目全部移除
    ///（压缩后队列长度 ≤ 缓存条目数 ≤ 容量，淘汰逻辑照常工作）
    fn compact_order(&mut self) {
        let mut seen = HashSet::new();
        let mut compact: VecDeque<(u64, u64)> = VecDeque::with_capacity(self.entries.len());
        for &(hash, generation) in self.order.iter().rev() {
            if seen.insert(hash) && self.generations.get(&hash) == Some(&generation) {
                compact.push_front((hash, generation));
            }
        }
        self.order = compact;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fingerprint() -> u64 {
        TranslationCache::compute_content_hash("测试语境")
    }

    fn sample_output(suffix: &str) -> TranspileOutput {
        TranspileOutput::new(format!("翻译输出{}", suffix))
    }

    #[test]
    fn test_content_hash_deterministic_and_different() {
        let hash1 = TranslationCache::compute_content_hash("函数 主函数() {}");
        let hash2 = TranslationCache::compute_content_hash("函数 主函数() {}");
        assert_eq!(hash1, hash2);
        assert_ne!(
            hash1,
            TranslationCache::compute_content_hash("函数 主函数() { }")
        );
        assert_ne!(TranslationCache::compute_content_hash(""), hash1);
    }

    /// 组合指纹：`None` 与 `Some(0)` 必须区分（异或会把二者混为一谈）
    #[test]
    fn test_combine_fingerprint_distinguishes_none_from_zero() {
        let 语言包 = sample_fingerprint();
        let 无项目 = TranslationCache::combine_fingerprint(语言包, None);
        let 零指纹项目 = TranslationCache::combine_fingerprint(语言包, Some(0));
        assert_ne!(无项目, 零指纹项目, "无项目上下文不应与指纹为 0 的项目同键");
    }

    /// 组合指纹：区分不同项目上下文，且对同一输入稳定
    #[test]
    fn test_combine_fingerprint_distinguishes_projects() {
        let 语言包 = sample_fingerprint();
        let 甲 = TranslationCache::combine_fingerprint(语言包, Some(1));
        let 乙 = TranslationCache::combine_fingerprint(语言包, Some(2));
        assert_ne!(甲, 乙);
        assert_eq!(甲, TranslationCache::combine_fingerprint(语言包, Some(1)));
    }

    #[test]
    fn test_context_fingerprint_independent_of_map_order() {
        let a = HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("让".to_string(), "let".to_string()),
        ]);
        let b = HashMap::from([
            ("让".to_string(), "let".to_string()),
            ("函数".to_string(), "fn".to_string()),
        ]);
        let empty = HashMap::new();
        assert_eq!(
            TranslationCache::generate_context_fingerprint(&a, &empty, &empty, &empty),
            TranslationCache::generate_context_fingerprint(&b, &empty, &empty, &empty)
        );
        let c = HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("让".to_string(), "let".to_string()),
            ("可变".to_string(), "mut".to_string()),
        ]);
        assert_ne!(
            TranslationCache::generate_context_fingerprint(&a, &empty, &empty, &empty),
            TranslationCache::generate_context_fingerprint(&c, &empty, &empty, &empty)
        );
    }

    /// 语境指纹须覆盖派生特征表
    ///
    /// 回归：指纹曾只算关键字 / 模块路径 / 别名三表，而「派生特征」节由
    /// `MappingManager` 单独存放（不并入 keyword_map，避免与方法名别名冲突）。
    /// 只改派生表时指纹不变，缓存会返回旧转译产物。
    #[test]
    fn test_context_fingerprint_covers_derive_map() {
        let keywords = HashMap::from([("函数".to_string(), "fn".to_string())]);
        let empty = HashMap::new();
        let derive_a = HashMap::from([("克隆".to_string(), "Clone".to_string())]);
        let derive_b = HashMap::from([("克隆".to_string(), "Debug".to_string())]);

        // 仅派生表内容不同 → 指纹必须不同，否则缓存不会失效
        assert_ne!(
            TranslationCache::generate_context_fingerprint(&keywords, &empty, &empty, &derive_a),
            TranslationCache::generate_context_fingerprint(&keywords, &empty, &empty, &derive_b)
        );
        // 派生表内容相同（与插入顺序无关）→ 指纹一致
        let derive_c = HashMap::from([("克隆".to_string(), "Clone".to_string())]);
        assert_eq!(
            TranslationCache::generate_context_fingerprint(&keywords, &empty, &empty, &derive_a),
            TranslationCache::generate_context_fingerprint(&keywords, &empty, &empty, &derive_c)
        );
    }

    #[test]
    fn test_hit_and_miss() {
        let mut cache = TranslationCache::new(8);
        let fp = sample_fingerprint();
        let content = "函数 主函数() {}";

        assert!(cache.query(content, fp).is_none());
        cache.insert(content, fp, sample_output("甲"));
        assert_eq!(cache.query(content, fp).unwrap().output, "翻译输出甲");
        assert_eq!(cache.current_count(), 1);
        assert_eq!(cache.hit_count(), 1);
        assert_eq!(cache.miss_count(), 1);
        assert_eq!(cache.hit_rate(), 0.5);
    }

    #[test]
    fn test_content_change_causes_miss() {
        let mut cache = TranslationCache::new(8);
        let fp = sample_fingerprint();
        cache.insert("函数 主函数() {}", fp, sample_output("甲"));
        assert!(cache.query("函数 主函数() { 让 x = 1; }", fp).is_none());
        assert_eq!(cache.current_count(), 1);
        assert_eq!(cache.miss_count(), 1);
    }

    #[test]
    fn test_context_change_invalidates_cache() {
        let mut cache = TranslationCache::new(8);
        let fp1 = TranslationCache::compute_content_hash("语言包版本 1");
        let fp2 = TranslationCache::compute_content_hash("语言包版本 2");
        cache.insert("函数 主函数() {}", fp1, sample_output("旧"));
        assert!(cache.query("函数 主函数() {}", fp2).is_none());
        cache.insert("函数 主函数() {}", fp2, sample_output("新"));
        assert_eq!(
            cache.query("函数 主函数() {}", fp2).unwrap().output,
            "翻译输出新"
        );
    }

    #[test]
    fn test_capacity_evicts_oldest() {
        let mut cache = TranslationCache::new(2);
        let fp = sample_fingerprint();
        cache.insert("内容甲", fp, sample_output("甲"));
        cache.insert("内容乙", fp, sample_output("乙"));
        cache.insert("内容丙", fp, sample_output("丙"));
        assert_eq!(cache.current_count(), 2);
        assert!(cache.query("内容甲", fp).is_none());
        assert!(cache.query("内容乙", fp).is_some());
        assert!(cache.query("内容丙", fp).is_some());
    }

    /// 大量命中后 LRU 队列必须压缩，不能随命中次数无限增长
    ///（长期运行如 LSP 会话下，过时条目累积是内存泄漏）
    #[test]
    fn test_queue_compacts_after_many_hits() {
        let mut cache = TranslationCache::new(2);
        let fp = sample_fingerprint();
        cache.insert("内容甲", fp, sample_output("甲"));
        cache.insert("内容乙", fp, sample_output("乙"));
        // 命中 100 次：队列越过压缩阈值（容量 4 倍、最少 64）后必须压缩回有界；
        // 无压缩机制时 100 次命中会积累到 102 条
        for _ in 0..100 {
            cache
                .get_or_transpile("内容甲", fp, || Ok(sample_output("甲")))
                .expect("翻译失败");
        }
        assert!(cache.order.len() <= 64, "队列未压缩: {}", cache.order.len());
        // 压缩不影响 LRU 淘汰语义：再插入第三个条目，最旧的乙被淘汰
        cache.insert("内容丙", fp, sample_output("丙"));
        assert!(cache.query("内容甲", fp).is_some());
        assert!(cache.query("内容乙", fp).is_none());
        assert!(cache.query("内容丙", fp).is_some());
    }

    /// 键/值含 `=` 时指纹不得碰撞（旧实现用 `=` 拼接会产生歧义）
    #[test]
    fn test_fingerprint_no_separator_collision() {
        let empty = HashMap::new();
        let a = HashMap::from([("a=b".to_string(), "c".to_string())]);
        let b = HashMap::from([("a".to_string(), "b=c".to_string())]);
        assert_ne!(
            TranslationCache::generate_context_fingerprint(&a, &empty, &empty, &empty),
            TranslationCache::generate_context_fingerprint(&b, &empty, &empty, &empty)
        );
    }

    #[test]
    fn test_hit_refreshes_lru_order() {
        let mut cache = TranslationCache::new(2);
        let fp = sample_fingerprint();
        cache.insert("内容甲", fp, sample_output("甲"));
        cache.insert("内容乙", fp, sample_output("乙"));
        let result = cache
            .get_or_transpile("内容甲", fp, || Ok(sample_output("甲")))
            .expect("翻译失败");
        assert_eq!(result.output, "翻译输出甲");
        cache.insert("内容丙", fp, sample_output("丙"));
        assert!(cache.query("内容甲", fp).is_some());
        assert!(cache.query("内容乙", fp).is_none());
        assert!(cache.query("内容丙", fp).is_some());
    }

    #[test]
    fn test_get_or_transpile_closure_execution_count() {
        let mut cache = TranslationCache::with_default_capacity();
        let fp = sample_fingerprint();
        let content = "函数 主函数() {}";
        let mut exec_count = 0;

        let first = cache
            .get_or_transpile(content, fp, || {
                exec_count += 1;
                Ok(sample_output("甲"))
            })
            .expect("翻译失败");
        assert_eq!(first.output, "翻译输出甲");
        assert_eq!(exec_count, 1);

        let second = cache
            .get_or_transpile(content, fp, || {
                exec_count += 1;
                Ok(sample_output("乙"))
            })
            .expect("翻译失败");
        assert_eq!(second.output, "翻译输出甲");
        assert_eq!(exec_count, 1);
        assert_eq!(cache.hit_count(), 1);
        assert_eq!(cache.miss_count(), 1);
    }

    #[test]
    fn test_get_or_transpile_error_propagation() {
        let mut cache = TranslationCache::with_default_capacity();
        let fp = sample_fingerprint();
        let content = "函数 主函数() {}";

        let result = cache.get_or_transpile(content, fp, || {
            Err(TranspileError::InvalidInput {
                reason: "模拟失败".to_string(),
            })
        });
        assert!(matches!(result, Err(TranspileError::InvalidInput { .. })));
        assert_eq!(cache.current_count(), 0);
    }

    #[test]
    fn test_source_map_entry() {
        let entry = SourceMapEntry::new(0, 6, "函数", "fn");
        assert_eq!(entry.source_offset, 0);
        assert_eq!(entry.length, 6);
        assert_eq!(entry.original, "函数");
        assert_eq!(entry.replacement, "fn");
    }

    #[test]
    fn test_clear_and_stats() {
        let mut cache = TranslationCache::new(4);
        let fp = sample_fingerprint();
        cache.insert("内容甲", fp, sample_output("甲"));
        cache.insert("内容乙", fp, sample_output("乙"));
        assert_eq!(cache.current_count(), 2);
        cache.clear();
        assert_eq!(cache.current_count(), 0);
        assert_eq!(cache.capacity_value(), 4);
        assert_eq!(cache.hit_rate(), 0.0);
    }

    #[test]
    fn test_capacity_at_least_one() {
        let cache = TranslationCache::new(0);
        assert_eq!(cache.capacity_value(), 1);
    }

    // ===== 磁盘持久化 =====

    #[test]
    fn test_persistent_roundtrip() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("transpile.json");
        let fp = sample_fingerprint();

        // 第一次：插入只累积变更，写盘延迟到 flush（或进程退出时 Drop 兜底）
        {
            let mut cache = TranslationCache::persistent(&path);
            cache.insert("函数 主函数() {}", fp, sample_output("甲"));
            assert!(
                !path.exists(),
                "插入不应立即写盘：批量转译下逐次全量写会把并行转译串行化"
            );
            cache.flush();
            assert!(path.exists(), "flush 后应落盘");
        }
        // 第二次（模拟新进程）：从磁盘加载，可直接命中
        {
            let cache = TranslationCache::persistent(&path);
            let hit = cache.query("函数 主函数() {}", fp);
            assert_eq!(hit.map(|o| o.output.as_str()), Some("翻译输出甲"));
            assert_eq!(cache.current_count(), 1);
        }
    }

    #[test]
    fn test_persistent_with_full_map() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("transpile.json");
        let fp = sample_fingerprint();
        let output = TranspileOutput::with_full_map(
            "fn 主函数() {}".to_string(),
            vec![SourceMapEntry::new(0, 6, "函数", "fn")],
            vec![SourceMapEntry::new(0, 6, "函数", "fn")],
        );
        {
            let mut cache = TranslationCache::persistent(&path);
            cache.insert("函数 主函数() {}", fp, output);
        }
        let cache = TranslationCache::persistent(&path);
        let hit = cache.query("函数 主函数() {}", fp).expect("应命中");
        assert_eq!(hit.output, "fn 主函数() {}");
        assert_eq!(
            hit.source_map,
            vec![SourceMapEntry::new(0, 6, "函数", "fn")]
        );
        assert_eq!(hit.pipeline_map.len(), 1);
    }

    #[test]
    fn test_persistent_version_mismatch_discarded() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("transpile.json");
        std::fs::write(&path, r#"{"version": 999, "entries": []}"#).expect("写入旧版本文件");
        let cache = TranslationCache::persistent(&path);
        assert_eq!(cache.current_count(), 0, "版本不匹配应丢弃");
    }

    #[test]
    fn test_persistent_corrupted_file_discarded() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("transpile.json");
        std::fs::write(&path, "这不是 JSON{{{ 内容").expect("写入损坏文件");
        let cache = TranslationCache::persistent(&path);
        assert_eq!(cache.current_count(), 0, "损坏文件应丢弃");
    }

    #[test]
    fn test_persistent_clear_writes_empty_file() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("transpile.json");
        let fp = sample_fingerprint();
        {
            let mut cache = TranslationCache::persistent(&path);
            cache.insert("内容甲", fp, sample_output("甲"));
            cache.clear();
        }
        let cache = TranslationCache::persistent(&path);
        assert_eq!(cache.current_count(), 0, "清空后磁盘文件应同步为空");
    }

    #[test]
    fn test_persistent_lru_export_keeps_valid_entries() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("transpile.json");
        let fp = sample_fingerprint();
        {
            // 容量 2：插入 3 条，最旧一条被淘汰
            let mut cache = TranslationCache::persistent(&path);
            cache.capacity = 2;
            cache.insert("内容甲", fp, sample_output("甲"));
            cache.insert("内容乙", fp, sample_output("乙"));
            cache.insert("内容丙", fp, sample_output("丙"));
            assert_eq!(cache.current_count(), 2);
        }
        let cache = TranslationCache::persistent(&path);
        assert_eq!(cache.current_count(), 2, "淘汰后的有效条目应完整持久化");
        assert!(cache.query("内容乙", fp).is_some());
        assert!(cache.query("内容丙", fp).is_some());
        assert!(cache.query("内容甲", fp).is_none(), "被淘汰条目不应复活");
    }

    /// 批量转译：N 次插入只在 flush 时写盘一次，且全部落盘
    ///
    /// 回归：此前每次 insert 都会把整个缓存序列化写盘（N 文件 → N 次全量写，
    /// 且发生在调用方持锁期间），是批量转译的主要 I/O 开销来源。
    #[test]
    fn test_persistent_batches_writes_until_flush() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("transpile.json");
        let fp = sample_fingerprint();
        let mut cache = TranslationCache::persistent(&path);
        for 名 in ["甲", "乙", "丙", "丁"] {
            cache.insert(&format!("内容{名}"), fp, sample_output(名));
            assert!(!path.exists(), "flush 前不应有任何写盘");
        }
        assert_eq!(cache.current_count(), 4);
        cache.flush();
        let 重载 = TranslationCache::persistent(&path);
        assert_eq!(重载.current_count(), 4, "一次 flush 应写入全部累积条目");
        // 幂等：无新变更时再 flush 不产生写盘（文件 mtime 不变）
        let 修改前 = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        cache.flush();
        let 修改后 = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        assert_eq!(修改前, 修改后, "无变更时 flush 应为空操作");
    }
}
