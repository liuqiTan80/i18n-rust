//! 响应映射单元测试（自原单文件测试模块原样迁移）。

use super::响应::标签标识后缀;
use super::诊断文本::翻译诊断消息;
use super::*;
use crate::翻译缓存::转译缓存;
use std::collections::HashMap;
use std::sync::Arc;

fn 构造测试缓存() -> (Arc<转译缓存>, tempfile::TempDir) {
    let 管理器 = i18n_rust_engine::映射管理器::映射管理器::从扁表构造(
        HashMap::from([("函数".into(), "fn".into()), ("让".into(), "let".into())]),
        HashMap::new(),
        HashMap::new(),
    );
    let 临时 = tempfile::tempdir().unwrap();
    let 缓存值 = 转译缓存::新建(管理器, 临时.path().to_path_buf());
    (缓存值, 临时)
}

#[test]
fn 还原资源标识测试() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());

    let (条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    assert_eq!(
        映射器.还原资源标识(&条目值.虚拟资源标识),
        "file:///test/main.zh"
    );
}

#[test]
fn 还原范围测试() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());

    let (条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "让 x = 1;\n让 y = 2;", 1)
        .unwrap();

    // 行映射：虚拟行号还原为中文行号（1:1）
    // 列映射：让 → 英文关键字 每行偏移 +2，英文列 4-5（y）对应中文列 2-3
    let 范围值 = 映射器.还原范围(
        &条目值.虚拟资源标识,
        &serde_json::json!({ "start": { "line": 1, "character": 4 }, "end": { "line": 1, "character": 5 } }),
    );
    assert_eq!(范围值["start"]["line"], 1);
    assert_eq!(范围值["start"]["character"], 2);
    assert_eq!(范围值["end"]["line"], 1);
    assert_eq!(范围值["end"]["character"], 3);
}

#[test]
fn 映射引用响应测试() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let (条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "让 x = 1;\n函数 主() {}", 1)
        .unwrap();

    let 响应值 = serde_json::json!([
        {
            "uri": 条目值.虚拟资源标识,
            "range": {
                "start": { "line": 1, "character": 0 },
                "end": { "line": 1, "character": 2 }
            }
        }
    ]);

    let 映射后 = 映射器.映射引用响应(&响应值);
    assert_eq!(映射后[0]["uri"].as_str().unwrap(), "file:///test/main.zh");
    assert_eq!(映射后[0]["range"]["start"]["line"], 1);
    assert_eq!(映射后[0]["range"]["start"]["character"], 0);

    // 无结果（空值）时应返回空数组而不是 空值
    assert_eq!(映射器.映射引用响应(&Value::Null), Value::Array(Vec::new()));
}

#[test]
fn 映射重命名响应跨文件() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let (条目甲, _) = 缓存值
        .更新文档("file:///test/main.zh", "让 x = 1;\n函数 主() {}", 1)
        .unwrap();
    let (条目乙, _) = 缓存值
        .更新文档("file:///test/lib.zh", "函数 主() {}", 1)
        .unwrap();

    // rust-analyzer 返回跨文件编辑（变更表形式）
    let 响应值 = serde_json::json!({
        "changes": {
            条目甲.虚拟资源标识.clone(): [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 2 }
                },
                "newText": "fn"
            }],
            条目乙.虚拟资源标识.clone(): [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 2 }
                },
                "newText": "fn"
            }]
        }
    });

    let 映射后 = 映射器.映射重命名响应(&响应值);
    let 变更表 = 映射后["changes"].as_object().unwrap();

    // 两个文件的资源标识都还原为 .zh 源文件
    assert!(变更表.contains_key("file:///test/main.zh"));
    assert!(变更表.contains_key("file:///test/lib.zh"));

    // 新文本反向翻译：英文关键字 → 母语
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

    // 文档变更形式也应正确处理
    let 响应值二 = serde_json::json!({
        "documentChanges": [{
            "textDocument": { "uri": 条目甲.虚拟资源标识.clone(), "version": Value::Null },
            "edits": [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 2 }
                },
                "newText": "fn"
            }]
        }]
    });
    let 映射后二 = 映射器.映射重命名响应(&响应值二);
    assert_eq!(
        映射后二["documentChanges"][0]["textDocument"]["uri"]
            .as_str()
            .unwrap(),
        "file:///test/main.zh"
    );
    assert_eq!(
        映射后二["documentChanges"][0]["edits"][0]["newText"]
            .as_str()
            .unwrap(),
        "函数"
    );
}

