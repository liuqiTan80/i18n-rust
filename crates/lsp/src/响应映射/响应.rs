//! 各 LSP 请求类型的响应映射：诊断、补全、悬停、签名帮助、
//! 引用/定义、重命名、代码操作、文档符号与教学动作注入。

use std::sync::Arc;

use serde_json::Value;

use super::响应映射器;
use super::诊断文本::{
    提取所有权详情, 是主函数提示, 是未打开模块引用, 是缺依赖噪音, 是缺内嵌资源, 是缺项目依赖,
    查找期望实得标签, 翻译诊断消息,
};
use super::还原单行;
use crate::翻译缓存::英文列转母语列;

impl 响应映射器 {
    /// 映射 rust-analyzer 的 发布诊断通知
    ///
    /// 将诊断信息中的资源标识和位置还原为原始 .zh 文件，
    /// 并尝试翻译诊断消息为中文。
    pub fn 映射诊断(&self, 参数值: &Value) -> Value {
        let 虚拟资源标识 = 参数值["uri"].as_str().unwrap_or("");
        // 预取条目一次：所有诊断的范围值/关联信息映射基于同一
        // 文档，循环内复用条目走无锁纯函数（诊断通常 10-50 条，避免每条
        // 范围值 × 2 锁、每条关联信息 × 4 锁）
        let 条目值 = self
            .缓存
            .按虚拟资源标识查询(虚拟资源标识)
            .or_else(|| self.缓存.查询原始(虚拟资源标识));
        let 原始资源标识 = match &条目值 {
            Some(条目) => 条目.原始资源标识.clone(),
            None => 虚拟资源标识.to_string(),
        };
        let 诊断列表 = 参数值["diagnostics"].as_array();

        let mut 映射后诊断 = Vec::new();

        if let Some(诊断数组) = 诊断列表 {
            for 诊断值 in 诊断数组 {
                // 过滤虚拟项目主程序文件中关于主函数的提示级提示
                // （主函数在模块内不是真正的入口，rust-analyzer 会发出
                // “此处是名为主函数的函数”等教学无关的提示）
                if 是主函数提示(诊断值, 虚拟资源标识) {
                    continue;
                }
                // 过滤虚拟项目固有的过程宏误报（#3 症状二）：虚拟项目无第三方
                // 依赖且禁用过程宏，命令行解析库等派生未展开时的辅助属性
                // （`#[arg(...)]`）与派生宏名字解析必然失败——用户在项目中
                // 构建正常（rzc 检查通过），此类诊断恒为误报
                if 是缺依赖噪音(诊断值) {
                    continue;
                }
                // 过滤“引用未打开模块文件”的 E0433/E0432 误报：虚拟项目只
                // 聚合已打开文件，被引用模块未打开时无法解析（打开后即恢复）；
                // 同名方言文件存在于同目录时判定为误报（覆盖分析器新旧消息格式）
                if 是未打开模块引用(诊断值, 条目值.as_deref()) {
                    continue;
                }
                // 过滤“真实依赖在虚拟项目缺失”的误报：虚拟项目 Cargo.toml
                // 不含用户依赖，serde/serde_json 等已声明依赖的导入必然无法
                // 解析（用户项目中 rzc 检查正常通过）
                if 是缺项目依赖(诊断值, 条目值.as_deref()) {
                    continue;
                }
                // 过滤 内嵌字符串与内嵌字节资源在虚拟项目中缺失的
                // 误报：资源文件在原方言文件同目录存在时，虚拟 .rs 的相对
                // 路径解析失败属虚拟项目固有现象
                if 是缺内嵌资源(诊断值, 条目值.as_deref()) {
                    continue;
                }

                let mut 映射项 = 诊断值.clone();

                // 映射范围（使用列映射）
                if 诊断值.get("range").is_some() {
                    映射项["range"] =
                        Self::带条目还原范围(条目值.as_deref(), &诊断值["range"]);
                }

                // 映射关联信息中的位置
                if let Some(关联信息) = 诊断值
                    .get("relatedInformation")
                    .and_then(|值| 值.as_array())
                {
                    let mut 映射后关联 = Vec::new();
                    for 项 in 关联信息 {
                        let mut 映射后项 = 项.clone();
                        if let Some(位置值) = 项.get("location") {
                            // 同文件位置复用主条目（跨文件才按需查询）
                            let 位置资源标识 = 位置值["uri"].as_str().unwrap_or("");
                            let 位置条目 = if 位置资源标识 == 虚拟资源标识 {
                                条目值.clone()
                            } else {
                                self.缓存
                                    .按虚拟资源标识查询(位置资源标识)
                                    .or_else(|| self.缓存.查询原始(位置资源标识))
                            };
                            映射后项["location"] = Self::带条目还原位置(
                                位置条目.as_deref(),
                                位置资源标识,
                                位置值,
                            );
                        }
                        // 子消息（help/note）同样翻译——悬停查看诊断详情时
                        // 不泄漏英文（如 "value moved here"、"consider ..."）；
                        // 子消息无独立错误码/标签，走消息表/短语兜底
                        if let Some(消息) = 项.get("message").and_then(|值| 值.as_str()) {
                            映射后项["message"] =
                                Value::String(翻译诊断消息(None, 消息, None));
                        }
                        映射后关联.push(映射后项);
                    }
                    映射项["relatedInformation"] = Value::Array(映射后关联);
                }

                // 翻译诊断主消息（与 CLI 同口径）：传入错误码供码表优先命中；
                // rust-analyzer 把 expected/found 放在关联信息而非主
                // 跨度标签，故从关联信息归集候选标签回填 {期望}/{实际}（无则自动
                // 回退消息表）
                let 错误码串 = 诊断值.get("code").and_then(|值| 值.as_str());
                let 主标签 = 查找期望实得标签(诊断值);
                映射项["message"] = Value::String(翻译诊断消息(
                    错误码串,
                    诊断值["message"].as_str().unwrap_or(""),
                    主标签.as_deref(),
                ));

                // 所有权错误：提取叙事化详情并存入数据字段（供 VS Code 扩展可视化）
                if let Some(详情) = 提取所有权详情(诊断值, &映射项, &原始资源标识)
                    && let Ok(详情值) = serde_json::to_value(&详情)
                {
                    // 保留 rust-analyzer 已有的数据（如代码操作数据），嵌套存入
                    match 映射项.get_mut("data") {
                        Some(既有值) if 既有值.is_object() => {
                            既有值["所有权详情"] = 详情值;
                        }
                        _ => {
                            映射项["data"] = 详情值;
                        }
                    }
                }

                // 添加教学提示标记
                映射项["source"] = Value::String("i18n-rust".to_string());

                映射后诊断.push(映射项);
            }
        }

        serde_json::json!({
            "uri": 原始资源标识,
            "diagnostics": 映射后诊断,
            "version": 参数值.get("version").cloned().unwrap_or(Value::Null)
        })
    }

