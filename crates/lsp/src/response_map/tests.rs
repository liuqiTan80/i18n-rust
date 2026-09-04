//! response_map 单元测试（自原单文件 tests 模块原样迁移）。

use super::diag_text::translate_diagnostic_message;
use super::responses::label_identifier_suffix;
use super::*;
use crate::translation_cache::TranslationCache;
use std::collections::HashMap;
use std::sync::Arc;

fn create_test_cache() -> (Arc<TranslationCache>, tempfile::TempDir) {
    let manager = i18n_rust_engine::mapping_manager::MappingManager::from_flat_maps(
        HashMap::from([("函数".into(), "fn".into()), ("让".into(), "let".into())]),
        HashMap::new(),
        HashMap::new(),
    );
    let temp = tempfile::tempdir().unwrap();
    let cache = TranslationCache::new(manager, temp.path().to_path_buf());
    (cache, temp)
}

#[test]
fn test_restore_uri() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());

    let (entry, _) = cache
        .update_document("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    assert_eq!(
        mapper.restore_uri(&entry.virtual_uri),
        "file:///test/main.zh"
    );
}

#[test]
fn test_restore_range() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());

    let (entry, _) = cache
        .update_document("file:///test/main.zh", "让 x = 1;\n让 y = 2;", 1)
        .unwrap();

    // 行映射：虚拟行号还原为中文行号（1:1）
    // 列映射：`让`→`let` 每行偏移 +2，英文列 4-5（y）对应中文列 2-3
    let range = mapper.restore_range(
        &entry.virtual_uri,
        &json!({ "start": { "line": 1, "character": 4 }, "end": { "line": 1, "character": 5 } }),
    );
    assert_eq!(range["start"]["line"], 1);
    assert_eq!(range["start"]["character"], 2);
    assert_eq!(range["end"]["line"], 1);
    assert_eq!(range["end"]["character"], 3);
}

#[test]
fn test_map_references_response() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let (entry, _) = cache
        .update_document("file:///test/main.zh", "让 x = 1;\n函数 主() {}", 1)
        .unwrap();

    let response = json!([
        {
            "uri": entry.virtual_uri,
            "range": {
                "start": { "line": 1, "character": 0 },
                "end": { "line": 1, "character": 2 }
            }
        }
    ]);

    let mapped = mapper.map_references_response(&response);
    assert_eq!(mapped[0]["uri"].as_str().unwrap(), "file:///test/main.zh");
    assert_eq!(mapped[0]["range"]["start"]["line"], 1);
    assert_eq!(mapped[0]["range"]["start"]["character"], 0);

    // 无结果（null）时应返回空数组而不是 null
    assert_eq!(
        mapper.map_references_response(&Value::Null),
        Value::Array(Vec::new())
    );
}

#[test]
fn test_map_rename_response_cross_file() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let (entry_a, _) = cache
        .update_document("file:///test/main.zh", "让 x = 1;\n函数 主() {}", 1)
        .unwrap();
    let (entry_b, _) = cache
        .update_document("file:///test/lib.zh", "函数 主() {}", 1)
        .unwrap();

    // rust-analyzer 返回跨文件编辑（changes 形式）
    let response = json!({
        "changes": {
            entry_a.virtual_uri.clone(): [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 2 }
                },
                "newText": "fn"
            }],
            entry_b.virtual_uri.clone(): [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 2 }
                },
                "newText": "fn"
            }]
        }
    });

    let mapped = mapper.map_rename_response(&response);
    let changes = mapped["changes"].as_object().unwrap();

    // 两个文件的 URI 都还原为 .zh 源文件
    assert!(changes.contains_key("file:///test/main.zh"));
    assert!(changes.contains_key("file:///test/lib.zh"));

    // newText 反向翻译：fn → 函数
    assert_eq!(
        changes["file:///test/main.zh"][0]["newText"]
            .as_str()
            .unwrap(),
        "函数"
    );
    assert_eq!(
        changes["file:///test/lib.zh"][0]["newText"]
            .as_str()
            .unwrap(),
        "函数"
    );

    // documentChanges 形式也应正确处理
    let response2 = json!({
        "documentChanges": [{
            "textDocument": { "uri": entry_a.virtual_uri.clone(), "version": null },
            "edits": [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 2 }
                },
                "newText": "fn"
            }]
        }]
    });
    let mapped2 = mapper.map_rename_response(&response2);
    assert_eq!(
        mapped2["documentChanges"][0]["textDocument"]["uri"]
            .as_str()
            .unwrap(),
        "file:///test/main.zh"
    );
    assert_eq!(
        mapped2["documentChanges"][0]["edits"][0]["newText"]
            .as_str()
            .unwrap(),
        "函数"
    );
}

