//! rustdoc JSON 解析：提取公开接口（仅名称与签名，绝不读取文档字段）
//! 并渲染为可读的类型签名。

use serde_json::Value;
use std::collections::HashSet;

use super::{接口条目, 接口种类};

/// 解析 rustdoc JSON，提取所有公开接口（仅名称与签名，绝不读取文档字段）
use anyhow::anyhow;

pub fn 提取公开接口(文档文本: &str) -> anyhow::Result<Vec<接口条目>> {
    Ok(提取公开接口带通配源(文档文本)?.0)
}

/// 同 [`提取公开接口`]，额外返回整批重导出（`公开 使用 依赖::*`）指向的
/// 依赖包名列表：薄壳包（转发包）的公开接口全部来自这些重导出，
/// 调用方需对这些依赖包单独生成 rustdoc JSON 并合并提取
pub fn 提取公开接口带通配源(
    文档文本: &str,
) -> anyhow::Result<(Vec<接口条目>, Vec<String>)> {
    let 界面表 = crate::界面::界面消息表::全局表();
    let 文档: Value = serde_json::from_str(文档文本).map_err(|错误值| {
        anyhow!(
            "{}",
            界面表.取带参消息("mg_err_parse_rustdoc", &[&错误值.to_string()])
        )
    })?;
    let 索引 = 文档
        .get("index")
        .filter(|值| 值.is_object())
        .ok_or_else(|| anyhow!("{}", 界面表.取消息("mg_err_no_index")))?;
    let 根标识 = 文档
        .get("root")
        .map(|值| {
            值.as_str()
                .map(String::from)
                .unwrap_or_else(|| 值.to_string())
        })
        .unwrap_or_else(|| "0".to_string());
    let mut 结果表 = Vec::new();
    let mut 通配源表 = Vec::new();
    let mut 已访问 = HashSet::new();
    收集模块条目(索引, &根标识, &mut 已访问, &mut 结果表, &mut 通配源表);
    Ok((结果表, 通配源表))
}

