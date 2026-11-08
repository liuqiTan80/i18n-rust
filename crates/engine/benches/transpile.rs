//! 转译管线性能基准
//!
//! 覆盖教学工具的四类热点：
//! - `pipeline_no_cache()`：无缓存全量转译（Unicode/全角/教学检查 + 词法 +
//!   模块路径 + 别名替换），即 CLI / LSP 冷启动单文件转译的真实开销；
//! - `pipeline_cache_hit()`：带增量缓存的转译（编辑器反复输入同一文件的场景，
//!   内容哈希未变时直接复用翻译结果）；
//! - `teaching_checks()`：全角标点扫描与教学检查单独计费（LSP 诊断发布前
//!   每次变更都会执行，单独量化便于发现回归）；
//! - `pipeline_*_2000`：约 2000 行大文件输入的冷转译与缓存命中，守住
//!   转译的线性低延迟（由 `tools/bench-check.sh` 阈值守卫）；
//! - `pipeline_no_cache_2000_alias_heavy()`：同规模但**别名密集**的语料。关键字
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
fn 中文管理器() -> 映射管理器 {
    let 目录 = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
    映射管理器::从目录载入(&目录).expect("加载 zh 语言包失败")
}

/// 教学场景中等规模源码：约 250 行混合结构
/// （函数/类型注解/字符串/宏/导入路径/遍历循环/向量），模拟真实教学文件
fn 教学源码() -> String {
    let mut 源码 = String::new();
    源码.push_str("使用 标准库::集合::映射;\n\n");
    for 序号 in 0..50 {
        源码.push_str(&format!(
            "函数 函数_{序号}(数量: 整数, 名称: 字符串) -> 整数 {{\n\
             \x20   让 结果 = 数量 * 2;\n\
             \x20   // 计算注释：{序号}\n\
             \x20   打印行!(\"处理第 {序号} 个：{{}}\", 名称);\n\
             \x20   返回 结果;\n\
             }}\n\n",
        ));
    }
    源码.push_str(
        "函数 主函数() {\n\
         \x20   让 全部 = 向量::新建();\n\
         \x20   for i in 0..50 {\n\
         \x20       全部.推送(函数_i(i, \"测试\"));\n\
         \x20   }\n\
         \x20   打印行!(\"总计：{{:?}}\", 全部);\n\
         }\n",
    );
    源码
}

/// 大规模源码：约 2000 行（280 个函数 × ~7 行 + 入口），模拟大文件输入。
/// 冷转译成本随行数近线性增长；本基准由 bench-check 阈值守卫，防止在
/// 语言包/管线演进中丢掉转译的低延迟优势。
fn 大源码() -> String {
    let mut 源码 = String::with_capacity(2000 * 40);
    源码.push_str("使用 标准库::集合::映射;\n\n");
    for 序号 in 0..280 {
        源码.push_str(&format!(
            "函数 函数_{序号}(数量: 整数, 名称: 字符串) -> 整数 {{\n\
             \x20   让 结果 = 数量 * 2;\n\
             \x20   // 计算注释：{序号}\n\
             \x20   打印行!(\"处理第 {序号} 个：{{}}\", 名称);\n\
             \x20   返回 结果;\n\
             }}\n\n",
        ));
    }
    源码.push_str("函数 主函数() {\n    让 全部 = 向量::新建();\n}\n");
    源码
}

/// 别名密集语料：约 2000 行，但每函数多次标准库成员调用（`.长度()`
/// `.推入()` `.迭代().计数()` `.插入()` `.元素数()`）
///
/// 与大源码样例的行数量级相同，后续阶段的编辑数却从 2 条涨到上千条：
/// 专测阶段间偏移换算的乘积项（见模块文档）。不写整数字面量、方法名全部
/// 收录在语言包里，以免教学告警写标准错误落在计时区里干扰读数。
fn 别名密集源码() -> String {
    let mut 源码 = String::with_capacity(2000 * 40);
    源码.push_str("使用 标准库::向量::向量;\n使用 标准库::映射::映射;\n\n");
    for 序号 in 0..280 {
        源码.push_str(&format!(
            "函数 函数_{序号}(项: 向量<字符串>, 表: 映射<字符串, 整数>) -> 整数 {{\n\
             \x20   让 个数 = 项.长度();\n\
             \x20   项.推入(\"新项{序号}\");\n\
             \x20   让 合计 = 项.迭代().计数();\n\
             \x20   表.插入(\"键{序号}\", 合计);\n\
             \x20   返回 个数 + 表.元素数();\n\
             }}\n\n",
        ));
    }
    源码.push_str("函数 主函数() {\n    让 全部 = 向量::新建();\n}\n");
    源码
}

/// 转译管线：无缓存全量与缓存命中
fn 基准转译(标准器: &mut Criterion) {
    let 管理器 = 中文管理器();
    let 源码 = 教学源码();
    let mut 组 = 标准器.benchmark_group("transpile");
    组.sample_size(50);

    组.bench_function("pipeline_no_cache", |台架| {
        台架.iter(|| 转译管线(black_box(&源码), &管理器))
    });

    // 缓存命中场景：先用真实转译预热缓存，迭代期间内容哈希不变全命中
    let mut 缓存 = 转译缓存::默认容量();
    转译源文本(&源码, &管理器, &mut 缓存).expect("预热转译失败");
    组.bench_function("pipeline_cache_hit", |台架| {
        台架.iter(|| 转译源文本(black_box(&源码), &管理器, &mut 缓存).expect("缓存转译失败"))
    });

    组.finish();
}