#[test]
fn 映射代码操作响应测试() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let (条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    let 响应值 = serde_json::json!([
        {
            "title": "导入 std::io",
            "kind": "quickfix",
            "edit": {
                "changes": {
                    条目值.虚拟资源标识.clone(): [{
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

    let 映射后 = 映射器.映射代码操作响应(&响应值, "file:///test/main.zh");
    let 变更表 = 映射后[0]["edit"]["changes"].as_object().unwrap();
    assert!(变更表.contains_key("file:///test/main.zh"));

    // 代码操作插入的英文代码经反向翻译：测试映射表中无“使用”关键字，
    // 故保持英文原样（若语言包含母语“使用”与英文的映射，则会被还原为母语）
    assert_eq!(
        变更表["file:///test/main.zh"][0]["newText"]
            .as_str()
            .unwrap(),
        "use std::io;"
    );

    // 空响应 → 空数组
    assert_eq!(
        映射器.映射代码操作响应(&Value::Null, ""),
        Value::Array(Vec::new())
    );
}

#[test]
fn 映射代码操作解析响应测试() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let (条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    // 解析响应是单个代码操作对象（非数组），编辑走文档变更
    let 响应值 = serde_json::json!({
        "title": "改为 pub(crate)",
        "kind": "refactor.rewrite",
        "data": { "id": 1 },
        "edit": {
            "documentChanges": [{
                "textDocument": { "uri": 条目值.虚拟资源标识, "version": 1 },
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

    let 映射后 = 映射器.映射代码操作解析响应(&响应值);
    // 虚拟资源标识必须还原为原始文件资源标识（否则 VS Code 无法应用编辑）
    assert_eq!(
        映射后["edit"]["documentChanges"][0]["textDocument"]["uri"]
            .as_str()
            .unwrap(),
        "file:///test/main.zh"
    );
    // 位置从虚拟坐标映射回母语坐标
    let 范围值 = &映射后["edit"]["documentChanges"][0]["edits"][0]["range"];
    assert_eq!(范围值["start"]["line"], 0);
    assert_eq!(范围值["start"]["character"], 0);
    // 数据与标题字段原样保留
    assert_eq!(映射后["data"]["id"], 1);
    assert_eq!(映射后["title"].as_str().unwrap(), "改为 pub(crate)");

    // 无编辑字段时原样返回
    let 普通值 = serde_json::json!({"title": "仅命令", "command": {"title": "c", "command": "x"}});
    assert_eq!(映射器.映射代码操作解析响应(&普通值), 普通值);
    // 空响应原样返回
    assert_eq!(映射器.映射代码操作解析响应(&Value::Null), Value::Null);
}

#[test]
fn 映射文档符号响应测试() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let (条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "函数 主() {\n    让 x = 1;\n}", 1)
        .unwrap();
    assert_eq!(条目值.英文内容, "fn 主() {\n    let x = 1;\n}");

    let 响应值 = serde_json::json!([
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

    let 映射后 = 映射器.映射文档符号响应(&响应值, "file:///test/main.zh");

    // 符号名恢复为中文
    assert_eq!(映射后[0]["name"].as_str().unwrap(), "函数");
    assert_eq!(映射后[0]["children"][0]["name"].as_str().unwrap(), "让");

    // 位置映射回母语文件（行 1:1、列按偏移转换）
    assert_eq!(映射后[0]["range"]["start"]["line"], 0);
    assert_eq!(映射后[0]["selectionRange"]["start"]["character"], 0);
    assert_eq!(映射后[0]["children"][0]["range"]["start"]["line"], 1);

    // 空响应 → 空数组
    assert_eq!(
        映射器.映射文档符号响应(&Value::Null, ""),
        Value::Array(Vec::new())
    );
}

#[test]
fn 映射诊断所有权详情入数据() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    // 多行文档，保证行映射存在（诊断行号 2/4 可还原）
    let (条目值, _) = 缓存值
        .更新文档(
            "file:///test/main.zh",
            "让 数据 = 1;\n让 a = 1;\n让 b = 1;\n让 c = 1;\n让 d = 1;",
            1,
        )
        .unwrap();

    // rust-analyzer 风格的 E0382 诊断：主范围是再次使用处，关联信息标记移动
    let 诊断值 = serde_json::json!({
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
                "uri": 条目值.虚拟资源标识.clone(),
                "range": {
                    "start": { "line": 2, "character": 8 },
                    "end": { "line": 2, "character": 10 }
                }
            },
            "message": "value moved here"
        }]
    });
    let 参数值 = serde_json::json!({
        "uri": 条目值.虚拟资源标识,
        "version": 1,
        "diagnostics": [诊断值]
    });

    let 映射后 = 映射器.映射诊断(&参数值);
    let 数据值 = 映射后["diagnostics"][0]["data"]
        .as_object()
        .expect("所有权诊断的 data 字段应为 JSON 对象");

    // 变量名与位置（LSP 0 起行号 +1 → 1 起）
    assert_eq!(数据值["变量名"], "数据");
    assert_eq!(数据值["移动发生"]["起始行"], 3);
    assert_eq!(数据值["再次使用"]["起始行"], 5);
    assert!(数据值["借用发生"] == Value::Null);
}

#[test]
fn 映射诊断借用冲突两侧不互换() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let (条目值, _) = 缓存值
        .更新文档(
            "file:///test/main.zh",
            "让 a = 1;\n让 b = 1;\n让 c = 1;\n让 d = 1;\n让 e = 1;\n让 f = 1;\n让 g = 1;\n让 h = 1;",
            1,
        )
        .unwrap();

    // rust-analyzer 风格的 E0502：主范围是被拒的可变借用（1 起第 7 行），
    // 关联信息里是不可变借用（第 5 行，与 later-used 同行，与实机一致）
    let 关联信息 = |行号: u32, 消息: &str| {
        serde_json::json!({
            "location": {
                "uri": 条目值.虚拟资源标识.clone(),
                "range": {
                    "start": { "line": 行号, "character": 8 },
                    "end": { "line": 行号, "character": 12 }
                }
            },
            "message": 消息
        })
    };
    let 诊断值 = serde_json::json!({
        "range": {
            "start": { "line": 6, "character": 8 },
            "end": { "line": 6, "character": 18 }
        },
        "severity": 1,
        "code": "E0502",
        "source": "rust-analyzer",
        "message": "cannot borrow `分数` as mutable because it is also borrowed as immutable",
        "relatedInformation": [
            关联信息(4, "immutable borrow occurs here"),
            关联信息(4, "immutable borrow later used here"),
        ]
    });
    let 参数值 = serde_json::json!({
        "uri": 条目值.虚拟资源标识,
        "version": 1,
        "diagnostics": [诊断值]
    });

    let 映射后 = 映射器.映射诊断(&参数值);
    let 数据值 = 映射后["diagnostics"][0]["data"]
        .as_object()
        .expect("所有权诊断的 data 字段应为 JSON 对象");
    assert_eq!(数据值["变量名"], "分数");
    // 可变借用取主范围（第 7 行），不可变借用取关联信息（第 5 行），
    // 两侧不得互换（否则叙事又把行号读反）
    assert_eq!(数据值["借用冲突"]["可变借用"]["起始行"], 7);
    assert_eq!(数据值["借用冲突"]["不可变借用"]["起始行"], 5);
}

#[test]
fn 映射诊断非所有权无详情() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let (条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    // 类型不匹配错误不应附带所有权详情
    let 诊断值 = serde_json::json!({
        "range": {
            "start": { "line": 0, "character": 4 },
            "end": { "line": 0, "character": 8 }
        },
        "severity": 1,
        "message": "mismatched types",
        "relatedInformation": []
    });
    let 参数值 = serde_json::json!({
        "uri": 条目值.虚拟资源标识,
        "version": 1,
        "diagnostics": [诊断值]
    });

    let 映射后 = 映射器.映射诊断(&参数值);
    assert!(映射后["diagnostics"][0].get("data").is_none());
}

/// 虚拟项目固有的过程宏误报被过滤（#3 症状二）：命令行解析库辅助属性
/// `cannot find attribute` 与 `cannot find derive macro` 不发布；
/// 普通诊断（类型错误、未解析导入）照常保留
#[test]
fn 映射诊断过滤过程宏噪音() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let (条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    let 属性噪音 = serde_json::json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 4 }
        },
        "severity": 1,
        "message": "cannot find attribute `arg` in this scope"
    });
    let 派生噪音 = serde_json::json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 4 }
        },
        "severity": 1,
        "message": "cannot find derive macro `Parser` in this scope"
    });
    let 未解析 = serde_json::json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 4 }
        },
        "severity": 1,
        "message": "unresolved import `clap`"
    });
    let 参数值 = serde_json::json!({
        "uri": 条目值.虚拟资源标识,
        "version": 1,
        "diagnostics": [属性噪音, 派生噪音, 未解析]
    });

    let 映射后 = 映射器.映射诊断(&参数值);
    let 诊断们 = 映射后["diagnostics"].as_array().unwrap();
    // 两类过程宏误报被过滤，未解析导入保留（快速修复输入；
    // 消息可能已被汉化，按反引号内的包名断言）
    assert_eq!(诊断们.len(), 1, "仅保留 unresolved import：{映射后}");
    assert!(
        诊断们[0]["message"].as_str().unwrap().contains("clap"),
        "保留的诊断应为 unresolved import：{映射后}"
    );
}

