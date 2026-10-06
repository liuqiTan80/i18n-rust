//! 各 LSP 请求类型的响应映射：诊断、补全、悬停、签名帮助、
//! 引用/定义、重命名、代码操作、文档符号与教学动作注入。

use serde_json::{Value, json};

use super::响应映射器;
use super::诊断文本::{
    提取所有权详情, 是主函数提示, 是未打开模块引用, 是缺失依赖噪声, 是缺失包含资源, 是缺失项目依赖,
    查找期望实际标签, 翻译诊断消息,
};
use super::还原单行;
use crate::翻译缓存::英文列转中文列;

impl 响应映射器 {
    /// 映射 rust-analyzer 的 publishDiagnostics 通知
    ///
    /// 将诊断信息中的 URI 和位置还原为原始 .zh 文件，
    /// 并尝试翻译诊断消息为中文。
    pub fn 映射诊断(&self, 参数集: &Value) -> Value {
        let 虚拟资源定位 = 参数集["uri"].as_str().unwrap_or("");
        // 预取条目一次：所有诊断的 range/relatedInformation 映射基于同一
        // 文档，循环内复用条目走无锁纯函数（诊断通常 10-50 条，避免每条
        // range × 2 锁、每条 relatedInformation × 4 锁）
        let 翻译条目 = self
            .缓存
            .按虚拟资源定位查询(虚拟资源定位)
            .or_else(|| self.缓存.查询原文(虚拟资源定位));
        let 原始资源定位 = match &翻译条目 {
            Some(条目项) => 条目项.原始资源定位.clone(),
            None => 虚拟资源定位.to_string(),
        };
        let 诊断列表 = 参数集["diagnostics"].as_array();

        let mut 映射诊断列表 = Vec::new();

        if let Some(诊断数组) = 诊断列表 {
            for 诊断 in 诊断数组 {
                // 过滤虚拟项目 main.rs 中关于 main 函数的 Hint 级提示
                // （fn main() 在模块内不是真正的入口，rust-analyzer 会发出
                // "here is a function named `main`" 等教学无关的提示）
                if 是主函数提示(诊断, 虚拟资源定位) {
                    continue;
                }
                // 过滤虚拟项目固有的过程宏误报（#3 症状二）：虚拟项目无第三方
                // 依赖且禁用过程宏，clap 等 derive 未展开时的辅助属性
                // （`#[arg(...)]`）与派生宏名字解析必然失败——用户在项目中
                // 构建正常（rzc check 通过），此类诊断恒为误报
                if 是缺失依赖噪声(诊断) {
                    continue;
                }
                // 过滤“引用未打开模块文件”的 E0433/E0432 误报：虚拟项目只
                // 聚合已打开文件，被引用模块未打开时无法解析（打开后即恢复）；
                // 同名方言文件存在于同目录时判定为误报（覆盖 RA 新旧消息格式）
                if 是未打开模块引用(诊断, 翻译条目.as_deref()) {
                    continue;
                }
                // 过滤“真实依赖在虚拟项目缺失”的误报：虚拟项目 Cargo.toml
                // 不含用户依赖，serde/serde_json 等已声明依赖的导入必然无法
                // 解析（用户项目中 rzc check 正常通过）
                if 是缺失项目依赖(诊断, 翻译条目.as_deref()) {
                    continue;
                }
                // 过滤 include_str!/include_bytes! 资源在虚拟项目中缺失的
                // 误报：资源文件在原方言文件同目录存在时，虚拟 .rs 的相对
                // 路径解析失败属虚拟项目固有现象
                if 是缺失包含资源(诊断, 翻译条目.as_deref()) {
                    continue;
                }

                let mut 映射结果 = 诊断.clone();

                // 映射范围（使用列映射）
                if 诊断.get("range").is_some() {
                    映射结果["range"] =
                        Self::带条目还原范围(翻译条目.as_deref(), &诊断["range"]);
                }

                // 映射 relatedInformation 中的位置
                if let Some(关联信息) = 诊断
                    .get("relatedInformation")
                    .and_then(|值项| 值项.as_array())
                {
                    let mut 映射关联 = Vec::new();
                    for 项 in 关联信息 {
                        let mut 映射项 = 项.clone();
                        if let Some(位置项) = 项.get("location") {
                            // 同文件位置复用主条目（跨文件才按需查询）
                            let 位置uri = 位置项["uri"].as_str().unwrap_or("");
                            let 位置条目 = if 位置uri == 虚拟资源定位 {
                                翻译条目.clone()
                            } else {
                                self.缓存
                                    .按虚拟资源定位查询(位置uri)
                                    .or_else(|| self.缓存.查询原文(位置uri))
                            };
                            映射项["location"] =
                                Self::带条目还原位置(位置条目.as_deref(), 位置uri, 位置项);
                        }
                        // 子消息（help/note）同样翻译——悬停查看诊断详情时
                        // 不泄漏英文（如 "value moved here"、"consider ..."）；
                        // 子消息无独立错误码/标签，走消息表/短语兜底
                        if let Some(消息) = 项.get("message").and_then(|值项| 值项.as_str())
                        {
                            映射项["message"] = Value::String(翻译诊断消息(None, 消息, None));
                        }
                        映射关联.push(映射项);
                    }
                    映射结果["relatedInformation"] = Value::Array(映射关联);
                }

                // 翻译诊断主消息（与 CLI 同口径）：传入错误码供码表优先命中；
                // rust-analyzer 把 expected/found 放在 relatedInformation 而非主
                // span 标签，故从 related 归集候选标签回填 {期望}/{实际}（无则自动
                // 回退消息表）
                let 码文本 = 诊断.get("code").and_then(|值项| 值项.as_str());
                let 主标签 = 查找期望实际标签(诊断);
                映射结果["message"] = Value::String(翻译诊断消息(
                    码文本,
                    诊断["message"].as_str().unwrap_or(""),
                    主标签.as_deref(),
                ));

                // 所有权错误：提取叙事化详情并存入 data 字段（供 VS Code 扩展可视化）
                if let Some(详情) = 提取所有权详情(诊断, &映射结果, &原始资源定位)
                    && let Ok(详情值) = serde_json::to_value(&详情)
                {
                    // 保留 rust-analyzer 已有的 data（如代码操作数据），嵌套存入
                    match 映射结果.get_mut("data") {
                        Some(现有值) if 现有值.is_object() => {
                            现有值["所有权详情"] = 详情值;
                        }
                        _ => {
                            映射结果["data"] = 详情值;
                        }
                    }
                }

                // 添加教学提示标记
                映射结果["source"] = Value::String("i18n-rust".to_string());

                映射诊断列表.push(映射结果);
            }
        }

        json!({
            "uri": 原始资源定位,
            "diagnostics": 映射诊断列表,
            "version": 参数集.get("version").cloned().unwrap_or(Value::Null)
        })
    }