    /// 映射补全响应中的位置信息
    ///
    /// 将文本编辑/附加文本编辑中的范围映射回原始文件，
    /// 并将英文标识符/代码反向翻译为母语（否则接受补全会把
    /// 英文关键字插入母语源文件，或自动导入编辑落在错误位置）。
    pub fn 映射补全响应(&self, 响应值: &Value, 原始资源标识: &str) -> Value {
        let mut 结果值 = 响应值.clone();

        if let Some(项列表) = 结果值.get("items").and_then(|值| 值.as_array()) {
            let mut 映射后项 = Vec::new();
            // 语言过滤白名单（用户源码中出现过的标识符）懒加载，
            // 仅在确实遇到未翻译的纯英文项时才扫描一次；
            // 引用计数共享避免每次补全请求克隆整个集合
            let mut 用户词汇: Option<Arc<std::collections::HashSet<String>>> = None;
            for 项 in 项列表 {
                let mut 映射项 = 项.clone();

                // 1. 映射文本编辑的范围并反向翻译新文本
                if let Some(文本编辑) = 项.get("textEdit") {
                    if let Some(范围值) = 文本编辑.get("range") {
                        映射项["textEdit"]["range"] = self.还原范围(原始资源标识, 范围值);
                    }
                    if let Some(新文本) = 文本编辑.get("newText").and_then(|值| 值.as_str())
                    {
                        映射项["textEdit"]["newText"] = Value::String(self.翻译代码(新文本));
                    }
                }

                // 2. 附加文本编辑（如自动导入）：位置与内容同样需要还原
                if let Some(附加编辑) = 项.get("additionalTextEdits") {
                    映射项["additionalTextEdits"] =
                        self.映射编辑列表(附加编辑, 原始资源标识, true);
                }

                // 3. 标签：先精确反查（关键字等），未命中则词法级转译
                //    （标准库接口如向量新建、打印宏也能还原为母语）
                if let Some(标签) = 项.get("label").and_then(|值| 值.as_str()) {
                    let 映射后标签 = self.反向查找(标签).unwrap_or_else(|| self.翻译代码(标签));

                    // 3.5 语言过滤（禁止串语言）：非英文方言下，补全列表
                    //     只保留母语项，过滤未翻译的外部英文项（第三方库、
                    //     未收录的标准库 API 等）。保留条件（满足其一）：
                    //     a. 翻译命中（映射后标签与原文不同）；
                    //     b. 标签含母语字符（用户定义的母语标识符）；
                    //     c. 标签的末段标识符在用户源码中出现过
                    //        （用户自己定义的项，含英文命名）。
                    if self.严格母语过滤 {
                        let 含母语字符 = !映射后标签.is_ascii();
                        let 已翻译 = 映射后标签 != 标签;
                        let 用户定义 = if 含母语字符 || 已翻译 {
                            true
                        } else {
                            let 词汇集 = 用户词汇.get_or_insert_with(|| self.缓存.用户定义词汇());
                            标签标识后缀(标签).is_some_and(|名称| 词汇集.contains(名称))
                        };
                        if !(含母语字符 || 已翻译 || 用户定义) {
                            continue;
                        }
                    }

                    映射项["label"] = Value::String(映射后标签);
                }

                // 4. 详情（类型签名）：词法级转译，如英文签名转成中文签名
                if let Some(详情) = 项.get("detail").and_then(|值| 值.as_str()) {
                    映射项["detail"] = Value::String(self.翻译代码(详情));
                }

                // 4.5 标签详情：VS Code 提示框右侧优先显示此字段
                //     （描述为签名、详情为包/模块路径），
                //     不还原会把英文签名泄漏给母语用户
                if let Some(标签详情) = 项.get("labelDetails").and_then(|值| 值.as_object())
                {
                    let mut 映射后详情 = 标签详情.clone();
                    if let Some(描述) = 标签详情.get("description").and_then(|值| 值.as_str())
                    {
                        映射后详情.insert(
                            "description".to_string(),
                            Value::String(self.翻译代码(描述)),
                        );
                    }
                    if let Some(详情) = 标签详情.get("detail").and_then(|值| 值.as_str()) {
                        映射后详情.insert("detail".to_string(), Value::String(self.翻译代码(详情)));
                    }
                    映射项["labelDetails"] = Value::Object(映射后详情);
                }

                // 5. 文档：命中解释表（大白话）时替换为中文，
                //    未命中保留英文原文（避免丢失签名等关键信息）
                if let Some(文档值) = 项.get("documentation") {
                    映射项["documentation"] = self.翻译补全文档(项, 文档值);
                }

                // 6. 反向映射插入文本（可能含代码片段占位符，仅精确匹配时替换）
                if let Some(插入文本) = 项.get("insertText").and_then(|值| 值.as_str())
                    && let Some(母语名) = self.反向查找(插入文本)
                {
                    映射项["insertText"] = Value::String(母语名);
                }

                // 7. 方法/函数补全补括号：rust-analyzer 的代码片段配置在代理
                //    环境不可靠（方法补全默认不带括号），这里对方法/函数类
                //    补全项在文本编辑末尾补代码片段括号，光标自动落在括号内
                //    （教学常用场景如 `长度()`）；已带括号（含参数占位）跳过。
                let 种类 = 项.get("kind").and_then(|值| 值.as_i64()).unwrap_or(0);
                if matches!(种类, 2 | 3)
                    && let Some(文本串) = 映射项["textEdit"]["newText"].as_str()
                    && !文本串.contains('(')
                {
                    映射项["textEdit"]["newText"] = Value::String(format!("{}(${{1:}})", 文本串));
                    // 2 = 代码片段格式：占位符由客户端解析，光标落在括号内
                    映射项["insertTextFormat"] = Value::Number(2.into());
                }

                // 8. 关键字补全前导空格：rust-analyzer 的“让 可变”组合代码片段
                //    在代理环境不可用，散落的关键字项（如 `可变`）直接插入会与
                //    前一标识符粘连（`让可变`）。判断新文本的首个标识符是否
                //    为关键字映射表中的词（不依赖种类，rust-analyzer 的关键字
                //    补全种类不可靠），且前一字符是标识符时在文本编辑前补空格。
                if let Some(文本串) = 映射项["textEdit"]["newText"].as_str() {
                    let 首个词 = 文本串
                        .split(|字符值: char| 字符值.is_whitespace() || 字符值 == '$')
                        .next()
                        .unwrap_or("");
                    if !首个词.is_empty() && self.缓存.关键词映射().contains_key(首个词)
                    {
                        let 起始位置 = 映射项["textEdit"]["range"]["start"].clone();
                        if let (Some(行号), Some(列号)) = (
                            起始位置["line"].as_u64().map(|值| 值 as usize),
                            起始位置["character"].as_u64().map(|值| 值 as usize),
                        ) && 列号 > 0
                            && let Some(条目) = self.缓存.查询原始(原始资源标识)
                            && let Some(行文本) = 条目.母语内容.lines().nth(行号)
                        {
                            let 前字符 = 行文本.chars().nth(列号 - 1);
                            let 需要空格 = 前字符.is_some_and(|字符值| {
                                !字符值.is_whitespace()
                                    && !matches!(
                                        字符值,
                                        '(' | '.' | ':' | ',' | ';' | '{' | '[' | '!'
                                    )
                            });
                            if 需要空格 && !文本串.starts_with(char::is_whitespace) {
                                映射项["textEdit"]["newText"] = Value::String(format!(" {文本串}"));
                            }
                        }
                    }
                }

                映射后项.push(映射项);
            }
            结果值["items"] = Value::Array(映射后项);
        }

        结果值
    }

    /// 补全文档：优先用中文解释表替换英文文档
    ///
    /// 查键顺序：从文档解析类型::方法（如选项的解包）→ 标签直查
    /// → 方法名兜底；全部未命中时保留英文原文。
    fn 翻译补全文档(&self, 项: &Value, 文档值: &Value) -> Value {
        let 文档文本 = match 文档值 {
            Value::String(串) => Some(串.as_str()),
            Value::Object(对象) => 对象.get("value").and_then(|值| 值.as_str()),
            _ => None,
        };
        if let Some(文本串) = 文档文本
            && let Some(解释) = self.查找解释(文本串)
        {
            return Value::String(解释);
        }
        // 标签直查 + 方法名兜底
        let 界面 = crate::界面消息::全局界面();
        if let Some(标签) = 项.get("label").and_then(|值| 值.as_str())
            && let Some(解释) = 界面.取解释(标签)
        {
            return Value::String(解释.to_string());
        }
        if let Some(文本串) = 文档文本
            && let Some(名称) = 提取函数名(文本串)
            && let Some(解释) = 界面.取解释(&名称)
        {
            return Value::String(解释.to_string());
        }
        文档值.clone()
    }