/// 深度优先收集模块树中的公开项（跳过实现方法、私有项、结构体字段等）
///
/// 通配源表收集整批重导出指向的依赖包名，供薄壳包追踪。
fn 收集模块条目(
    索引: &Value,
    标识: &str,
    已访问: &mut HashSet<String>,
    结果表: &mut Vec<接口条目>,
    通配源表: &mut Vec<String>,
) {
    if !已访问.insert(标识.to_string()) {
        return;
    }
    let Some(条目值) = 索引.get(标识) else {
        return;
    };
    if 条目值.get("visibility").and_then(Value::as_str) != Some("public") {
        return;
    }
    let Some((类别键, 内部值)) = 条目值
        .get("inner")
        .and_then(Value::as_object)
        .and_then(|对象值| 对象值.iter().next())
    else {
        return;
    };
    let 名称 = 条目值
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match 类别键.as_str() {
        "module" => {
            if let Some(子项表) = 内部值.get("items").and_then(Value::as_array) {
                for 子项 in 子项表 {
                    收集模块条目(索引, &子项.to_string(), 已访问, 结果表, 通配源表);
                }
            }
        }
        "function" => {
            结果表.push(接口条目 {
                种类: 接口种类::函数项,
                英文名: 名称.clone(),
                签名: 渲染函数签名(&名称, 条目值),
            });
        }
        "struct" => {
            let 泛型文本 = 渲染泛型(内部值.get("generics").unwrap_or(&Value::Null));
            结果表.push(接口条目 {
                种类: 接口种类::结构体项,
                英文名: 名称.clone(),
                签名: format!("struct {}{}", 名称, 泛型文本),
            });
        }
        "enum" => {
            let 泛型文本 = 渲染泛型(内部值.get("generics").unwrap_or(&Value::Null));
            结果表.push(接口条目 {
                种类: 接口种类::枚举项,
                英文名: 名称.clone(),
                签名: format!("enum {}{}", 名称, 泛型文本),
            });
        }
        "trait" => {
            let 泛型文本 = 渲染泛型(内部值.get("generics").unwrap_or(&Value::Null));
            结果表.push(接口条目 {
                种类: 接口种类::特征项,
                英文名: 名称.clone(),
                签名: format!("trait {}{}", 名称, 泛型文本),
            });
        }
        "type_alias" => {
            let 类型文本 = 渲染类型(内部值.get("type").unwrap_or(&Value::Null));
            结果表.push(接口条目 {
                种类: 接口种类::类型别名项,
                英文名: 名称.clone(),
                签名: format!("type {} = {}", 名称, 类型文本),
            });
        }
        "constant" => {
            let 类型文本 = 渲染类型(内部值.get("type").unwrap_or(&Value::Null));
            结果表.push(接口条目 {
                种类: 接口种类::常量项,
                英文名: 名称.clone(),
                签名: format!("const {}: {}", 名称, 类型文本),
            });
        }
        // rustdoc JSON 当前工具链不输出声明式宏（官方格式限制），预留支持
        "macro" => {
            结果表.push(接口条目 {
                种类: 接口种类::宏项,
                英文名: 名称.clone(),
                签名: format!("{}!", 名称),
            });
        }
        // 重导出（公开 使用）：薄壳包的公开接口全部是重导出。
        // 名称在内部值的名称字段（顶层名称为空）。
        "use" => {
            let 使用对象 = 内部值; // 内部值即使用对象（单键解构结果）
            if 使用对象
                .get("is_glob")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                // 整批重导出（公开 使用 依赖::*）：外部包的项不在本 JSON 的
                // 索引里，无法枚举名称；记录来源路径的首段（被重导出的
                // 包名），由调用方对该依赖包单独生成文档后合并提取（薄壳包追踪）。
                // 以 包、自身、超级 开头的内部整批重导出无法枚举，跳过。
                if let Some(来源) = 使用对象.get("source").and_then(Value::as_str) {
                    let 来源 = 来源.trim_start_matches("::");
                    let 依赖名 = 来源.split("::").next().unwrap_or("");
                    if !依赖名.is_empty()
                        && 依赖名 != "crate"
                        && 依赖名 != "self"
                        && 依赖名 != "super"
                    {
                        通配源表.push(依赖名.to_string());
                    }
                }
                return;
            }
            let Some(英文名) = 使用对象.get("name").and_then(Value::as_str) else {
                return;
            };
            // 若指向本包内的模块（公开 使用 包::模块），递归跟随收集其公开项
            if let Some(目标标识) = 使用对象.get("id").map(|值| 值.to_string())
                && 目标标识 != 标识
                && 索引.get(目标标识.as_str()).is_some()
                && 索引
                    .get(目标标识.as_str())
                    .and_then(|节点值| 节点值.get("inner"))
                    .and_then(|内部项| 内部项.get("module"))
                    .is_some()
            {
                收集模块条目(索引, 目标标识.as_str(), 已访问, 结果表, 通配源表);
                return;
            }
            let 来源 = 使用对象
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("?");
            结果表.push(接口条目 {
                种类: 接口种类::类型别名项,
                英文名: 英文名.to_string(),
                签名: format!("type {} = {}", 英文名, 来源),
            });
        }
        // 实现块 / 导入 / 结构体字段 / 枚举变体 / 关联类型等跳过
        _ => {}
    }
}

/// 渲染函数签名：`async fn 名称<T>(甲: u32) -> Result<T>`
fn 渲染函数签名(名称: &str, 条目值: &Value) -> String {
    let 函数值 = 条目值
        .get("inner")
        .and_then(|内部项| 内部项.get("function"))
        .unwrap_or(&Value::Null);
    let 签名值 = 函数值.get("sig").unwrap_or(&Value::Null);
    let 头部 = 函数值.get("header").unwrap_or(&Value::Null);
    let mut 签名文本 = String::new();
    if 头部
        .get("is_const")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        签名文本.push_str("const ");
    }
    if 头部
        .get("is_unsafe")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        签名文本.push_str("unsafe ");
    }
    if 头部
        .get("is_async")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        签名文本.push_str("async ");
    }
    签名文本.push_str("fn ");
    签名文本.push_str(名称);
    签名文本.push_str(&渲染泛型(
        函数值.get("generics").unwrap_or(&Value::Null),
    ));
    签名文本.push_str(&渲染入出参(
        签名值.get("inputs").unwrap_or(&Value::Null),
        签名值.get("output"),
    ));
    签名文本
}