    /// 映射补全响应中的位置信息
    ///
    /// 将 textEdit/additionalTextEdits 中的 range 映射回原始文件，
    /// 并将英文标识符/代码反向翻译为母语（否则接受补全会把
    /// 英文关键字插入母语源文件，或自动导入编辑落在错误位置）。
    pub fn 映射补全响应(&self, 响应: &Value, 原始资源定位: &str) -> Value {
        let mut 响应集 = 响应.clone();

        if let Some(项集合) = 响应集.get("items").and_then(|值项| 值项.as_array()) {
            let mut 映射项集合 = Vec::new();
            // 语言过滤白名单（用户源码中出现过的标识符）懒加载，
            // 仅在确实遇到未翻译的纯英文项时才扫描一次；
            // Arc 共享避免每次补全请求克隆整个集合
            let mut 用户词集: Option<std::sync::Arc<std::collections::HashSet<String>>> = None;
            for 项 in 项集合 {
                let mut 映射结果 = 项.clone();

                // 1. 映射 textEdit 的 range 并反向翻译 newText
                if let Some(文本编辑) = 项.get("textEdit") {
                    if let Some(跨度) = 文本编辑.get("range") {
                        映射结果["textEdit"]["range"] = self.还原范围(原始资源定位, 跨度);
                    }
                    if let Some(新文本) = 文本编辑.get("newText").and_then(|值项| 值项.as_str())
                    {
                        映射结果["textEdit"]["newText"] = Value::String(self.翻译代码(新文本));
                    }
                }

                // 2. additionalTextEdits（如自动导入）：位置与内容同样需要还原
                if let Some(额外编辑) = 项.get("additionalTextEdits") {
                    映射结果["additionalTextEdits"] =
                        self.映射编辑列表(额外编辑, 原始资源定位, true);
                }

                // 3. label：先精确反查（关键字等），未命中则词法级转译
                //    （标准库 API 如 Vec::new / println! 也能还原为母语）
                if let Some(标签) = 项.get("label").and_then(|值项| 值项.as_str()) {
                    let 映射标签 = self.反向查词(标签).unwrap_or_else(|| self.翻译代码(标签));

                    // 3.5 语言过滤（禁止串语言）：非英文方言下，补全列表
                    //     只保留母语项，过滤未翻译的外部英文项（第三方库、
                    //     未收录的标准库 API 等）。保留条件（满足其一）：
                    //     a. 翻译命中（mapped_label 与原文不同）；
                    //     b. label 含母语字符（用户定义的母语标识符）；
                    //     c. label 的末段标识符在用户源码中出现过
                    //        （用户自己定义的项，含英文命名）。
                    if self.严格母语过滤 {
                        let 母语字符 = !映射标签.is_ascii();
                        let 已翻译 = 映射标签 != 标签;
                        let 用户自定义 = if 母语字符 || 已翻译 {
                            true
                        } else {
                            let 记号集 = 用户词集.get_or_insert_with(|| self.缓存.用户自定义词元());
                            取标签末段(标签).is_some_and(|名字| 记号集.contains(名字))
                        };
                        if !(母语字符 || 已翻译 || 用户自定义) {
                            continue;
                        }
                    }

                    映射结果["label"] = Value::String(映射标签);
                }

                // 4. detail（类型签名）：词法级转译，如 fn push(...) → 函数 推入(...)
                if let Some(详情) = 项.get("detail").and_then(|值项| 值项.as_str()) {
                    映射结果["detail"] = Value::String(self.翻译代码(详情));
                }

                // 4.5 labelDetails：VS Code 提示框右侧优先显示此字段
                //     （description 为签名如 fn()、detail 为 crate/模块路径），
                //     不还原会把英文 fn() 泄漏给母语用户
                if let Some(标签详情) = 项.get("labelDetails").and_then(|值项| 值项.as_object())
                {
                    let mut 映射详情 = 标签详情.clone();
                    if let Some(描述文本) =
                        标签详情.get("description").and_then(|值项| 值项.as_str())
                    {
                        映射详情.insert(
                            "description".to_string(),
                            Value::String(self.翻译代码(描述文本)),
                        );
                    }
                    if let Some(详情) = 标签详情.get("detail").and_then(|值项| 值项.as_str())
                    {
                        映射详情.insert("detail".to_string(), Value::String(self.翻译代码(详情)));
                    }
                    映射结果["labelDetails"] = Value::Object(映射详情);
                }

                // 5. documentation：命中解释表（大白话）时替换为中文，
                //    未命中保留英文原文（避免丢失签名等关键信息）
                if let Some(文档) = 项.get("documentation") {
                    映射结果["documentation"] = self.翻译补全文档(项, 文档);
                }

                // 6. 反向映射 insertText（可能含 snippet 占位符，仅精确匹配时替换）
                if let Some(插入文本) = 项.get("insertText").and_then(|值项| 值项.as_str())
                    && let Some(中文名) = self.反向查词(插入文本)
                {
                    映射结果["insertText"] = Value::String(中文名);
                }

                // 7. 方法/函数补全补括号：rust-analyzer 的 snippet 配置在代理
                //    环境不可靠（方法补全默认不带括号），这里对方法/函数类
                //    补全项在 textEdit 末尾补 snippet 括号，光标自动落在括号内
                //    （教学常用场景如 `长度()`）；已带括号（含参数占位）跳过。
                let 项类型 = 项.get("kind").and_then(|值项| 值项.as_i64()).unwrap_or(0);
                if matches!(项类型, 2 | 3)
                    && let Some(文本内容) = 映射结果["textEdit"]["newText"].as_str()
                    && !文本内容.contains('(')
                {
                    映射结果["textEdit"]["newText"] =
                        Value::String(format!("{}(${{1:}})", 文本内容));
                    // 2 = Snippet 格式：占位符由客户端解析，光标落在括号内
                    映射结果["insertTextFormat"] = Value::Number(2.into());
                }

                // 8. 关键字补全前导空格：rust-analyzer 的 "let mut" 组合 snippet
                //    在代理环境不可用，散落的关键字项（如 `可变`）直接插入会与
                //    前一标识符粘连（`让可变`）。判断 newText 的首个标识符是否
                //    为关键字映射表中的词（不依赖 kind，rust-analyzer 的关键字
                //    补全 kind 不可靠），且前一字符是标识符时在 textEdit 前补空格。
                if let Some(文本内容) = 映射结果["textEdit"]["newText"].as_str() {
                    let 首词 = 文本内容
                        .split(|字符项: char| 字符项.is_whitespace() || 字符项 == '$')
                        .next()
                        .unwrap_or("");
                    if !首词.is_empty() && self.缓存.关键词映射表().contains_key(首词) {
                        let 起点 = 映射结果["textEdit"]["range"]["start"].clone();
                        if let (Some(行号), Some(列号)) = (
                            起点["line"].as_u64().map(|值项| 值项 as usize),
                            起点["character"].as_u64().map(|值项| 值项 as usize),
                        ) && 列号 > 0
                            && let Some(翻译条目) = self.缓存.查询原文(原始资源定位)
                            && let Some(行文本) = 翻译条目.中文原文.lines().nth(行号)
                        {
                            let 前一字符 = 行文本.chars().nth(列号 - 1);
                            let 需要空格 = 前一字符.is_some_and(|字符项| {
                                !字符项.is_whitespace()
                                    && !matches!(
                                        字符项,
                                        '(' | '.' | ':' | ',' | ';' | '{' | '[' | '!'
                                    )
                            });
                            if 需要空格 && !文本内容.starts_with(char::is_whitespace) {
                                映射结果["textEdit"]["newText"] =
                                    Value::String(format!(" {文本内容}"));
                            }
                        }
                    }
                }

                映射项集合.push(映射结果);
            }
            响应集["items"] = Value::Array(映射项集合);
        }

        响应集
    }