/// 虚拟项目“引用未打开模块文件”的 E0433 误报被过滤：
/// 同名方言文件存在于条目同目录时过滤（打开后即可解析）；
/// 文件不存在（拼写错误）时诊断保留供用户修正
#[test]
fn 映射诊断过滤未打开模块引用() {
    let (缓存值, 临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    // 同目录存在被引用但未打开的模块文件
    std::fs::write(临时.path().join("日志设置.zh"), "公开 函数 初始化() {}").unwrap();
    let 主资源标识 = format!("file://{}", 临时.path().join("main.zh").display());
    let (条目值, _) = 缓存值.更新文档(&主资源标识, "让 x = 1;", 1).unwrap();

    let 构造诊断 = |消息: &str| {
        serde_json::json!({
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 }
            },
            "severity": 1,
            "code": "E0433",
            "message": 消息
        })
    };
    let 未打开 = 构造诊断("cannot find module or crate `日志设置` in this scope");
    let 拼写错误 = 构造诊断("cannot find module or crate `日志设值` in this scope");
    let 参数值 = serde_json::json!({
        "uri": 条目值.虚拟资源标识,
        "version": 1,
        "diagnostics": [未打开, 拼写错误]
    });

    let 映射后 = 映射器.映射诊断(&参数值);
    let 诊断们 = 映射后["diagnostics"].as_array().unwrap();
    // 未打开模块（文件存在）的误报被过滤，拼写错误保留
    assert_eq!(诊断们.len(), 1, "仅保留拼写错误诊断：{映射后}");
    assert!(
        诊断们[0]["message"].as_str().unwrap().contains("日志设值"),
        "保留的诊断应为拼写错误：{映射后}"
    );
}

/// 虚拟项目“引用未打开模块文件”误报的新消息格式（rust-analyzer 0.3.3025）：
/// 新 E0432（未解析导入包路径 X）、新 E0433（在包内找不到 X
/// ）与同伴提示（根作用域无 X）在同名方言文件存在时过滤；
/// 文件不存在的拼写错误保留
#[test]
fn 映射诊断过滤未打开模块引用新格式() {
    let (缓存值, 临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    // 同目录存在被引用但未打开的模块文件
    std::fs::write(临时.path().join("日志设置.zh"), "公开 函数 初始化() {}").unwrap();
    let 主资源标识 = format!("file://{}", 临时.path().join("main.zh").display());
    let (条目值, _) = 缓存值.更新文档(&主资源标识, "让 x = 1;", 1).unwrap();

    let 构造诊断 = |代码: &str, 严重级: u32, 消息: &str| {
        serde_json::json!({
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 }
            },
            "severity": 严重级,
            "code": 代码,
            "message": 消息
        })
    };
    let 旧四三二 = 构造诊断(
        "E0432",
        1,
        "unresolved import `crate::日志设置`\ncould not find `日志设置` in the crate root",
    );
    let 旧四三三 = 构造诊断("E0433", 1, "cannot find `日志设置` in `crate`");
    let 提示值 = 构造诊断("E0432", 4, "no `日志设置` in the root");
    let 四三二拼错 = 构造诊断(
        "E0432",
        1,
        "unresolved import `crate::日志设值`\ncould not find `日志设值` in the crate root",
    );
    let 四三三拼错 = 构造诊断("E0433", 1, "cannot find `日志设值` in `crate`");
    let 参数值 = serde_json::json!({
        "uri": 条目值.虚拟资源标识,
        "version": 1,
        "diagnostics": [旧四三二, 旧四三三, 提示值, 四三二拼错, 四三三拼错]
    });

    let 映射后 = 映射器.映射诊断(&参数值);
    let 诊断们 = 映射后["diagnostics"].as_array().unwrap();
    // 三处“文件存在”的误报被过滤，两处拼写错误保留
    assert_eq!(诊断们.len(), 2, "仅保留拼写错误诊断：{映射后}");
    for 诊断值 in 诊断们 {
        assert!(
            诊断值["message"].as_str().unwrap().contains("日志设值"),
            "保留的诊断应为拼写错误：{映射后}"
        );
    }
}

