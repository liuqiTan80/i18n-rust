//! LSP 热路径性能基准（基于基准框架）
//!
//! 覆盖代理服务器最频繁的两条路径：
//! - `更新文档`：编辑器打开/每次按键触发的文档同步（全量转译 +
//!   列映射回放 + 教学检查 + 虚拟文件写盘）。分「打开文件」（首次打开，
//!   模块集合变化 → 全量重写 + 虚拟项目刷新）与「连续编辑」（集合不变 →
//!   仅重写当前条目）两个场景；另有一条**别名密集**的打字变体：
//!   默认语料里方法名多为未收录词（原样
//!   保留），后续阶段只产生几条编辑，盖不住「后续阶段编辑 × 词法编辑」这类
//!   乘积项，故换一份密集调用已收录方法名的语料单独盯住它；
//! - `反向转译`：rust-analyzer 响应映射热路径（英文文本 → 母语）。
//!   分「补全片段」（无文档上下文的短片段）与「整文档还原」（格式化响应）。
//!
//! 运行：`cargo bench -p i18n-rust-lsp`
//! CI 快速模式：`cargo bench -p i18n-rust-lsp -- --quick`

use std::hint::black_box;
use std::path::{Path, PathBuf};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use i18n_rust_engine::映射管理器::映射管理器;
use i18n_rust_lsp::翻译缓存::转译缓存;

/// 构造内置 zh 语言包映射管理器（与 CLI / LSP 默认一致）
fn 中文映射管理器() -> 映射管理器 {
    let 目录 = Path::new(env!("CARGO_MANIFEST_DIR")).join("../engine/lang-packs/zh");
    映射管理器::从目录载入(&目录).expect("加载 zh 语言包失败")
}

/// 教学场景中等规模源码：约 250 行混合结构
/// （函数/类型注解/字符串/宏/使用路径/循环/向量，与引擎基准同语料）
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

/// 别名密集源码：约 2000 行，每函数多次已收录的标准库成员调用
///
/// 与 `教学源码` 的行数量级相同，但后续阶段产生的编辑从几条涨到上千条：
/// `教学源码` 里的 `.推送()` 未收录在语言包（原样保留），所以它根本
/// 不产生别名编辑，阶段间偏移换算的乘积项在打字路径上测不到（历史教训：
/// 逐条重扫时同规模语料的冷转译从 3.2 ms 涨到 37 ms，旧语料完全看不出）。
/// 不写整数字面量、方法名全收录，以免教学告警刷屏引入 I/O 抖动。
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
    源码
}

/// 在演示目录中写入真实源文件（`同步兄弟模块` / `磁盘元数据` 走真实磁盘分支），
/// 返回源文件路径与其文件地址
fn 搭建演示项目(根目录: &Path, 目录名: &str, 内容: &str) -> (PathBuf, String) {
    let 源码目录 = 根目录.join(目录名).join("src");
    std::fs::create_dir_all(&源码目录).expect("创建演示项目目录失败");
    let 路径 = 源码目录.join("main.zh");
    std::fs::write(&路径, 内容).expect("写入演示源码失败");
    let 地址 = format!("file://{}", 路径.display());
    (路径, 地址)
}

/// `更新文档`：打开文件（全量路径）与连续编辑（增量路径）
fn 基准_更新文档(基准: &mut Criterion) {
    // 基准中抑制引擎教学日志（默认警告级会因教学检查刷屏并引入
    // 终端 I/O 抖动；与引擎基准的静默口径一致）
    i18n_rust_engine::日志::设置等级(i18n_rust_engine::日志::日志等级::错误级);
    let 管理器 = 中文映射管理器();
    let 源码 = 教学源码();
    let mut 分组 = 基准.benchmark_group("lsp_update_document");
    分组.sample_size(50);

    // ① 打开文件：冷缓存首次打开（模块集合空 → 全量重写 + 虚拟项目刷新）。
    //    每次迭代完整重建缓存与项目目录，量的是"打开一个文件"的端到端成本
    分组.bench_function("update_document_open", |轮| {
        轮.iter_batched(
            || {
                let 临时目录 = tempfile::tempdir().expect("创建临时目录失败");
                let (_路径, 地址) = 搭建演示项目(临时目录.path(), "proj", &源码);
                let 缓存 = 转译缓存::新建(管理器.clone(), 临时目录.path().join("virtual"));
                (临时目录, 缓存, 地址)
            },
            |(_临时目录, 缓存, 地址)| {
                缓存
                    .更新文档(black_box(&地址), black_box(&源码), 1)
                    .expect("打开文件失败")
            },
            BatchSize::SmallInput,
        )
    });

    // ② 连续编辑：预热打开后反复修改同一文件（内容每次不同，模拟真实打字：
    //    模块集合不变 → 仅重写当前条目，声明名缓存按内容哈希逐次失效）
    let 临时目录 = tempfile::tempdir().expect("创建临时目录失败");
    let (_路径, 地址) = 搭建演示项目(临时目录.path(), "proj_typing", &源码);
    let 缓存 = 转译缓存::新建(管理器.clone(), 临时目录.path().join("virtual"));
    缓存.更新文档(&地址, &源码, 1).expect("预热打开失败");
    let mut 版本 = 1i32;
    分组.bench_function("update_document_typing", |轮| {
        let mut 计数 = 0usize;
        轮.iter(|| {
            计数 += 1;
            版本 += 1;
            let 编辑后 = format!("{源码}\n// 连续编辑 {计数}");
            缓存
                .更新文档(black_box(&地址), black_box(&编辑后), 版本)
                .expect("编辑失败")
        })
    });

    分组.finish();
}

