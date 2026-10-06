//! rustdoc JSON 解析：提取公开 API（仅名称与签名，绝不读取 docs 字段）
//! 并渲染为可读的类型签名。

use anyhow::anyhow;
use serde_json::{Map, Value};
use std::collections::HashSet;

use super::{接口条目, 接口种类};

/// 解析 rustdoc JSON，提取所有公开 API（仅名称与签名，绝不读取 docs 字段）
pub fn 提取公开接口(文本内容: &str) -> anyhow::Result<Vec<接口条目>> {
    Ok(提取公开接口含重导出(文本内容)?.0)
}

/// 同 [`提取公开接口`]，额外返回 glob 重导出（`pub use 依赖::*`）指向的
/// 依赖 crate 名列表：薄壳 crate（meta crate）的公开 API 全部来自这些重导出，
/// 调用方需对这些依赖 crate 单独生成 rustdoc JSON 并合并提取
pub fn 提取公开接口含重导出(
    文本内容: &str,
) -> anyhow::Result<(Vec<接口条目>, Vec<String>)> {
    let 文档: Value = serde_json::from_str(文本内容).map_err(|首错| {
        anyhow!(
            "{}",
            crate::ui::界面::全局().取文带参("mg_err_parse_rustdoc", &[&首错.to_string()])
        )
    })?;
    let 索引 = 文档
        .get("index")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("{}", crate::ui::界面::全局().取文("mg_err_no_index")))?;
    let 根 = 文档
        .get("root")
        .map(|每| {
            每.as_str()
                .map(String::from)
                .unwrap_or_else(|| 每.to_string())
        })
        .unwrap_or_else(|| "0".to_string());
    let mut 结果表 = Vec::new();
    let mut 重导出源表 = Vec::new();
    let mut 已访问集 = HashSet::new();
    收集模块项(索引, &根, &mut 已访问集, &mut 结果表, &mut 重导出源表);
    Ok((结果表, 重导出源表))
}