    /// 补全文档（documentation）：优先用中文解释表替换英文文档
    ///
    /// 查键顺序：从文档解析类型::方法（如 Option::unwrap）→ label 直查
    /// → 方法名兜底；全部未命中时保留英文原文。
    fn 翻译补全文档(&self, 项: &Value, 文档: &Value) -> Value {
        let 文档文本 = match 文档 {
            Value::String(文本项) => Some(文本项.as_str()),
            Value::Object(对象体) => 对象体.get("value").and_then(|值项| 值项.as_str()),
            _ => None,
        };
        if let Some(文本内容) = 文档文本
            && let Some(解释) = self.查解释(文本内容)
        {
            return Value::String(解释);
        }
        // label 直查 + 方法名兜底
        let 界面 = crate::本地化::全局();
        if let Some(标签) = 项.get("label").and_then(|值项| 值项.as_str())
            && let Some(解释) = 界面.取解释(标签)
        {
            return Value::String(解释.to_string());
        }
        if let Some(文本内容) = 文档文本
            && let Some(名字) = 提取函数名(文本内容)
            && let Some(解释) = 界面.取解释(&名字)
        {
            return Value::String(解释.to_string());
        }
        文档.clone()
    }

    /// 映射文档高亮响应（documentHighlight）
    pub fn 映射文档高亮响应(&self, 响应: &Value, 原始资源定位: &str) -> Value {
        match 响应 {
            Value::Array(项集合) => {
                let 映射集合 = 项集合
                    .iter()
                    .map(|项| {
                        let mut 映射项 = 项.clone();
                        if let Some(跨度) = 项.get("range") {
                            映射项["range"] = self.还原范围(原始资源定位, 跨度);
                        }
                        映射项
                    })
                    .collect();
                Value::Array(映射集合)
            }
            Value::Null => Value::Array(Vec::new()),
            _ => 响应.clone(),
        }
    }

