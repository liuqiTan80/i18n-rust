//! response_map 单元测试（自原单文件 tests 模块原样迁移）。

use super::diag_text::翻译诊断消息;
use super::responses::取标签末段;
use super::*;
use crate::翻译缓存::转译缓存;
use std::collections::HashMap;
use std::sync::Arc;

fn 构造测试缓存() -> (Arc<转译缓存>, tempfile::TempDir) {
    let 管理器 = i18n_rust_engine::映射管理::映射管理器::自扁平映射新建(
        HashMap::from([("函数".into(), "fn".into()), ("让".into(), "let".into())]),
        HashMap::new(),
        HashMap::new(),
    );
    let 临时路径 = tempfile::tempdir().unwrap();
    let 缓存 = 转译缓存::新建缓存(管理器, 临时路径.path().to_path_buf());
    (缓存, 临时路径)
}

#[test]
fn 测试还原资源定位() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());

    let (翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    assert_eq!(
        映射器.还原资源定位(&翻译条目.虚拟资源定位),
        "file:///test/main.zh"
    );
}

#[test]
fn 测试还原范围() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());

    let (翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "让 x = 1;\n让 y = 2;", 1)
        .unwrap();

    // 行映射：虚拟行号还原为中文行号（1:1）
    // 列映射：`让`→`let` 每行偏移 +2，英文列 4-5（y）对应中文列 2-3
    let 跨度 = 映射器.还原范围(
        &翻译条目.虚拟资源定位,
        &json!({ "start": { "line": 1, "character": 4 }, "end": { "line": 1, "character": 5 } }),
    );
    assert_eq!(跨度["start"]["line"], 1);
    assert_eq!(跨度["start"]["character"], 2);
    assert_eq!(跨度["end"]["line"], 1);
    assert_eq!(跨度["end"]["character"], 3);
}

#[test]
fn 测试映射引用响应() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let (翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "让 x = 1;\n函数 主() {}", 1)
        .unwrap();

    let 响应 = json!([
        {
            "uri": 翻译条目.虚拟资源定位,
            "range": {
                "start": { "line": 1, "character": 0 },
                "end": { "line": 1, "character": 2 }
            }
        }
    ]);

    let 映射结果 = 映射器.映射引用响应(&响应);
    assert_eq!(映射结果[0]["uri"].as_str().unwrap(), "file:///test/main.zh");
    assert_eq!(映射结果[0]["range"]["start"]["line"], 1);
    assert_eq!(映射结果[0]["range"]["start"]["character"], 0);

    // 无结果（null）时应返回空数组而不是 null
    assert_eq!(映射器.映射引用响应(&Value::Null), Value::Array(Vec::new()));
}

#[test]
fn 测试映射重命名响应跨文件() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let (翻译条目甲, _) = 缓存
        .更新文档("file:///test/main.zh", "让 x = 1;\n函数 主() {}", 1)
        .unwrap();
    let (翻译条目乙, _) = 缓存
        .更新文档("file:///test/lib.zh", "函数 主() {}", 1)
        .unwrap();

    // rust-analyzer 返回跨文件编辑（changes 形式）
    let 响应 = json!({
        "changes": {
            翻译条目甲.虚拟资源定位.clone(): [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 2 }
                },
                "newText": "fn"
            }],
            翻译条目乙.虚拟资源定位.clone(): [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 2 }
                },
                "newText": "fn"
            }]
        }
    });

    let 映射结果 = 映射器.映射重命名响应(&响应);
    let 变更表 = 映射结果["changes"].as_object().unwrap();

    // 两个文件的 URI 都还原为 .zh 源文件
    assert!(变更表.contains_key("file:///test/main.zh"));
    assert!(变更表.contains_key("file:///test/lib.zh"));

    // newText 反向翻译：fn → 函数
    assert_eq!(
        变更表["file:///test/main.zh"][0]["newText"]
            .as_str()
            .unwrap(),
        "函数"
    );
    assert_eq!(
        变更表["file:///test/lib.zh"][0]["newText"]
            .as_str()
            .unwrap(),
        "函数"
    );

    // documentChanges 形式也应正确处理
    let 响应二 = json!({
        "documentChanges": [{
            "textDocument": { "uri": 翻译条目甲.虚拟资源定位.clone(), "version": null },
            "edits": [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 2 }
                },
                "newText": "fn"
            }]
        }]
    });
    let 映射结果二 = 映射器.映射重命名响应(&响应二);
    assert_eq!(
        映射结果二["documentChanges"][0]["textDocument"]["uri"]
            .as_str()
            .unwrap(),
        "file:///test/main.zh"
    );
    assert_eq!(
        映射结果二["documentChanges"][0]["edits"][0]["newText"]
            .as_str()
            .unwrap(),
        "函数"
    );
}