    /// 映射文档高亮响应（文档高亮数组：范围值 + 种类）
    ///
    /// 请求方向已把位置转为虚拟文件坐标，响应必须还原，
    /// 否则高亮位置以虚拟文件坐标泄漏给客户端。
    pub fn 映射文档高亮响应(&self, 响应值: &Value, 原始资源标识: &str) -> Value {
        match 响应值 {
            Value::Array(项集) => {
                let 映射后: Vec<Value> = 项集
                    .iter()
                    .map(|项| {
                        let mut 映射项 = 项.clone();
                        if let Some(范围值) = 项.get("range") {
                            映射项["range"] = self.还原范围(原始资源标识, 范围值);
                        }
                        映射项
                    })
                    .collect();
                Value::Array(映射后)
            }
            Value::Null => Value::Array(Vec::new()),
            _ => 响应值.clone(),
        }
    }

    /// 映射语义着色响应（semanticTokens/full、semanticTokens/range）
    ///
    /// rust-analyzer 返回的词法记号坐标基于虚拟 .rs（转译产物），
    /// 必须还原到方言文件坐标，否则变量/参数等颜色落在错误位置
    /// 或完全不显示。数据字段为 LSP 增量编码（每 5 项一组）：
    /// `[deltaLine, deltaStart, length, tokenType, tokenModifiers]`——
    /// 先还原为绝对坐标，逐词法记号映射起点与终点列，再重新增量编码。
    /// 结果标识原样透传（客户端依赖它做增量请求）。
    pub fn 映射语义记号响应(&self, 响应值: &Value, 原始资源标识: &str) -> Value {
        let mut 结果值 = 响应值.clone();
        let Some(数据) = 结果值.get("data").and_then(|值| 值.as_array()) else {
            return 结果值;
        };

        // 预取翻译条目：每个词法记号的列映射都基于同一文档，
        // 只查一次（常数时间索引），避免每个词法记号重复锁与表扫描
        let 条目值 = self
            .缓存
            .按虚拟资源标识查询(原始资源标识)
            .or_else(|| self.缓存.查询原始(原始资源标识));
        // 无条目（异常资源标识）时位置原样透传
        let 映射位置 = |行号: u32, 列号: u32| -> (u32, u32) {
            match &条目值 {
                Some(条目) => {
                    let 母语行 = 还原单行(条目, 行号);
                    let 母语列 = 英文列转母语列(条目, 行号, 列号);
                    (母语行, 母语列)
                }
                None => (行号, 列号),
            }
        };

        // 1. 增量编码 → 绝对坐标（跨行时列归零重置，同行时列累加）
        let mut 记号集: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
        let mut 行号 = 0u32;
        let mut 列号 = 0u32;
        for 分块值 in 数据.chunks(5) {
            if 分块值.len() < 5 {
                break;
            }
            let 增量行 = 分块值[0].as_u64().unwrap_or(0) as u32;
            let 增量起列 = 分块值[1].as_u64().unwrap_or(0) as u32;
            let 词法长 = 分块值[2].as_u64().unwrap_or(0) as u32;
            let 词法类型 = 分块值[3].as_u64().unwrap_or(0) as u32;
            let 修饰值 = 分块值[4].as_u64().unwrap_or(0) as u32;
            行号 = 行号.saturating_add(增量行);
            列号 = if 增量行 == 0 {
                列号.saturating_add(增量起列)
            } else {
                增量起列
            };
            记号集.push((行号, 列号, 词法长, 词法类型, 修饰值));
        }

        // 2. 起点/终点列分别还原到方言坐标，长度取映射后的差值
        //（关键字替换改变了列宽，长度必须重算）
        // 修饰符强制清零：rust-analyzer 对同一标识符在不同位置打不同修饰符
        //（声明处、宏参数内等），VS Code 对修饰符有独立
        // 颜色/样式（如宏参数蓝色、声明加粗），导致同一变量不同行颜色不一，
        // 干扰用户按颜色查找标识符。教学场景以类型色为准，修饰符信息放弃。
        let mut 映射后: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
        for (词法行, 词法列, 词法长, 词法类型, _词法修饰) in 记号集 {
            let (母语行, 母语起始列) = 映射位置(词法行, 词法列);
            let (_, 母语结束列) = 映射位置(词法行, 词法列.saturating_add(词法长));
            let 母语长 = 母语结束列.saturating_sub(母语起始列).max(1);
            映射后.push((母语行, 母语起始列, 母语长, 词法类型, 0));
        }

        // 3. 重新增量编码
        let mut 新数据 = Vec::with_capacity(映射后.len() * 5);
        let mut 前行 = 0u32;
        let mut 前列 = 0u32;
        for (词法行, 词法列, 词法长, 词法类型, 词法修饰) in 映射后 {
            let 增量行 = 词法行.saturating_sub(前行);
            let 增量起列 = if 增量行 == 0 {
                词法列.saturating_sub(前列)
            } else {
                词法列
            };
            新数据.push(serde_json::json!(增量行));
            新数据.push(serde_json::json!(增量起列));
            新数据.push(serde_json::json!(词法长));
            新数据.push(serde_json::json!(词法类型));
            新数据.push(serde_json::json!(词法修饰));
            前行 = 词法行;
            前列 = 词法列;
        }
        结果值["data"] = Value::Array(新数据);
        结果值
    }

    /// 映射定义跳转响应（位置值 或 位置值数组）
    pub fn 映射定义响应(&self, 响应值: &Value) -> Value {
        match 响应值 {
            Value::Null => Value::Null,
            Value::Array(数组) => {
                let 映射后: Vec<Value> = 数组.iter().map(|项| self.还原位置(项)).collect();
                Value::Array(映射后)
            }
            Value::Object(_) => {
                // 单个位置值
                self.还原位置(响应值)
            }
            _ => 响应值.clone(),
        }
    }

    /// 映射悬停响应中的位置信息
    ///
    /// 范围字段映射回原始文件位置；内容命中语言包 ["解释"] 表时
    /// 在文档上方插入一行加粗大白话提示（未命中保持原样透传）。
    pub fn 映射悬停响应(&self, 响应值: &Value, 原始资源标识: &str) -> Value {
        let mut 结果值 = 响应值.clone();
        if let Some(范围值) = 响应值.get("range") {
            结果值["range"] = self.还原范围(原始资源标识, 范围值);
        }
        if let Some(内容值) = 响应值.get("contents") {
            结果值["contents"] = self.充实悬停内容(内容值);
        }
        结果值
    }

    /// 映射签名帮助响应
    ///
    /// 签名标签（如 `fn push(&mut self, value: T)`）与参数标签做词法级
    /// 中文化（英文关键字转中文等）；参数标签为 [起, 止] 索引形式时按
    /// 原标签提取文本翻译后转为字符串形式（VS Code 不再高亮参数，但
    /// 母语用户可读性优先——翻译前后长度变化无法保持索引）。
    pub fn 映射签名帮助响应(&self, 响应值: &Value) -> Value {
        let mut 结果值 = 响应值.clone();
        let Some(签名集) = 结果值.get("signatures").and_then(|值| 值.as_array()) else {
            return 结果值;
        };
        let 映射后: Vec<Value> = 签名集
            .iter()
            .map(|签名值| {
                let mut 映射后签名 = 签名值.clone();
                let Some(标签) = 签名值.get("label").and_then(|值| 值.as_str()) else {
                    return 映射后签名;
                };
                let 翻译后标签 = self.翻译代码(标签);
                if let Some(参数集) = 签名值.get("parameters").and_then(|值| 值.as_array())
                {
                    let 翻译后参数: Vec<Value> = 参数集
                        .iter()
                        .map(|参数值| {
                            let mut 映射后参数 = 参数值.clone();
                            if let Some(范围值) = 参数值.get("label").and_then(|值| 值.as_array())
                            {
                                // [起, 止] 索引：原标签为英文（ASCII），
                                // UTF-16 索引等于字节索引，可直接切片
                                if let (Some(起始), Some(结束)) =
                                    (范围值[0].as_i64(), 范围值[1].as_i64())
                                    && let Some(参数文本) = 标签.get(起始 as usize..结束 as usize)
                                {
                                    映射后参数["label"] = Value::String(self.翻译代码(参数文本));
                                }
                            } else if let Some(参数标签) =
                                参数值.get("label").and_then(|值| 值.as_str())
                            {
                                映射后参数["label"] = Value::String(self.翻译代码(参数标签));
                            }
                            映射后参数
                        })
                        .collect();
                    映射后签名["parameters"] = Value::Array(翻译后参数);
                }
                映射后签名["label"] = Value::String(翻译后标签);
                映射后签名
            })
            .collect();
        结果值["signatures"] = Value::Array(映射后);
        结果值
    }