/// `更新文档` 打字路径的别名密集变体
///
/// 与 `基准_更新文档` 的口径一致（预热打开 + 每次按键改内容），只换
/// 语料：让后续阶段的编辑量从几条涨到上千条，覆盖阶段间偏移换算的乘积项。
/// 这条基准盯的是「大文件 + 密集方法调用」下每次按键的延迟，`教学源码`
/// 测不到它。
fn 基准_更新文档别名密集(基准: &mut Criterion) {
    i18n_rust_engine::日志::设置等级(i18n_rust_engine::日志::日志等级::错误级);
    let 管理器 = 中文映射管理器();
    let 源码 = 别名密集源码();
    let mut 分组 = 基准.benchmark_group("lsp_update_document");
    // 大输入每次迭代数毫秒级，降样本数以免 CI 过长（与引擎侧同口径）
    分组.sample_size(30);

    let 临时目录 = tempfile::tempdir().expect("创建临时目录失败");
    let (_路径, 地址) = 搭建演示项目(临时目录.path(), "proj_typing_alias", &源码);
    let 缓存 = 转译缓存::新建(管理器.clone(), 临时目录.path().join("virtual"));
    缓存.更新文档(&地址, &源码, 1).expect("预热打开失败");
    let mut 版本 = 1i32;
    分组.bench_function("update_document_typing_alias_heavy", |轮| {
        let mut 计数 = 0usize;
        轮.iter(|| {
            计数 += 1;
            版本 += 1;
            let 编辑后 = format!("{源码}\n// 连续编辑 {计数}");
            缓存
                .更新文档(black_box(&地址), black_box(&编辑后), 版本)
                .expect("编辑失败")
        })
    });

    分组.finish();
}

/// `反向转译`：补全片段（无上下文）与整文档还原（格式化响应）
fn 基准_反向转译(基准: &mut Criterion) {
    // 同上：静默引擎教学日志
    i18n_rust_engine::日志::设置等级(i18n_rust_engine::日志::日志等级::错误级);
    let 管理器 = 中文映射管理器();
    let 源码 = 教学源码();
    let 临时目录 = tempfile::tempdir().expect("创建临时目录失败");
    let (_路径, 地址) = 搭建演示项目(临时目录.path(), "proj_reverse", &源码);
    let 缓存 = 转译缓存::新建(管理器.clone(), 临时目录.path().join("virtual"));
    let (条目, _) = 缓存.更新文档(&地址, &源码, 1).expect("预热打开失败");
    // 转译产物即"经 `rustfmt` 格式化后的英文文本"的典型素材
    let 英文文档 = 条目.英文内容.clone();

    let mut 分组 = 基准.benchmark_group("lsp_reverse_transpile");
    分组.sample_size(50);

    // ① 补全片段：地址为空的无文档上下文场景（补全/代码操作每个片段都走这里）
    let 片段 = "let mut 总数 = String::new();";
    分组.bench_function("reverse_transpile_completion", |轮| {
        轮.iter(|| 缓存.反向转译(None, black_box(片段)))
    });

    // ② 整文档还原：`textDocument/formatting` 响应（传入地址以精确处理
    //    代理添加的 `crate::` 前缀，避免误删用户手写前缀）
    分组.bench_function("reverse_transpile_formatting", |轮| {
        轮.iter(|| 缓存.反向转译(Some(black_box(地址.as_str())), black_box(英文文档.as_str())))
    });

    分组.finish();
}

criterion_group!(基准集, 基准_更新文档, 基准_更新文档别名密集, 基准_反向转译);
criterion_main!(基准集);