#[test]
fn 测试映射代码操作响应() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let (翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    let 响应 = json!([
        {
            "title": "导入 std::io",
            "kind": "quickfix",
            "edit": {
                "changes": {
                    翻译条目.虚拟资源定位.clone(): [{
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 2 }
                        },
                        "newText": "use std::io;"
                    }]
                }
            }
        }
    ]);

    let 映射结果 = 映射器.映射代码操作响应(&响应, "file:///test/main.zh");
    let 变更表 = 映射结果[0]["edit"]["changes"].as_object().unwrap();
    assert!(变更表.contains_key("file:///test/main.zh"));

    // 代码操作插入的英文代码经反向翻译：测试映射表中无 use 关键字，
    // 故保持英文原样（若语言包含 使用→use 映射，则会被还原为母语）
    assert_eq!(
        变更表["file:///test/main.zh"][0]["newText"]
            .as_str()
            .unwrap(),
        "use std::io;"
    );

    // null 响应 → 空数组
    assert_eq!(
        映射器.映射代码操作响应(&Value::Null, ""),
        Value::Array(Vec::new())
    );
}

#[test]
fn 测试映射代码操作解析响应() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let (翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    // resolve 响应是单个 CodeAction 对象（非数组），edit 走 documentChanges
    let 响应 = json!({
        "title": "改为 pub(crate)",
        "kind": "refactor.rewrite",
        "data": { "id": 1 },
        "edit": {
            "documentChanges": [{
                "textDocument": { "uri": 翻译条目.虚拟资源定位, "version": 1 },
                "edits": [{
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 0 }
                    },
                    "newText": "pub(crate) "
                }]
            }]
        }
    });

    let 映射结果 = 映射器.映射代码操作解析响应(&响应);
    // 虚拟 URI 必须还原为原始文件 URI（否则 VSCode 无法应用编辑）
    assert_eq!(
        映射结果["edit"]["documentChanges"][0]["textDocument"]["uri"]
            .as_str()
            .unwrap(),
        "file:///test/main.zh"
    );
    // 位置从虚拟坐标映射回母语坐标
    let 跨度 = &映射结果["edit"]["documentChanges"][0]["edits"][0]["range"];
    assert_eq!(跨度["start"]["line"], 0);
    assert_eq!(跨度["start"]["character"], 0);
    // data 与 title 原样保留
    assert_eq!(映射结果["data"]["id"], 1);
    assert_eq!(映射结果["title"].as_str().unwrap(), "改为 pub(crate)");

    // 无 edit 字段时原样返回
    let 纯动作 = json!({"title": "仅命令", "command": {"title": "c", "command": "x"}});
    assert_eq!(映射器.映射代码操作解析响应(&纯动作), 纯动作);
    // null 响应原样返回
    assert_eq!(映射器.映射代码操作解析响应(&Value::Null), Value::Null);
}

#[test]
fn 测试映射文档符号响应() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let (翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "函数 主() {\n    让 x = 1;\n}", 1)
        .unwrap();
    assert_eq!(翻译条目.英文源码, "fn 主() {\n    let x = 1;\n}");

    let 响应 = json!([
        {
            "name": "fn",
            "kind": 12,
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 2, "character": 1 }
            },
            "selectionRange": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 2 }
            },
            "children": [{
                "name": "let",
                "kind": 13,
                "range": {
                    "start": { "line": 1, "character": 0 },
                    "end": { "line": 1, "character": 3 }
                },
                "selectionRange": {
                    "start": { "line": 1, "character": 0 },
                    "end": { "line": 1, "character": 3 }
                }
            }]
        }
    ]);

    let 映射结果 = 映射器.映射文档符号响应(&响应, "file:///test/main.zh");

    // 符号名恢复为中文
    assert_eq!(映射结果[0]["name"].as_str().unwrap(), "函数");
    assert_eq!(映射结果[0]["children"][0]["name"].as_str().unwrap(), "让");

    // 位置映射回母语文件（行 1:1、列按偏移转换）
    assert_eq!(映射结果[0]["range"]["start"]["line"], 0);
    assert_eq!(映射结果[0]["selectionRange"]["start"]["character"], 0);
    assert_eq!(映射结果[0]["children"][0]["range"]["start"]["line"], 1);

    // null 响应 → 空数组
    assert_eq!(
        映射器.映射文档符号响应(&Value::Null, ""),
        Value::Array(Vec::new())
    );
}

#[test]
fn 测试映射诊断所有权详情入数据() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    // 多行文档，保证行映射存在（诊断行号 2/4 可还原）
    let (翻译条目, _) = 缓存
        .更新文档(
            "file:///test/main.zh",
            "让 数据 = 1;\n让 a = 1;\n让 b = 1;\n让 c = 1;\n让 d = 1;",
            1,
        )
        .unwrap();

    // rust-analyzer 风格的 E0382 诊断：主 range 是再次使用处，relatedInformation 标记移动
    let 诊断 = json!({
        "range": {
            "start": { "line": 4, "character": 4 },
            "end": { "line": 4, "character": 8 }
        },
        "severity": 1,
        "code": "E0382",
        "source": "rust-analyzer",
        "message": "use of moved value: `数据`",
        "relatedInformation": [{
            "location": {
                "uri": 翻译条目.虚拟资源定位.clone(),
                "range": {
                    "start": { "line": 2, "character": 8 },
                    "end": { "line": 2, "character": 10 }
                }
            },
            "message": "value moved here"
        }]
    });
    let 参数集 = json!({
        "uri": 翻译条目.虚拟资源定位,
        "version": 1,
        "diagnostics": [诊断]
    });

    let 映射结果 = 映射器.映射诊断(&参数集);
    let 数据 = 映射结果["diagnostics"][0]["data"]
        .as_object()
        .expect("所有权诊断的 data 字段应为 JSON 对象");

    // 变量名与位置（LSP 0-based 行号 +1 → 1-based）
    assert_eq!(数据["变量名"], "数据");
    assert_eq!(数据["移动发生"]["起始行"], 3);
    assert_eq!(数据["再次使用"]["起始行"], 5);
    assert!(数据["借用发生"].is_null());
}