    /// 映射语义着色响应（semanticTokens/full、semanticTokens/range）
    ///
    /// rust-analyzer 返回的 token 坐标基于虚拟 .rs（转译产物），
    /// 必须还原到方言文件坐标，否则变量/参数等颜色落在错误位置
    /// 或完全不显示。data 为 LSP delta 编码（每 5 项一组）：
    /// `[deltaLine, deltaStart, length, tokenType, tokenModifiers]`——
    /// 先还原为绝对坐标，逐 token 映射起点与终点列，再重新 delta 编码。
    /// resultId 原样透传（客户端依赖它做增量请求）。
    pub fn 映射语义记号响应(&self, 响应: &Value, 原始资源定位: &str) -> Value {
        let mut 映射结果 = 响应.clone();
        let Some(记号数据) = 映射结果.get("data").and_then(|值项| 值项.as_array())
        else {
            return 映射结果;
        };

        // 预取翻译条目：每个 token 的列映射都基于同一文档，
        // 只查一次（O(1) 索引），避免每个 token 重复锁与表扫描
        let 翻译条目 = self
            .缓存
            .按虚拟资源定位查询(原始资源定位)
            .or_else(|| self.缓存.查询原文(原始资源定位));
        // 无条目（异常 URI）时位置原样透传
        let 映射位置 = |行号: u32, 列号: u32| -> (u32, u32) {
            match &翻译条目 {
                Some(条目项) => {
                    let 中文行 = 还原单行(条目项, 行号);
                    let 中文列 = 英文列转中文列(条目项, 行号, 列号);
                    (中文行, 中文列)
                }
                None => (行号, 列号),
            }
        };

        // 1. delta 编码 → 绝对坐标（跨行时列归零重置，同行时列累加）
        let mut 记号列表: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
        let mut 行号 = 0u32;
        let mut 列号 = 0u32;
        for 记号组 in 记号数据.chunks(5) {
            if 记号组.len() < 5 {
                break;
            }
            let 增量行 = 记号组[0].as_u64().unwrap_or(0) as u32;
            let 增量列 = 记号组[1].as_u64().unwrap_or(0) as u32;
            let 记号长 = 记号组[2].as_u64().unwrap_or(0) as u32;
            let 记号类型 = 记号组[3].as_u64().unwrap_or(0) as u32;
            let 修饰符 = 记号组[4].as_u64().unwrap_or(0) as u32;
            行号 = 行号.saturating_add(增量行);
            列号 = if 增量行 == 0 {
                列号.saturating_add(增量列)
            } else {
                增量列
            };
            记号列表.push((行号, 列号, 记号长, 记号类型, 修饰符));
        }

        // 2. 起点/终点列分别还原到方言坐标，长度取映射后的差值
        //（关键字替换改变了列宽，如 `让`(1) → `let`(3)，长度必须重算）
        // 修饰符强制清零：教学场景以类型色为准，修饰符信息放弃。
        let mut 映射记号: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
        for (原始行, 原始列, 原始长, 原始类型, _) in 记号列表 {
            let (中文行, 中文起始) = 映射位置(原始行, 原始列);
            let (_, 中文结束) = 映射位置(原始行, 原始列.saturating_add(原始长));
            let 中文长 = 中文结束.saturating_sub(中文起始).max(1);
            映射记号.push((中文行, 中文起始, 中文长, 原始类型, 0));
        }

        // 3. 重新 delta 编码
        let mut 新记号数据 = Vec::with_capacity(映射记号.len() * 5);
        let mut 前行 = 0u32;
        let mut 前列 = 0u32;
        for (记号行, 记号列, 记号长, 记号类型, 记号修饰) in 映射记号 {
            let 增量行 = 记号行.saturating_sub(前行);
            let 增量列 = if 增量行 == 0 {
                记号列.saturating_sub(前列)
            } else {
                记号列
            };
            新记号数据.push(json!(增量行));
            新记号数据.push(json!(增量列));
            新记号数据.push(json!(记号长));
            新记号数据.push(json!(记号类型));
            新记号数据.push(json!(记号修饰));
            前行 = 记号行;
            前列 = 记号列;
        }
        映射结果["data"] = Value::Array(新记号数据);
        映射结果
    }

    /// 映射定义跳转响应（Location 或 Location[]）
    pub fn 映射定义响应(&self, 响应: &Value) -> Value {
        match 响应 {
            Value::Null => Value::Null,
            Value::Array(列表) => {
                let 映射集合 = 列表.iter().map(|项| self.还原位置(项)).collect();
                Value::Array(映射集合)
            }
            Value::Object(_) => {
                // 单个 Location
                self.还原位置(响应)
            }
            _ => 响应.clone(),
        }
    }

    /// 映射悬停响应中的位置信息
    ///
    /// range 字段映射回原始文件位置；contents 命中语言包 ["解释"] 表时
    /// 在文档上方插入一行加粗大白话提示（未命中保持原样透传）。
    pub fn 映射悬停响应(&self, 响应: &Value, 原始资源定位: &str) -> Value {
        let mut 映射结果 = 响应.clone();
        if let Some(跨度) = 响应.get("range") {
            映射结果["range"] = self.还原范围(原始资源定位, 跨度);
        }
        if let Some(正文) = 响应.get("contents") {
            映射结果["contents"] = self.丰富悬停内容(正文);
        }
        映射结果
    }

    /// 映射签名帮助响应
    ///
    /// 签名 label（如 `fn push(&mut self, value: T)`）与参数 label 做词法级
    /// 中文化（`fn` → `函数` 等）；参数 label 为 [start, end] 索引形式时按
    /// 原 label 提取文本翻译后转为字符串形式。
    pub fn 映射签名帮助响应(&self, 响应: &Value) -> Value {
        let mut 映射结果 = 响应.clone();
        let Some(签名列表) = 映射结果.get("signatures").and_then(|值项| 值项.as_array())
        else {
            return 映射结果;
        };
        let 映射集合 = 签名列表
            .iter()
            .map(|签名项| {
                let mut 映射签名项 = 签名项.clone();
                let Some(标签) = 签名项.get("label").and_then(|值项| 值项.as_str()) else {
                    return 映射签名项;
                };
                let 映射标签 = self.翻译代码(标签);
                if let Some(参数列表) = 签名项.get("parameters").and_then(|值项| 值项.as_array())
                {
                    let 映射参数集 = 参数列表
                        .iter()
                        .map(|参数项| {
                            let mut 映射参数项 = 参数项.clone();
                            if let Some(跨度) = 参数项.get("label").and_then(|值项| 值项.as_array())
                            {
                                // [start, end] 索引：原 label 为英文（ASCII），
                                // UTF-16 索引 == 字节索引，可直接切片
                                if let (Some(起始), Some(结束)) =
                                    (跨度[0].as_i64(), 跨度[1].as_i64())
                                    && let Some(参数文本) = 标签.get(起始 as usize..结束 as usize)
                                {
                                    映射参数项["label"] = Value::String(self.翻译代码(参数文本));
                                }
                            } else if let Some(参数标签) =
                                参数项.get("label").and_then(|值项| 值项.as_str())
                            {
                                映射参数项["label"] = Value::String(self.翻译代码(参数标签));
                            }
                            映射参数项
                        })
                        .collect();
                    映射签名项["parameters"] = Value::Array(映射参数集);
                }
                映射签名项["label"] = Value::String(映射标签);
                映射签名项
            })
            .collect();
        映射结果["signatures"] = Value::Array(映射集合);
        映射结果
    }