#[test]
fn test_map_code_action_response() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let (entry, _) = cache
        .update_document("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    let response = json!([
        {
            "title": "导入 std::io",
            "kind": "quickfix",
            "edit": {
                "changes": {
                    entry.virtual_uri.clone(): [{
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

    let mapped = mapper.map_code_action_response(&response, "file:///test/main.zh");
    let changes = mapped[0]["edit"]["changes"].as_object().unwrap();
    assert!(changes.contains_key("file:///test/main.zh"));

    // 代码操作插入的英文代码经反向翻译：测试映射表中无 use 关键字，
    // 故保持英文原样（若语言包含 使用→use 映射，则会被还原为母语）
    assert_eq!(
        changes["file:///test/main.zh"][0]["newText"]
            .as_str()
            .unwrap(),
        "use std::io;"
    );

    // null 响应 → 空数组
    assert_eq!(
        mapper.map_code_action_response(&Value::Null, ""),
        Value::Array(Vec::new())
    );
}

#[test]
fn test_map_code_action_resolve_response() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let (entry, _) = cache
        .update_document("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    // resolve 响应是单个 CodeAction 对象（非数组），edit 走 documentChanges
    let response = json!({
        "title": "改为 pub(crate)",
        "kind": "refactor.rewrite",
        "data": { "id": 1 },
        "edit": {
            "documentChanges": [{
                "textDocument": { "uri": entry.virtual_uri, "version": 1 },
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

    let mapped = mapper.map_code_action_resolve_response(&response);
    // 虚拟 URI 必须还原为原始文件 URI（否则 VSCode 无法应用编辑）
    assert_eq!(
        mapped["edit"]["documentChanges"][0]["textDocument"]["uri"]
            .as_str()
            .unwrap(),
        "file:///test/main.zh"
    );
    // 位置从虚拟坐标映射回母语坐标
    let range = &mapped["edit"]["documentChanges"][0]["edits"][0]["range"];
    assert_eq!(range["start"]["line"], 0);
    assert_eq!(range["start"]["character"], 0);
    // data 与 title 原样保留
    assert_eq!(mapped["data"]["id"], 1);
    assert_eq!(mapped["title"].as_str().unwrap(), "改为 pub(crate)");

    // 无 edit 字段时原样返回
    let plain = json!({"title": "仅命令", "command": {"title": "c", "command": "x"}});
    assert_eq!(mapper.map_code_action_resolve_response(&plain), plain);
    // null 响应原样返回
    assert_eq!(
        mapper.map_code_action_resolve_response(&Value::Null),
        Value::Null
    );
}

#[test]
fn test_map_document_symbol_response() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let (entry, _) = cache
        .update_document("file:///test/main.zh", "函数 主() {\n    让 x = 1;\n}", 1)
        .unwrap();
    assert_eq!(entry.en_content, "fn 主() {\n    let x = 1;\n}");

    let response = json!([
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

    let mapped = mapper.map_document_symbol_response(&response, "file:///test/main.zh");

    // 符号名恢复为中文
    assert_eq!(mapped[0]["name"].as_str().unwrap(), "函数");
    assert_eq!(mapped[0]["children"][0]["name"].as_str().unwrap(), "让");

    // 位置映射回母语文件（行 1:1、列按偏移转换）
    assert_eq!(mapped[0]["range"]["start"]["line"], 0);
    assert_eq!(mapped[0]["selectionRange"]["start"]["character"], 0);
    assert_eq!(mapped[0]["children"][0]["range"]["start"]["line"], 1);

    // null 响应 → 空数组
    assert_eq!(
        mapper.map_document_symbol_response(&Value::Null, ""),
        Value::Array(Vec::new())
    );
}

#[test]
fn test_map_diagnostics_ownership_details_in_data() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    // 多行文档，保证行映射存在（诊断行号 2/4 可还原）
    let (entry, _) = cache
        .update_document(
            "file:///test/main.zh",
            "让 数据 = 1;\n让 a = 1;\n让 b = 1;\n让 c = 1;\n让 d = 1;",
            1,
        )
        .unwrap();

    // rust-analyzer 风格的 E0382 诊断：主 range 是再次使用处，relatedInformation 标记移动
    let diag = json!({
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
                "uri": entry.virtual_uri.clone(),
                "range": {
                    "start": { "line": 2, "character": 8 },
                    "end": { "line": 2, "character": 10 }
                }
            },
            "message": "value moved here"
        }]
    });
    let params = json!({
        "uri": entry.virtual_uri,
        "version": 1,
        "diagnostics": [diag]
    });

    let mapped = mapper.map_diagnostics(&params);
    let data = mapped["diagnostics"][0]["data"]
        .as_object()
        .expect("所有权诊断的 data 字段应为 JSON 对象");

    // 变量名与位置（LSP 0-based 行号 +1 → 1-based）
    assert_eq!(data["变量名"], "数据");
    assert_eq!(data["移动发生"]["起始行"], 3);
    assert_eq!(data["再次使用"]["起始行"], 5);
    assert!(data["借用发生"].is_null());
}

#[test]
fn test_map_diagnostics_non_ownership_no_ownership_details() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let (entry, _) = cache
        .update_document("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    // 类型不匹配错误不应附带所有权详情
    let diag = json!({
        "range": {
            "start": { "line": 0, "character": 4 },
            "end": { "line": 0, "character": 8 }
        },
        "severity": 1,
        "message": "mismatched types",
        "relatedInformation": []
    });
    let params = json!({
        "uri": entry.virtual_uri,
        "version": 1,
        "diagnostics": [diag]
    });

    let mapped = mapper.map_diagnostics(&params);
    assert!(mapped["diagnostics"][0].get("data").is_none());
}

/// documentHighlight 响应的 range 必须还原为母语坐标
#[test]
fn test_map_document_highlight_response() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let (_entry, _) = cache
        .update_document("file:///test/main.zh", "让 x = 1;\n让 y = x;", 1)
        .unwrap();

    // 英文坐标（"let" 占 3 列；行0 x 在英文列 4，行1 "let y = " 后 x 在英文列 8）
    let response = json!([
        { "range": { "start": { "line": 0, "character": 4 }, "end": { "line": 0, "character": 5 } }, "kind": 2 },
        { "range": { "start": { "line": 1, "character": 8 }, "end": { "line": 1, "character": 9 } }, "kind": 2 }
    ]);
    let mapped = mapper.map_document_highlight_response(&response, "file:///test/main.zh");
    // 中文列："让 x" 中 x 在列 2（"让" 占 1 个 UTF-16 单元）；"让 y = x" 中 x 在列 6
    assert_eq!(mapped[0]["range"]["start"]["character"], 2);
    assert_eq!(mapped[1]["range"]["start"]["character"], 6);
    assert_eq!(mapped[0]["kind"], 2);

    // null 响应 → 空数组
    assert_eq!(
        mapper.map_document_highlight_response(&Value::Null, ""),
        Value::Array(Vec::new())
    );
}

/// 语义着色响应的 delta 编码必须还原为母语坐标，
/// 且长度按映射后的列差重算（关键字替换改变列宽）
#[test]
fn test_map_semantic_tokens_response() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let (_entry, _) = cache
        .update_document("file:///test/main.zh", "让 x = 1;\n让 y = x;", 1)
        .unwrap();

    // 英文坐标（"let" 占 3 列，变量 x/y 在列 4/8）；
    // delta 编码：[deltaLine, deltaStart, length, tokenType, tokenModifiers]
    let response = json!({
        "resultId": "abc",
        "data": [
            0, 0, 3, 14, 0,   // let   行0 列0
            0, 4, 1, 6, 0,    // x     行0 列4
            1, 4, 1, 6, 0,    // y     行1 列4（跨行，列重置为绝对）
            0, 4, 1, 6, 0     // x     行1 列8
        ]
    });
    let mapped = mapper.map_semantic_tokens_response(&response, "file:///test/main.zh");
    // resultId 透传；重新 delta 编码后的中文坐标：
    // 让(0,0,len1)、x(0,2)、y(1,2)、x(1,6)
    assert_eq!(mapped["resultId"], "abc");
    assert_eq!(
        mapped["data"],
        json!([0, 0, 1, 14, 0, 0, 2, 1, 6, 0, 1, 2, 1, 6, 0, 0, 4, 1, 6, 0])
    );
}

/// hover 命中解释表：完整路径降级匹配短路径键（std::option::Option<T>::unwrap）
#[test]
fn test_map_hover_response_full_path_hit() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let doc =
        "```rust\nstd::option::Option<T>::unwrap\n```\n\nPanics if the value is a [`None`]...";
    let response = json!({"contents": {"kind": "markdown", "value": doc}});
    let mapped = mapper.map_hover_response(&response, "file:///test/main.zh");
    let value = mapped["contents"]["value"].as_str().unwrap();
    assert!(value.starts_with("**大白话："), "应插入加粗提示: {value}");
    assert!(
        value.contains("直接取出"),
        "解释应为 unwrap 的大白话: {value}"
    );
    assert!(value.ends_with(doc), "原文应保留在提示之后: {value}");
}

/// hover 命中解释表：impl 标题 + 签名行 → 短路径键（Option::unwrap）
#[test]
fn test_map_hover_response_impl_title_hit() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let doc = "**`impl<T> Option<T>`**\n\n```rust\npub fn unwrap(self) -> T\n```\n\nPanics if the value is a None...";
    let response = json!({"contents": {"kind": "markdown", "value": doc}});
    let mapped = mapper.map_hover_response(&response, "file:///test/main.zh");
    let value = mapped["contents"]["value"].as_str().unwrap();
    assert!(value.starts_with("**大白话："), "应插入加粗提示: {value}");
    assert!(value.ends_with(doc));
}

/// hover 未命中解释表：原样透传，不改变任何内容
#[test]
fn test_map_hover_response_miss_unchanged() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let doc = "```rust\npub fn 自定义函数(x: i32) -> i32\n```\n\n自定义函数说明";
    let response = json!({"contents": {"kind": "markdown", "value": doc}});
    let mapped = mapper.map_hover_response(&response, "file:///test/main.zh");
    assert_eq!(mapped["contents"]["value"].as_str().unwrap(), doc);
}

/// signatureHelp：label 与参数 label 词法级中文化
#[test]
fn test_map_signature_help_response_translated() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let response = json!({
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
    let mapped = mapper.map_signature_help_response(&response);
    let label = mapped["signatures"][0]["label"].as_str().unwrap();
    assert!(label.starts_with("函数 push"), "fn 应译为函数：{label}");
    assert!(label.contains("&mut self"), "self 无映射应保留：{label}");
    // 参数 [start,end] 索引按原 label 切片（[0,2] = "fn"）翻译后转为字符串
    let param0 = mapped["signatures"][0]["parameters"][0]["label"]
        .as_str()
        .unwrap();
    assert_eq!(param0, "函数", "索引形式参数应翻译：{param0}");
    // 字符串形式参数同样翻译
    let param1 = mapped["signatures"][0]["parameters"][1]["label"]
        .as_str()
        .unwrap();
    assert!(param1.contains("value: T"), "泛型参数保留：{param1}");
    assert_eq!(mapped["activeParameter"], 1);
}

/// signatureHelp 无 signatures（null/空）：原样返回不报错
#[test]
fn test_map_signature_help_response_empty() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let response = json!(null);
    assert_eq!(mapper.map_signature_help_response(&response), Value::Null);
    let response = json!({"signatures": []});
    assert_eq!(
        mapper.map_signature_help_response(&response)["signatures"],
        json!([])
    );
}

/// hover MarkedString：先查解释表加大白话前缀，再对代码做词法级中文化
#[test]
fn test_map_hover_response_marked_string_translated() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let response = json!({
        "contents": {"language": "rust", "value": "pub fn push(&mut self, value: T)"}
    });
    let mapped = mapper.map_hover_response(&response, "file:///test/main.zh");
    let value = mapped["contents"]["value"].as_str().unwrap();
    assert!(
        value.contains("函数 push"),
        "代码签名应中文化（fn→函数）：{value}"
    );
}

/// hover MarkedString 数组形式：简单键命中（clone），其余元素原样
#[test]
fn test_map_hover_response_marked_string_array_hit() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let response = json!({
        "contents": [
            {"language": "rust", "value": "pub fn clone(&self) -> Self"},
            {"kind": "markdown", "value": "Returns a copy of the value."}
        ]
    });
    let mapped = mapper.map_hover_response(&response, "file:///test/main.zh");
    assert!(
        mapped["contents"][0]["value"]
            .as_str()
            .unwrap()
            .starts_with("**大白话：")
    );
    assert_eq!(
        mapped["contents"][1]["value"].as_str().unwrap(),
        "Returns a copy of the value."
    );
}