#[test]
fn 测试映射诊断非所有权无详情() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let (翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    // 类型不匹配错误不应附带所有权详情
    let 诊断 = json!({
        "range": {
            "start": { "line": 0, "character": 4 },
            "end": { "line": 0, "character": 8 }
        },
        "severity": 1,
        "message": "mismatched types",
        "relatedInformation": []
    });
    let 参数集 = json!({
        "uri": 翻译条目.虚拟资源定位,
        "version": 1,
        "diagnostics": [诊断]
    });

    let 映射结果 = 映射器.映射诊断(&参数集);
    assert!(映射结果["diagnostics"][0].get("data").is_none());
}

/// 虚拟项目固有的过程宏误报被过滤（#3 症状二）：clap 辅助属性
/// `cannot find attribute` 与 `cannot find derive macro` 不发布；
/// 普通诊断（类型错误、unresolved import）照常保留
#[test]
fn 测试映射诊断过滤过程宏噪声() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let (翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    let 属性噪声 = json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 4 }
        },
        "severity": 1,
        "message": "cannot find attribute `arg` in this scope"
    });
    let 派生噪声 = json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 4 }
        },
        "severity": 1,
        "message": "cannot find derive macro `Parser` in this scope"
    });
    let 未解析项 = json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 4 }
        },
        "severity": 1,
        "message": "unresolved import `clap`"
    });
    let 参数集 = json!({
        "uri": 翻译条目.虚拟资源定位,
        "version": 1,
        "diagnostics": [属性噪声, 派生噪声, 未解析项]
    });

    let 映射结果 = 映射器.映射诊断(&参数集);
    let 诊断列表 = 映射结果["diagnostics"].as_array().unwrap();
    // 两类过程宏误报被过滤，unresolved import 保留（快速修复输入；
    // 消息可能已被汉化，按反引号内的 crate 名断言）
    assert_eq!(诊断列表.len(), 1, "仅保留 unresolved import：{映射结果}");
    assert!(
        诊断列表[0]["message"].as_str().unwrap().contains("clap"),
        "保留的诊断应为 unresolved import：{映射结果}"
    );
}

/// 虚拟项目“引用未打开模块文件”的 E0433 误报被过滤：
/// 同名方言文件存在于条目同目录时过滤（打开后即可解析）；
/// 文件不存在（拼写错误）时诊断保留供用户修正
#[test]
fn 测试映射诊断过滤未打开模块引用() {
    let (缓存, 临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    // 同目录存在被引用但未打开的模块文件
    std::fs::write(临时路径.path().join("日志设置.zh"), "公开 函数 初始化() {}").unwrap();
    let 主uri = format!("file://{}", 临时路径.path().join("main.zh").display());
    let (翻译条目, _) = 缓存.更新文档(&主uri, "让 x = 1;", 1).unwrap();

    let 构造诊断 = |消息: &str| {
        json!({
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 }
            },
            "severity": 1,
            "code": "E0433",
            "message": 消息
        })
    };
    let 未打开项 = 构造诊断("cannot find module or crate `日志设置` in this scope");
    let 拼写错误项 = 构造诊断("cannot find module or crate `日志设值` in this scope");
    let 参数集 = json!({
        "uri": 翻译条目.虚拟资源定位,
        "version": 1,
        "diagnostics": [未打开项, 拼写错误项]
    });

    let 映射结果 = 映射器.映射诊断(&参数集);
    let 诊断列表 = 映射结果["diagnostics"].as_array().unwrap();
    // 未打开模块（文件存在）的误报被过滤，拼写错误保留
    assert_eq!(诊断列表.len(), 1, "仅保留拼写错误诊断：{映射结果}");
    assert!(
        诊断列表[0]["message"]
            .as_str()
            .unwrap()
            .contains("日志设值"),
        "保留的诊断应为拼写错误：{映射结果}"
    );
}

