//! rustdoc JSON 解析：提取公开 API（仅名称与签名，绝不读取 docs 字段）
//! 并渲染为可读的类型签名。

use anyhow::anyhow;
use serde_json::{Map, Value};
use std::collections::HashSet;

use super::{ApiEntry, ApiKind};

/// 解析 rustdoc JSON，提取所有公开 API（仅名称与签名，绝不读取 docs 字段）
pub fn extract_public_api(json_text: &str) -> anyhow::Result<Vec<ApiEntry>> {
    Ok(extract_public_api_with_glob_sources(json_text)?.0)
}

/// 同 [`extract_public_api`]，额外返回 glob 重导出（`pub use 依赖::*`）指向的
/// 依赖 crate 名列表：薄壳 crate（meta crate）的公开 API 全部来自这些重导出，
/// 调用方需对这些依赖 crate 单独生成 rustdoc JSON 并合并提取
pub fn extract_public_api_with_glob_sources(
    json_text: &str,
) -> anyhow::Result<(Vec<ApiEntry>, Vec<String>)> {
    let doc: Value = serde_json::from_str(json_text).map_err(|e| {
        anyhow!(
            "{}",
            crate::ui::Ui::global().f("mg_err_parse_rustdoc", &[&e.to_string()])
        )
    })?;
    let index = doc
        .get("index")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("{}", crate::ui::Ui::global().t("mg_err_no_index")))?;
    let root = doc
        .get("root")
        .map(|v| {
            v.as_str()
                .map(String::from)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_else(|| "0".to_string());
    let mut results = Vec::new();
    let mut glob_sources = Vec::new();
    let mut visited = HashSet::new();
    collect_module_items(index, &root, &mut visited, &mut results, &mut glob_sources);
    Ok((results, glob_sources))
}

/// 深度优先收集模块树中的公开项（跳过 impl 方法、私有项、struct 字段等）
///
/// `glob_sources` 收集 glob 重导出指向的依赖 crate 名，供薄壳 crate 追踪。
fn collect_module_items(
    index: &Map<String, Value>,
    id: &str,
    visited: &mut HashSet<String>,
    results: &mut Vec<ApiEntry>,
    glob_sources: &mut Vec<String>,
) {
    if !visited.insert(id.to_string()) {
        return;
    }
    let Some(item) = index.get(id) else { return };
    if item.get("visibility").and_then(Value::as_str) != Some("public") {
        return;
    }
    let Some((key, inner)) = item
        .get("inner")
        .and_then(Value::as_object)
        .and_then(|o| o.iter().next())
    else {
        return;
    };
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match key.as_str() {
        "module" => {
            if let Some(items) = inner.get("items").and_then(Value::as_array) {
                for child in items {
                    collect_module_items(index, &child.to_string(), visited, results, glob_sources);
                }
            }
        }
        "function" => {
            results.push(ApiEntry {
                kind: ApiKind::Function,
                english_name: name.clone(),
                signature: render_function_signature(&name, item),
            });
        }
        "struct" => {
            let generics = render_generics(inner.get("generics").unwrap_or(&Value::Null));
            results.push(ApiEntry {
                kind: ApiKind::Struct,
                english_name: name.clone(),
                signature: format!("struct {}{}", name, generics),
            });
        }
        "enum" => {
            let generics = render_generics(inner.get("generics").unwrap_or(&Value::Null));
            results.push(ApiEntry {
                kind: ApiKind::Enum,
                english_name: name.clone(),
                signature: format!("enum {}{}", name, generics),
            });
        }
        "trait" => {
            let generics = render_generics(inner.get("generics").unwrap_or(&Value::Null));
            results.push(ApiEntry {
                kind: ApiKind::Trait,
                english_name: name.clone(),
                signature: format!("trait {}{}", name, generics),
            });
        }
        "type_alias" => {
            let ty = render_type(inner.get("type").unwrap_or(&Value::Null));
            results.push(ApiEntry {
                kind: ApiKind::TypeAlias,
                english_name: name.clone(),
                signature: format!("type {} = {}", name, ty),
            });
        }
        "constant" => {
            let ty = render_type(inner.get("type").unwrap_or(&Value::Null));
            results.push(ApiEntry {
                kind: ApiKind::Const,
                english_name: name.clone(),
                signature: format!("const {}: {}", name, ty),
            });
        }
        // rustdoc JSON 当前工具链不输出 macro_rules! 宏（官方格式限制），预留支持
        "macro" => {
            results.push(ApiEntry {
                kind: ApiKind::Macro,
                english_name: name.clone(),
                signature: format!("{}!", name),
            });
        }
        // re-export（pub use）：薄壳 crate（如 serde 1.0.229 = serde_core 的转发层）
        // 的公开 API 全部是 re-export。名称在 inner.use.name（顶层 name 为 null）。
        "use" => {
            let use_obj = inner; // inner 即 use 对象（inner 单键解构结果）
            if use_obj
                .get("is_glob")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                // glob 重导出（pub use 依赖::*）：外部 crate 的项不在本 JSON 的
                // index 中，无法枚举名称；记录 source 路径的首段（被重导出的
                // crate 名，如 `salvo_core::prelude` → `salvo_core`），由调用方
                // 对该依赖 crate 单独生成文档后合并提取（薄壳 crate 追踪）。
                // crate/self/super 开头的内部 glob 无法枚举，跳过。
                if let Some(source) = use_obj.get("source").and_then(Value::as_str) {
                    let source = source.trim_start_matches("::");
                    let dep_name = source.split("::").next().unwrap_or("");
                    if !dep_name.is_empty()
                        && dep_name != "crate"
                        && dep_name != "self"
                        && dep_name != "super"
                    {
                        glob_sources.push(dep_name.to_string());
                    }
                }
                return;
            }
            let Some(english_name) = use_obj.get("name").and_then(Value::as_str) else {
                return;
            };
            // 若指向本 crate 内的模块（pub use crate::模块），递归跟随收集其公开项
            if let Some(target_id) = use_obj.get("id").map(|v| v.to_string())
                && target_id != id
                && index.contains_key(&target_id)
                && index[&target_id]
                    .get("inner")
                    .and_then(|i| i.get("module"))
                    .is_some()
            {
                collect_module_items(index, target_id.as_str(), visited, results, glob_sources);
                return;
            }
            let source = use_obj.get("source").and_then(Value::as_str).unwrap_or("?");
            results.push(ApiEntry {
                kind: ApiKind::TypeAlias,
                english_name: english_name.to_string(),
                signature: format!("type {} = {}", english_name, source),
            });
        }
        // impl / import / struct_field / variant / assoc_type 等跳过
        _ => {}
    }
}