/// 深度优先收集模块树中的公开项（跳过 impl 方法、私有项、struct 字段等）
///
/// `重导出源表` 收集 glob 重导出指向的依赖 crate 名，供薄壳 crate 追踪。
fn 收集模块项(
    索引: &Map<String, Value>,
    标识: &str,
    已访问集: &mut HashSet<String>,
    结果表: &mut Vec<接口条目>,
    重导出源表: &mut Vec<String>,
) {
    if !已访问集.insert(标识.to_string()) {
        return;
    }
    let Some(项) = 索引.get(标识) else {
        return;
    };
    if 项.get("visibility").and_then(Value::as_str) != Some("public") {
        return;
    }
    let Some((键名, 内部)) = 项
        .get("inner")
        .and_then(Value::as_object)
        .and_then(|表| 表.iter().next())
    else {
        return;
    };
    let 名字 = 项
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match 键名.as_str() {
        "module" => {
            if let Some(项表) = 内部.get("items").and_then(Value::as_array) {
                for 子项 in 项表 {
                    收集模块项(索引, &子项.to_string(), 已访问集, 结果表, 重导出源表);
                }
            }
        }
        "function" => {
            结果表.push(接口条目 {
                种类: 接口种类::函数项,
                英文原名: 名字.clone(),
                类型签名: 渲染函数签名(&名字, 项),
            });
        }
        "struct" => {
            let 泛型参数 = 渲染泛型(内部.get("generics").unwrap_or(&Value::Null));
            结果表.push(接口条目 {
                种类: 接口种类::结构项,
                英文原名: 名字.clone(),
                类型签名: format!("struct {}{}", 名字, 泛型参数),
            });
        }
        "enum" => {
            let 泛型参数 = 渲染泛型(内部.get("generics").unwrap_or(&Value::Null));
            结果表.push(接口条目 {
                种类: 接口种类::枚举项,
                英文原名: 名字.clone(),
                类型签名: format!("enum {}{}", 名字, 泛型参数),
            });
        }
        "trait" => {
            let 泛型参数 = 渲染泛型(内部.get("generics").unwrap_or(&Value::Null));
            结果表.push(接口条目 {
                种类: 接口种类::特征项,
                英文原名: 名字.clone(),
                类型签名: format!("trait {}{}", 名字, 泛型参数),
            });
        }
        "type_alias" => {
            let 类型值 = 渲染类型(内部.get("type").unwrap_or(&Value::Null));
            结果表.push(接口条目 {
                种类: 接口种类::类型别名,
                英文原名: 名字.clone(),
                类型签名: format!("type {} = {}", 名字, 类型值),
            });
        }
        "constant" => {
            let 类型值 = 渲染类型(内部.get("type").unwrap_or(&Value::Null));
            结果表.push(接口条目 {
                种类: 接口种类::常量项,
                英文原名: 名字.clone(),
                类型签名: format!("const {}: {}", 名字, 类型值),
            });
        }
        // rustdoc JSON 当前工具链不输出 macro_rules! 宏（官方格式限制），预留支持
        "macro" => {
            结果表.push(接口条目 {
                种类: 接口种类::宏项,
                英文原名: 名字.clone(),
                类型签名: format!("{}!", 名字),
            });
        }
        // re-export（pub use）：薄壳 crate（如 serde 1.0.229 = serde_core 的转发层）
        // 的公开 API 全部是 re-export。名称在 inner.use.name（顶层 name 为 null）。
        "use" => {
            let 引用对象 = 内部; // 内部 即 use 对象（内部 单键解构结果）
            if 引用对象
                .get("is_glob")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                // glob 重导出（pub use 依赖::*）：外部 crate 的项不在本 JSON 的
                // index 中，无法枚举名称；记录 source 路径的首段（被重导出的
                // crate 名，如 `salvo_core::prelude` → `salvo_core`），由调用方
                // 对该依赖 crate 单独生成文档后合并提取（薄壳 crate 追踪）。
                // crate/self/super 开头的内部 glob 无法枚举，跳过。
                if let Some(来源) = 引用对象.get("source").and_then(Value::as_str) {
                    let 来源 = 来源.trim_start_matches("::");
                    let 依赖名 = 来源.split("::").next().unwrap_or("");
                    if !依赖名.is_empty()
                        && 依赖名 != "crate"
                        && 依赖名 != "self"
                        && 依赖名 != "super"
                    {
                        重导出源表.push(依赖名.to_string());
                    }
                }
                return;
            }
            let Some(英文名) = 引用对象.get("name").and_then(Value::as_str) else {
                return;
            };
            // 若指向本 crate 内的模块（pub use crate::模块），递归跟随收集其公开项
            if let Some(目标标识) = 引用对象.get("id").map(|每| 每.to_string())
                && 目标标识 != 标识
                && 索引.contains_key(&目标标识)
                && 索引[&目标标识]
                    .get("inner")
                    .and_then(|节点| 节点.get("module"))
                    .is_some()
            {
                收集模块项(索引, 目标标识.as_str(), 已访问集, 结果表, 重导出源表);
                return;
            }
            let 来源 = 引用对象
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("?");
            结果表.push(接口条目 {
                种类: 接口种类::类型别名,
                英文原名: 英文名.to_string(),
                类型签名: format!("type {} = {}", 英文名, 来源),
            });
        }
        // impl / import / struct_field / variant / assoc_type 等跳过
        _ => {}
    }
}

/// 渲染函数签名：`async fn name<T>(a: u32) -> Result<T>`
fn 渲染函数签名(名字: &str, 项: &Value) -> String {
    let 函数节点 = 项
        .get("inner")
        .and_then(|节点| 节点.get("function"))
        .unwrap_or(&Value::Null);
    let 签名节点 = 函数节点.get("sig").unwrap_or(&Value::Null);
    let 头部 = 函数节点.get("header").unwrap_or(&Value::Null);
    let mut 串 = String::new();
    if 头部
        .get("is_const")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        串.push_str("const ");
    }
    if 头部
        .get("is_unsafe")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        串.push_str("unsafe ");
    }
    if 头部
        .get("is_async")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        串.push_str("async ");
    }
    串.push_str("fn ");
    串.push_str(名字);
    串.push_str(&渲染泛型(
        函数节点.get("generics").unwrap_or(&Value::Null),
    ));
    串.push_str(&渲染签名进出(
        签名节点.get("inputs").unwrap_or(&Value::Null),
        签名节点.get("output"),
    ));
    串
}