/// hover 内容为 null 等异常形态时安全透传
#[test]
fn test_map_hover_response_null_contents() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let mapped = mapper.map_hover_response(&json!({"contents": null}), "");
    assert!(mapped["contents"].is_null());
}

/// 补全响应的 textEdit.newText 反向翻译、additionalTextEdits 位置还原
#[test]
fn test_map_completion_text_edit_reverse_translated() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let (entry, _) = cache
        .update_document("file:///test/main.zh", "让 x = 1;", 1)
        .unwrap();

    let response = json!({
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
    let mapped = mapper.map_completion_response(&response, "file:///test/main.zh");
    let item = &mapped["items"][0];
    // label 与 newText 均还原为母语关键字
    assert_eq!(item["label"].as_str().unwrap(), "让");
    assert_eq!(item["textEdit"]["newText"].as_str().unwrap(), "让");
    // snippet 占位符保持原样
    let snippet_item = json!({
        "items": [{ "label": "x", "textEdit": {
            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
            "newText": "fn ${1:name}() {}" } }]
    });
    let mapped_snippet = mapper.map_completion_response(&snippet_item, "file:///test/main.zh");
    assert_eq!(
        mapped_snippet["items"][0]["textEdit"]["newText"]
            .as_str()
            .unwrap(),
        "fn ${1:name}() {}"
    );
    // additionalTextEdits 中的 fn 被反向翻译
    assert_eq!(
        item["additionalTextEdits"][0]["newText"].as_str().unwrap(),
        "函数 辅助() {}"
    );
    let _ = entry;
}

/// labelDetails 反向翻译：VS Code 提示框右侧优先显示此字段，
/// fn() 等英文签名必须还原为母语（如 函数()）
#[test]
fn test_map_completion_label_details_translated() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    // 文档中声明 my_func，使其通过语言过滤的用户词汇白名单
    let (entry, _) = cache
        .update_document("file:///test/main.zh", "函数 my_func() {}", 1)
        .unwrap();
    let response = json!({
        "items": [{
            "label": "my_func",
            "detail": "fn my_func()",
            "labelDetails": {
                "description": "fn()",
                "detail": "crate::辅助"
            }
        }]
    });
    let mapped = mapper.map_completion_response(&response, "file:///test/main.zh");
    let item = &mapped["items"][0];
    assert_eq!(item["detail"].as_str().unwrap(), "函数 my_func()");
    assert_eq!(
        item["labelDetails"]["description"].as_str().unwrap(),
        "函数()"
    );
    // crate/模块路径中无映射命中时保持原样
    assert_eq!(
        item["labelDetails"]["detail"].as_str().unwrap(),
        "crate::辅助"
    );
    let _ = entry;
}