/// 虚拟项目“引用未打开模块文件”误报的新消息格式（rust-analyzer 0.3.3025）：
/// 新 E0432（unresolved import `crate::X`）、新 E0433（cannot find `X` in
/// `crate`）与同伴 hint（no `X` in the root）在同名方言文件存在时过滤；
/// 文件不存在的拼写错误保留
#[test]
fn 测试映射诊断过滤未打开模块引用新格式() {
    let (缓存, 临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    // 同目录存在被引用但未打开的模块文件
    std::fs::write(临时路径.path().join("日志设置.zh"), "公开 函数 初始化() {}").unwrap();
    let 主uri = format!("file://{}", 临时路径.path().join("main.zh").display());
    let (翻译条目, _) = 缓存.更新文档(&主uri, "让 x = 1;", 1).unwrap();

    let 构造诊断 = |码文本: &str, 严重度: u32, 消息: &str| {
        json!({
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 }
            },
            "severity": 严重度,
            "code": 码文本,
            "message": 消息
        })
    };
    let 甲项 = 构造诊断(
        "E0432",
        1,
        "unresolved import `crate::日志设置`\ncould not find `日志设置` in the crate root",
    );
    let 乙项 = 构造诊断("E0433", 1, "cannot find `日志设置` in `crate`");
    let 提示项 = 构造诊断("E0432", 4, "no `日志设置` in the root");
    let 拼错项甲 = 构造诊断(
        "E0432",
        1,
        "unresolved import `crate::日志设值`\ncould not find `日志设值` in the crate root",
    );
    let 拼错项乙 = 构造诊断("E0433", 1, "cannot find `日志设值` in `crate`");
    let 参数集 = json!({
        "uri": 翻译条目.虚拟资源定位,
        "version": 1,
        "diagnostics": [甲项, 乙项, 提示项, 拼错项甲, 拼错项乙]
    });

    let 映射结果 = 映射器.映射诊断(&参数集);
    let 诊断列表 = 映射结果["diagnostics"].as_array().unwrap();
    // 三处“文件存在”的误报被过滤，两处拼写错误保留
    assert_eq!(诊断列表.len(), 2, "仅保留拼写错误诊断：{映射结果}");
    for 诊断 in 诊断列表 {
        assert!(
            诊断["message"].as_str().unwrap().contains("日志设值"),
            "保留的诊断应为拼写错误：{映射结果}"
        );
    }
}

/// 虚拟项目“第三方依赖缺失”的误报被过滤：
/// 候选 crate 名出现在项目最近 Cargo.toml 依赖表中（含 target 节与
/// `-`/`_` 归一化）时过滤；未声明的库（拼写错误/待添加）保留
#[test]
fn 测试映射诊断过滤项目依赖() {
    let (缓存, 临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    // 用户项目置于子目录：与虚拟项目根（临时路径）隔离，避免虚拟项目生成的
    // Cargo.toml 覆盖测试清单（真实场景中两者天然分离）
    let 工程路径 = 临时路径.path().join("proj");
    std::fs::create_dir_all(&工程路径).unwrap();
    std::fs::write(
        工程路径.join("Cargo.toml"),
        r#"[package]
name = "demo"
version = "0.1.0"

[dependencies]
serde = "1"
serde-json = "1"

[target.'cfg(unix)'.dependencies]
getrandom = "0.2"
"#,
    )
    .unwrap();
    let 主uri = format!("file://{}", 工程路径.join("main.zh").display());
    let (翻译条目, _) = 缓存.更新文档(&主uri, "让 x = 1;", 1).unwrap();

    let 构造诊断 = |消息: &str| {
        json!({
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 }
            },
            "severity": 1,
            "message": 消息
        })
    };
    let 参数集 = json!({
        "uri": 翻译条目.虚拟资源定位,
        "version": 1,
        "diagnostics": [
            构造诊断("unresolved import `serde`"),
            构造诊断("unresolved import `serde_json::Value`"),
            构造诊断("cannot find module or crate `getrandom` in this scope"),
            构造诊断("unresolved import `clap`")
        ]
    });

    let 映射结果 = 映射器.映射诊断(&参数集);
    let 诊断列表 = 映射结果["diagnostics"].as_array().unwrap();
    // 依赖表中的名字被过滤，未声明的 clap 保留
    assert_eq!(诊断列表.len(), 1, "仅保留未声明依赖：{映射结果}");
    assert!(
        诊断列表[0]["message"].as_str().unwrap().contains("clap"),
        "保留的诊断应为未声明依赖：{映射结果}"
    );
}

/// 虚拟项目“include_str! 资源缺失”的误报被过滤：
/// 资源文件在原方言文件同目录存在（消息路径按虚拟项目根显示，带
/// `src/` 前缀）时过滤；资源真实缺失时保留
#[test]
fn 测试映射诊断过滤缺失包含资源() {
    let (缓存, 临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    std::fs::write(临时路径.path().join("页面.html"), "<html></html>").unwrap();
    let 主uri = format!("file://{}", 临时路径.path().join("main.zh").display());
    let (翻译条目, _) = 缓存.更新文档(&主uri, "让 x = 1;", 1).unwrap();

    let 构造诊断 = |消息: &str| {
        json!({
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 }
            },
            "severity": 1,
            "message": 消息
        })
    };
    let 参数集 = json!({
        "uri": 翻译条目.虚拟资源定位,
        "version": 1,
        "diagnostics": [
            构造诊断("couldn't read `src/页面.html`: No such file or directory (os error 2)"),
            构造诊断("couldn't read `src/缺失.html`: No such file or directory (os error 2)")
        ]
    });

    let 映射结果 = 映射器.映射诊断(&参数集);
    let 诊断列表 = 映射结果["diagnostics"].as_array().unwrap();
    // 原目录存在的资源误报被过滤，真实缺失的保留
    assert_eq!(诊断列表.len(), 1, "仅保留真实缺失的资源：{映射结果}");
    assert!(
        诊断列表[0]["message"]
            .as_str()
            .unwrap()
            .contains("缺失.html"),
        "保留的诊断应为真实缺失的资源：{映射结果}"
    );
}