/// 虚拟项目“第三方依赖缺失”的误报被过滤：
/// 候选包名出现在项目最近 Cargo.toml 依赖表中（含目标平台节与
/// `-`/`_` 归一化）时过滤；未声明的库（拼写错误/待添加）保留
#[test]
fn 映射诊断过滤项目依赖() {
    let (缓存值, 临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    // 用户项目置于子目录：与虚拟项目根（临时）隔离，避免虚拟项目生成的
    // Cargo.toml 覆盖测试清单（真实场景中两者天然分离）
    let 项目目录 = 临时.path().join("proj");
    std::fs::create_dir_all(&项目目录).unwrap();
    std::fs::write(
        项目目录.join("Cargo.toml"),
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
    let 主资源标识 = format!("file://{}", 项目目录.join("main.zh").display());
    let (条目值, _) = 缓存值.更新文档(&主资源标识, "让 x = 1;", 1).unwrap();

    let 构造诊断 = |消息: &str| {
        serde_json::json!({
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 }
            },
            "severity": 1,
            "message": 消息
        })
    };
    let 参数值 = serde_json::json!({
        "uri": 条目值.虚拟资源标识,
        "version": 1,
        "diagnostics": [
            构造诊断("unresolved import `serde`"),
            构造诊断("unresolved import `serde_json::Value`"),
            构造诊断("cannot find module or crate `getrandom` in this scope"),
            构造诊断("unresolved import `clap`")
        ]
    });

    let 映射后 = 映射器.映射诊断(&参数值);
    let 诊断们 = 映射后["diagnostics"].as_array().unwrap();
    // 依赖表中的名字被过滤，未声明的命令行解析库保留
    assert_eq!(诊断们.len(), 1, "仅保留未声明依赖：{映射后}");
    assert!(
        诊断们[0]["message"].as_str().unwrap().contains("clap"),
        "保留的诊断应为未声明依赖：{映射后}"
    );
}

/// 虚拟项目“内嵌字符串资源缺失”的误报被过滤：
/// 资源文件在原方言文件同目录存在（消息路径按虚拟项目根显示，带
/// 源目录前缀）时过滤；资源真实缺失时保留
#[test]
fn 映射诊断过滤缺失内嵌资源() {
    let (缓存值, 临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    std::fs::write(临时.path().join("页面.html"), "<html></html>").unwrap();
    let 主资源标识 = format!("file://{}", 临时.path().join("main.zh").display());
    let (条目值, _) = 缓存值.更新文档(&主资源标识, "让 x = 1;", 1).unwrap();

    let 构造诊断 = |消息: &str| {
        serde_json::json!({
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 }
            },
            "severity": 1,
            "message": 消息
        })
    };
    let 参数值 = serde_json::json!({
        "uri": 条目值.虚拟资源标识,
        "version": 1,
        "diagnostics": [
            构造诊断("couldn't read `src/页面.html`: No such file or directory (os error 2)"),
            构造诊断("couldn't read `src/缺失.html`: No such file or directory (os error 2)")
        ]
    });

    let 映射后 = 映射器.映射诊断(&参数值);
    let 诊断们 = 映射后["diagnostics"].as_array().unwrap();
    // 原目录存在的资源误报被过滤，真实缺失的保留
    assert_eq!(诊断们.len(), 1, "仅保留真实缺失的资源：{映射后}");
    assert!(
        诊断们[0]["message"].as_str().unwrap().contains("缺失.html"),
        "保留的诊断应为真实缺失的资源：{映射后}"
    );
}

/// 文档高亮响应的范围必须还原为母语坐标
#[test]
fn 映射文档高亮响应测试() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let (_条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "让 x = 1;\n让 y = x;", 1)
        .unwrap();

    // 英文坐标（英文关键字占 3 列；第 0 行的 x 在英文列 4，第 1 行赋值后的 x 在英文列 8）
    let 响应值 = serde_json::json!([
        { "range": { "start": { "line": 0, "character": 4 }, "end": { "line": 0, "character": 5 } }, "kind": 2 },
        { "range": { "start": { "line": 1, "character": 8 }, "end": { "line": 1, "character": 9 } }, "kind": 2 }
    ]);
    let 映射后 = 映射器.映射文档高亮响应(&响应值, "file:///test/main.zh");
    // 中文列："让 x" 中 x 在列 2（"让" 占 1 个 UTF-16 单元）；"让 y = x" 中 x 在列 6
    assert_eq!(映射后[0]["range"]["start"]["character"], 2);
    assert_eq!(映射后[1]["range"]["start"]["character"], 6);
    assert_eq!(映射后[0]["kind"], 2);

    // 空响应 → 空数组
    assert_eq!(
        映射器.映射文档高亮响应(&Value::Null, ""),
        Value::Array(Vec::new())
    );
}

/// 语义着色响应的增量编码必须还原为母语坐标，
/// 且长度按映射后的列差重算（关键字替换改变列宽）
#[test]
fn 映射语义记号响应测试() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let (_条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "让 x = 1;\n让 y = x;", 1)
        .unwrap();

    // 英文坐标（英文关键字占 3 列，变量 x/y 在列 4/8）；
    // 增量编码：[增量行, 增量起列, 长度, 记号类型, 记号修饰]
    let 响应值 = serde_json::json!({
        "resultId": "abc",
        "data": [
            0, 0, 3, 14, 0,   // 关键字 第0行 第0列
            0, 4, 1, 6, 0,    // x     行0 列4
            1, 4, 1, 6, 0,    // y     行1 列4（跨行，列重置为绝对）
            0, 4, 1, 6, 0     // x     行1 列8
        ]
    });
    let 映射后 = 映射器.映射语义记号响应(&响应值, "file:///test/main.zh");
    // 结果标识透传；重新增量编码后的中文坐标：
    // 让(0,0,长度1)、x(0,2)、y(1,2)、x(1,6)
    assert_eq!(映射后["resultId"], "abc");
    assert_eq!(
        映射后["data"],
        serde_json::json!([0, 0, 1, 14, 0, 0, 2, 1, 6, 0, 1, 2, 1, 6, 0, 0, 4, 1, 6, 0])
    );
}

/// 悬停命中解释表：完整路径降级匹配短路径键（完整标准库路径）
#[test]
fn 映射悬停响应完整路径命中() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 文档串 =
        "```rust\nstd::option::Option<T>::unwrap\n```\n\nPanics if the value is a [`None`]...";
    let 响应值 = serde_json::json!({"contents": {"kind": "markdown", "value": 文档串}});
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(文本串.starts_with("**大白话："), "应插入加粗提示: {文本串}");
    assert!(
        文本串.contains("直接取出"),
        "解释应为 unwrap 的大白话: {文本串}"
    );
    assert!(文本串.ends_with(文档串), "原文应保留在提示之后: {文本串}");
}