/// 语言过滤：非英文方言下补全列表不得串语言。
/// 保留：翻译命中项、母语字符项、用户自定义项（含英文命名）；
/// 过滤：未翻译的外部英文项。
#[test]
fn test_map_completion_language_filter() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    // 用户源码同时含母语定义（自定义函数）与英文命名（helper）
    let (entry, _) = cache
        .update_document(
            "file:///test/main.zh",
            "函数 自定义函数() {} 函数 helper() {}",
            1,
        )
        .unwrap();

    let response = json!({
        "items": [
            { "label": "let" },
            { "label": "自定义函数" },
            { "label": "helper(…)" },
            { "label": "serde_json" },
            { "label": "BTreeMap" }
        ]
    });
    let mapped = mapper.map_completion_response(&response, "file:///test/main.zh");
    let labels: Vec<&str> = mapped["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    // 翻译命中（let → 让）、母语标识符、用户定义的英文名保留
    assert_eq!(labels, vec!["让", "自定义函数", "helper(…)"]);
    // 未翻译的外部英文项（serde_json/BTreeMap）被过滤
    assert!(!labels.contains(&"serde_json"));
    assert!(!labels.contains(&"BTreeMap"));
    let _ = entry;
}

/// 英文语言包（恒等映射）不启用语言过滤，所有项保留
#[test]
fn test_map_completion_no_filter_for_identity_pack() {
    let manager = i18n_rust_engine::mapping_manager::MappingManager::from_flat_maps(
        HashMap::from([("fn".into(), "fn".into()), ("let".into(), "let".into())]),
        HashMap::new(),
        HashMap::new(),
    );
    let temp = tempfile::tempdir().unwrap();
    let cache = TranslationCache::new(manager, temp.path().to_path_buf());
    let mapper = ResponseMapper::new(cache);

    let response = json!({
        "items": [
            { "label": "let" },
            { "label": "BTreeMap" },
            { "label": "serde_json" }
        ]
    });
    let mapped = mapper.map_completion_response(&response, "file:///test/main.zh");
    assert_eq!(mapped["items"].as_array().unwrap().len(), 3);
}