/// documentHighlight 响应的 range 必须还原为母语坐标
#[test]
fn 测试映射文档高亮响应() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let (_翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "让 x = 1;\n让 y = x;", 1)
        .unwrap();

    // 英文坐标（"let" 占 3 列；行0 x 在英文列 4，行1 "let y = " 后 x 在英文列 8）
    let 响应 = json!([
        { "range": { "start": { "line": 0, "character": 4 }, "end": { "line": 0, "character": 5 } }, "kind": 2 },
        { "range": { "start": { "line": 1, "character": 8 }, "end": { "line": 1, "character": 9 } }, "kind": 2 }
    ]);
    let 映射结果 = 映射器.映射文档高亮响应(&响应, "file:///test/main.zh");
    // 中文列："让 x" 中 x 在列 2（"让" 占 1 个 UTF-16 单元）；"让 y = x" 中 x 在列 6
    assert_eq!(映射结果[0]["range"]["start"]["character"], 2);
    assert_eq!(映射结果[1]["range"]["start"]["character"], 6);
    assert_eq!(映射结果[0]["kind"], 2);

    // null 响应 → 空数组
    assert_eq!(
        映射器.映射文档高亮响应(&Value::Null, ""),
        Value::Array(Vec::new())
    );
}

/// 语义着色响应的 delta 编码必须还原为母语坐标，
/// 且长度按映射后的列差重算（关键字替换改变列宽）
#[test]
fn 测试映射语义记号响应() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let (_翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "让 x = 1;\n让 y = x;", 1)
        .unwrap();

    // 英文坐标（"let" 占 3 列，变量 x/y 在列 4/8）；
    // delta 编码：[deltaLine, deltaStart, length, tokenType, tokenModifiers]
    let 响应 = json!({
        "resultId": "abc",
        "data": [
            0, 0, 3, 14, 0,   // let   行0 列0
            0, 4, 1, 6, 0,    // x     行0 列4
            1, 4, 1, 6, 0,    // y     行1 列4（跨行，列重置为绝对）
            0, 4, 1, 6, 0     // x     行1 列8
        ]
    });
    let 映射结果 = 映射器.映射语义记号响应(&响应, "file:///test/main.zh");
    // resultId 透传；重新 delta 编码后的中文坐标：
    // 让(0,0,len1)、x(0,2)、y(1,2)、x(1,6)
    assert_eq!(映射结果["resultId"], "abc");
    assert_eq!(
        映射结果["data"],
        json!([0, 0, 1, 14, 0, 0, 2, 1, 6, 0, 1, 2, 1, 6, 0, 0, 4, 1, 6, 0])
    );
}

/// hover 命中解释表：完整路径降级匹配短路径键（std::option::Option<T>::unwrap）
#[test]
fn 测试映射悬停响应完整路径命中() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let 文档 =
        "```rust\nstd::option::Option<T>::unwrap\n```\n\nPanics if the value is a [`None`]...";
    let 响应 = json!({"contents": {"kind": "markdown", "value": 文档}});
    let 映射结果 = 映射器.映射悬停响应(&响应, "file:///test/main.zh");
    let 值文本 = 映射结果["contents"]["value"].as_str().unwrap();
    assert!(值文本.starts_with("**大白话："), "应插入加粗提示: {值文本}");
    assert!(
        值文本.contains("直接取出"),
        "解释应为 unwrap 的大白话: {值文本}"
    );
    assert!(值文本.ends_with(文档), "原文应保留在提示之后: {值文本}");
}

/// hover 命中解释表：impl 标题 + 签名行 → 短路径键（Option::unwrap）
#[test]
fn 测试映射悬停响应impl标题命中() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let 文档 = "**`impl<T> Option<T>`**\n\n```rust\npub fn unwrap(self) -> T\n```\n\nPanics if the value is a None...";
    let 响应 = json!({"contents": {"kind": "markdown", "value": 文档}});
    let 映射结果 = 映射器.映射悬停响应(&响应, "file:///test/main.zh");
    let 值文本 = 映射结果["contents"]["value"].as_str().unwrap();
    assert!(值文本.starts_with("**大白话："), "应插入加粗提示: {值文本}");
    assert!(值文本.ends_with(文档));
}