/// 悬停命中解释表：实现标题 + 签名行 → 短路径键（选项的解包）
/// 代码块中文化后正文原样保留（正文是英文文档，只翻代码片段）
#[test]
fn 映射悬停响应实现标题命中() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 文档串 = "**`impl<T> Option<T>`**\n\n```rust\npub fn unwrap(self) -> T\n```\n\nPanics if the value is a None...";
    let 响应值 = serde_json::json!({"contents": {"kind": "markdown", "value": 文档串}});
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(文本串.starts_with("**大白话："), "应插入加粗提示: {文本串}");
    // 代码块内的英文关键字译成中文，正文英文说明不按词表乱换
    assert!(文本串.contains("pub 函数 unwrap(self) -> T"), "{文本串}");
    assert!(
        文本串.ends_with("Panics if the value is a None..."),
        "{文本串}"
    );
}

/// 悬停未命中解释表：不加提示，但代码块仍要中文化（正文原样）
#[test]
fn 映射悬停响应未命中仍中文化代码() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 文档串 = "```rust\npub fn 自定义函数(x: i32) -> i32\n```\n\n自定义函数说明";
    let 响应值 = serde_json::json!({"contents": {"kind": "markdown", "value": 文档串}});
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(
        文本串.starts_with("```rust\npub 函数 自定义函数(x: i32) -> i32\n```"),
        "代码块应中文化: {文本串}"
    );
    assert!(文本串.ends_with("自定义函数说明"), "正文应保留: {文本串}");
}

/// 关键字悬停：rust-analyzer 的英文关键字文档整段换成中文说明
#[test]
fn 映射悬停响应关键字中文化() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 文档串 = "```rust\nfn\n```\n\nFunction keyword";
    let 响应值 = serde_json::json!({"contents": {"kind": "markdown", "value": 文档串}});
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(
        文本串.starts_with("**函数**"),
        "标题应为中文关键字: {文本串}"
    );
    assert!(
        文本串.contains("声明函数的关键字"),
        "应带中文说明: {文本串}"
    );
    assert!(!文本串.contains("Function"), "不应残留英文说明: {文本串}");
}

/// 函数签名悬停：中英混排的 `fn 名字()` 变成 `函数 名字()`，
/// 英文正文里的 match/for 等说明文字不被词表误换
#[test]
fn 映射悬停响应签名中文化保留正文() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 文档串 = "```rust\nfn 缺失键逐级回退()\n```\n\nReturns the match, for example.";
    let 响应值 = serde_json::json!({"contents": {"kind": "markdown", "value": 文档串}});
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(
        文本串.contains("```rust\n函数 缺失键逐级回退()\n```"),
        "签名应完整中文化: {文本串}"
    );
    assert!(
        文本串.contains("Returns the match, for example."),
        "英文正文不该按词表替换: {文本串}"
    );
}

/// 自定义符号悬停：丢弃虚拟项目内部路径块，不挂入口函数说明
///
/// rust-analyzer 对用户符号会先返回内部路径块（`i18n_virtual::main`），
/// 再返回真实签名块。路径块对学习者无意义，且拿它查解释表会把主函数
/// 的「程序入口函数」误挂到任意自定义函数上。
#[test]
fn 映射悬停响应用户符号跳过虚拟路径() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 文档串 = "```rust\ni18n_virtual::main\n```\n\n```rust\nfn 缺失键逐级回退(表: 向量<整数>)\n```\n\nReturns the match.";
    let 响应值 = serde_json::json!({"contents": {"kind": "markdown", "value": 文档串}});
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(
        !文本串.contains("i18n_virtual"),
        "不应泄露虚拟路径: {文本串}"
    );
    assert!(
        !文本串.contains("程序入口函数"),
        "不应误挂 main 说明: {文本串}"
    );
    assert!(
        文本串.contains("函数 缺失键逐级回退(表: 向量<整数>)"),
        "签名应完整中文化: {文本串}"
    );
}

/// 标准库方法悬停：从实现块取类型名，命中「类型::方法」解释键
#[test]
fn 映射悬停响应实现块方法提示() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 文档串 = "```rust\nalloc::vec::Vec\n```\n\n```rust\nimpl<T, A> Vec<T, A>\npub fn push(&mut self, value: T)\n```\n\nPushes an element.";
    let 响应值 = serde_json::json!({"contents": {"kind": "markdown", "value": 文档串}});
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(
        文本串.starts_with("**大白话：在末尾追加一个元素**"),
        "应命中 Vec::push 大白话: {文本串}"
    );
    assert!(文本串.contains("函数 push"), "签名应中文化: {文本串}");
}

/// 标准库方法悬停：路径块前缀丢弃，内部注释清理，正文保留
#[test]
fn 映射悬停响应丢弃路径与内部注释() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 文档串 = "```rust\nalloc::vec::Vec\n```\n\n```rust\nimpl<T, A> Vec<T, A>\npub fn push(&mut self, value: T)\nwhere\n    // Bounds from impl:\n    A: Allocator,\n```\n\nPushes an element.";
    let 响应值 = serde_json::json!({"contents": {"kind": "markdown", "value": 文档串}});
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(
        !文本串.contains("alloc::vec::Vec"),
        "路径前缀应丢弃: {文本串}"
    );
    assert!(
        !文本串.contains("Bounds from impl"),
        "内部注释应清理: {文本串}"
    );
    assert!(文本串.contains("A: Allocator,"), "约束行应保留: {文本串}");
}

/// 内建构造器悬停：解释表缺失时补上中文大白话（向量新建）
#[test]
fn 映射悬停响应构造器提示() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 文档串 = "```rust\nalloc::vec::Vec\n```\n\n```rust\nimpl<T> Vec<T, Global>\npub const fn new() -> Self\n```\n\nConstructs a new, empty `Vec<T>`.";
    let 响应值 = serde_json::json!({"contents": {"kind": "markdown", "value": 文档串}});
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(
        文本串.starts_with("**大白话：创建一个空的动态数组"),
        "构造器应带中文大白话: {文本串}"
    );
}