/// 渲染参数列表与返回类型：`(a: u32, b: String) -> Result<T>`
fn 渲染签名进出(输入: &Value, 产出: Option<&Value>) -> String {
    let 参数表: Vec<String> = 输入
        .as_array()
        .map(|列表| {
            列表
                .iter()
                .map(|项| {
                    if let Some(对) = 项.as_array() {
                        let 名字 = 对.first().and_then(Value::as_str).unwrap_or("_");
                        let 类型值 = 对.get(1).unwrap_or(&Value::Null);
                        format!("{}: {}", 名字, 渲染类型(类型值))
                    } else {
                        渲染类型(项)
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let 返回类型 = 产出.map(渲染类型).filter(|串| 串 != "()");
    match 返回类型 {
        Some(返回类型) => format!("({}) -> {}", 参数表.join(", "), 返回类型),
        None => format!("({})", 参数表.join(", ")),
    }
}

/// 渲染泛型参数：`<T, U>` 或空串
fn 渲染泛型(泛型参数: &Value) -> String {
    let 参数表: Vec<String> = 泛型参数
        .get("params")
        .and_then(Value::as_array)
        .map(|列表| {
            列表
                .iter()
                .map(|参| {
                    let 名字 = 参.get("name").and_then(Value::as_str).unwrap_or("?");
                    if 参
                        .get("kind")
                        .and_then(|节点| 节点.get("lifetime"))
                        .is_some()
                    {
                        if 名字.starts_with('\'') {
                            名字.to_string()
                        } else {
                            format!("'{}'", 名字)
                        }
                    } else if 参.get("kind").and_then(|节点| 节点.get("const")).is_some() {
                        format!("const {}", 名字)
                    } else {
                        名字.to_string()
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    if 参数表.is_empty() {
        String::new()
    } else {
        format!("<{}>", 参数表.join(", "))
    }
}

/// 递归渲染 rustdoc 类型节点为可读签名（不支持的类型显示 `?`）
fn 渲染类型(类型值: &Value) -> String {
    if let Some(串) = 类型值.get("primitive").and_then(Value::as_str) {
        return if 串 == "unit" {
            "()".to_string()
        } else {
            串.to_string()
        };
    }
    if let Some(串) = 类型值.get("generic").and_then(Value::as_str) {
        return 串.to_string();
    }
    if let Some(路径节点) = 类型值.get("resolved_path") {
        let 名字 = 路径节点.get("path").and_then(Value::as_str).unwrap_or("?");
        if let Some(实参) = 路径节点
            .get("args")
            .and_then(|每| 每.get("angle_bracketed"))
        {
            let 参数表: Vec<String> = 实参
                .get("args")
                .and_then(Value::as_array)
                .map(|列表| {
                    列表
                        .iter()
                        .map(|项| {
                            if let Some(子类型) = 项.get("type") {
                                渲染类型(子类型)
                            } else if let Some(生命周期) = 项.get("lifetime") {
                                format!("'{}'", 生命周期.as_str().unwrap_or("?"))
                            } else {
                                "?".to_string()
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            return if 参数表.is_empty() {
                名字.to_string()
            } else {
                format!("{}<{}>", 名字, 参数表.join(", "))
            };
        }
        return 名字.to_string();
    }
    if let Some(借用节点) = 类型值.get("borrowed_ref") {
        let mut 串 = String::from("&");
        if let Some(生命周期) = 借用节点.get("lifetime").and_then(Value::as_str) {
            串.push('\'');
            串.push_str(生命周期);
            串.push(' ');
        }
        if 借用节点
            .get("mutable")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            串.push_str("mut ");
        }
        串.push_str(&渲染类型(借用节点.get("type").unwrap_or(&Value::Null)));
        return 串;
    }
    if let Some(元组) = 类型值.get("tuple") {
        let 元素表: Vec<String> = 元组
            .as_array()
            .map(|列表| 列表.iter().map(渲染类型).collect())
            .unwrap_or_default();
        return format!("({})", 元素表.join(", "));
    }
    if let Some(切片) = 类型值.get("slice") {
        return format!("[{}]", 渲染类型(切片.get("type").unwrap_or(&Value::Null)));
    }
    if let Some(数组节点) = 类型值.get("array") {
        let 数组长度 = 数组节点
            .get("len")
            .and_then(|每| 每.get("expr"))
            .and_then(Value::as_str)
            .unwrap_or("?");
        return format!(
            "[{}; {}]",
            渲染类型(数组节点.get("type").unwrap_or(&Value::Null)),
            数组长度
        );
    }
    if let Some(每节点) = 类型值.get("impl_trait") {
        return format!("impl {}", 渲染边界(每节点.get("bounds")));
    }
    if let Some(每节点) = 类型值.get("dyn_trait") {
        return format!("dyn {}", 渲染边界(每节点.get("bounds")));
    }
    if let Some(限定路径) = 类型值.get("qualified_path") {
        let 类型名 = 限定路径.get("name").and_then(Value::as_str).unwrap_or("?");
        return format!(
            "{}::{}",
            渲染类型(限定路径.get("self_type").unwrap_or(&Value::Null)),
            类型名
        );
    }
    if let Some(函数指针) = 类型值.get("function_pointer")
        && let Some(签名节点) = 函数指针.get("sig")
    {
        return format!(
            "fn{}",
            渲染签名进出(&签名节点["inputs"], 签名节点.get("output"))
        );
    }
    if let Some(裸指针) = 类型值.get("raw_pointer") {
        let 可变性 = 裸指针
            .get("mutable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let 星号 = if 可变性 { "*mut " } else { "*const " };
        return format!(
            "{}{}",
            星号,
            渲染类型(裸指针.get("type").unwrap_or(&Value::Null))
        );
    }
    "?".to_string()
}

/// 渲染 trait 边界列表：`A + B`
fn 渲染边界(边界: Option<&Value>) -> String {
    边界
        .and_then(Value::as_array)
        .map(|列表| {
            列表
                .iter()
                .map(|界| {
                    界.get("trait_bound")
                        .and_then(|每| 每.get("trait"))
                        .and_then(|每| 每.get("resolved_path"))
                        .and_then(|每| 每.get("path"))
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
pub(crate) fn 样例文本() -> String {
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
mod 单元测试 {
    use super::{样例文本, *};

    #[test]
    fn 测试提取公开接口() {
        let 条目表 = 提取公开接口(&样例文本()).expect("应能解析");
        // 顶层 7 类 + 递归子模块中的公开项
        let 名字清单: Vec<(&str, 接口种类)> = 条目表
            .iter()
            .map(|项| (项.英文原名.as_str(), 项.种类))
            .collect();
        assert!(名字清单.contains(&("new", 接口种类::函数项)));
        assert!(名字清单.contains(&("Foo", 接口种类::结构项)));
        assert!(名字清单.contains(&("State", 接口种类::枚举项)));
        assert!(名字清单.contains(&("Behavior", 接口种类::特征项)));
        assert!(名字清单.contains(&("Count", 接口种类::类型别名)));
        assert!(名字清单.contains(&("MAX", 接口种类::常量项)));
        assert!(名字清单.contains(&("print", 接口种类::宏项)));
        // re-export：名称在 inner.use.name，按 类型别名（type 名 = 来源路径）提取
        assert!(名字清单.contains(&("Deserialize", 接口种类::类型别名)));
        let 重导出项 = 条目表
            .iter()
            .find(|项| 项.英文原名 == "Deserialize")
            .unwrap();
        assert_eq!(
            重导出项.类型签名,
            "type Deserialize = serde_core::Deserialize"
        );
        // 模块 re-export 递归跟随（pub use crate::sub）
        assert!(名字清单.contains(&("子函数", 接口种类::函数项)));
        // glob 导入（pub use core::*）跳过、私有模块项跳过
        assert!(!名字清单.contains(&("hidden_fn", 接口种类::函数项)));
        // 私有模块（visibility=crate）不提取；impl 里的方法不提取
        assert!(!名字清单.iter().any(|(名字, _)| *名字 == "hidden_fn"));
        assert!(!名字清单.iter().any(|(名字, _)| *名字 == "实例方法"));
    }

    #[test]
    fn 测试签名渲染() {
        let 条目表 = 提取公开接口(&样例文本()).unwrap();
        let 新建函数 = 条目表.iter().find(|项| 项.英文原名 == "new").unwrap();
        // async fn new<T>(x: u32) -> Result<T, String>
        assert_eq!(
            新建函数.类型签名,
            "async fn new<T>(x: u32) -> Result<T, String>"
        );
        let 别名项 = 条目表.iter().find(|项| 项.英文原名 == "Count").unwrap();
        assert_eq!(别名项.类型签名, "type Count = u32");
        let 常量项 = 条目表.iter().find(|项| 项.英文原名 == "MAX").unwrap();
        assert_eq!(常量项.类型签名, "const MAX: u32");
    }
}