/// 渲染函数签名：`async fn name<T>(a: u32) -> Result<T>`
fn render_function_signature(name: &str, item: &Value) -> String {
    let f = item
        .get("inner")
        .and_then(|i| i.get("function"))
        .unwrap_or(&Value::Null);
    let sig = f.get("sig").unwrap_or(&Value::Null);
    let header = f.get("header").unwrap_or(&Value::Null);
    let mut s = String::new();
    if header
        .get("is_const")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        s.push_str("const ");
    }
    if header
        .get("is_unsafe")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        s.push_str("unsafe ");
    }
    if header
        .get("is_async")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        s.push_str("async ");
    }
    s.push_str("fn ");
    s.push_str(name);
    s.push_str(&render_generics(f.get("generics").unwrap_or(&Value::Null)));
    s.push_str(&render_io(
        sig.get("inputs").unwrap_or(&Value::Null),
        sig.get("output"),
    ));
    s
}

/// 渲染参数列表与返回类型：`(a: u32, b: String) -> Result<T>`
fn render_io(inputs: &Value, output: Option<&Value>) -> String {
    let params: Vec<String> = inputs
        .as_array()
        .map(|list| {
            list.iter()
                .map(|item| {
                    if let Some(pair) = item.as_array() {
                        let name = pair.first().and_then(Value::as_str).unwrap_or("_");
                        let ty = pair.get(1).unwrap_or(&Value::Null);
                        format!("{}: {}", name, render_type(ty))
                    } else {
                        render_type(item)
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let ret = output.map(render_type).filter(|s| s != "()");
    match ret {
        Some(ret) => format!("({}) -> {}", params.join(", "), ret),
        None => format!("({})", params.join(", ")),
    }
}

/// 渲染泛型参数：`<T, U>` 或空串
fn render_generics(generics: &Value) -> String {
    let params: Vec<String> = generics
        .get("params")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|p| {
                    let name = p.get("name").and_then(Value::as_str).unwrap_or("?");
                    if p.get("kind").and_then(|k| k.get("lifetime")).is_some() {
                        if name.starts_with('\'') {
                            name.to_string()
                        } else {
                            format!("'{}'", name)
                        }
                    } else if p.get("kind").and_then(|k| k.get("const")).is_some() {
                        format!("const {}", name)
                    } else {
                        name.to_string()
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

/// 递归渲染 rustdoc 类型节点为可读签名（不支持的类型显示 `?`）
fn render_type(ty: &Value) -> String {
    if let Some(s) = ty.get("primitive").and_then(Value::as_str) {
        return if s == "unit" {
            "()".to_string()
        } else {
            s.to_string()
        };
    }
    if let Some(s) = ty.get("generic").and_then(Value::as_str) {
        return s.to_string();
    }
    if let Some(rp) = ty.get("resolved_path") {
        let name = rp.get("path").and_then(Value::as_str).unwrap_or("?");
        if let Some(args) = rp.get("args").and_then(|a| a.get("angle_bracketed")) {
            let params: Vec<String> = args
                .get("args")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .map(|item| {
                            if let Some(t) = item.get("type") {
                                render_type(t)
                            } else if let Some(l) = item.get("lifetime") {
                                format!("'{}'", l.as_str().unwrap_or("?"))
                            } else {
                                "?".to_string()
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            return if params.is_empty() {
                name.to_string()
            } else {
                format!("{}<{}>", name, params.join(", "))
            };
        }
        return name.to_string();
    }
    if let Some(br) = ty.get("borrowed_ref") {
        let mut s = String::from("&");
        if let Some(l) = br.get("lifetime").and_then(Value::as_str) {
            s.push('\'');
            s.push_str(l);
            s.push(' ');
        }
        if br.get("mutable").and_then(Value::as_bool).unwrap_or(false) {
            s.push_str("mut ");
        }
        s.push_str(&render_type(br.get("type").unwrap_or(&Value::Null)));
        return s;
    }
    if let Some(t) = ty.get("tuple") {
        let elements: Vec<String> = t
            .as_array()
            .map(|list| list.iter().map(render_type).collect())
            .unwrap_or_default();
        return format!("({})", elements.join(", "));
    }
    if let Some(sl) = ty.get("slice") {
        return format!("[{}]", render_type(sl.get("type").unwrap_or(&Value::Null)));
    }
    if let Some(arr) = ty.get("array") {
        let len = arr
            .get("len")
            .and_then(|l| l.get("expr"))
            .and_then(Value::as_str)
            .unwrap_or("?");
        return format!(
            "[{}; {}]",
            render_type(arr.get("type").unwrap_or(&Value::Null)),
            len
        );
    }
    if let Some(i) = ty.get("impl_trait") {
        return format!("impl {}", render_bounds(i.get("bounds")));
    }
    if let Some(d) = ty.get("dyn_trait") {
        return format!("dyn {}", render_bounds(d.get("bounds")));
    }
    if let Some(q) = ty.get("qualified_path") {
        let type_name = q.get("name").and_then(Value::as_str).unwrap_or("?");
        return format!(
            "{}::{}",
            render_type(q.get("self_type").unwrap_or(&Value::Null)),
            type_name
        );
    }
    if let Some(f) = ty.get("function_pointer")
        && let Some(sig) = f.get("sig")
    {
        return format!("fn{}", render_io(&sig["inputs"], sig.get("output")));
    }
    if let Some(rp) = ty.get("raw_pointer") {
        let mutable = rp.get("mutable").and_then(Value::as_bool).unwrap_or(false);
        let asterisk = if mutable { "*mut " } else { "*const " };
        return format!(
            "{}{}",
            asterisk,
            render_type(rp.get("type").unwrap_or(&Value::Null))
        );
    }
    "?".to_string()
}

/// 渲染 trait 边界列表：`A + B`
fn render_bounds(bounds: Option<&Value>) -> String {
    bounds
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|b| {
                    b.get("trait_bound")
                        .and_then(|tb| tb.get("trait"))
                        .and_then(|t| t.get("resolved_path"))
                        .and_then(|p| p.get("path"))
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join(" + ")
        })
        .unwrap_or_else(|| "?".to_string())
}

/// 迷你 rustdoc JSON 样本（结构与真实输出一致：index/root/inner 等）。
/// 供本模块与 toml_output / doc_json 的测试共用。
#[cfg(test)]
pub(crate) fn sample_json() -> String {
    r#"{
  "root": "100",
  "index": {
    "100": { "id": 100, "name": "sample", "visibility": "public", "inner": { "module": { "items": [101, 102, 103, 104, 105, 106, 107, 108, 111, 112, 115] } } },
    "101": { "id": 101, "name": "new", "visibility": "public", "inner": { "function": { "sig": { "inputs": [["x", { "primitive": "u32" }]], "output": { "resolved_path": { "path": "Result", "id": 9, "args": { "angle_bracketed": { "args": [ { "type": { "generic": "T" } }, { "type": { "resolved_path": { "path": "String", "id": 10, "args": null } } } ], "constraints": [] } } } } }, "generics": { "params": [ { "name": "T", "kind": { "type": { "bounds": [], "default": null, "is_synthetic": false } } } ], "where_predicates": [] }, "header": { "is_const": false, "is_unsafe": false, "is_async": true, "abi": "Rust" }, "has_body": true } } },
    "102": { "id": 102, "name": "Foo", "visibility": "public", "inner": { "struct": { "kind": { "plain": { "fields": [], "has_stripped_fields": false } }, "generics": { "params": [], "where_predicates": [] }, "impls": [] } } },
    "103": { "id": 103, "name": "State", "visibility": "public", "inner": { "enum": { "generics": { "params": [], "where_predicates": [] }, "has_stripped_variants": false, "variants": [], "impls": [] } } },
    "104": { "id": 104, "name": "Behavior", "visibility": "public", "inner": { "trait": { "is_auto": false, "is_unsafe": false, "is_dyn_compatible": true, "items": [], "generics": { "params": [], "where_predicates": [] }, "bounds": [], "implementations": [] } } },
    "105": { "id": 105, "name": "Count", "visibility": "public", "inner": { "type_alias": { "type": { "primitive": "u32" }, "generics": { "params": [], "where_predicates": [] } } } },
    "106": { "id": 106, "name": "MAX", "visibility": "public", "inner": { "constant": { "type": { "primitive": "u32" }, "const": { "expr": "100", "value": "100u32", "is_literal": true } } } },
    "107": { "id": 107, "name": "print", "visibility": "public", "inner": { "macro": {} } },
    "108": { "id": 108, "name": "inner_tools", "visibility": "crate", "inner": { "module": { "items": [110] } } },
    "110": { "id": 110, "name": "hidden_fn", "visibility": "public", "inner": { "function": { "sig": { "inputs": [], "output": null }, "generics": { "params": [], "where_predicates": [] }, "header": { "is_const": false, "is_unsafe": false, "is_async": false, "abi": "Rust" }, "has_body": true } } },
    "109": { "id": 109, "name": "实例方法", "visibility": "public", "inner": { "function": { "sig": { "inputs": [["self", { "borrowed_ref": { "lifetime": null, "mutable": false, "type": { "resolved_path": { "path": "Foo", "id": 102, "args": null } } } }]], "output": null }, "generics": { "params": [], "where_predicates": [] }, "header": { "is_const": false, "is_unsafe": false, "is_async": false, "abi": "Rust" }, "has_body": true } } },
    "111": { "id": 111, "name": null, "visibility": "public", "inner": { "use": { "source": "serde_core::Deserialize", "name": "Deserialize", "id": 999, "is_glob": false, "is_import": true } } },
    "112": { "id": 112, "name": null, "visibility": "public", "inner": { "use": { "source": "crate::sub", "name": "sub", "id": 113, "is_glob": false, "is_import": true } } },
    "113": { "id": 113, "name": "sub", "visibility": "public", "inner": { "module": { "items": [114] } } },
    "114": { "id": 114, "name": "子函数", "visibility": "public", "inner": { "function": { "sig": { "inputs": [], "output": null }, "generics": { "params": [], "where_predicates": [] }, "header": { "is_const": false, "is_unsafe": false, "is_async": false, "abi": "Rust" }, "has_body": true } } },
    "115": { "id": 115, "name": null, "visibility": "public", "inner": { "use": { "source": "core::*", "name": null, "id": 1, "is_glob": true, "is_import": true } } }
  }
}"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{sample_json, *};

    #[test]
    fn test_extract_public_api() {
        let entries = extract_public_api(&sample_json()).expect("应能解析");
        // 顶层 7 类 + 递归子模块中的公开项
        let name_list: Vec<(&str, ApiKind)> = entries
            .iter()
            .map(|e| (e.english_name.as_str(), e.kind))
            .collect();
        assert!(name_list.contains(&("new", ApiKind::Function)));
        assert!(name_list.contains(&("Foo", ApiKind::Struct)));
        assert!(name_list.contains(&("State", ApiKind::Enum)));
        assert!(name_list.contains(&("Behavior", ApiKind::Trait)));
        assert!(name_list.contains(&("Count", ApiKind::TypeAlias)));
        assert!(name_list.contains(&("MAX", ApiKind::Const)));
        assert!(name_list.contains(&("print", ApiKind::Macro)));
        // re-export：名称在 inner.use.name，按 类型别名（type 名 = 来源路径）提取
        assert!(name_list.contains(&("Deserialize", ApiKind::TypeAlias)));
        let deser = entries
            .iter()
            .find(|e| e.english_name == "Deserialize")
            .unwrap();
        assert_eq!(
            deser.signature,
            "type Deserialize = serde_core::Deserialize"
        );
        // 模块 re-export 递归跟随（pub use crate::sub）
        assert!(name_list.contains(&("子函数", ApiKind::Function)));
        // glob 导入（pub use core::*）跳过、私有模块项跳过
        assert!(!name_list.contains(&("hidden_fn", ApiKind::Function)));
        // 私有模块（visibility=crate）不提取；impl 里的方法不提取
        assert!(!name_list.iter().any(|(name, _)| *name == "hidden_fn"));
        assert!(!name_list.iter().any(|(name, _)| *name == "实例方法"));
    }

    #[test]
    fn test_signature_rendering() {
        let entries = extract_public_api(&sample_json()).unwrap();
        let new_fn = entries.iter().find(|e| e.english_name == "new").unwrap();
        // async fn new<T>(x: u32) -> Result<T, String>
        assert_eq!(
            new_fn.signature,
            "async fn new<T>(x: u32) -> Result<T, String>"
        );
        let alias = entries.iter().find(|e| e.english_name == "Count").unwrap();
        assert_eq!(alias.signature, "type Count = u32");
        let constant = entries.iter().find(|e| e.english_name == "MAX").unwrap();
        assert_eq!(constant.signature, "const MAX: u32");
    }
}