/// hover 未命中解释表：原样透传，不改变任何内容
#[test]
fn 测试映射悬停响应未命中原样() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let 文档 = "```rust\npub fn 自定义函数(x: i32) -> i32\n```\n\n自定义函数说明";
    let 响应 = json!({"contents": {"kind": "markdown", "value": 文档}});
    let 映射结果 = 映射器.映射悬停响应(&响应, "file:///test/main.zh");
    assert_eq!(映射结果["contents"]["value"].as_str().unwrap(), 文档);
}

/// signatureHelp：label 与参数 label 词法级中文化
#[test]
fn 测试映射签名帮助响应翻译() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let 响应 = json!({
        "signatures": [{
            "label": "fn push(&mut self, value: T)",
            "parameters": [
                {"label": [0, 2]},
                {"label": "value: T"}
            ]
        }],
        "activeSignature": 0,
        "activeParameter": 1
    });
    let 映射结果 = 映射器.映射签名帮助响应(&响应);
    let 标签 = 映射结果["signatures"][0]["label"].as_str().unwrap();
    assert!(标签.starts_with("函数 push"), "fn 应译为函数：{标签}");
    assert!(标签.contains("&mut self"), "self 无映射应保留：{标签}");
    // 参数 [start,end] 索引按原 label 切片（[0,2] = "fn"）翻译后转为字符串
    let 参数零 = 映射结果["signatures"][0]["parameters"][0]["label"]
        .as_str()
        .unwrap();
    assert_eq!(参数零, "函数", "索引形式参数应翻译：{参数零}");
    // 字符串形式参数同样翻译
    let 参数一 = 映射结果["signatures"][0]["parameters"][1]["label"]
        .as_str()
        .unwrap();
    assert!(参数一.contains("value: T"), "泛型参数保留：{参数一}");
    assert_eq!(映射结果["activeParameter"], 1);
}

/// signatureHelp 无 signatures（null/空）：原样返回不报错
#[test]
fn 测试映射签名帮助响应空() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let 响应 = json!(null);
    assert_eq!(映射器.映射签名帮助响应(&响应), Value::Null);
    let 响应 = json!({"signatures": []});
    assert_eq!(映射器.映射签名帮助响应(&响应)["signatures"], json!([]));
}

/// hover MarkedString：先查解释表加大白话前缀，再对代码做词法级中文化
#[test]
fn 测试映射悬停响应标记串翻译() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let 响应 = json!({
        "contents": {"language": "rust", "value": "pub fn push(&mut self, value: T)"}
    });
    let 映射结果 = 映射器.映射悬停响应(&响应, "file:///test/main.zh");
    let 值文本 = 映射结果["contents"]["value"].as_str().unwrap();
    assert!(
        值文本.contains("函数 push"),
        "代码签名应中文化（fn→函数）：{值文本}"
    );
}

/// hover MarkedString 数组形式：简单键命中（clone），其余元素原样
#[test]
fn 测试映射悬停响应标记串数组命中() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let 响应 = json!({
        "contents": [
            {"language": "rust", "value": "pub fn clone(&self) -> Self"},
            {"kind": "markdown", "value": "Returns a copy of the value."}
        ]
    });
    let 映射结果 = 映射器.映射悬停响应(&响应, "file:///test/main.zh");
    assert!(
        映射结果["contents"][0]["value"]
            .as_str()
            .unwrap()
            .starts_with("**大白话：")
    );
    assert_eq!(
        映射结果["contents"][1]["value"].as_str().unwrap(),
        "Returns a copy of the value."
    );
}

/// hover 内容为 null 等异常形态时安全透传
#[test]
fn 测试映射悬停响应null内容() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let 映射结果 = 映射器.映射悬停响应(&json!({"contents": null}), "");
    assert!(映射结果["contents"].is_null());
}

/// 补全响应的 textEdit.newText 反向翻译、additionalTextEdits 位置还原
#[test]
fn 测试映射补全文本编辑反向翻译() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let (翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    let 响应 = json!({
        "items": [{
            "label": "let",
            "textEdit": {
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 3 } },
                "newText": "let"
            },
            "additionalTextEdits": [{
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
                "newText": "fn 辅助() {}"
            }]
        }]
    });
    let 映射结果 = 映射器.映射补全响应(&响应, "file:///test/main.zh");
    let 项 = &映射结果["items"][0];
    // label 与 newText 均还原为母语关键字
    assert_eq!(项["label"].as_str().unwrap(), "让");
    assert_eq!(项["textEdit"]["newText"].as_str().unwrap(), "让");
    // snippet 占位符保持原样
    let 片段项 = json!({
        "items": [{ "label": "x", "textEdit": {
            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
            "newText": "fn ${1:name}() {}" } }]
    });
    let 映射片段 = 映射器.映射补全响应(&片段项, "file:///test/main.zh");
    assert_eq!(
        映射片段["items"][0]["textEdit"]["newText"]
            .as_str()
            .unwrap(),
        "fn ${1:name}() {}"
    );
    // additionalTextEdits 中的 fn 被反向翻译
    assert_eq!(
        项["additionalTextEdits"][0]["newText"].as_str().unwrap(),
        "函数 辅助() {}"
    );
    let _ = 翻译条目;
}