    /// 给悬停内容前置大白话提示并中文化代码片段
    ///
    /// 标记文本（语言字段存在）是纯代码签名行：先按英文原文查
    /// 解释表加“大白话”前缀，再对整个文本做词法级中文化（英文关键字转中文）。
    /// markdown 文本只对代码块与行内代码做词法级中文化，正文保持原样
    /// （正文是 rust-analyzer 的英文文档，也是教学性说明，不能按词表乱换）。
    /// 关键字悬停（如英文关键字）整段替换为语言包里的中文说明。
    fn 充实悬停内容(&self, 内容值: &Value) -> Value {
        match 内容值 {
            // 标记内容：类型为 markdown、值为文本
            Value::Object(对象)
                if 对象.get("kind").and_then(|值| 值.as_str()) == Some("markdown")
                    && 对象.get("value").and_then(|值| 值.as_str()).is_some() =>
            {
                let mut 映射项 = 对象.clone();
                // 守卫已确认值字段为字符串，读取条目回退空串仅为满足类型
                let 文本串 = 对象.get("value").and_then(|值| 值.as_str()).unwrap_or("");
                if let Some(翻译后) = self.关键词悬停文本(文本串) {
                    映射项["value"] = Value::String(翻译后);
                    return Value::Object(映射项);
                }
                // 查解释依赖英文原词（`fn name` 等），先查再中文化代码
                let 已提示 = self.命中则前置(文本串);
                映射项["value"] = Value::String(self.本地化悬停标记文本(&已提示));
                Value::Object(映射项)
            }
            // 标记文本：语言为 rust、值为代码签名
            Value::Object(对象)
                if 对象.get("language").is_some()
                    && 对象.get("value").and_then(|值| 值.as_str()).is_some() =>
            {
                let mut 映射项 = 对象.clone();
                // 守卫已确认值字段为字符串，读取条目回退空串仅为满足类型
                let 文本串 = 对象.get("value").and_then(|值| 值.as_str()).unwrap_or("");
                if let Some(翻译后) = self.关键词悬停文本(文本串) {
                    return serde_json::json!({ "kind": "markdown", "value": 翻译后 });
                }
                let 已提示 = self.命中则前置(文本串);
                映射项["value"] = Value::String(self.翻译代码(&已提示));
                Value::Object(映射项)
            }
            Value::String(文本串) => {
                if let Some(翻译后) = self.关键词悬停文本(文本串) {
                    return serde_json::json!({ "kind": "markdown", "value": 翻译后 });
                }
                Value::String(self.命中则前置(文本串))
            }
            // 数组形态（标记文本数组）：任一元素是关键字文档时整段换成
            // 中文说明，避免同一条提示里残留英文正文元素（中英混杂）
            Value::Array(项集) => {
                for 项 in 项集 {
                    let 文档文本 = match 项 {
                        Value::Object(对象) => 对象.get("value").and_then(|值| 值.as_str()),
                        Value::String(文本串) => Some(文本串.as_str()),
                        _ => None,
                    };
                    if let Some(文本串) = 文档文本
                        && let Some(翻译后) = self.关键词悬停文本(文本串)
                    {
                        return serde_json::json!({ "kind": "markdown", "value": 翻译后 });
                    }
                }
                Value::Array(项集.iter().map(|项| self.充实悬停内容(项)).collect())
            }
            _ => 内容值.clone(),
        }
    }

    /// 关键字悬停：把 rust-analyzer 的英文关键字文档整段换成中文说明
    ///
    /// rust-analyzer 对关键字（英文关键字）返回英文原文；不同版本
    /// 的形态不同（代码块首行、加粗/反引号标题行、纯文本标识符），逐一
    /// 识别。命中语言包 `["解释"]` 中的关键字条目时整段替换；未配置说明
    /// 时返回 `无`，调用方回退到常规“大白话 + 代码中文化”路径。
    fn 关键词悬停文本(&self, 文档串: &str) -> Option<String> {
        let 界面 = crate::界面消息::全局界面();
        let 解释 = |词: &str| -> Option<String> {
            // `macro_rules!` 这类关键字文档可能带感叹号，去掉后再识别
            let 词 = 词.strip_suffix('!').unwrap_or(词);
            if !是标识符(词) {
                return None;
            }
            let 母语词形 = self.反向查找(词)?;
            let 说明 = 界面.取解释(词)?;
            Some(format!("**{}**\n\n{}", 母语词形, 说明))
        };
        // 形态一：代码块首行是单个标识符（关键字）；含空白的签名不会命中
        for 代码块 in 提取悬停代码块(文档串) {
            let Some(首行) = 代码块
                .lines()
                .map(str::trim)
                .find(|行文本| !行文本.is_empty())
            else {
                continue;
            };
            if let Some(文本串) = 解释(首行) {
                return Some(文本串);
            }
        }
        // 形态二：加粗标题行（加粗标题或反引号标题）或反引号标题行
        for 行文本 in 文档串.lines() {
            let 裁剪后 = 行文本.trim();
            let 词 = if 裁剪后.len() >= 4 && 裁剪后.starts_with("**") && 裁剪后.ends_with("**")
            {
                裁剪后[2..裁剪后.len() - 2].trim_matches('`').trim()
            } else if 裁剪后.len() >= 2 && 裁剪后.starts_with('`') && 裁剪后.ends_with('`')
            {
                裁剪后.trim_matches('`').trim()
            } else {
                continue;
            };
            if let Some(文本串) = 解释(词) {
                return Some(文本串);
            }
        }
        // 形态三：整段只有一个（可带反引号/加粗）标识符
        解释(
            文档串
                .trim()
                .trim_matches(|字符值| 字符值 == '*' || 字符值 == '`')
                .trim(),
        )
    }

    /// 中文化 markdown 悬停中的代码块与行内代码，正文保持原样
    ///
    /// rust-analyzer 的函数/类型悬停形如 ```` ```rust\nfn 名字()\n``` ````
    /// 加英文文档；只翻代码片段既能消除英文关键字残留，又不会把英文正文
    /// 按词表误换（正文里的 match/for 等英文单词是说明文字，不是代码）。
    /// 只含内部路径（`i18n_virtual::main`、`alloc::vec::Vec`）且后面还有
    /// 签名块的代码块整体丢弃：那是语言服务器内部结构，泄露给学习者
    /// 只会制造“多余前缀”。签名里的内部注释（`// matched arm #1`、
    /// `// Bounds from impl:`）同样清理，避免中英混杂。
    fn 本地化悬停标记文本(&self, 文档串: &str) -> String {
        let 行集: Vec<&str> = 文档串.split_inclusive('\n').collect();
        let 总块数 = 提取悬停代码块(文档串).len();
        let mut 输出 = String::with_capacity(文档串.len());
        let mut 序号 = 0;
        while 序号 < 行集.len() {
            let (主体, 换行) = 拆分行尾(行集[序号]);
            if 主体.trim_start().starts_with("```") {
                // 找闭合围栏；未闭合时按到文末处理（不恐慌）
                let mut 闭合 = 序号 + 1;
                while 闭合 < 行集.len() && !拆分行尾(行集[闭合]).0.trim_start().starts_with("```")
                {
                    闭合 += 1;
                }
                let 内部: Vec<&str> = 行集[序号 + 1..闭合]
                    .iter()
                    .map(|行文本| 拆分行尾(行文本).0)
                    .collect();
                if 是虚拟路径块(&内部) || (总块数 > 1 && 是纯路径块(&内部)) {
                    // 跳过整块，并吃掉紧随其后的一个空行，避免留下双空行
                    序号 = if 闭合 < 行集.len() {
                        闭合 + 1
                    } else {
                        闭合
                    };
                    if 序号 < 行集.len() && 拆分行尾(行集[序号]).0.trim().is_empty() {
                        序号 += 1;
                    }
                    continue;
                }
                输出.push_str(主体);
                输出.push_str(换行);
                for 行文本 in &行集[序号 + 1..闭合] {
                    let (内部主体, 内部换行) = 拆分行尾(行文本);
                    let Some(代码) = 去除悬停标注(内部主体) else {
                        continue;
                    };
                    输出.push_str(&self.翻译代码(代码));
                    输出.push_str(内部换行);
                }
                if 闭合 < 行集.len() {
                    let (闭合主体, 闭合换行) = 拆分行尾(行集[闭合]);
                    输出.push_str(闭合主体);
                    输出.push_str(闭合换行);
                    序号 = 闭合 + 1;
                } else {
                    序号 = 闭合;
                }
                continue;
            }
            输出.push_str(&self.翻译行内代码(主体));
            输出.push_str(换行);
            序号 += 1;
        }
        输出
    }