    /// 给 hover contents 前置大白话提示（MarkupContent / MarkedString / 数组）
    fn 丰富悬停内容(&self, 正文: &Value) -> Value {
        match 正文 {
            // MarkupContent：{"kind": "markdown", "value": ...}
            Value::Object(对象体)
                if 对象体.get("kind").and_then(|值项| 值项.as_str()) == Some("markdown")
                    && 对象体.get("value").and_then(|值项| 值项.as_str()).is_some() =>
            {
                let mut 映射对象 = 对象体.clone();
                let 值文本 = 对象体
                    .get("value")
                    .and_then(|值项| 值项.as_str())
                    .unwrap_or("");
                映射对象["value"] = Value::String(self.命中则前置(值文本));
                Value::Object(映射对象)
            }
            // MarkedString：{"language": "rust", "value": 代码签名}
            Value::Object(对象体)
                if 对象体.get("language").is_some()
                    && 对象体.get("value").and_then(|值项| 值项.as_str()).is_some() =>
            {
                let mut 映射对象 = 对象体.clone();
                let 值文本 = 对象体
                    .get("value")
                    .and_then(|值项| 值项.as_str())
                    .unwrap_or("");
                let 加提示后 = self.命中则前置(值文本);
                映射对象["value"] = Value::String(self.翻译代码(&加提示后));
                Value::Object(映射对象)
            }
            Value::String(文档文本) => Value::String(self.命中则前置(文档文本)),
            Value::Array(项集合) => {
                Value::Array(项集合.iter().map(|项| self.丰富悬停内容(项)).collect())
            }
            _ => 正文.clone(),
        }
    }

    /// 命中解释表时在文档上方插入加粗大白话行，否则原样返回
    fn 命中则前置(&self, 文档: &str) -> String {
        match self.查解释(文档) {
            Some(提示) => format!("**大白话：{}**\n\n{}", 提示, 文档),
            None => 文档.to_string(),
        }
    }

    /// 从 hover 文档提取候选解释键并查表（完整路径 → 类型::方法 → 方法名）
    fn 查解释(&self, 文档: &str) -> Option<String> {
        let (类型名, 代码行) = 提取悬停部分(文档);
        let mut 键列表: Vec<String> = Vec::new();
        if let Some(行文本) = 代码行 {
            // 1. 完整路径形式（如 std::option::Option<T>::unwrap），清洗泛型参数
            if 行文本.contains("::") && !行文本.contains("fn ") {
                let 净路径 = 清理路径段(&行文本);
                if 净路径.len() >= 2 {
                    键列表.push(净路径.join("::"));
                    // 降级匹配末两段（如 Option::unwrap），兼容短路径键数据
                    if 净路径.len() > 2 {
                        键列表.push(净路径[净路径.len() - 2..].join("::"));
                    }
                }
            }
            // 2. 类型::方法（短路径，如 Option::unwrap）+ 3. 方法名兜底
            if let Some(名字) = 提取函数名(&行文本) {
                if let Some(类型捕获) = &类型名 {
                    键列表.push(format!("{}::{}", 类型捕获, 名字));
                }
                if !键列表.iter().any(|键名项| 键名项 == &名字) {
                    键列表.push(名字);
                }
            }
        }
        let 界面 = crate::本地化::全局();
        键列表
            .iter()
            .find_map(|键名项| 界面.取解释(键名项).map(str::to_string))
    }

    /// 映射引用响应
    ///
    /// 引用响应是 Location[]；无结果时为 null，统一转为空数组。
    pub fn 映射引用响应(&self, 响应: &Value) -> Value {
        match 响应 {
            Value::Null => Value::Array(Vec::new()),
            _ => self.映射定义响应(响应), // 与定义跳转格式相同
        }
    }

    /// 映射重命名响应
    ///
    /// 处理跨文件重命名：changes（uri → [TextEdit]）与 documentChanges；
    /// 将每个编辑的 range 映射回原始文件，并将 newText 反向翻译为母语。
    /// 编辑目标不是已打开 .zh 的虚拟文件时直接丢弃。
    pub fn 映射重命名响应(&self, 响应: &Value) -> Value {
        let mut 映射结果 = 响应.clone();

        // 1. 处理 changes 形式
        if let Some(变更表) = 响应.get("changes").and_then(|值项| 值项.as_object()) {
            let mut 映射变更 = serde_json::Map::new();
            for (资源定位, 编辑列表) in 变更表 {
                if !self.是虚拟资源定位(资源定位) {
                    continue;
                }
                let 目标资源定位 = self.还原资源定位(资源定位);
                let 映射后编辑 = self.映射编辑列表(编辑列表, 资源定位, true);
                映射变更.insert(目标资源定位, 映射后编辑);
            }
            映射结果["changes"] = Value::Object(映射变更);
        }

        // 2. 处理 documentChanges 形式
        if let Some(文档变更列表) = 响应.get("documentChanges").and_then(|值项| 值项.as_array())
        {
            let 映射文档变更 = 文档变更列表
                .iter()
                .filter_map(|项| {
                    let 资源定位 = 项
                        .get("textDocument")
                        .and_then(|文本文档| 文本文档.get("uri"))
                        .and_then(|值项| 值项.as_str())
                        .unwrap_or("");
                    if !self.是虚拟资源定位(资源定位) {
                        return None;
                    }
                    let mut 映射项 = 项.clone();
                    映射项["textDocument"]["uri"] = Value::String(self.还原资源定位(资源定位));
                    if let Some(编辑列表) = 项.get("edits") {
                        映射项["edits"] = self.映射编辑列表(编辑列表, 资源定位, true);
                    }
                    Some(映射项)
                })
                .collect();
            映射结果["documentChanges"] = Value::Array(映射文档变更);
        }

        映射结果
    }