/// labelDetails 反向翻译：VS Code 提示框右侧优先显示此字段，
/// fn() 等英文签名必须还原为母语（如 函数()）
#[test]
fn 测试映射补全标签详情翻译() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    // 文档中声明 my_func，使其通过语言过滤的用户词汇白名单
    let (翻译条目, _) = 缓存
        .更新文档("file:///test/main.zh", "函数 my_func() {}", 1)
        .unwrap();
    let 响应 = json!({
        "items": [{
            "label": "my_func",
            "detail": "fn my_func()",
            "labelDetails": {
                "description": "fn()",
                "detail": "crate::辅助"
            }
        }]
    });
    let 映射结果 = 映射器.映射补全响应(&响应, "file:///test/main.zh");
    let 项 = &映射结果["items"][0];
    assert_eq!(项["detail"].as_str().unwrap(), "函数 my_func()");
    assert_eq!(
        项["labelDetails"]["description"].as_str().unwrap(),
        "函数()"
    );
    // crate/模块路径中无映射命中时保持原样
    assert_eq!(
        项["labelDetails"]["detail"].as_str().unwrap(),
        "crate::辅助"
    );
    let _ = 翻译条目;
}

/// 语言过滤：非英文方言下补全列表不得串语言。
/// 保留：翻译命中项、母语字符项、用户自定义项（含英文命名）；
/// 过滤：未翻译的外部英文项。
#[test]
fn 测试映射补全语言过滤() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    // 用户源码同时含母语定义（自定义函数）与英文命名（helper）
    let (翻译条目, _) = 缓存
        .更新文档(
            "file:///test/main.zh",
            "函数 自定义函数() {} 函数 helper() {}",
            1,
        )
        .unwrap();

    let 响应 = json!({
        "items": [
            { "label": "let" },
            { "label": "自定义函数" },
            { "label": "helper(…)" },
            { "label": "serde_json" },
            { "label": "BTreeMap" }
        ]
    });
    let 映射结果 = 映射器.映射补全响应(&响应, "file:///test/main.zh");
    let 标签列表: Vec<&str> = 映射结果["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    // 翻译命中（let → 让）、母语标识符、用户定义的英文名保留
    assert_eq!(标签列表, vec!["让", "自定义函数", "helper(…)"]);
    // 未翻译的外部英文项（serde_json/BTreeMap）被过滤
    assert!(!标签列表.contains(&"serde_json"));
    assert!(!标签列表.contains(&"BTreeMap"));
    let _ = 翻译条目;
}

/// 英文语言包（恒等映射）不启用语言过滤，所有项保留
#[test]
fn 测试映射补全恒等包不过滤() {
    let 管理器 = i18n_rust_engine::映射管理::映射管理器::自扁平映射新建(
        HashMap::from([("fn".into(), "fn".into()), ("let".into(), "let".into())]),
        HashMap::new(),
        HashMap::new(),
    );
    let 临时路径 = tempfile::tempdir().unwrap();
    let 缓存 = 转译缓存::新建缓存(管理器, 临时路径.path().to_path_buf());
    let 映射器 = 响应映射器::新建映射器(缓存);

    let 响应 = json!({
        "items": [
            { "label": "let" },
            { "label": "BTreeMap" },
            { "label": "serde_json" }
        ]
    });
    let 映射结果 = 映射器.映射补全响应(&响应, "file:///test/main.zh");
    assert_eq!(映射结果["items"].as_array().unwrap().len(), 3);
}

/// label 末段标识符提取：后缀、路径前缀、模块补全形式
#[test]
fn 测试取标签末段() {
    assert_eq!(取标签末段("foo(…)"), Some("foo"));
    assert_eq!(取标签末段("Foo {…}"), Some("Foo"));
    assert_eq!(取标签末段("m::Spam::Bar(…)"), Some("Bar"));
    assert_eq!(取标签末段("m::"), Some("m"));
    assert_eq!(取标签末段("println!"), Some("println"));
    assert_eq!(取标签末段("(…)"), None);
}

/// 注入添加依赖动作：空候选列表原样返回，非空追加 quickfix 动作
#[test]
fn 测试注入添加依赖动作() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());

    // 空 crates：原响应原样返回
    let 原响应 = json!([{"title": "既有动作", "kind": "quickfix"}]);
    assert_eq!(映射器.注入添加依赖动作(&原响应, &[]), 原响应);

    // 非空 crates：追加动作，command 指向扩展注册的 cargoAdd
    let 注入后 = 映射器.注入添加依赖动作(&原响应, &["serde_json".to_string()]);
    let 动作列表 = 注入后.as_array().unwrap();
    assert_eq!(动作列表.len(), 2);
    let 新增项 = &动作列表[1];
    assert_eq!(新增项["kind"], "quickfix");
    assert_eq!(新增项["command"]["command"], "i18n-rust.cargoAdd");
    assert_eq!(新增项["command"]["arguments"][0], "serde_json");

    // 非数组响应（如 null）也能注入
    let 注入后 = 映射器.注入添加依赖动作(&Value::Null, &["tokio".to_string()]);
    assert_eq!(注入后.as_array().unwrap().len(), 1);
}