/// 宏悬停：`macro_rules! name // matched arm #N` 的注释被清理
#[test]
fn 映射悬停响应宏分支标注清理() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 文档串 = "```rust\nalloc::macros\n```\n\n```rust\nmacro_rules! println // matched arm #1\n```\n\nPrints to the standard output.";
    let 响应值 = serde_json::json!({"contents": {"kind": "markdown", "value": 文档串}});
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(
        文本串.starts_with("**大白话：把文本打印到标准输出并换行"),
        "宏应带中文大白话: {文本串}"
    );
    assert!(!文本串.contains("matched arm"), "内部注释应清理: {文本串}");
    assert!(文本串.contains("macro_rules"), "签名应保留: {文本串}");
    assert!(
        !文本串.contains("alloc::macros"),
        "路径前缀应丢弃: {文本串}"
    );
}

/// 关键字悬停（加粗标题形态）：整段换成中文说明，英文正文不残留
#[test]
fn 映射悬停响应关键字标题中文化() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 文档串 = "**`fn`**\n\nFunction keyword";
    let 响应值 = serde_json::json!({"contents": {"kind": "markdown", "value": 文档串}});
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(
        文本串.starts_with("**函数**"),
        "标题应为中文关键字: {文本串}"
    );
    assert!(
        文本串.contains("声明函数的关键字"),
        "应带中文说明: {文本串}"
    );
    assert!(!文本串.contains("Function"), "不应残留英文说明: {文本串}");
}

/// 关键字悬停（标记文本数组形态）：整体替换，不残留英文元素
#[test]
fn 映射悬停响应关键字数组折叠() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 响应值 = serde_json::json!({
        "contents": [
            {"language": "rust", "value": "fn"},
            "Function keyword"
        ]
    });
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    assert!(
        映射后["contents"].is_object(),
        "应折叠为单个中文说明: {}",
        映射后["contents"]
    );
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(文本串.starts_with("**函数**"), "{文本串}");
    assert!(!文本串.contains("Function"), "{文本串}");
}

/// 签名帮助：标签与参数标签词法级中文化
#[test]
fn 映射签名帮助响应翻译() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 响应值 = serde_json::json!({
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
    let 映射后 = 映射器.映射签名帮助响应(&响应值);
    let 标签 = 映射后["signatures"][0]["label"].as_str().unwrap();
    assert!(标签.starts_with("函数 push"), "fn 应译为函数：{标签}");
    assert!(标签.contains("&mut self"), "self 无映射应保留：{标签}");
    // 参数 [起,止] 索引按原标签切片（索引区间对应英文关键字）翻译后转为字符串
    let 参数零 = 映射后["signatures"][0]["parameters"][0]["label"]
        .as_str()
        .unwrap();
    assert_eq!(参数零, "函数", "索引形式参数应翻译：{参数零}");
    // 字符串形式参数同样翻译
    let 参数一 = 映射后["signatures"][0]["parameters"][1]["label"]
        .as_str()
        .unwrap();
    assert!(参数一.contains("value: T"), "泛型参数保留：{参数一}");
    assert_eq!(映射后["activeParameter"], 1);
}

/// 签名帮助无签名数组（空值/空数组）：原样返回不报错
#[test]
fn 映射签名帮助响应空() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 响应值 = Value::Null;
    assert_eq!(映射器.映射签名帮助响应(&响应值), Value::Null);
    let 响应值 = serde_json::json!({"signatures": []});
    assert_eq!(
        映射器.映射签名帮助响应(&响应值)["signatures"],
        serde_json::json!([])
    );
}

/// 悬停标记文本：先查解释表加大白话前缀，再对代码做词法级中文化
#[test]
fn 映射悬停响应标记文本翻译() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 响应值 = serde_json::json!({
        "contents": {"language": "rust", "value": "pub fn push(&mut self, value: T)"}
    });
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    let 文本串 = 映射后["contents"]["value"].as_str().unwrap();
    assert!(
        文本串.contains("函数 push"),
        "代码签名应中文化（fn→函数）：{文本串}"
    );
}

/// 悬停标记文本数组形式：简单键命中（克隆），其余元素原样
#[test]
fn 映射悬停响应标记文本数组命中() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 响应值 = serde_json::json!({
        "contents": [
            {"language": "rust", "value": "pub fn clone(&self) -> Self"},
            {"kind": "markdown", "value": "Returns a copy of the value."}
        ]
    });
    let 映射后 = 映射器.映射悬停响应(&响应值, "file:///test/main.zh");
    assert!(
        映射后["contents"][0]["value"]
            .as_str()
            .unwrap()
            .starts_with("**大白话：")
    );
    assert_eq!(
        映射后["contents"][1]["value"].as_str().unwrap(),
        "Returns a copy of the value."
    );
}

/// 悬停内容为空值等异常形态时安全透传
#[test]
fn 映射悬停响应空内容() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 映射后 = 映射器.映射悬停响应(&serde_json::json!({"contents": Value::Null}), "");
    assert!(映射后["contents"] == Value::Null);
}

