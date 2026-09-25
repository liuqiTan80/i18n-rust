//! LSP 热路径性能基准（criterion）
//!
//! 覆盖代理服务器最频繁的两条路径：
//! - `update_document`：编辑器打开/每次按键触发的文档同步（全量转译 +
//!   列映射回放 + 教学检查 + 虚拟文件写盘）。分「打开文件」（首次打开，
//!   模块集合变化 → 全量重写 + 虚拟项目刷新）与「连续编辑」（集合不变 →
//!   仅重写当前条目）两个场景；
//! - `reverse_transpile`：rust-analyzer 响应映射热路径（英文文本 → 母语）。
//!   分「补全片段」（无文档上下文的短片段）与「整文档还原」（格式化响应）。
//!
//! 运行：`cargo bench -p i18n-rust-lsp`
//! CI 快速模式：`cargo bench -p i18n-rust-lsp -- --quick`

use std::hint::black_box;
use std::path::{Path, PathBuf};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use i18n_rust_engine::mapping_manager::MappingManager;
use i18n_rust_lsp::translation_cache::TranslationCache;

/// 构造内置 zh 语言包映射管理器（与 CLI / LSP 默认一致）
fn zh_manager() -> MappingManager {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../engine/lang-packs/zh");
    MappingManager::load_from_dir(&dir).expect("加载 zh 语言包失败")
}

/// 教学场景中等规模源码：约 250 行混合结构
/// （函数/类型注解/字符串/宏/use 路径/for 循环/向量，与 engine 基准同语料）
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

/// 在演示目录中写入真实源文件（sync_sibling_modules / disk_meta 走真实磁盘分支），
/// 返回源文件路径与其 file:// URI
fn setup_project(root: &Path, dir_name: &str, content: &str) -> (PathBuf, String) {
    let src_dir = root.join(dir_name).join("src");
    std::fs::create_dir_all(&src_dir).expect("创建演示项目目录失败");
    let path = src_dir.join("main.zh");
    std::fs::write(&path, content).expect("写入演示源码失败");
    let uri = format!("file://{}", path.display());
    (path, uri)
}

/// update_document：打开文件（全量路径）与连续编辑（增量路径）
fn bench_update_document(c: &mut Criterion) {
    // 基准中抑制引擎教学日志（默认警告级会因教学 lint 刷屏并引入
    // 终端 I/O 抖动；与 engine 基准的静默口径一致）
    i18n_rust_engine::logger::set_log_level(i18n_rust_engine::logger::LogLevel::Error);
    let manager = zh_manager();
    let source = teaching_source();
    let mut group = c.benchmark_group("lsp_update_document");
    group.sample_size(50);

    // ① 打开文件：冷缓存首次打开（模块集合空 → 全量重写 + 虚拟项目刷新）。
    //    每次迭代完整重建缓存与项目目录，量的是"打开一个文件"的端到端成本
    group.bench_function("update_document_open", |b| {
        b.iter_batched(
            || {
                let tmp = tempfile::tempdir().expect("创建临时目录失败");
                let (_path, uri) = setup_project(tmp.path(), "proj", &source);
                let cache = TranslationCache::new(manager.clone(), tmp.path().join("virtual"));
                (tmp, cache, uri)
            },
            |(_tmp, cache, uri)| {
                cache
                    .update_document(black_box(&uri), black_box(&source), 1)
                    .expect("打开文件失败")
            },
            BatchSize::SmallInput,
        )
    });

    // ② 连续编辑：预热打开后反复修改同一文件（内容每次不同，模拟真实打字：
    //    模块集合不变 → 仅重写当前条目，声明名缓存按内容哈希逐次失效）
    let tmp = tempfile::tempdir().expect("创建临时目录失败");
    let (_path, uri) = setup_project(tmp.path(), "proj_typing", &source);
    let cache = TranslationCache::new(manager.clone(), tmp.path().join("virtual"));
    cache
        .update_document(&uri, &source, 1)
        .expect("预热打开失败");
    let mut version = 1i32;
    group.bench_function("update_document_typing", |b| {
        let mut counter = 0usize;
        b.iter(|| {
            counter += 1;
            version += 1;
            let edited = format!("{source}\n// 连续编辑 {counter}");
            cache
                .update_document(black_box(&uri), black_box(&edited), version)
                .expect("编辑失败")
        })
    });

    group.finish();
}

/// reverse_transpile：补全片段（无上下文）与整文档还原（格式化响应）
fn bench_reverse_transpile(c: &mut Criterion) {
    // 同上：静默引擎教学日志
    i18n_rust_engine::logger::set_log_level(i18n_rust_engine::logger::LogLevel::Error);
    let manager = zh_manager();
    let source = teaching_source();
    let tmp = tempfile::tempdir().expect("创建临时目录失败");
    let (_path, uri) = setup_project(tmp.path(), "proj_reverse", &source);
    let cache = TranslationCache::new(manager.clone(), tmp.path().join("virtual"));
    let (entry, _) = cache
        .update_document(&uri, &source, 1)
        .expect("预热打开失败");
    // 转译产物即"经 rustfmt 格式化后的英文文本"的典型素材
    let en_doc = entry.en_content.clone();

    let mut group = c.benchmark_group("lsp_reverse_transpile");
    group.sample_size(50);

    // ① 补全片段：uri=None 的无文档上下文场景（补全/代码操作每个片段都走这里）
    let snippet = "let mut 总数 = String::new();";
    group.bench_function("reverse_transpile_completion", |b| {
        b.iter(|| cache.reverse_transpile(None, black_box(snippet)))
    });

    // ② 整文档还原：textDocument/formatting 响应（传入 uri 以精确处理
    //    代理添加的 crate:: 前缀，避免误删用户手写前缀）
    group.bench_function("reverse_transpile_formatting", |b| {
        b.iter(|| {
            cache.reverse_transpile(Some(black_box(uri.as_str())), black_box(en_doc.as_str()))
        })
    });

    group.finish();
}

criterion_group!(benches, bench_update_document, bench_reverse_transpile);
criterion_main!(benches);