/// 教学诊断注入：全角标点替换动作 + 教学 lint 忽略动作（方言坐标直用）
#[test]
fn 测试注入教学动作() {
    let (缓存, _临时路径) = 构造测试缓存();
    let 映射器 = 响应映射器::新建映射器(缓存.clone());
    let 资源定位 = "file:///test/main.zh";
    缓存
        .更新文档(资源定位, "函数 主函数() {\n    让 x = 1;\n}", 1)
        .unwrap();

    let 原响应 = json!([{"title": "既有动作", "kind": "quickfix"}]);
    // 无教学诊断：原样返回
    assert_eq!(映射器.注入教学动作(&原响应, &[], 资源定位), 原响应);

    let 全角诊断 = json!({
        "range": {
            "start": { "line": 0, "character": 8 },
            "end": { "line": 0, "character": 9 }
        },
        "code": "fullwidth",
        "source": "i18n-rust",
        "message": "第 1 行第 9 列：检测到全角标点「，」，应改为半角「,」",
        "data": { "character": "，", "replacement": "," }
    });
    let lint诊断 = json!({
        "range": {
            "start": { "line": 1, "character": 4 },
            "end": { "line": 1, "character": 5 }
        },
        "code": "lint-untyped-let",
        "source": "i18n-rust",
        "message": "第 2 行第 5 列：`让` 未标注类型"
    });
    let 注入后 = 映射器.注入教学动作(&原响应, &[全角诊断, lint诊断], 资源定位);
    let 动作列表 = 注入后.as_array().unwrap();
    assert_eq!(动作列表.len(), 3, "既有 1 + 教学 2");

    // 全角标点：同坐标替换为半角
    let 修复项 = &动作列表[1];
    assert_eq!(修复项["kind"], "quickfix");
    assert_eq!(修复项["edit"]["changes"][资源定位][0]["newText"], ",");
    assert_eq!(
        修复项["edit"]["changes"][资源定位][0]["range"]["start"]["line"],
        0
    );

    // 教学 lint：行尾插入忽略标记（第 2 行 0 起行号 1，行尾插入注释）
    let 忽略项 = &动作列表[2];
    assert_eq!(忽略项["kind"], "quickfix");
    let 编辑 = &忽略项["edit"]["changes"][资源定位][0];
    assert_eq!(编辑["range"]["start"]["line"], 1);
    assert_eq!(
        编辑["range"]["start"]["character"],
        编辑["range"]["end"]["character"]
    );
    assert!(
        编辑["newText"]
            .as_str()
            .unwrap()
            .contains(i18n_rust_engine::教学检查::教学忽略标记)
    );

    // 无可修复字符的全角标点（顿号等）：不产生动作
    let 仅提示诊断 = json!({
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } },
        "code": "fullwidth",
        "source": "i18n-rust",
        "message": "仅提示",
        "data": { "character": "、", "replacement": null }
    });
    let 注入后 = 映射器.注入教学动作(&Value::Null, &[仅提示诊断], 资源定位);
    assert_eq!(注入后.as_array().unwrap().len(), 0);
}

/// 未解析导入诊断追加依赖提示（内置 zh 回退含 lsp_hint_add_dependency 键）
#[test]
fn 测试翻译诊断未解析导入提示() {
    let 译文 = 翻译诊断消息(None, "unresolved import `serde_json`", None);
    // 反引号内容保留（提取依赖原名），且追加了 rzc add 提示
    assert!(译文.contains("`serde_json`"));
    assert!(译文.contains("rzc add serde_json"));
}

/// 诊断翻译不替换反引号内的标识符（避免误伤变量名中的子串）
#[test]
fn 测试翻译诊断跳过反引号内容() {
    let 译文 = 翻译诊断消息(
        None,
        "cannot find value `expected_value` in this scope",
        None,
    );
    // 反引号内的标识符保持原样
    assert!(译文.contains("`expected_value`"));
    // 反引号外的短语已被翻译（不再含英文原短语）
    assert!(!译文.contains("cannot find value"));
}

/// E0599（no method named ... found for ...）不得把 "found" 误译为「实际为」
#[test]
fn 测试翻译诊断无方法名不误导() {
    let 译文 = 翻译诊断消息(
        None,
        "no method named `拉平` found for struct `Vec<i32>` in the current scope",
        None,
    );
    assert!(!译文.contains("实际为"), "{译文}");
    let 译文二 = 翻译诊断消息(None, "method not found in `Vec<i32>`", None);
    assert!(!译文二.contains("实际为"), "{译文二}");
}

/// E0308（expected ..., found ...）保留「实际为」翻译（语境键 ", found " 生效）
#[test]
fn 测试翻译诊断类型不匹配保留实际为() {
    let 译文 = 翻译诊断消息(
        None,
        "mismatched types: expected `char`, found `&str`",
        None,
    );
    assert!(译文.contains("实际为"), "{译文}");
    // 反引号内类型名保留
    assert!(译文.contains("`char`") && 译文.contains("`&str`"), "{译文}");
}