/// 补全响应的文本编辑新文本反向翻译、附加文本编辑位置还原
#[test]
fn 映射补全文本编辑反向翻译() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let (条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    let 响应值 = serde_json::json!({
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
    let 映射后 = 映射器.映射补全响应(&响应值, "file:///test/main.zh");
    let 项 = &映射后["items"][0];
    // 标签与新文本均还原为母语关键字
    assert_eq!(项["label"].as_str().unwrap(), "让");
    assert_eq!(项["textEdit"]["newText"].as_str().unwrap(), "让");
    // 代码片段占位符保持原样
    let 片段项 = serde_json::json!({
        "items": [{ "label": "x", "textEdit": {
            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
            "newText": "fn ${1:name}() {}" } }]
    });
    let 片段映射后 = 映射器.映射补全响应(&片段项, "file:///test/main.zh");
    assert_eq!(
        片段映射后["items"][0]["textEdit"]["newText"]
            .as_str()
            .unwrap(),
        "fn ${1:name}() {}"
    );
    // 附加文本编辑中的英文关键字被反向翻译
    assert_eq!(
        项["additionalTextEdits"][0]["newText"].as_str().unwrap(),
        "函数 辅助() {}"
    );
    let _ = 条目值;
}

/// 标签详情反向翻译：VS Code 提示框右侧优先显示此字段，
/// 英文签名必须还原为母语（如 函数()）
#[test]
fn 映射补全标签详情翻译() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    // 文档中声明用户函数，使其通过语言过滤的用户词汇白名单
    let (条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "函数 my_func() {}", 1)
        .unwrap();
    let 响应值 = serde_json::json!({
        "items": [{
            "label": "my_func",
            "detail": "fn my_func()",
            "labelDetails": {
                "description": "fn()",
                "detail": "crate::辅助"
            }
        }]
    });
    let 映射后 = 映射器.映射补全响应(&响应值, "file:///test/main.zh");
    let 项 = &映射后["items"][0];
    assert_eq!(项["detail"].as_str().unwrap(), "函数 my_func()");
    assert_eq!(
        项["labelDetails"]["description"].as_str().unwrap(),
        "函数()"
    );
    // 包/模块路径中无映射命中时保持原样
    assert_eq!(
        项["labelDetails"]["detail"].as_str().unwrap(),
        "crate::辅助"
    );
    let _ = 条目值;
}

/// 语言过滤：非英文方言下补全列表不得串语言。
/// 保留：翻译命中项、母语字符项、用户自定义项（含英文命名）；
/// 过滤：未翻译的外部英文项。
#[test]
fn 映射补全语言过滤() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    // 用户源码同时含母语定义（自定义函数）与英文命名（辅助函数）
    let (条目值, _) = 缓存值
        .更新文档(
            "file:///test/main.zh",
            "函数 自定义函数() {} 函数 helper() {}",
            1,
        )
        .unwrap();

    let 响应值 = serde_json::json!({
        "items": [
            { "label": "let" },
            { "label": "自定义函数" },
            { "label": "helper(…)" },
            { "label": "serde_json" },
            { "label": "BTreeMap" }
        ]
    });
    let 映射后 = 映射器.映射补全响应(&响应值, "file:///test/main.zh");
    let 标签集: Vec<&str> = 映射后["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|项| 项["label"].as_str().unwrap())
        .collect();
    // 翻译命中（英文关键字 → 母语）、母语标识符、用户定义的英文名保留
    assert_eq!(标签集, vec!["让", "自定义函数", "helper(…)"]);
    // 未翻译的外部英文项（serde_json/BTreeMap）被过滤
    assert!(!标签集.contains(&"serde_json"));
    assert!(!标签集.contains(&"BTreeMap"));
    let _ = 条目值;
}

/// 英文语言包（恒等映射）不启用语言过滤，所有项保留
#[test]
fn 映射补全恒等包不过滤() {
    let 管理器 = i18n_rust_engine::映射管理器::映射管理器::从扁表构造(
        HashMap::from([("fn".into(), "fn".into()), ("let".into(), "let".into())]),
        HashMap::new(),
        HashMap::new(),
    );
    let 临时 = tempfile::tempdir().unwrap();
    let 缓存值 = 转译缓存::新建(管理器, 临时.path().to_path_buf());
    let 映射器 = 响应映射器::新建(缓存值);

    let 响应值 = serde_json::json!({
        "items": [
            { "label": "let" },
            { "label": "BTreeMap" },
            { "label": "serde_json" }
        ]
    });
    let 映射后 = 映射器.映射补全响应(&响应值, "file:///test/main.zh");
    assert_eq!(映射后["items"].as_array().unwrap().len(), 3);
}

/// 标签末段标识符提取：后缀、路径前缀、模块补全形式
#[test]
fn 标签标识后缀提取() {
    assert_eq!(标签标识后缀("foo(…)"), Some("foo"));
    assert_eq!(标签标识后缀("Foo {…}"), Some("Foo"));
    assert_eq!(标签标识后缀("m::Spam::Bar(…)"), Some("Bar"));
    assert_eq!(标签标识后缀("m::"), Some("m"));
    assert_eq!(标签标识后缀("println!"), Some("println"));
    assert_eq!(标签标识后缀("(…)"), None);
}

/// 注入添加依赖动作：空候选列表原样返回，非空追加快捷修复动作
#[test]
fn 注入添加依赖动作测试() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());

    // 空包名：原响应原样返回
    let 原始值 = serde_json::json!([{"title": "既有动作", "kind": "quickfix"}]);
    assert_eq!(映射器.注入添加依赖动作(&原始值, &[]), 原始值);

    // 非空包名：追加动作，命令字段指向扩展注册的添加依赖命令
    let 注入后 = 映射器.注入添加依赖动作(&原始值, &["serde_json".to_string()]);
    let 操作集 = 注入后.as_array().unwrap();
    assert_eq!(操作集.len(), 2);
    let 新增项 = &操作集[1];
    assert_eq!(新增项["kind"], "quickfix");
    assert_eq!(新增项["command"]["command"], "i18n-rust.cargoAdd");
    assert_eq!(新增项["command"]["arguments"][0], "serde_json");

    // 非数组响应（如 空值）也能注入
    let 注入后 = 映射器.注入添加依赖动作(&Value::Null, &["tokio".to_string()]);
    assert_eq!(注入后.as_array().unwrap().len(), 1);
}