/// 教学检查单独计费（LSP 每次变更都会执行全角扫描、教学检查与保留字撞名扫描）
///
/// `lint_scan()` 是空词表阈值（只看状态机本体）；`lint_scan_words()` 才是管线
/// 真实路径——带整张词表与歧义构造器集合，易混方法名判定在里面；
/// `teaching_all_scan()` 把英文回落与保留字撞名也算进来（两者共用一次遍历）。
///
/// `lint_scan_buckets()` 是转译管线真正走的分桶索引入口（索引由管理器缓存、
/// 带精确命中表）；上面几个 `*_words` 走惰性构建，每轮都要重建索引，只适合
/// 衡定单文件 API 的开销。
fn 基准教学检查(标准器: &mut Criterion) {
    let 管理器 = 中文管理器();
    let 源码 = 教学源码();
    let 关键词表 = 管理器.取关键词表();
    let 英文回落表 = 管理器.取英文回落();
    let 词表 = 管理器.取校验词表();
    let 歧义构造器 = 管理器.取歧义构造器();
    let 分桶 = 管理器.取校验分桶();
    // 管线已物化过字符切片（全角/教学检查共用一份），基准同样把它放在闭包外
    let 字符切片: Vec<char> = 源码.chars().collect();
    let mut 组 = 标准器.benchmark_group("teaching_checks");
    组.sample_size(50);

    for (名称, 代码) in [
        ("fullwidth_scan", black_box(&源码) as &str),
        ("lint_scan", black_box(源码.as_str())),
        ("lint_scan_words", black_box(源码.as_str())),
        ("lint_scan_buckets", black_box(源码.as_str())),
        ("collision_scan", black_box(源码.as_str())),
        ("teaching_all_scan", black_box(源码.as_str())),
    ] {
        组.bench_with_input(
            BenchmarkId::new(名称, 源码.len()),
            &代码,
            |台架, 输入| match 名称 {
                "fullwidth_scan" => 台架.iter(|| 检测全角标点(输入)),
                "collision_scan" => 台架.iter(|| 检查保留名撞词(输入, 关键词表)),
                "lint_scan_words" => 台架.iter(|| 按词表检查教学(输入, 词表, 歧义构造器)),
                "lint_scan_buckets" => {
                    台架.iter(|| 按词形桶检查字符(&字符切片, 分桶, 歧义构造器))
                }
                "teaching_all_scan" => {
                    台架.iter(|| 检查全部教学(输入, 词表, 歧义构造器, 英文回落表, 关键词表))
                }
                _ => 台架.iter(|| 检查教学(输入)),
            },
        );
    }
    组.finish();
}

/// 约 2000 行大文件：冷转译与缓存命中，守住转译的线性低延迟
fn 基准转译大文件(标准器: &mut Criterion) {
    let 管理器 = 中文管理器();
    let 源码 = 大源码();
    let mut 组 = 标准器.benchmark_group("transpile");
    // 大输入每次迭代约数毫秒，降低样本数以免 CI 过长（仍满足阈值判定）
    组.sample_size(30);

    组.bench_function("pipeline_no_cache_2000", |台架| {
        台架.iter(|| 转译管线(black_box(&源码), &管理器))
    });

    // 缓存命中场景：预热后内容哈希不变全命中
    let mut 缓存 = 转译缓存::默认容量();
    转译源文本(&源码, &管理器, &mut 缓存).expect("预热转译失败");
    组.bench_function("pipeline_cache_hit_2000", |台架| {
        台架.iter(|| 转译源文本(black_box(&源码), &管理器, &mut 缓存).expect("缓存转译失败"))
    });

    // 词法阶段单独计费（占冷转译一半以上：词元流物化 + 逐标识符查表）。
    // 2000 行大文件上 3930 个标识符高度重复，标识符事实缓存就命中在这里。
    组.bench_function("lexer_stage_2000", |台架| {
        台架.iter(|| {
            词法器::分词转译(
                black_box(源码.as_str()),
                管理器.取关键词表(),
                管理器.取宏表(),
                管理器.取派生特征表(),
                管理器.取让位词(),
                管理器.取方法让位词(),
                管理器.取别名表(),
            )
        })
    });

    // 别名阶段单独计费（词元流物化 + 声明收集三趟 + 逐标识符豁免判定）
    // 注意：这一项与 `lexer_stage_2000()` 都不走真实管线编排（自己从源文本重新
    // 分词），只能定序不能相加；管线里未被两项覆盖的部分（偏移换算链、
    // 编辑表合并、产物拼接）由下面两份语料的差值兼顾。
    组.bench_function("alias_stage_2000", |台架| {
        台架.iter(|| {
            别名::带上下文与模块路径替换别名(
                black_box(源码.as_str()),
                管理器.取别名表(),
                管理器.取模块路径表(),
                None,
            )
        })
    });

    // 别名密集语料：后续阶段编辑数从 2 条涨到上千条，守住阶段间偏移换算的
    // 乘积项（历史教训见模块文档：逐条重扫让同规模文件从 3.2 ms 涨到 37 ms）
    let 别名密集 = 别名密集源码();
    组.bench_function("pipeline_no_cache_2000_alias_heavy", |台架| {
        台架.iter(|| 转译管线(black_box(&别名密集), &管理器))
    });

    组.finish();
}

criterion_group!(基准组, 基准转译, 基准教学检查, 基准转译大文件);
criterion_main!(基准组);