    /// 中文化一行正文里的行内代码（`…` 包裹的片段）
    ///
    /// 按反引号切分，奇数段是代码；未闭合的尾段保持原样（按正文处理）。
    fn 翻译行内代码(&self, 行文本: &str) -> String {
        let 片段集: Vec<&str> = 行文本.split('`').collect();
        let mut 输出 = String::with_capacity(行文本.len());
        for (序号, 片段) in 片段集.iter().enumerate() {
            if 序号 > 0 {
                输出.push('`');
            }
            if 序号 % 2 == 1 && !片段.is_empty() {
                输出.push_str(&self.翻译代码(片段));
            } else {
                输出.push_str(片段);
            }
        }
        输出
    }

    /// 命中解释表时在文档上方插入加粗大白话行，否则原样返回
    fn 命中则前置(&self, 文档串: &str) -> String {
        match self.查找解释(文档串) {
            Some(提示) => format!("**大白话：{}**\n\n{}", 提示, 文档串),
            None => 文档串.to_string(),
        }
    }

    /// 从悬停文档提取候选解释键并查表（完整路径 → 类型::方法 → 方法名）
    ///
    /// 只在“签名块”上取键：rust-analyzer 的悬停常含两个代码块，首块是内部
    /// 模块路径（如 `i18n_virtual::main`），拿它取键会把入口函数说明
    /// （主函数）误挂到任意自定义函数/类型上。
    fn 查找解释(&self, 文档串: &str) -> Option<String> {
        let (类型名, _) = 提取悬停部分(文档串);
        let 代码块集 = 提取悬停代码块(文档串);
        let 签名行 = 选取悬停签名(&代码块集)?;
        let mut 键集: Vec<String> = Vec::new();
        // 1. 完整路径形式（如完整标准库路径），清洗泛型参数
        if 签名行.contains("::") && !签名行.contains("fn ") && !签名行.contains(char::is_whitespace)
        {
            let 清洗后 = 清洗路径段(&签名行);
            if 清洗后.len() >= 2 {
                键集.push(清洗后.join("::"));
                // 降级匹配末两段（如选项的解包），兼容短路径键数据
                if 清洗后.len() > 2 {
                    键集.push(清洗后[清洗后.len() - 2..].join("::"));
                }
            }
        }
        // 2. 类型::方法（短路径，如选项的解包）+ 3. 方法名兜底
        if let Some(名称) = 提取函数名(&签名行) {
            let 类型值 = 类型名.or_else(|| 提取实现类型(&签名行));
            if let Some(类型值) = 类型值 {
                键集.push(format!("{}::{}", 类型值, 名称));
            }
            if !键集.iter().any(|键| 键 == &名称) {
                键集.push(名称);
            }
        }
        let 界面 = crate::界面消息::全局界面();
        键集
            .iter()
            .find_map(|键| 界面.取解释(键).map(str::to_string))
    }

    /// 映射引用响应
    ///
    /// 引用响应是位置值数组；无结果时为 空值，统一转为空数组。
    pub fn 映射引用响应(&self, 响应值: &Value) -> Value {
        match 响应值 {
            Value::Null => Value::Array(Vec::new()),
            _ => self.映射定义响应(响应值), // 与定义跳转格式相同
        }
    }

    /// 映射重命名响应
    ///
    /// 处理跨文件重命名：
    /// - 变更表：{ 资源标识 → [文本编辑] }
    /// - 文档变更：[文本编辑]
    ///
    /// 将每个编辑的范围映射回原始文件，并将新文本反向翻译为母语。
    ///
    /// 编辑目标不是已打开 .zh 的虚拟文件时（如聚合模块的 main.rs、
    /// Cargo.toml）直接丢弃，避免客户端被引导编辑虚拟项目内部文件。
    pub fn 映射重命名响应(&self, 响应值: &Value) -> Value {
        let mut 结果值 = 响应值.clone();

        // 1. 处理变更表形式
        if let Some(变更表) = 响应值.get("changes").and_then(|值| 值.as_object()) {
            let mut 映射后变更 = serde_json::Map::new();
            for (资源标识, 编辑列表) in 变更表 {
                if !self.是虚拟资源标识(资源标识) {
                    continue;
                }
                let 目标资源标识 = self.还原资源标识(资源标识);
                let 映射后编辑 = self.映射编辑列表(编辑列表, 资源标识, true);
                映射后变更.insert(目标资源标识, 映射后编辑);
            }
            结果值["changes"] = Value::Object(映射后变更);
        }

        // 2. 处理文档变更形式
        if let Some(文档变更) = 响应值.get("documentChanges").and_then(|值| 值.as_array())
        {
            let 映射后文档变更: Vec<Value> = 文档变更
                .iter()
                .filter_map(|项| {
                    let 资源标识 = 项
                        .get("textDocument")
                        .and_then(|文档| 文档.get("uri"))
                        .and_then(|值| 值.as_str())
                        .unwrap_or("");
                    if !self.是虚拟资源标识(资源标识) {
                        return None;
                    }
                    let mut 映射项 = 项.clone();
                    映射项["textDocument"]["uri"] = Value::String(self.还原资源标识(资源标识));
                    if let Some(编辑) = 项.get("edits") {
                        映射项["edits"] = self.映射编辑列表(编辑, 资源标识, true);
                    }
                    Some(映射项)
                })
                .collect();
            结果值["documentChanges"] = Value::Array(映射后文档变更);
        }

        结果值
    }

    /// 映射代码操作响应
    ///
    /// LSP 规范中 `textDocument/codeAction` 响应是 `CodeAction[]` 数组，
    /// 每个操作可能携带编辑字段（工作区编辑）。将编辑位置映射回原始文件，
    /// 插入的英文代码反向翻译为母语（与源文件语言保持一致）。
    pub fn 映射代码操作响应(&self, 响应值: &Value, _原始资源标识: &str) -> Value {
        match 响应值 {
            Value::Array(操作集) => {
                let 映射后操作: Vec<Value> = 操作集
                    .iter()
                    .map(|操作值| {
                        let mut 映射项 = 操作值.clone();
                        if let Some(编辑) = 操作值.get("edit") {
                            映射项["edit"] = self.映射编辑(编辑, true);
                        }
                        映射项
                    })
                    .collect();
                Value::Array(映射后操作)
            }
            Value::Null => Value::Array(Vec::new()),
            _ => 响应值.clone(),
        }
    }

    /// 映射代码操作解析（codeAction/resolve）响应
    ///
    /// 解析响应是单个代码操作对象（非数组）：VS Code 点击操作后
    /// 解析懒加载的编辑。若不映射，编辑的资源标识是虚拟路径，
    /// VS Code 无法应用编辑（报请求失败）。
    pub fn 映射代码操作解析响应(&self, 响应值: &Value) -> Value {
        match 响应值 {
            Value::Object(_) => {
                let mut 映射项 = 响应值.clone();
                if let Some(编辑) = 响应值.get("edit") {
                    映射项["edit"] = self.映射编辑(编辑, true);
                }
                映射项
            }
            _ => 响应值.clone(),
        }
    }