    /// 映射代码操作响应
    ///
    /// 将编辑位置映射回原始文件，插入的英文代码反向翻译为母语。
    pub fn 映射代码操作响应(&self, 响应: &Value, _原始uri: &str) -> Value {
        match 响应 {
            Value::Array(动作列表) => {
                let 映射动作列表 = 动作列表
                    .iter()
                    .map(|动作| {
                        let mut 映射项 = 动作.clone();
                        if let Some(编辑) = 动作.get("edit") {
                            映射项["edit"] = self.映射编辑(编辑, true);
                        }
                        映射项
                    })
                    .collect();
                Value::Array(映射动作列表)
            }
            Value::Null => Value::Array(Vec::new()),
            _ => 响应.clone(),
        }
    }

    /// 映射代码操作解析（codeAction/resolve）响应
    ///
    /// resolve 响应是单个 CodeAction 对象（非数组）。
    pub fn 映射代码操作解析响应(&self, 响应: &Value) -> Value {
        match 响应 {
            Value::Object(_) => {
                let mut 映射项 = 响应.clone();
                if let Some(编辑) = 响应.get("edit") {
                    映射项["edit"] = self.映射编辑(编辑, true);
                }
                映射项
            }
            _ => 响应.clone(),
        }
    }

    /// 注入“添加依赖”快捷修复：未解析导入错误时提供一键 cargo add
    pub fn 注入添加依赖动作(&self, 响应: &Value, 依赖集: &[String]) -> Value {
        if 依赖集.is_empty() {
            return 响应.clone();
        }
        let mut 动作列表 = match 响应 {
            Value::Array(列表) => 列表.clone(),
            _ => Vec::new(),
        };
        for 依赖名 in 依赖集 {
            // 英文 crate 名反查母语别名（关键字/别名表），未命中保持原名
            let 显示名 = self.反向查词(依赖名).unwrap_or_else(|| 依赖名.clone());
            let 标题 = crate::本地化::全局().取文带参("lsp_action_add_dependency", &[&显示名]);
            动作列表.push(json!({
                "title": 标题,
                "kind": "quickfix",
                "command": {
                    "title": 标题,
                    "command": "i18n-rust.cargoAdd",
                    "arguments": [依赖名]
                }
            }));
        }
        Value::Array(动作列表)
    }

    /// 注入教学诊断的快捷修复：全角标点一键替换半角、教学 lint 忽略此行
    pub fn 注入教学动作(
        &self,
        响应: &Value,
        教学诊断集: &[Value],
        原始资源定位: &str,
    ) -> Value {
        if 教学诊断集.is_empty() {
            return 响应.clone();
        }
        let mut 动作列表 = match 响应 {
            Value::Array(列表) => 列表.clone(),
            _ => Vec::new(),
        };
        let 界面 = crate::本地化::全局();
        for 诊断 in 教学诊断集 {
            let 诊断码 = 诊断["code"].as_str().unwrap_or("");
            if 诊断码 == "fullwidth" {
                // 全角标点：可修复时提供替换动作；仅提示字符（顿号等）无动作
                let Some(替换文本) = 诊断["data"]["replacement"]
                    .as_str()
                    .map(|文本项| 文本项.to_string())
                else {
                    continue;
                };
                let 标点字符 = 诊断["data"]["character"].as_str().unwrap_or("").to_string();
                let 跨度 = 诊断["range"].clone();
                let 标题 = 界面.取文带参("lsp_action_fix_fullwidth", &[&标点字符, &替换文本]);
                动作列表.push(json!({
                    "title": 标题,
                    "kind": "quickfix",
                    "diagnostics": [诊断],
                    "edit": {
                        "changes": {
                            原始资源定位: [{
                                "range": 跨度,
                                "newText": 替换文本
                            }]
                        }
                    }
                }));
            } else if 诊断码.starts_with("lint-") {
                // 教学 lint：行尾插入忽略标记（教师标注故意不修的示例）
                let Some(行号) = 诊断["range"]["start"]["line"].as_u64() else {
                    continue;
                };
                let Some(翻译条目) = self.按原始取条目(原始资源定位) else {
                    continue;
                };
                let 行尾 = Self::行尾utf16(&翻译条目.中文原文, 行号 as usize);
                let 跨度 = json!({
                    "start": { "line": 行号, "character": 行尾 },
                    "end": { "line": 行号, "character": 行尾 }
                });
                let 标题 = 界面.取文("lsp_action_ignore_lint");
                动作列表.push(json!({
                    "title": 标题,
                    "kind": "quickfix",
                    "diagnostics": [诊断],
                    "edit": {
                        "changes": {
                            原始资源定位: [{
                                "range": 跨度,
                                "newText": format!("  // {}", i18n_rust_engine::教学检查::教学忽略标记)
                            }]
                        }
                    }
                }));
            }
        }
        Value::Array(动作列表)
    }

    /// 计算指定行（0 起）行尾的 UTF-16 字符偏移；行不存在时回退 0
    fn 行尾utf16(内容: &str, 行号: usize) -> u32 {
        内容
            .lines()
            .nth(行号)
            .map(|行文本| 行文本.encode_utf16().count() as u32)
            .unwrap_or(0)
    }

    /// 映射文档符号响应
    ///
    /// 将每个符号的 range 和 selectionRange 映射回原始文件，
    /// 并递归处理子符号。
    pub fn 映射文档符号响应(&self, 响应: &Value, 原始资源定位: &str) -> Value {
        match 响应 {
            Value::Array(列表) => {
                let 映射集合 = 列表
                    .iter()
                    .map(|符号项| self.映射单个符号(符号项, 原始资源定位))
                    .collect();
                Value::Array(映射集合)
            }
            Value::Null => Value::Array(Vec::new()),
            _ => 响应.clone(),
        }
    }

