//! 转译管线性能基准（criterion）
//!
//! 覆盖教学工具的四类热点：
//! - `pipeline_no_cache`：无缓存全量转译（Unicode/全角/lint 检查 + 词法 +
//!   模块路径 + 别名替换），即 CLI / LSP 冷启动单文件转译的真实开销；
//! - `pipeline_cache_hit`：带增量缓存的转译（编辑器反复输入同一文件的场景，
//!   内容哈希未变时直接复用翻译结果）；
//! - `teaching_checks`：全角标点扫描与教学 lint 单独计费（LSP 诊断发布前
//!   每次变更都会执行，单独量化便于发现回归）；
//! - `pipeline_*_2000`：约 2000 行大文件输入的冷转译与缓存命中，守住
//!   transpile 的线性低延迟（由 `tools/bench-check.sh` 阈值守卫）；
//! - `pipeline_no_cache_2000_alias_heavy`：同规模但**别名密集**的语料。关键字
//!   密集语料的后续阶段只有几条编辑，盖不住「后续阶段编辑 × 词法编辑」这类
//!   乘积项（历史教训：`组合流程映射` 的逆映射曾逐条从头重扫，
//!   同一文件从 3.2 ms 涨到 37 ms，而旧语料完全测不出），故单独一份语料。
//!
//! 运行：`cargo bench -p i18n-rust-engine`
//! CI 快速模式：`cargo bench -p i18n-rust-engine --bench transpile -- --quick`

use std::hint::black_box;
use std::path::Path;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use i18n_rust_engine::全角标点::检测全角标点;
use i18n_rust_engine::映射管理器::映射管理器;
use i18n_rust_engine::校验::{
    按词形桶检查字符, 按词表检查教学, 检查保留名撞词, 检查全部教学, 检查教学,
};
use i18n_rust_engine::缓存::转译缓存;
use i18n_rust_engine::{别名, 词法器, 转译源文本, 转译管线};