    /// 注入“添加依赖”快捷修复：未解析导入错误时提供一键 `cargo add`
    ///
    /// 命令由 VS Code 扩展注册（添加依赖命令）在工作区终端执行；
    /// 动作标题中的包名反查母语别名（如网络库 → HTTP客户端），
    /// 与母语源码的阅读体验保持一致。
    pub fn 注入添加依赖动作(&self, 响应值: &Value, 包名列表: &[String]) -> Value {
        if 包名列表.is_empty() {
            return 响应值.clone();
        }
        let mut 操作集 = match 响应值 {
            Value::Array(列表) => 列表.clone(),
            _ => Vec::new(),
        };
        for 包名 in 包名列表 {
            // 英文包名反查母语别名（关键字/别名表），未命中保持原名
            let 显示名 = self.反向查找(包名).unwrap_or_else(|| 包名.clone());
            let 标题 = crate::界面消息::全局界面()
                .取带参消息("lsp_action_add_dependency", &[&显示名]);
            操作集.push(serde_json::json!({
                "title": 标题,
                "kind": "quickfix",
                "command": {
                    "title": 标题,
                    "command": "i18n-rust.cargoAdd",
                    "arguments": [包名]
                }
            }));
        }
        Value::Array(操作集)
    }

    /// 注入教学诊断的快捷修复：全角标点一键替换半角、教学检查忽略此行
    ///
    /// 教学诊断由代理自身发布（方言坐标），rust-analyzer 不会为其生成
    /// 修复动作，故在响应时按上下文诊断注入。编辑坐标直接使用诊断携带的
    /// 方言坐标（客户端应用编辑的文件即方言源文件）。
    pub fn 注入教学动作(
        &self,
        响应值: &Value,
        教学诊断列表: &[Value],
        原始资源标识: &str,
    ) -> Value {
        if 教学诊断列表.is_empty() {
            return 响应值.clone();
        }
        let mut 操作集 = match 响应值 {
            Value::Array(列表) => 列表.clone(),
            _ => Vec::new(),
        };
        let 界面 = crate::界面消息::全局界面();
        for 诊断值 in 教学诊断列表 {
            let 代码 = 诊断值["code"].as_str().unwrap_or("");
            if 代码 == "fullwidth" {
                // 全角标点：可修复时提供替换动作；仅提示字符（顿号等）无动作
                let Some(替换值) = 诊断值["data"]["replacement"]
                    .as_str()
                    .map(|串| 串.to_string())
                else {
                    continue;
                };
                let 字符值 = 诊断值["data"]["character"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let 范围值 = 诊断值["range"].clone();
                let 标题 = 界面.取带参消息("lsp_action_fix_fullwidth", &[&字符值, &替换值]);
                操作集.push(serde_json::json!({
                    "title": 标题,
                    "kind": "quickfix",
                    "diagnostics": [诊断值],
                    "edit": {
                        "changes": {
                            原始资源标识: [{
                                "range": 范围值,
                                "newText": 替换值
                            }]
                        }
                    }
                }));
            } else if 代码.starts_with("lint-") {
                // 教学检查：行尾插入忽略标记（教师标注故意不修的示例）
                let Some(行号) = 诊断值["range"]["start"]["line"].as_u64() else {
                    continue;
                };
                let Some(条目) = self.原始条目(原始资源标识) else {
                    continue;
                };
                let 行尾 = Self::行尾偏移(&条目.母语内容, 行号 as usize);
                let 范围值 = serde_json::json!({
                    "start": { "line": 行号, "character": 行尾 },
                    "end": { "line": 行号, "character": 行尾 }
                });
                let 标题 = 界面.取消息("lsp_action_ignore_lint");
                操作集.push(serde_json::json!({
                    "title": 标题,
                    "kind": "quickfix",
                    "diagnostics": [诊断值],
                    "edit": {
                        "changes": {
                            原始资源标识: [{
                                "range": 范围值,
                                "newText": format!("  // {}", i18n_rust_engine::校验::忽略标记)
                            }]
                        }
                    }
                }));
            }
        }
        Value::Array(操作集)
    }

    /// 计算指定行（0 起）行尾的 UTF-16 字符偏移；行不存在时回退 0
    fn 行尾偏移(内容: &str, 行号: usize) -> u32 {
        内容
            .lines()
            .nth(行号)
            .map(|行文本| 行文本.encode_utf16().count() as u32)
            .unwrap_or(0)
    }

    /// 映射文档符号响应
    ///
    /// 将每个符号的范围和选择范围映射回原始文件，
    /// 并递归处理子符号。
    pub fn 映射文档符号响应(&self, 响应值: &Value, 原始资源标识: &str) -> Value {
        match 响应值 {
            Value::Array(数组) => {
                let 映射后: Vec<Value> = 数组
                    .iter()
                    .map(|符号值| self.映射单个符号(符号值, 原始资源标识))
                    .collect();
                Value::Array(映射后)
            }
            Value::Null => Value::Array(Vec::new()),
            _ => 响应值.clone(),
        }
    }

    /// 递归映射单个文档符号
    fn 映射单个符号(&self, 符号值: &Value, 原始资源标识: &str) -> Value {
        let mut 映射项 = 符号值.clone();

        // 将符号名反向恢复为中文（如英文入口名 → 主函数）
        if let Some(名称) = 符号值.get("name").and_then(|值| 值.as_str())
            && let Some(母语名) = self.反向查找(名称)
        {
            映射项["name"] = Value::String(母语名);
        }

        if let Some(范围值) = 符号值.get("range") {
            映射项["range"] = self.还原范围(原始资源标识, 范围值);
        }
        if let Some(选择范围) = 符号值.get("selectionRange") {
            映射项["selectionRange"] = self.还原范围(原始资源标识, 选择范围);
        }
        if let Some(子项) = 符号值.get("children").and_then(|值| 值.as_array()) {
            let 映射后子项: Vec<Value> = 子项
                .iter()
                .map(|子值| self.映射单个符号(子值, 原始资源标识))
                .collect();
            映射项["children"] = Value::Array(映射后子项);
        }
        映射项
    }
}

/// 判断字符串是否为合法 Rust 标识符片段（方法名/类型名，不含泛型）
fn 是标识符(名称: &str) -> bool {
    let mut 字符们 = 名称.chars();
    matches!(字符们.next(), Some(字符值) if 字符值 == '_' || 字符值.is_alphabetic())
        && 字符们.all(|字符值| 字符值 == '_' || 字符值.is_alphanumeric())
}

/// 从悬停文档提取标题类型名与代码块首行
///
/// - 标题行：加粗的实现标题或类型标题 → 类型名
/// - 代码行：第一个 ``` 代码块内的首个非空行（通常为签名或完整路径）
fn 提取悬停部分(文档串: &str) -> (Option<String>, Option<String>) {
    let mut 类型名 = None;
    let mut 代码行 = None;
    let 行集: Vec<&str> = 文档串.lines().collect();
    for (序号, 行文本) in 行集.iter().enumerate() {
        let 裁剪后 = 行文本.trim();
        // 元素数 >= 4 是切片 `t[2..t.元素数() - 2]` 的前置条件：
        // `**`（长度 2）与 `***`（长度 3）同样以 `**` 开头并以 `**` 结尾，
        // 但会得到起点大于终点的越界切片而恐慌（客户端源码文档注释可构造）。
        if 类型名.is_none()
            && 裁剪后.len() >= 4
            && 裁剪后.starts_with("**")
            && 裁剪后.ends_with("**")
        {
            let mut 内部 = 裁剪后[2..裁剪后.len() - 2].replace('`', "");
            内部 = 内部.trim().to_string();
            let 名称 = if let Some(剩余) = 内部.strip_prefix("impl") {
                // 实现头的泛型段之后取第一个标识符
                let mut 剩余 = 剩余.trim();
                while 剩余.starts_with('<') {
                    let 结束位置 = 剩余.find('>').map(|位置| 位置 + 1).unwrap_or(剩余.len());
                    剩余 = &剩余[结束位置..];
                }
                剩余.split('<').next().unwrap_or("").trim().to_string()
            } else {
                // 加粗类型标题 → 类型名
                内部.split('<').next().unwrap_or("").trim().to_string()
            };
            if 是标识符(&名称) {
                类型名 = Some(名称);
            }
        }
        if 代码行.is_none() && 裁剪后.starts_with("```") {
            for 行值 in 行集.iter().skip(序号 + 1) {
                let 行裁剪 = 行值.trim();
                if 行裁剪.is_empty() {
                    continue;
                }
                if 行裁剪.starts_with("```") {
                    break;
                }
                代码行 = Some(行裁剪.to_string());
                break;
            }
        }
        if 代码行.is_some() {
            break;
        }
    }
    // 标记文本纯代码形态（无 ``` 围栏）：整段文本当作代码行
    if 代码行.is_none()
        && !文档串
            .lines()
            .any(|行文本| 行文本.trim().starts_with("```"))
    {
        let 裁剪后 = 文档串.trim();
        if !裁剪后.is_empty() {
            代码行 = Some(裁剪后.to_string());
        }
    }
    (类型名, 代码行)
}

/// 按行拆出内容与行尾换行（兼容 `\n` 与 `\r\n`）
fn 拆分行尾(行文本: &str) -> (&str, &str) {
    if let Some(剩余) = 行文本.strip_suffix("\r\n") {
        (剩余, "\r\n")
    } else if let Some(剩余) = 行文本.strip_suffix('\n') {
        (剩余, "\n")
    } else {
        (行文本, "")
    }
}

/// 提取悬停文档中的所有代码块（块内非空行按 `\n` 连接，保留缩进）
///
/// 无围栏的标记文本把整段视为一个代码块。签名块与内部路径块分开
/// 返回，调用方才不会把路径块当成函数签名去查解释表。
fn 提取悬停代码块(文档串: &str) -> Vec<String> {
    let 行集: Vec<&str> = 文档串.lines().collect();
    let mut 代码块集 = Vec::new();
    let mut 序号 = 0;
    while 序号 < 行集.len() {
        if 行集[序号].trim_start().starts_with("```") {
            let mut 闭合 = 序号 + 1;
            while 闭合 < 行集.len() && !行集[闭合].trim_start().starts_with("```") {
                闭合 += 1;
            }
            let 代码块 = 行集[序号 + 1..闭合]
                .iter()
                .filter(|行文本| !行文本.trim().is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            if !代码块.is_empty() {
                代码块集.push(代码块);
            }
            序号 = if 闭合 < 行集.len() {
                闭合 + 1
            } else {
                闭合
            };
            continue;
        }
        序号 += 1;
    }
    if 代码块集.is_empty() {
        let 整体 = 文档串.trim();
        if !整体.is_empty() {
            代码块集.push(整体.to_string());
        }
    }
    代码块集
}

/// 选签名块：优先含 `fn `/`macro_rules!` 的块，其次含空白的定义块，
/// 最后退回首块（纯路径或关键字）
fn 选取悬停签名(代码块集: &[String]) -> Option<String> {
    let 像函数 = |代码块: &String| {
        代码块
            .lines()
            .any(|行文本| 行文本.contains("fn ") || 行文本.contains("macro_rules!"))
    };
    let 含空白 = |代码块: &String| {
        代码块
            .lines()
            .any(|行文本| 行文本.split_whitespace().count() >= 2)
    };
    if let Some(代码块) = 代码块集.iter().find(|代码块| 像函数(代码块)) {
        return Some(代码块.clone());
    }
    if let Some(代码块) = 代码块集.iter().find(|代码块| 含空白(代码块)) {
        return Some(代码块.clone());
    }
    代码块集.first().cloned()
}

/// 从 `impl<T, A> Vec<T, A>` 首行取实现类型名（给 `Vec::push` 之类键用）
fn 提取实现类型(代码块: &str) -> Option<String> {
    for 行文本 in 代码块.lines() {
        let 裁剪后 = 行文本.trim();
        let Some(剩余) = 裁剪后.strip_prefix("impl") else {
            continue;
        };
        let mut 剩余 = 剩余.trim_start();
        // 跳过实现关键字后的泛型段（按尖括号深度，兼容嵌套约束）
        if 剩余.starts_with('<') {
            let mut 深度 = 0usize;
            let mut 结束位置 = None;
            for (偏移, 字符值) in 剩余.char_indices() {
                match 字符值 {
                    '<' => 深度 += 1,
                    '>' => {
                        深度 -= 1;
                        if 深度 == 0 {
                            结束位置 = Some(偏移 + 1);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let 结束位置 = 结束位置?;
            剩余 = 剩余.get(结束位置..)?.trim_start();
        }
        let 名称 = 剩余.split(['<', ' ', '{', ':']).next().unwrap_or("");
        if 是标识符(名称) {
            return Some(名称.to_string());
        }
    }
    None
}

/// 是否为单行完整路径的代码块（如 `alloc::vec::Vec`、`i18n_virtual::main`）
///
/// 这类块只是语言服务器的内部结构路径，不含空白与签名信息。
fn 是纯路径块(行集: &[&str]) -> bool {
    let 内容: Vec<&str> = 行集
        .iter()
        .map(|行文本| 行文本.trim())
        .filter(|行文本| !行文本.is_empty())
        .collect();
    内容.len() == 1 && 内容[0].contains("::") && !内容[0].contains(char::is_whitespace)
}

/// 是否为只含虚拟项目内部路径的代码块（如 `i18n_virtual::main`）
///
/// 虚拟项目包路径（Cargo 名 i18n-virtual）不属于学习者世界；
/// 它不属于学习者世界，中文化时整块丢弃（即使它是唯一的代码块）。
fn 是虚拟路径块(行集: &[&str]) -> bool {
    let 内容: Vec<&str> = 行集
        .iter()
        .map(|行文本| 行文本.trim())
        .filter(|行文本| !行文本.is_empty())
        .collect();
    内容.len() == 1 && 内容[0].starts_with("i18n_virtual::") && 是纯路径块(行集)
}

/// 清理 rust-analyzer 悬停签名里的内部注释
///
/// `// matched arm #1`（宏匹配到的分支）与 `// Bounds from impl:`
/// （实现带来的约束）是分析器的内部标注，不属于语言语义；
/// 整行注释返回 `无`（丢弃该行），行尾注释截掉注释部分。
fn 去除悬停标注(行文本: &str) -> Option<&str> {
    if 行文本.trim_start().starts_with("// Bounds from impl") {
        return None;
    }
    if let Some(位置) = 行文本.find("// matched arm") {
        let 代码 = 行文本[..位置].trim_end();
        return if 代码.trim().is_empty() {
            None
        } else {
            Some(代码)
        };
    }
    Some(行文本)
}

/// 把完整路径行清洗为段列表（去掉泛型参数与空白）
///
/// 完整标准库路径 → 路径段列表
fn 清洗路径段(行文本: &str) -> Vec<String> {
    行文本
        .split("::")
        .map(|段| 段.split('<').next().unwrap_or("").trim())
        .filter(|段| !段.is_empty())
        .filter(|段| 是标识符(段))
        .map(str::to_string)
        .collect()
}

/// 从代码行提取方法名：路径末段（`a::b::c`）、签名（`fn name<...>(`）或宏（`macro_rules! name`）
fn 提取函数名(代码行: &str) -> Option<String> {
    let 行文本 = 代码行.trim();
    if 行文本.contains("::") {
        if let Some(末段) = 行文本.rsplit("::").next() {
            let 名称 = 末段.split(['(', '<', ' ']).next().unwrap_or("").trim();
            if 是标识符(名称) {
                return Some(名称.to_string());
            }
        }
        return None;
    }
    if let Some(位置) = 行文本.find("fn ") {
        let 之后 = 行文本[位置 + 3..].trim_start();
        let 名称 = 之后.split(['(', '<', ' ']).next().unwrap_or("").trim();
        if 是标识符(名称) {
            return Some(名称.to_string());
        }
    }
    // 宏形式：宏规则声明
    if let Some(位置) = 行文本.find("macro_rules!") {
        let 之后 = 行文本[位置 + 12..].trim_start();
        let 名称 = 之后.split(['(', '<', ' ', '!']).next().unwrap_or("").trim();
        if 是标识符(名称) {
            return Some(名称.to_string());
        }
    }
    None
}

/// 提取补全标签的末段标识符
///
/// rust-analyzer 的标签可能带后缀/路径前缀，如
/// `foo(…)`、`Foo {…}`、`m::Spam::Bar(…)`、`m::`；
/// 此处提取末段标识符（如路径末段）用于用户词汇白名单匹配。
/// 标签无标识符段（如纯符号）时返回 无。
pub(super) fn 标签标识后缀(标签: &str) -> Option<&str> {
    // 去掉形如 `(…)`、`{…}` 的参数/字段后缀
    let 头部 = 标签.split(['(', '{']).next().unwrap_or(标签);
    // 去掉宏感叹号与模块补全的尾部 `::`（如 `m::`）
    let 头部 = 头部.trim().trim_end_matches('!').trim_end_matches("::");
    let 末段 = 头部.rsplit("::").next().unwrap_or("").trim();
    if 末段.is_empty() {
        None
    } else {
        Some(末段)
    }
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    /// 回归：标题行仅为 `**` / `***` 时不得越界切片恐慌
    ///
    /// 此前判定条件只看前后缀，`t[2..t.元素数() - 2]` 在长度小于 4 时起点大于终点
    /// 直接恐慌；客户端源码的文档注释可构造该悬停内容。
    #[test]
    fn 悬停仅加粗行不恐慌() {
        for 文档串 in ["**", "***", "**\n**", "a\n***\nb"] {
            let (类型名, _) = 提取悬停部分(文档串);
            assert!(类型名.is_none(), "纯星号行不应解析出类型名");
        }
    }

    /// 正常标题行仍能提取类型名（含实现前缀与泛型两种形态）
    #[test]
    fn 悬停类型名提取() {
        let (类型名, _) = 提取悬停部分("**`Option<T>`**");
        assert_eq!(类型名.as_deref(), Some("Option"));
        let (类型名, _) = 提取悬停部分("**impl<T> Option<T>**");
        assert_eq!(类型名.as_deref(), Some("Option"));
    }

    /// 构造带缓存文档的映射器（关键字 + 别名最小集）
    fn 带文档映射器(
        来源: &str,
    ) -> (
        响应映射器,
        tempfile::TempDir,
        Arc<crate::翻译缓存::转译缓存>,
    ) {
        use std::collections::HashMap;
        use std::sync::Arc;
        let 管理器 = i18n_rust_engine::映射管理器::映射管理器::从扁表构造(
            HashMap::from([
                ("函数".into(), "fn".into()),
                ("让".into(), "let".into()),
                ("可变".into(), "mut".into()),
            ]),
            HashMap::new(),
            HashMap::from([("长度".into(), "len".into()), ("向量".into(), "Vec".into())]),
        );
        let 临时 = tempfile::tempdir().unwrap();
        let 缓存值: Arc<crate::翻译缓存::转译缓存> =
            crate::翻译缓存::转译缓存::新建(管理器, 临时.path().to_path_buf());
        缓存值.更新文档("file:///t/main.zh", 来源, 1).unwrap();
        (响应映射器::新建(缓存值.clone()), 临时, 缓存值)
    }

    /// 补全响应：关键字/方法项母语化（标签/新文本/插入文本/详情/
    /// 标签详情/文档/文本编辑括号补全）；未翻译的第三方英文项
    /// 被母语过滤丢弃；用户自定义英文标识项保留
    #[test]
    fn 补全响应完整用例() {
        // 源码含用户自定义英文标识：用户函数名与解包（后者用于保留文档项）
        let 来源 = "函数 my_func() {\n    unwrap();\n}\n";
        let (映射器, _临时, _缓存) = 带文档映射器(来源);
        let 虚拟范围 = serde_json::json!({
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 2 }
        });
        let 关键字项 = serde_json::json!({
            "label": "fn", "kind": 14,
            "textEdit": { "range": 虚拟范围, "newText": "fn" },
            "insertText": "fn",
            "detail": "fn item",
            "labelDetails": { "description": "fn()", "detail": "keyword" },
            "documentation": "plain english keyword doc",
            "additionalTextEdits": [{ "range": 虚拟范围, "newText": "use std::io;" }]
        });
        let 方法项 = serde_json::json!({
            "label": "len", "kind": 2,
            "textEdit": { "range": 虚拟范围, "newText": "len" }
        });
        // 未收录第三方英文项：严格母语过滤下必须丢弃
        let 外部项 = serde_json::json!({
            "label": "tokio::spawn", "kind": 3,
            "textEdit": { "range": 虚拟范围, "newText": "tokio::spawn" }
        });
        // 用户自定义英文标识：保留
        let 用户项 = serde_json::json!({
            "label": "my_func", "kind": 3,
            "textEdit": { "range": 虚拟范围, "newText": "my_func" }
        });
        // 文档大白话：markdown 对象形态，选项解包命中解释表
        let 文档项 = serde_json::json!({
            "label": "unwrap", "kind": 2,
            "textEdit": { "range": 虚拟范围, "newText": "unwrap" },
            "documentation": {
                "kind": "markdown",
                "value": "**`Option<T>`**\n\n`Option::unwrap()` 直接取值"
            }
        });
        let 响应值 = serde_json::json!({
            "isIncomplete": false,
            "items": [关键字项, 方法项, 外部项, 用户项, 文档项]
        });
        let 映射后 = 映射器.映射补全响应(&响应值, "file:///t/main.zh");
        let 项集 = 映射后["items"].as_array().expect("应有 items 数组");
        // 第三方项被过滤，其余 4 项保留
        assert_eq!(项集.len(), 4, "应过滤未翻译第三方项：{项集:?}");
        let 按标签: std::collections::HashMap<_, _> = 项集
            .iter()
            .map(|项| (项["label"].as_str().unwrap_or("").to_string(), 项.clone()))
            .collect();

        // 关键字项
        let 关键字 = 按标签.get("函数").expect("fn 应反查为 函数");
        assert_eq!(关键字["insertText"].as_str(), Some("函数"));
        assert_eq!(关键字["textEdit"]["newText"].as_str(), Some("函数"));
        assert_eq!(
            关键字["labelDetails"]["description"].as_str(),
            Some("函数()")
        );
        assert!(关键字.get("additionalTextEdits").is_some());

        // 方法项：别名反查 + 无括号补代码片段括号
        let 长度项 = 按标签.get("长度").expect("len 应反查为 长度");
        let 新文本 = 长度项["textEdit"]["newText"].as_str().unwrap();
        assert!(新文本.starts_with("长度("), "方法补全应补括号：{新文本}");
        assert_eq!(长度项["insertTextFormat"].as_i64(), Some(2));

        // 用户自定义项保留
        assert!(按标签.contains_key("my_func"), "用户英文标识应保留");

        // 文档解释命中大白话表
        let 解包项 = 按标签
            .values()
            .find(|项| {
                项["textEdit"]["newText"]
                    .as_str()
                    .is_some_and(|文本串| 文本串.contains("unwrap"))
            })
            .or_else(|| 按标签.get("unwrap"))
            .expect("unwrap 项应保留（源码出现过该标识）");
        let 文档 = 解包项["documentation"].as_str().unwrap_or("");
        assert!(
            文档.contains("直接取出里面的值"),
            "应替换为大白话解释：{文档}"
        );
    }

    /// 文档字符串形态 + 无条目数组的响应不恐慌，原样可处理
    #[test]
    fn 翻译补全文档形态() {
        let 来源 = "函数 主() {}\n";
        let (映射器, _临时, _缓存) = 带文档映射器(来源);
        // 空值 / 空数组 / 非对象响应均安全
        assert!(
            映射器
                .映射补全响应(&Value::Null, "file:///t/main.zh")
                .is_null()
        );
        let 数组值 =
            映射器.映射补全响应(&serde_json::json!({ "items": [] }), "file:///t/main.zh");
        assert_eq!(数组值["items"].as_array().unwrap().len(), 0);
    }
}