/// 渲染参数列表与返回类型：`(甲: u32, 乙: String) -> Result<T>`
fn 渲染入出参(输入表: &Value, 输出值: Option<&Value>) -> String {
    let 参数表: Vec<String> = 输入表
        .as_array()
        .map(|列表| {
            列表
                .iter()
                .map(|元素| {
                    if let Some(成对项) = 元素.as_array() {
                        let 名称 = 成对项.first().and_then(Value::as_str).unwrap_or("_");
                        let 类型值 = 成对项.get(1).unwrap_or(&Value::Null);
                        format!("{}: {}", 名称, 渲染类型(类型值))
                    } else {
                        渲染类型(元素)
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let 返回文本 = 输出值.map(渲染类型).filter(|文本值| 文本值 != "()");
    match 返回文本 {
        Some(返回项) => format!("({}) -> {}", 参数表.join(", "), 返回项),
        None => format!("({})", 参数表.join(", ")),
    }
}

/// 渲染泛型参数：`<T, U>` 或空串
fn 渲染泛型(泛型值: &Value) -> String {
    let 参数表: Vec<String> = 泛型值
        .get("params")
        .and_then(Value::as_array)
        .map(|列表| {
            列表
                .iter()
                .map(|单项| {
                    let 名称 = 单项.get("name").and_then(Value::as_str).unwrap_or("?");
                    if 单项
                        .get("kind")
                        .and_then(|类值| 类值.get("lifetime"))
                        .is_some()
                    {
                        if 名称.starts_with('\'') {
                            名称.to_string()
                        } else {
                            format!("'{}'", 名称)
                        }
                    } else if 单项
                        .get("kind")
                        .and_then(|类值| 类值.get("const"))
                        .is_some()
                    {
                        format!("const {}", 名称)
                    } else {
                        名称.to_string()
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
    if let Some(原语名) = 类型值.get("primitive").and_then(Value::as_str) {
        return if 原语名 == "unit" {
            "()".to_string()
        } else {
            原语名.to_string()
        };
    }
    if let Some(泛型名) = 类型值.get("generic").and_then(Value::as_str) {
        return 泛型名.to_string();
    }
    if let Some(解引用路径) = 类型值.get("resolved_path") {
        let 名称 = 解引用路径
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("?");
        if let Some(尖括号参数) = 解引用路径
            .get("args")
            .and_then(|参数值| 参数值.get("angle_bracketed"))
        {
            let 参数表: Vec<String> = 尖括号参数
                .get("args")
                .and_then(Value::as_array)
                .map(|列表| {
                    列表
                        .iter()
                        .map(|元素| {
                            if let Some(类型节点) = 元素.get("type") {
                                渲染类型(类型节点)
                            } else if let Some(寿命节点) = 元素.get("lifetime") {
                                format!("'{}'", 寿命节点.as_str().unwrap_or("?"))
                            } else {
                                "?".to_string()
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            return if 参数表.is_empty() {
                名称.to_string()
            } else {
                format!("{}<{}>", 名称, 参数表.join(", "))
            };
        }
        return 名称.to_string();
    }
    if let Some(借用引用) = 类型值.get("borrowed_ref") {
        let mut 签名文本 = String::from("&");
        if let Some(寿命值) = 借用引用.get("lifetime").and_then(Value::as_str) {
            签名文本.push('\'');
            签名文本.push_str(寿命值);
            签名文本.push(' ');
        }
        if 借用引用
            .get("mutable")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            签名文本.push_str("mut ");
        }
        签名文本.push_str(&渲染类型(借用引用.get("type").unwrap_or(&Value::Null)));
        return 签名文本;
    }
    if let Some(元组值) = 类型值.get("tuple") {
        let 元素表: Vec<String> = 元组值
            .as_array()
            .map(|列表| 列表.iter().map(渲染类型).collect())
            .unwrap_or_default();
        return format!("({})", 元素表.join(", "));
    }
    if let Some(切片值) = 类型值.get("slice") {
        return format!("[{}]", 渲染类型(切片值.get("type").unwrap_or(&Value::Null)));
    }
    if let Some(数组值) = 类型值.get("array") {
        let 长度文本 = 数组值
            .get("len")
            .and_then(|长度值| 长度值.get("expr"))
            .and_then(Value::as_str)
            .unwrap_or("?");
        return format!(
            "[{}; {}]",
            渲染类型(数组值.get("type").unwrap_or(&Value::Null)),
            长度文本
        );
    }
    if let Some(实现特征) = 类型值.get("impl_trait") {
        return format!("impl {}", 渲染边界(实现特征.get("bounds")));
    }
    if let Some(动态特征) = 类型值.get("dyn_trait") {
        return format!("dyn {}", 渲染边界(动态特征.get("bounds")));
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
        && let Some(签名段) = 函数指针.get("sig")
    {
        return format!("fn{}", 渲染入出参(&签名段["inputs"], 签名段.get("output")));
    }
    if let Some(裸指针) = 类型值.get("raw_pointer") {
        let 指针可变 = 裸指针
            .get("mutable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let 前缀符号 = if 指针可变 { "*mut " } else { "*const " };
        return format!(
            "{}{}",
            前缀符号,
            渲染类型(裸指针.get("type").unwrap_or(&Value::Null))
        );
    }
    "?".to_string()
}

/// 渲染特征边界列表：`A + B`
fn 渲染边界(边界值: Option<&Value>) -> String {
    边界值
        .and_then(Value::as_array)
        .map(|列表| {
            列表
                .iter()
                .map(|边界项| {
                    边界项
                        .get("trait_bound")
                        .and_then(|约束项| 约束项.get("trait"))
                        .and_then(|特征值| 特征值.get("resolved_path"))
                        .and_then(|路径值| 路径值.get("path"))
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join(" + ")
        })
        .unwrap_or_else(|| "?".to_string())
}

/// 迷你 rustdoc JSON 样本（结构与真实输出一致：索引/根/内部节点等）。
/// 供本模块与映射输出、文档结构的测试共用。
#[cfg(test)]
pub(crate) fn 示例文档() -> String {
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
mod 母语测试 {
    use super::{示例文档, *};

    #[test]
    fn 提取公开接口正确() {
        let 条目表 = 提取公开接口(&示例文档()).expect("应能解析");
        // 顶层七类 + 递归子模块中的公开项
        let 名称清单: Vec<(&str, 接口种类)> = 条目表
            .iter()
            .map(|条目| (条目.英文名.as_str(), 条目.种类))
            .collect();
        assert!(名称清单.contains(&("new", 接口种类::函数项)));
        assert!(名称清单.contains(&("Foo", 接口种类::结构体项)));
        assert!(名称清单.contains(&("State", 接口种类::枚举项)));
        assert!(名称清单.contains(&("Behavior", 接口种类::特征项)));
        assert!(名称清单.contains(&("Count", 接口种类::类型别名项)));
        assert!(名称清单.contains(&("MAX", 接口种类::常量项)));
        assert!(名称清单.contains(&("print", 接口种类::宏项)));
        // 重导出：名称在内部值里，按类型别名（类型名 = 来源路径）提取
        assert!(名称清单.contains(&("Deserialize", 接口种类::类型别名项)));
        let 反序列化条目 = 条目表
            .iter()
            .find(|条目| 条目.英文名 == "Deserialize")
            .unwrap();
        assert_eq!(
            反序列化条目.签名,
            "type Deserialize = serde_core::Deserialize"
        );
        // 模块重导出递归跟随（公开 使用 包::子模块）
        assert!(名称清单.contains(&("子函数", 接口种类::函数项)));
        // 整批导入（公开 使用 内核::*）跳过、私有模块项跳过
        assert!(!名称清单.contains(&("hidden_fn", 接口种类::函数项)));
        // 私有模块（可见性为包内）不提取；实现块里的方法不提取
        assert!(!名称清单.iter().any(|(名字, _)| *名字 == "hidden_fn"));
        assert!(!名称清单.iter().any(|(名字, _)| *名字 == "实例方法"));
    }

    #[test]
    fn 签名渲染() {
        let 条目表 = 提取公开接口(&示例文档()).unwrap();
        let 新建条目 = 条目表.iter().find(|条目| 条目.英文名 == "new").unwrap();
        // 异步函数 新建<T>(甲: 无符号整数) -> 结果<T, 字符串>
        assert_eq!(
            新建条目.签名,
            "async fn new<T>(x: u32) -> Result<T, String>"
        );
        let 别名条目 = 条目表.iter().find(|条目| 条目.英文名 == "Count").unwrap();
        assert_eq!(别名条目.签名, "type Count = u32");
        let 常量条目 = 条目表.iter().find(|条目| 条目.英文名 == "MAX").unwrap();
        assert_eq!(常量条目.签名, "const MAX: u32");
    }
}