/// label 末段标识符提取：后缀、路径前缀、模块补全形式
#[test]
fn test_label_identifier_suffix() {
    assert_eq!(label_identifier_suffix("foo(…)"), Some("foo"));
    assert_eq!(label_identifier_suffix("Foo {…}"), Some("Foo"));
    assert_eq!(label_identifier_suffix("m::Spam::Bar(…)"), Some("Bar"));
    assert_eq!(label_identifier_suffix("m::"), Some("m"));
    assert_eq!(label_identifier_suffix("println!"), Some("println"));
    assert_eq!(label_identifier_suffix("(…)"), None);
}

/// 注入添加依赖动作：空候选列表原样返回，非空追加 quickfix 动作
#[test]
fn test_inject_add_dependency_actions() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());

    // 空 crates：原响应原样返回
    let original = json!([{"title": "既有动作", "kind": "quickfix"}]);
    assert_eq!(
        mapper.inject_add_dependency_actions(&original, &[]),
        original
    );

    // 非空 crates：追加动作，command 指向扩展注册的 cargoAdd
    let injected = mapper.inject_add_dependency_actions(&original, &["serde_json".to_string()]);
    let actions = injected.as_array().unwrap();
    assert_eq!(actions.len(), 2);
    let added = &actions[1];
    assert_eq!(added["kind"], "quickfix");
    assert_eq!(added["command"]["command"], "i18n-rust.cargoAdd");
    assert_eq!(added["command"]["arguments"][0], "serde_json");

    // 非数组响应（如 null）也能注入
    let injected = mapper.inject_add_dependency_actions(&Value::Null, &["tokio".to_string()]);
    assert_eq!(injected.as_array().unwrap().len(), 1);
}

