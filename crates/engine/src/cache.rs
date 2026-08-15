// 翻译缓存模块
// 基于内容哈希的增量翻译缓存。
// 内容未变化且语言包映射未变化时直接复用翻译结果，避免重复翻译；
// 同时缓存源映射（被替换标识符的源偏移与替换文本），供 LSP/调试使用。

use crate::error::TranspileError;
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};

/// 源映射条目：源文件中一个被替换的标识符 token
///
/// 记录源侧信息（字节偏移、长度）与翻译前后文本；
/// 不提供目标偏移——完整管线后续的模块路径替换/别名替换会改变输出偏移，
/// 源侧信息始终保持精确。
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// 翻译产物：翻译后的代码与源映射
#[derive(Debug, Clone, PartialEq)]
pub struct TranspileOutput {
    /// 翻译后的代码文本
    pub output: String,
    /// 源映射条目列表
    pub source_map: Vec<SourceMapEntry>,
}

impl TranspileOutput {
    /// 仅创建输出（无源映射）
    pub fn new(output: String) -> Self {
        Self {
            output,
            source_map: Vec::new(),
        }
    }

    /// 创建输出并附带源映射
    pub fn with_map(output: String, source_map: Vec<SourceMapEntry>) -> Self {
        Self { output, source_map }
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
pub struct TranslationCache {
    entries: HashMap<u64, CacheEntry>,
    /// LRU 顺序：队首最旧、队尾最新
    order: VecDeque<u64>,
    capacity: usize,
    hits: Cell<u64>,
    misses: Cell<u64>,
}

impl TranslationCache {
    /// 新建缓存（容量至少为 1）
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            capacity: capacity.max(1),
            hits: Cell::new(0),
            misses: Cell::new(0),
        }
    }

    /// 默认容量：256 个文件条目
    pub fn with_default_capacity() -> Self {
        Self::new(256)
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

    /// 生成翻译语境指纹：任一映射表内容变化时指纹变化
    ///
    /// 基于排序后的键值对拼接哈希，与映射表的插入顺序无关。
    pub fn generate_context_fingerprint(
        keyword_map: &HashMap<String, String>,
        module_path_map: &HashMap<String, String>,
        alias_map: &HashMap<String, String>,
    ) -> u64 {
        let mut pairs: Vec<String> = Vec::new();
        for map in [keyword_map, module_path_map, alias_map] {
            for (key, value) in map {
                pairs.push(format!("{}={}", key, value));
            }
        }
        pairs.sort();
        Self::compute_content_hash(&pairs.join("\n"))
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
                self.hits.set(self.hits.get() + 1);
                Some(&entry.output)
            }
            _ => {
                self.misses.set(self.misses.get() + 1);
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
        if let Some(output) = self.query(content, context_fingerprint) {
            let output = output.clone();
            let hash = Self::compute_content_hash(content);
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
        let hash = Self::compute_content_hash(content);
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

    /// 清空全部条目（统计计数保留）
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
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
        self.hits.get()
    }

    /// 累计未命中次数
    pub fn miss_count(&self) -> u64 {
        self.misses.get()
    }

    /// 命中率（0.0 ~ 1.0；无查询记录时为 0.0）
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits.get() + self.misses.get();
        if total == 0 {
            0.0
        } else {
            self.hits.get() as f64 / total as f64
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
        self.order.push_back(hash);
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
                crate::log_debug!(
                    "translation_cache",
                    "{}",
                    crate::语言::f("log_cache_evict", &[&oldest.to_string()])
                );
            }
        }
    }

    fn mark_hit(&mut self, hash: u64) {
        self.order.retain(|key| *key != hash);
        self.order.push_back(hash);
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
            TranslationCache::generate_context_fingerprint(&a, &empty, &empty),
            TranslationCache::generate_context_fingerprint(&b, &empty, &empty)
        );
        let c = HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("让".to_string(), "let".to_string()),
            ("可变".to_string(), "mut".to_string()),
        ]);
        assert_ne!(
            TranslationCache::generate_context_fingerprint(&a, &empty, &empty),
            TranslationCache::generate_context_fingerprint(&c, &empty, &empty)
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
}
