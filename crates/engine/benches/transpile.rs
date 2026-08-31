//! 转译管线性能基准（criterion）
//!
//! 覆盖教学工具的三大热点：
//! - `pipeline_no_cache`：无缓存全量转译（Unicode/全角/lint 检查 + 词法 +
//!   模块路径 + 别名替换），即 CLI / LSP 冷启动单文件转译的真实开销；
//! - `pipeline_cache_hit`：带增量缓存的转译（编辑器反复输入同一文件的场景，
//!   内容哈希未变时直接复用翻译结果）；
//! - `teaching_checks`：全角标点扫描与教学 lint 单独计费（LSP 诊断发布前
//!   每次变更都会执行，单独量化便于发现回归）。
//!
//! 运行：`cargo bench -p i18n-rust-engine`
//! CI 快速模式：`cargo bench -p i18n-rust-engine --bench transpile -- --quick`

use std::hint::black_box;
use std::path::Path;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use i18n_rust_engine::cache::TranslationCache;
use i18n_rust_engine::fullwidth::find_fullwidth_punct;
use i18n_rust_engine::lint::lint_teaching;
use i18n_rust_engine::mapping_manager::MappingManager;
use i18n_rust_engine::{transpile_pipeline, transpile_source};

/// 构造内置 zh 语言包的映射管理器（与 CLI 默认一致）
fn zh_manager() -> MappingManager {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
    MappingManager::load_from_dir(&dir).expect("加载 zh 语言包失败")
}

/// 教学场景中等规模源码：约 250 行混合结构
/// （函数/类型注解/字符串/宏/use 路径/for 循环/向量），模拟真实教学文件
fn teaching_source() -> String {
    let mut src = String::new();
    src.push_str("使用 标准库::集合::映射;\n\n");
    for i in 0..50 {
        src.push_str(&format!(
            "函数 函数_{i}(数量: 整数, 名称: 字符串) -> 整数 {{\n\
             \x20   让 结果 = 数量 * 2;\n\
             \x20   // 计算注释：{i}\n\
             \x20   打印行!(\"处理第 {i} 个：{{}}\", 名称);\n\
             \x20   返回 结果;\n\
             }}\n\n",
        ));
    }
    src.push_str(
        "函数 主函数() {\n\
         \x20   让 全部 = 向量::新建();\n\
         \x20   for i in 0..50 {\n\
         \x20       全部.推送(函数_i(i, \"测试\"));\n\
         \x20   }\n\
         \x20   打印行!(\"总计：{{:?}}\", 全部);\n\
         }\n",
    );
    src
}

/// 转译管线：无缓存全量 vs 缓存命中
fn bench_transpile(c: &mut Criterion) {
    let manager = zh_manager();
    let source = teaching_source();
    let mut group = c.benchmark_group("transpile");
    group.sample_size(50);

    group.bench_function("pipeline_no_cache", |b| {
        b.iter(|| transpile_pipeline(black_box(&source), &manager))
    });

    // 缓存命中场景：先用真实转译预热缓存，迭代期间内容哈希不变全命中
    let mut cache = TranslationCache::with_default_capacity();
    transpile_source(&source, &manager, &mut cache).expect("预热转译失败");
    group.bench_function("pipeline_cache_hit", |b| {
        b.iter(|| transpile_source(black_box(&source), &manager, &mut cache).expect("缓存转译失败"))
    });

    group.finish();
}

/// 教学检查单独计费（LSP 每次变更都会执行全角扫描与教学 lint）
fn bench_teaching_checks(c: &mut Criterion) {
    let source = teaching_source();
    let mut group = c.benchmark_group("teaching_checks");
    group.sample_size(50);

    for (名称, 代码) in [
        ("fullwidth_scan", black_box(&source) as &str),
        ("lint_scan", black_box(source.as_str())),
    ] {
        group.bench_with_input(
            BenchmarkId::new(名称, source.len()),
            &代码,
            |b, 源码| {
                if 名称 == "fullwidth_scan" {
                    b.iter(|| find_fullwidth_punct(源码));
                } else {
                    b.iter(|| lint_teaching(源码));
                }
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_transpile, bench_teaching_checks);
criterion_main!(benches);