/// 教学诊断注入：全角标点替换动作 + 教学 lint 忽略动作（方言坐标直用）
#[test]
fn test_inject_teaching_actions() {
    let (cache, _temp) = create_test_cache();
    let mapper = ResponseMapper::new(cache.clone());
    let uri = "file:///test/main.zh";
    cache
        .update_document(uri, "函数 主函数() {\n    让 x = 1;\n}", 1)
        .unwrap();

    let original = json!([{"title": "既有动作", "kind": "quickfix"}]);
    // 无教学诊断：原样返回
    assert_eq!(
        mapper.inject_teaching_actions(&original, &[], uri),
        original
    );

    let fullwidth_diag = json!({
        "range": {
            "start": { "line": 0, "character": 8 },
            "end": { "line": 0, "character": 9 }
        },
        "code": "fullwidth",
        "source": "i18n-rust",
        "message": "第 1 行第 9 列：检测到全角标点「，」，应改为半角「,」",
        "data": { "character": "，", "replacement": "," }
    });
    let lint_diag = json!({
        "range": {
            "start": { "line": 1, "character": 4 },
            "end": { "line": 1, "character": 5 }
        },
        "code": "lint-untyped-let",
        "source": "i18n-rust",
        "message": "第 2 行第 5 列：`让` 未标注类型"
    });
    let injected = mapper.inject_teaching_actions(&original, &[fullwidth_diag, lint_diag], uri);
    let actions = injected.as_array().unwrap();
    assert_eq!(actions.len(), 3, "既有 1 + 教学 2");

    // 全角标点：同坐标替换为半角
    let fix = &actions[1];
    assert_eq!(fix["kind"], "quickfix");
    assert_eq!(fix["edit"]["changes"][uri][0]["newText"], ",");
    assert_eq!(fix["edit"]["changes"][uri][0]["range"]["start"]["line"], 0);

    // 教学 lint：行尾插入忽略标记（第 2 行 0 起行号 1，行尾插入注释）
    let ignore = &actions[2];
    assert_eq!(ignore["kind"], "quickfix");
    let edit = &ignore["edit"]["changes"][uri][0];
    assert_eq!(edit["range"]["start"]["line"], 1);
    assert_eq!(
        edit["range"]["start"]["character"],
        edit["range"]["end"]["character"]
    );
    assert!(
        edit["newText"]
            .as_str()
            .unwrap()
            .contains(i18n_rust_engine::lint::IGNORE_MARK)
    );

    // 无可修复字符的全角标点（顿号等）：不产生动作
    let hint_only = json!({
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } },
        "code": "fullwidth",
        "source": "i18n-rust",
        "message": "仅提示",
        "data": { "character": "、", "replacement": null }
    });
    let injected = mapper.inject_teaching_actions(&Value::Null, &[hint_only], uri);
    assert_eq!(injected.as_array().unwrap().len(), 0);
}

/// 未解析导入诊断追加依赖提示（内置 zh 回退含 lsp_hint_add_dependency 键）
#[test]
fn test_translate_diagnostic_unresolved_import_hint() {
    let translated = translate_diagnostic_message("unresolved import `serde_json`");
    // 反引号内容保留（提取依赖原名），且追加了 rzc add 提示
    assert!(translated.contains("`serde_json`"));
    assert!(translated.contains("rzc add serde_json"));
}

/// 诊断翻译不替换反引号内的标识符（避免误伤变量名中的子串）
#[test]
fn test_translate_diagnostic_skips_backtick_content() {
    let translated =
        translate_diagnostic_message("cannot find value `expected_value` in this scope");
    // 反引号内的标识符保持原样
    assert!(translated.contains("`expected_value`"));
    // 反引号外的短语已被翻译（不再含英文原短语）
    assert!(!translated.contains("cannot find value"));
}