    /// 递归映射单个文档符号
    fn 映射单个符号(&self, 符号项: &Value, 原始资源定位: &str) -> Value {
        let mut 映射项 = 符号项.clone();

        // 将符号名反向恢复为中文（如 main → 主函数）
        if let Some(名字) = 符号项.get("name").and_then(|值项| 值项.as_str())
            && let Some(中文名) = self.反向查词(名字)
        {
            映射项["name"] = Value::String(中文名);
        }

        if let Some(跨度) = 符号项.get("range") {
            映射项["range"] = self.还原范围(原始资源定位, 跨度);
        }
        if let Some(选中范围) = 符号项.get("selectionRange") {
            映射项["selectionRange"] = self.还原范围(原始资源定位, 选中范围);
        }
        if let Some(子符号) = 符号项.get("children").and_then(|值项| 值项.as_array()) {
            let 映射子符号 = 子符号
                .iter()
                .map(|符号数据项| self.映射单个符号(符号数据项, 原始资源定位))
                .collect();
            映射项["children"] = Value::Array(映射子符号);
        }
        映射项
    }
}

/// 判断字符串是否为合法 Rust 标识符片段（方法名/类型名，不含泛型）
fn 是标识符(名字: &str) -> bool {
    let mut 字符集 = 名字.chars();
    matches!(字符集.next(), Some(字符项) if 字符项 == '_' || 字符项.is_alphabetic())
        && 字符集.all(|字符项| 字符项 == '_' || 字符项.is_alphanumeric())
}

/// 从 hover 文档提取标题类型名与代码块首行
fn 提取悬停部分(文档: &str) -> (Option<String>, Option<String>) {
    let mut 类型名 = None;
    let mut 代码行 = None;
    let 行集合: Vec<&str> = 文档.lines().collect();
    for (序号, 行文本) in 行集合.iter().enumerate() {
        let 文本项 = 行文本.trim();
        // len >= 4 是切片 `文本项[2..文本项.len() - 2]` 的前置条件（避免 start > end panic）
        if 类型名.is_none()
            && 文本项.len() >= 4
            && 文本项.starts_with("**")
            && 文本项.ends_with("**")
        {
            let mut 内部文本 = 文本项[2..文本项.len() - 2].replace('`', "");
            内部文本 = 内部文本.trim().to_string();
            let 名字 = if let Some(剩余) = 内部文本.strip_prefix("impl") {
                // impl<...> X<...> for Y / impl str：跳过泛型段后取第一个标识符
                let mut 剩余 = 剩余.trim();
                while 剩余.starts_with('<') {
                    let 结束点 = 剩余
                        .find('>')
                        .map(|偏移项| 偏移项 + 1)
                        .unwrap_or(剩余.len());
                    剩余 = &剩余[结束点..];
                }
                剩余.split('<').next().unwrap_or("").trim().to_string()
            } else {
                // **`Option<T>`** → Option
                内部文本.split('<').next().unwrap_or("").trim().to_string()
            };
            if 是标识符(&名字) {
                类型名 = Some(名字);
            }
        }
        if 代码行.is_none() && 文本项.starts_with("```") {
            for 子行 in 行集合.iter().skip(序号 + 1) {
                let 净子行 = 子行.trim();
                if 净子行.is_empty() {
                    continue;
                }
                if 净子行.starts_with("```") {
                    break;
                }
                代码行 = Some(净子行.to_string());
                break;
            }
        }
        if 代码行.is_some() {
            break;
        }
    }
    // MarkedString 纯代码形态（无 ``` 围栏）：整段文本当作代码行
    if 代码行.is_none() && !文档.lines().any(|行文本| 行文本.trim().starts_with("```")) {
        let 文本项 = 文档.trim();
        if !文本项.is_empty() {
            代码行 = Some(文本项.to_string());
        }
    }
    (类型名, 代码行)
}

/// 把完整路径行清洗为段列表（去掉泛型参数与空白）
fn 清理路径段(路径行: &str) -> Vec<String> {
    路径行
        .split("::")
        .map(|段| 段.split('<').next().unwrap_or("").trim())
        .filter(|文本项| !文本项.is_empty())
        .filter(|文本项| 是标识符(文本项))
        .map(str::to_string)
        .collect()
}

/// 从代码行提取方法名：路径末段、签名或宏
fn 提取函数名(代码行: &str) -> Option<String> {
    let 行文本 = 代码行.trim();
    if 行文本.contains("::") {
        if let Some(末段) = 行文本.rsplit("::").next() {
            let 名字 = 末段.split(['(', '<', ' ']).next().unwrap_or("").trim();
            if 是标识符(名字) {
                return Some(名字.to_string());
            }
        }
        return None;
    }
    if let Some(索引) = 行文本.find("fn ") {
        let 后段 = &行文本[索引 + 3..];
        let 名字 = 后段.split(['(', '<', ' ']).next().unwrap_or("").trim();
        if 是标识符(名字) {
            return Some(名字.to_string());
        }
    }
    // 宏形式：macro_rules! select
    if let Some(索引) = 行文本.find("macro_rules!") {
        let 后段 = &行文本[索引 + 12..];
        let 名字 = 后段.split(['(', '<', ' ', '!']).next().unwrap_or("").trim();
        if 是标识符(名字) {
            return Some(名字.to_string());
        }
    }
    None
}