/// 教学诊断注入：全角标点替换动作 + 教学检查忽略动作（方言坐标直用）
#[test]
fn 注入教学动作测试() {
    let (缓存值, _临时) = 构造测试缓存();
    let 映射器 = 响应映射器::新建(缓存值.clone());
    let 资源标识 = "file:///test/main.zh";
    缓存值
        .更新文档(资源标识, "函数 主函数() {\n    让 x = 1;\n}", 1)
        .unwrap();

    let 原始值 = serde_json::json!([{"title": "既有动作", "kind": "quickfix"}]);
    // 无教学诊断：原样返回
    assert_eq!(映射器.注入教学动作(&原始值, &[], 资源标识), 原始值);

    let 全角诊断值 = serde_json::json!({
        "range": {
            "start": { "line": 0, "character": 8 },
            "end": { "line": 0, "character": 9 }
        },
        "code": "fullwidth",
        "source": "i18n-rust",
        "message": "第 1 行第 9 列：检测到全角标点「，」，应改为半角「,」",
        "data": { "character": "，", "replacement": "," }
    });
    let 检查诊断值 = serde_json::json!({
        "range": {
            "start": { "line": 1, "character": 4 },
            "end": { "line": 1, "character": 5 }
        },
        "code": "lint-untyped-let",
        "source": "i18n-rust",
        "message": "第 2 行第 5 列：`让` 未标注类型"
    });
    let 注入后 = 映射器.注入教学动作(&原始值, &[全角诊断值, 检查诊断值], 资源标识);
    let 操作集 = 注入后.as_array().unwrap();
    assert_eq!(操作集.len(), 3, "既有 1 + 教学 2");

    // 全角标点：同坐标替换为半角
    let 修复动作 = &操作集[1];
    assert_eq!(修复动作["kind"], "quickfix");
    assert_eq!(修复动作["edit"]["changes"][资源标识][0]["newText"], ",");
    assert_eq!(
        修复动作["edit"]["changes"][资源标识][0]["range"]["start"]["line"],
        0
    );

    // 教学检查：行尾插入忽略标记（第 2 行 0 起行号 1，行尾插入注释）
    let 忽略动作 = &操作集[2];
    assert_eq!(忽略动作["kind"], "quickfix");
    let 编辑值 = &忽略动作["edit"]["changes"][资源标识][0];
    assert_eq!(编辑值["range"]["start"]["line"], 1);
    assert_eq!(
        编辑值["range"]["start"]["character"],
        编辑值["range"]["end"]["character"]
    );
    assert!(
        编辑值["newText"]
            .as_str()
            .unwrap()
            .contains(i18n_rust_engine::校验::忽略标记)
    );

    // 无可修复字符的全角标点（顿号等）：不产生动作
    let 仅提示值 = serde_json::json!({
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } },
        "code": "fullwidth",
        "source": "i18n-rust",
        "message": "仅提示",
        "data": { "character": "、", "replacement": Value::Null }
    });
    let 注入后 = 映射器.注入教学动作(&Value::Null, &[仅提示值], 资源标识);
    assert_eq!(注入后.as_array().unwrap().len(), 0);
}

/// 保留字撞名：声明名字转译后是 Rust 保留字时出提示级诊断
/// （诊断码为 `lint-reserved-name-collision`，与其它教学检查同一发布链）
#[test]
fn 教学检查诊断保留名撞词() {
    let 管理器 = i18n_rust_engine::映射管理器::映射管理器::从扁表构造(
        HashMap::from([
            ("函数".into(), "fn".into()),
            ("让".into(), "let".into()),
            ("匹配".into(), "match".into()),
        ]),
        HashMap::new(),
        HashMap::new(),
    );
    let 临时 = tempfile::tempdir().unwrap();
    let 缓存值 = 转译缓存::新建(管理器, 临时.path().to_path_buf());
    let (条目值, _) = 缓存值
        .更新文档("file:///test/main.zh", "函数 匹配(数量: 整数) {}", 1)
        .unwrap();

    let 诊断们 = super::诊断文本::教学检查诊断(
        &条目值,
        缓存值.校验词分桶(),
        缓存值.歧义构造器词(),
        缓存值.英文回落映射(),
        缓存值.关键词映射(),
    );
    let 命中项 = 诊断们
        .iter()
        .find(|诊断值| 诊断值["code"] == "lint-reserved-name-collision")
        .unwrap_or_else(|| panic!("应产生保留字撞名诊断：{诊断们:?}"));
    assert_eq!(命中项["severity"], 3);
    assert!(命中项["message"].as_str().unwrap().contains("match"));
    // 行列指向声明名字（第 1 行第 4 列，LSP 0 起 → 字符列 3）
    assert_eq!(命中项["range"]["start"]["line"], 0);
    assert_eq!(命中项["range"]["start"]["character"], 3);
}

/// 未解析导入诊断追加依赖提示（内置中文回退含添加依赖提示键）
#[test]
fn 翻译诊断未解析导入提示() {
    let 翻译后 = 翻译诊断消息(None, "unresolved import `serde_json`", None);
    // 反引号内容保留（提取依赖原名），且追加了 rzc 添加依赖提示
    assert!(翻译后.contains("`serde_json`"));
    assert!(翻译后.contains("rzc add serde_json"));
}

/// 诊断翻译不替换反引号内的标识符（避免误伤变量名中的子串）
#[test]
fn 翻译诊断跳过反引号内容() {
    let 翻译后 = 翻译诊断消息(
        None,
        "cannot find value `expected_value` in this scope",
        None,
    );
    // 反引号内的标识符保持原样
    assert!(翻译后.contains("`expected_value`"));
    // 反引号外的短语已被翻译（不再含英文原短语）
    assert!(!翻译后.contains("cannot find value"));
}

/// E0599（未找到方法）不得把“找到”误译为「实际为」
///
/// 回归：裸词“找到”替换会把未找到方法的消息
/// 产出“没有名为 X 实际为……”（“找到”义被误译），
/// 并与“方法未找到”变体相互干扰。现改为语境键“，实际为 ”。
/// 本测试在所有路径（消息表命中/轻量兜底）下均不得出现「实际为」。
#[test]
fn 翻译诊断方法未找到不误导() {
    let 翻译后 = 翻译诊断消息(
        None,
        "no method named `拉平` found for struct `Vec<i32>` in the current scope",
        None,
    );
    assert!(!翻译后.contains("实际为"), "{翻译后}");
    let 翻译后二 = 翻译诊断消息(None, "method not found in `Vec<i32>`", None);
    assert!(!翻译后二.contains("实际为"), "{翻译后二}");
}

/// E0308（期望与实际字样）保留「实际为」翻译（语境键“，实得 ”生效）
///
/// 轻量兜底路径（测试环境未初始化消息表）：英文期望词转中文、
/// 语境键“，实得 ”→“，实际为 ”。
#[test]
fn 翻译诊断类型不匹配保留实得短语() {
    let 翻译后 = 翻译诊断消息(
        None,
        "mismatched types: expected `char`, found `&str`",
        None,
    );
    assert!(翻译后.contains("实际为"), "{翻译后}");
    // 反引号内类型名保留
    assert!(
        翻译后.contains("`char`") && 翻译后.contains("`&str`"),
        "{翻译后}"
    );
}
