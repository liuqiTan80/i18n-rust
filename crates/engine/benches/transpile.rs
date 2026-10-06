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
use i18n_rust_engine::全角标点::查找全角标点;
use i18n_rust_engine::教学检查::执行教学检查;
use i18n_rust_engine::映射管理::映射管理器;
use i18n_rust_engine::缓存::转译缓存;
use i18n_rust_engine::{源码转译, 转译管线};

/// 构造内置 zh 语言包的映射管理器（与 CLI 默认一致）
fn 内置管理器() -> 映射管理器 {
    let 目录 = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
    映射管理器::自目录加载(&目录).expect("加载 zh 语言包失败")
}

/// 教学场景中等规模源码：约 250 行混合结构
/// （函数/类型注解/字符串/宏/use 路径/for 循环/向量），模拟真实教学文件
fn 教学语料() -> String {
    let mut 语料文本 = String::new();
    语料文本.push_str("使用 标准库::集合::映射;\n\n");
    for 序号 in 0..50 {
        语料文本.push_str(&format!(
            "函数 函数_{序号}(数量: 整数, 名称: 字符串) -> 整数 {{\n\
             \x20   让 结果 = 数量 * 2;\n\
             \x20   // 计算注释：{序号}\n\
             \x20   打印行!(\"处理第 {序号} 个：{{}}\", 名称);\n\
             \x20   返回 结果;\n\
             }}\n\n",
        ));
    }
    语料文本.push_str(
        "函数 主函数() {\n\
         \x20   让 全部 = 向量::新建();\n\
         \x20   for i in 0..50 {\n\
         \x20       全部.推送(函数_i(i, \"测试\"));\n\
         \x20   }\n\
         \x20   打印行!(\"总计：{{:?}}\", 全部);\n\
         }\n",
    );
    语料文本
}

/// 转译管线：无缓存全量 vs 缓存命中
fn 转译基准(基准目标: &mut Criterion) {
    let 管理器 = 内置管理器();
    let 语料 = 教学语料();
    let mut 基准组 = 基准目标.benchmark_group("transpile");
    基准组.sample_size(50);

    基准组.bench_function("pipeline_no_cache", |基准器| {
        基准器.iter(|| 转译管线(black_box(&语料), &管理器))
    });

    // 缓存命中场景：先用真实转译预热缓存，迭代期间内容哈希不变全命中
    let mut 增量缓存 = 转译缓存::默认容量新建();
    源码转译(&语料, &管理器, &mut 增量缓存).expect("预热转译失败");
    基准组.bench_function("pipeline_cache_hit", |基准器| {
        基准器.iter(|| 源码转译(black_box(&语料), &管理器, &mut 增量缓存).expect("缓存转译失败"))
    });

    基准组.finish();
}

/// 教学检查单独计费（LSP 每次变更都会执行全角扫描与教学 lint）
fn 教学检查基准(基准目标: &mut Criterion) {
    let 语料 = 教学语料();
    let mut 基准组 = 基准目标.benchmark_group("teaching_checks");
    基准组.sample_size(50);

    for (名称, 代码) in [
        ("fullwidth_scan", black_box(&语料) as &str),
        ("lint_scan", black_box(语料.as_str())),
    ] {
        基准组.bench_with_input(
            BenchmarkId::new(名称, 语料.len()),
            &代码,
            |基准器, 源码| {
                if 名称 == "fullwidth_scan" {
                    基准器.iter(|| 查找全角标点(源码));
                } else {
                    基准器.iter(|| 执行教学检查(源码));
                }
            },
        );
    }
    基准组.finish();
}

criterion_group!(基准集合, 转译基准, 教学检查基准);
criterion_main!(基准集合);