/// 构造内置 zh 语言包的映射管理器（与 CLI 默认一致）
fn zh_manager() -> 映射管理器 {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
    映射管理器::从目录载入(&dir).expect("加载 zh 语言包失败")
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

/// 大规模源码：约 2000 行（280 个函数 × ~7 行 + 入口），模拟大文件输入。
/// 冷转译成本随行数近线性增长；本基准由 bench-check 阈值守卫，防止在
/// 语言包/管线演进中丢掉 transpile 的低延迟优势。
fn large_source() -> String {
    let mut src = String::with_capacity(2000 * 40);
    src.push_str("使用 标准库::集合::映射;\n\n");
    for i in 0..280 {
        src.push_str(&format!(
            "函数 函数_{i}(数量: 整数, 名称: 字符串) -> 整数 {{\n\
             \x20   让 结果 = 数量 * 2;\n\
             \x20   // 计算注释：{i}\n\
             \x20   打印行!(\"处理第 {i} 个：{{}}\", 名称);\n\
             \x20   返回 结果;\n\
             }}\n\n",
        ));
    }
    src.push_str("函数 主函数() {\n    让 全部 = 向量::新建();\n}\n");
    src
}

/// 别名密集语料：约 2000 行，但每函数多次标准库成员调用（`.长度()`
/// `.推入()` `.迭代().计数()` `.插入()` `.元素数()`）
///
/// 与 `large_source` 的行数量级相同，后续阶段的编辑数却从 2 条涨到上千条：
/// 专测阶段间偏移换算的乘积项（见模块文档）。不写整数字面量、方法名全部
/// 收录在语言包里，以免教学告警写 stderr 落在计时区里干扰读数。
fn alias_heavy_source() -> String {
    let mut src = String::with_capacity(2000 * 40);
    src.push_str("使用 标准库::向量::向量;\n使用 标准库::映射::映射;\n\n");
    for i in 0..280 {
        src.push_str(&format!(
            "函数 函数_{i}(项: 向量<字符串>, 表: 映射<字符串, 整数>) -> 整数 {{\n\
             \x20   让 个数 = 项.长度();\n\
             \x20   项.推入(\"新项{i}\");\n\
             \x20   让 合计 = 项.迭代().计数();\n\
             \x20   表.插入(\"键{i}\", 合计);\n\
             \x20   返回 个数 + 表.元素数();\n\
             }}\n\n",
        ));
    }
    src.push_str("函数 主函数() {\n    让 全部 = 向量::新建();\n}\n");
    src
}

/// 转译管线：无缓存全量 vs 缓存命中
fn bench_transpile(c: &mut Criterion) {
    let manager = zh_manager();
    let source = teaching_source();
    let mut group = c.benchmark_group("transpile");
    group.sample_size(50);

    group.bench_function("pipeline_no_cache", |b| {
        b.iter(|| 转译管线(black_box(&source), &manager))
    });

    // 缓存命中场景：先用真实转译预热缓存，迭代期间内容哈希不变全命中
    let mut cache = 转译缓存::默认容量();
    转译源文本(&source, &manager, &mut cache).expect("预热转译失败");
    group.bench_function("pipeline_cache_hit", |b| {
        b.iter(|| 转译源文本(black_box(&source), &manager, &mut cache).expect("缓存转译失败"))
    });

    group.finish();
}

/// 教学检查单独计费（LSP 每次变更都会执行全角扫描、教学 lint 与保留字撞名扫描）
///
/// `lint_scan` 是空词表阈值（只看状态机本体）；`lint_scan_words` 才是管线
/// 真实路径——带整张词表与歧义构造器集合，易混方法名判定在里面；
/// `teaching_all_scan` 把英文回落与保留字撞名也算进来（两者共用一次遍历）。
///
/// `lint_scan_buckets` 是转译管线真正走的分桶索引入口（索引由 manager 缓存、
/// 带精确命中表）；上面几个 `*_words` 走惰性构建，每轮都要重建索引，只适合
/// 衡定单文件 API 的开销。
fn bench_teaching_checks(c: &mut Criterion) {
    let manager = zh_manager();
    let source = teaching_source();
    let keyword_map = manager.取关键词表();
    let fallback_map = manager.取英文回落();
    let words = manager.取校验词表();
    let ambiguous = manager.取歧义构造器();
    let buckets = manager.取校验分桶();
    // 管线已物化过字符切片（全角/教学 lint 共用一份），基准同样把它放在闭包外
    let chars: Vec<char> = source.chars().collect();
    let mut group = c.benchmark_group("teaching_checks");
    group.sample_size(50);

    for (名称, 代码) in [
        ("fullwidth_scan", black_box(&source) as &str),
        ("lint_scan", black_box(source.as_str())),
        ("lint_scan_words", black_box(source.as_str())),
        ("lint_scan_buckets", black_box(source.as_str())),
        ("collision_scan", black_box(source.as_str())),
        ("teaching_all_scan", black_box(source.as_str())),
    ] {
        group.bench_with_input(
            BenchmarkId::new(名称, source.len()),
            &代码,
            |b, 源码| match 名称 {
                "fullwidth_scan" => b.iter(|| 检测全角标点(源码)),
                "collision_scan" => b.iter(|| 检查保留名撞词(源码, keyword_map)),
                "lint_scan_words" => b.iter(|| 按词表检查教学(源码, words, ambiguous)),
                "lint_scan_buckets" => {
                    b.iter(|| 按词形桶检查字符(&chars, buckets, ambiguous))
                }
                "teaching_all_scan" => {
                    b.iter(|| 检查全部教学(源码, words, ambiguous, fallback_map, keyword_map))
                }
                _ => b.iter(|| 检查教学(源码)),
            },
        );
    }
    group.finish();
}

/// 约 2000 行大文件：冷转译与缓存命中，守住 transpile 的线性低延迟
fn bench_transpile_large(c: &mut Criterion) {
    let manager = zh_manager();
    let source = large_source();
    let mut group = c.benchmark_group("transpile");
    // 大输入每次迭代约数毫秒，降低样本数以免 CI 过长（仍满足阈值判定）
    group.sample_size(30);

    group.bench_function("pipeline_no_cache_2000", |b| {
        b.iter(|| 转译管线(black_box(&source), &manager))
    });

    // 缓存命中场景：预热后内容哈希不变全命中
    let mut cache = 转译缓存::默认容量();
    转译源文本(&source, &manager, &mut cache).expect("预热转译失败");
    group.bench_function("pipeline_cache_hit_2000", |b| {
        b.iter(|| 转译源文本(black_box(&source), &manager, &mut cache).expect("缓存转译失败"))
    });

    // 词法阶段单独计费（占冷转译一半以上：token 流物化 + 逐标识符查表）。
    // 2000 行大文件上 3930 个标识符高度重复，标识符事实缓存就命中在这里。
    group.bench_function("lexer_stage_2000", |b| {
        b.iter(|| {
            词法器::分词转译(
                black_box(source.as_str()),
                manager.取关键词表(),
                manager.取宏表(),
                manager.取派生特征表(),
                manager.取让位词(),
                manager.取方法让位词(),
                manager.取别名表(),
            )
        })
    });

    // 别名阶段单独计费（token 流物化 + 声明收集三趟 + 逐标识符豁免判定）
    // 注意：这一项与 `lexer_stage_2000` 都不走真实管线编排（自己从源文本重新
    // 分词），只能定序不能相加；管线里未被两项覆盖的部分（偏移换算链、
    // 编辑表合并、产物拼接）由下面两份语料的差值兼顾。
    group.bench_function("alias_stage_2000", |b| {
        b.iter(|| {
            别名::带上下文与模块路径替换别名(
                black_box(source.as_str()),
                manager.取别名表(),
                manager.取模块路径表(),
                None,
            )
        })
    });

    // 别名密集语料：后续阶段编辑数从 2 条涨到上千条，守住阶段间偏移换算的
    // 乘积项（历史教训见模块文档：逐条重扫让同规模文件从 3.2 ms 涨到 37 ms）
    let alias_heavy = alias_heavy_source();
    group.bench_function("pipeline_no_cache_2000_alias_heavy", |b| {
        b.iter(|| 转译管线(black_box(&alias_heavy), &manager))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_transpile,
    bench_teaching_checks,
    bench_transpile_large
);
criterion_main!(benches);