/// 提取补全 label 的末段标识符
///
/// rust-analyzer 的 label 可能带后缀/路径前缀，此处提取末段标识符
/// 用于用户词汇白名单匹配；label 无标识符段时返回 None。
pub(super) fn 取标签末段(标签: &str) -> Option<&str> {
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
mod 单元测试 {
    use super::*;

    /// 回归：标题行仅为 `**` / `***` 时不得越界切片 panic
    #[test]
    fn 测试悬停纯星号行不panic() {
        for 文档 in ["**", "***", "**\n**", "a\n***\nb"] {
            let (类型名, _) = 提取悬停部分(文档);
            assert!(类型名.is_none(), "纯星号行不应解析出类型名");
        }
    }

    /// 正常标题行仍能提取类型名（含 `impl` 前缀与泛型两种形态）
    #[test]
    fn 测试悬停类型名提取() {
        let (类型名, _) = 提取悬停部分("**`Option<T>`**");
        assert_eq!(类型名.as_deref(), Some("Option"));
        let (类型名, _) = 提取悬停部分("**impl<T> Option<T>**");
        assert_eq!(类型名.as_deref(), Some("Option"));
    }

    /// 构造带缓存文档的 mapper（关键字 + 别名最小集）
    fn 带文档映射器(
        源码: &str,
    ) -> (
        响应映射器,
        tempfile::TempDir,
        std::sync::Arc<crate::翻译缓存::转译缓存>,
    ) {
        use std::collections::HashMap;
        use std::sync::Arc;
        let 管理器 = i18n_rust_engine::映射管理::映射管理器::自扁平映射新建(
            HashMap::from([
                ("函数".into(), "fn".into()),
                ("让".into(), "let".into()),
                ("可变".into(), "mut".into()),
            ]),
            HashMap::new(),
            HashMap::from([("长度".into(), "len".into()), ("向量".into(), "Vec".into())]),
        );
        let 临时路径 = tempfile::tempdir().unwrap();
        let 缓存: Arc<crate::翻译缓存::转译缓存> =
            crate::翻译缓存::转译缓存::新建缓存(管理器, 临时路径.path().to_path_buf());
        缓存.更新文档("file:///t/main.zh", 源码, 1).unwrap();
        (响应映射器::新建映射器(缓存.clone()), 临时路径, 缓存)
    }

    /// 补全响应：关键字/方法项母语化；未翻译的第三方英文项被母语过滤丢弃；
    /// 用户自定义英文标识项保留
    #[test]
    fn 测试映射补全响应完整() {
        let 源码 = "函数 my_func() {\n    unwrap();\n}\n";
        let (映射器, _临时, _缓存) = 带文档映射器(源码);
        let 虚拟跨度 = json!({
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 2 }
        });
        let 关键字项 = json!({
            "label": "fn", "kind": 14,
            "textEdit": { "range": 虚拟跨度, "newText": "fn" },
            "insertText": "fn",
            "detail": "fn item",
            "labelDetails": { "description": "fn()", "detail": "keyword" },
            "documentation": "plain english keyword doc",
            "additionalTextEdits": [{ "range": 虚拟跨度, "newText": "use std::io;" }]
        });
        let 方法项 = json!({
            "label": "len", "kind": 2,
            "textEdit": { "range": 虚拟跨度, "newText": "len" }
        });
        let 外部项 = json!({
            "label": "tokio::spawn", "kind": 3,
            "textEdit": { "range": 虚拟跨度, "newText": "tokio::spawn" }
        });
        let 用户项 = json!({
            "label": "my_func", "kind": 3,
            "textEdit": { "range": 虚拟跨度, "newText": "my_func" }
        });
        let 文档项 = json!({
            "label": "unwrap", "kind": 2,
            "textEdit": { "range": 虚拟跨度, "newText": "unwrap" },
            "documentation": {
                "kind": "markdown",
                "value": "**`Option<T>`**\n\n`Option::unwrap()` 直接取值"
            }
        });
        let 响应 = json!({
            "isIncomplete": false,
            "items": [关键字项, 方法项, 外部项, 用户项, 文档项]
        });
        let 映射结果 = 映射器.映射补全响应(&响应, "file:///t/main.zh");
        let 项集合 = 映射结果["items"].as_array().expect("应有 items 数组");
        assert_eq!(项集合.len(), 4, "应过滤未翻译第三方项：{项集合:?}");
        let 按键: std::collections::HashMap<_, _> = 项集合
            .iter()
            .map(|候选项| {
                (
                    候选项["label"].as_str().unwrap_or("").to_string(),
                    候选项.clone(),
                )
            })
            .collect();

        // 关键字项
        let 关键字映射 = 按键.get("函数").expect("fn 应反查为 函数");
        assert_eq!(关键字映射["insertText"].as_str(), Some("函数"));
        assert_eq!(关键字映射["textEdit"]["newText"].as_str(), Some("函数"));
        assert_eq!(
            关键字映射["labelDetails"]["description"].as_str(),
            Some("函数()")
        );
        assert!(关键字映射.get("additionalTextEdits").is_some());

        // 方法项：别名反查 + 无括号补 snippet 括号
        let 长度项 = 按键.get("长度").expect("len 应反查为 长度");
        let 新文本 = 长度项["textEdit"]["newText"].as_str().unwrap();
        assert!(新文本.starts_with("长度("), "方法补全应补括号：{新文本}");
        assert_eq!(长度项["insertTextFormat"].as_i64(), Some(2));

        // 用户自定义项保留
        assert!(按键.contains_key("my_func"), "用户英文标识应保留");

        // 文档解释命中大白话表
        let 取出项 = 按键
            .values()
            .find(|候选项| {
                候选项["textEdit"]["newText"]
                    .as_str()
                    .is_some_and(|文本项| 文本项.contains("unwrap"))
            })
            .or_else(|| 按键.get("unwrap"))
            .expect("unwrap 项应保留（源码出现过该标识）");
        let 文档 = 取出项["documentation"].as_str().unwrap_or("");
        assert!(
            文档.contains("直接取出里面的值"),
            "应替换为大白话解释：{文档}"
        );
    }

    /// 文档字符串形态 + 无 items 的响应不 panic，原样可处理
    #[test]
    fn 测试翻译补全文档形态() {
        let 源码 = "函数 主() {}\n";
        let (映射器, _临时, _缓存) = 带文档映射器(源码);
        assert!(
            映射器
                .映射补全响应(&Value::Null, "file:///t/main.zh")
                .is_null()
        );
        let 列表结果 = 映射器.映射补全响应(&json!({ "items": [] }), "file:///t/main.zh");
        assert_eq!(列表结果["items"].as_array().unwrap().len(), 0);
    }
}
