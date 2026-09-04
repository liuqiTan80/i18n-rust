//! 规则驱动的中文名生成：整名特例 → 前缀规则 → 拆词翻译，完全离线；
//! 另含生成名与语言包关键字映射的冲突检测。

use std::fs;
use std::path::Path;

/// 检测中文名与语言包关键字映射的冲突。
///
/// 引擎的词法转译（关键字映射）先于别名替换执行，因此若生成的中文名
/// 在 keywords.toml 中已映射为不同英文（如 `错误` → `Err`），则本映射不会生效。
/// 返回冲突列表：(中文名, 关键字映射的英文, 本映射的英文)；语言包目录不存在时返回空。
pub fn detect_keyword_conflicts(
    lang_pack_dir: &Path,
    chinese_name_table: &[(String, String)],
) -> Vec<(String, String, String)> {
    let Ok(content) = fs::read_to_string(lang_pack_dir.join("keywords.toml")) else {
        return Vec::new();
    };
    // 复用引擎权威语义（按节名升序合并、后到覆盖），与运行时生效的
    // 关键字映射一致；否则按文档顺序先到先得会误报冲突
    let Ok(sections) = i18n_rust_engine::mapping_source::parse_toml_sections(&content) else {
        return Vec::new();
    };
    let keyword_map = i18n_rust_engine::mapping_source::flatten_sections(&sections);
    let mut conflicts = Vec::new();
    for (chinese, english) in chinese_name_table {
        if let Some(keyword_english) = keyword_map.get(chinese)
            && keyword_english != english
        {
            conflicts.push((chinese.clone(), keyword_english.clone(), english.clone()));
        }
    }
    conflicts
}

/// 整名特例（优先于一切规则，保证常见 API 的中文名准确直观）
const WHOLE_NAME_EXCEPTIONS: &[(&str, &str)] = &[
    ("new", "新建"),
    ("from_str", "从字符串解析"),
    ("from_bytes", "从字节解析"),
    ("to_string", "转字符串"),
    ("to_vec", "转字节向量"),
    ("as_str", "作为字符串"),
    ("is_empty", "是否为空"),
    ("is_some", "是否有值"),
    ("is_none", "是否无值"),
    ("is_ok", "是否成功"),
    ("is_err", "是否出错"),
    ("unwrap", "解包"),
    ("expect", "期望"),
    ("default", "默认值"),
    ("clone", "克隆"),
    ("parse", "解析"),
    ("len", "长度"),
    ("capacity", "容量"),
    ("clear", "清空"),
    ("push", "压入"),
    ("pop", "弹出"),
    ("insert", "插入"),
    ("remove", "移除"),
    ("contains", "包含"),
    ("find", "查找"),
    ("sort", "排序"),
    ("iter", "迭代"),
    ("iter_mut", "可变迭代"),
    ("into_iter", "转迭代器"),
    ("map", "映射"),
    ("filter", "过滤"),
    ("collect", "收集"),
    ("join", "拼接"),
    ("split", "分割"),
    ("trim", "去除空白"),
    ("hash", "哈希"),
    ("to_owned", "转自有"),
    ("to_lowercase", "转小写"),
    ("to_uppercase", "转大写"),
    ("main", "主函数"),
    ("println", "打印行"),
];

/// 前缀规则（按长度降序匹配，`get_xxx` → 获取xxx）
const PREFIX_RULES: &[(&str, &str)] = &[
    ("from_str", "从字符串解析"),
    ("to_string", "转字符串"),
    ("is_empty", "是否为空"),
    ("is_some", "是否有值"),
    ("is_none", "是否无值"),
    ("is_ok", "是否成功"),
    ("is_err", "是否出错"),
    ("to_vec", "转字节向量"),
    ("try_", "尝试"),
    ("get_", "获取"),
    ("set_", "设置"),
    ("is_", "是否"),
    ("has_", "是否有"),
    ("to_", "转为"),
    ("as_", "作为"),
    ("from_", "从"),
    ("into_", "转为"),
    ("parse_", "解析"),
    ("with_", "设置"),
    ("create_", "创建"),
    ("build_", "构建"),
    ("add_", "添加"),
    ("remove_", "移除"),
    ("update_", "更新"),
    ("delete_", "删除"),
    ("read_", "读取"),
    ("write_", "写入"),
    ("load_", "加载"),
    ("save_", "保存"),
    ("send_", "发送"),
    ("receive_", "接收"),
    ("encode_", "编码"),
    ("decode_", "解码"),
    ("connect_", "连接"),
    ("listen_", "监听"),
];

/// 英文单词 → 中文（用于类型名与函数名的拆词翻译）
const WORD_TABLE: &[(&str, &str)] = &[
    ("anyhow", "任意错误"),
    ("serde", "序列化"),
    ("serde_json", "JSON序列化"),
    ("error", "错误"),
    ("errors", "错误列表"),
    ("result", "结果"),
    ("option", "选项"),
    ("builder", "构建器"),
    ("config", "配置"),
    ("configuration", "配置"),
    ("settings", "设置"),
    ("client", "客户端"),
    ("server", "服务器"),
    ("request", "请求"),
    ("response", "响应"),
    ("handler", "处理器"),
    ("parser", "解析器"),
    ("reader", "读取器"),
    ("writer", "写入器"),
    ("iterator", "迭代器"),
    ("mapping", "映射"),
    ("map", "映射"),
    ("set", "集合"),
    ("list", "列表"),
    ("vector", "向量"),
    ("vec", "向量"),
    ("string", "字符串"),
    ("str", "字符串"),
    ("char", "字符"),
    ("bytes", "字节"),
    ("number", "数字"),
    ("integer", "整数"),
    ("float", "浮点数"),
    ("boolean", "布尔值"),
    ("bool", "布尔值"),
    ("time", "时间"),
    ("date", "日期"),
    ("duration", "时长"),
    ("thread", "线程"),
    ("task", "任务"),
    ("event", "事件"),
    ("callback", "回调"),
    ("factory", "工厂"),
    ("manager", "管理器"),
    ("service", "服务"),
    ("protocol", "协议"),
    ("format", "格式"),
    ("version", "版本"),
    ("path", "路径"),
    ("file", "文件"),
    ("directory", "目录"),
    ("folder", "文件夹"),
    ("network", "网络"),
    ("connection", "连接"),
    ("stream", "流"),
    ("buffer", "缓冲区"),
    ("cache", "缓存"),
    ("context", "上下文"),
    ("metadata", "元数据"),
    ("parameter", "参数"),
    ("arguments", "参数"),
    ("arg", "参数"),
    ("options", "选项"),
    ("default", "默认"),
    ("info", "信息"),
    ("message", "消息"),
    ("data", "数据"),
    ("value", "值"),
    ("key", "键"),
    ("token", "令牌"),
    ("session", "会话"),
    ("user", "用户"),
    ("name", "名称"),
    ("item", "条目"),
    ("entry", "条目"),
    ("state", "状态"),
    ("status", "状态"),
    ("code", "代码"),
    ("kind", "类型"),
    ("type", "类型"),
    ("id", "标识"),
    ("url", "网址"),
    ("uri", "资源标识"),
    ("serialize", "序列化"),
    ("deserialize", "反序列化"),
    ("serializer", "序列化器"),
    ("deserializer", "反序列化器"),
    ("serialization", "序列化"),
    ("async", "异步"),
    ("await", "等待"),
    ("future", "未来值"),
    ("runtime", "运行时"),
    ("logger", "日志器"),
    ("log", "日志"),
    ("json", "JSON"),
    ("xml", "XML"),
    ("yaml", "YAML"),
    ("toml", "TOML"),
    ("http", "HTTP"),
    ("https", "HTTPS"),
    ("address", "地址"),
    ("socket", "套接字"),
    ("port", "端口"),
    ("query", "查询"),
    ("behavior", "行为"),
    ("max", "最大"),
    ("bail", "立即报错"),
    ("ensure", "确保"),
    ("panic", "恐慌"),
    ("abort", "中止"),
    ("header", "头部"),
    ("body", "主体"),
    ("method", "方法"),
    ("function", "函数"),
    ("module", "模块"),
    ("trait", "特征"),
    ("enum", "枚举"),
    ("struct", "结构体"),
    ("macro", "宏"),
    ("constant", "常量"),
    ("variable", "变量"),
    ("mut", "可变"),
    ("const", "常量"),
    ("static", "静态"),
    ("pub", "公开"),
    ("private", "私有"),
    ("public", "公开"),
    ("import", "导入"),
    ("export", "导出"),
    ("new", "新建"),
    ("create", "创建"),
    ("build", "构建"),
    ("add", "添加"),
    ("remove", "移除"),
    ("delete", "删除"),
    ("update", "更新"),
    ("get", "获取"),
    ("set", "设置"),
    ("open", "打开"),
    ("close", "关闭"),
    ("read", "读取"),
    ("write", "写入"),
    ("load", "加载"),
    ("save", "保存"),
    ("send", "发送"),
    ("receive", "接收"),
    ("run", "运行"),
    ("start", "启动"),
    ("stop", "停止"),
    ("serialize", "序列化"),
    ("parse", "解析"),
    ("encode", "编码"),
    ("decode", "解码"),
    ("connect", "连接"),
    ("disconnect", "断开"),
    ("listen", "监听"),
    ("bind", "绑定"),
    ("print", "打印"),
    ("assert", "断言"),
    ("wait", "等待"),
    ("sleep", "休眠"),
    ("count", "数量"),
    ("size", "大小"),
    ("length", "长度"),
    ("capacity", "容量"),
    ("first", "首个"),
    ("last", "末尾"),
    ("next", "下一个"),
    ("current", "当前"),
    ("all", "全部"),
    ("any", "任意"),
    ("some", "某些"),
    ("none", "无"),
    ("ok", "成功"),
    ("yes", "是"),
    ("no", "否"),
    ("true", "真"),
    ("false", "假"),
    ("left", "左"),
    ("right", "右"),
    ("top", "顶部"),
    ("bottom", "底部"),
    ("begin", "开始"),
    ("end", "结束"),
    ("in", "进入"),
    ("out", "输出"),
    ("up", "上"),
    ("down", "下"),
    ("push", "压入"),
    ("pop", "弹出"),
    ("insert", "插入"),
    ("clear", "清空"),
    ("contains", "包含"),
    ("find", "查找"),
    ("search", "搜索"),
    ("sort", "排序"),
    ("iter", "迭代"),
    ("map", "映射"),
    ("filter", "过滤"),
    ("collect", "收集"),
    ("join", "拼接"),
    ("split", "分割"),
    ("trim", "去除空白"),
    ("hash", "哈希"),
    ("sign", "签名"),
    ("verify", "验证"),
    ("validate", "校验"),
    ("auth", "认证"),
    ("login", "登录"),
    ("logout", "登出"),
    ("secret", "密钥"),
    ("cert", "证书"),
    ("lock", "锁"),
    ("unlock", "解锁"),
    ("queue", "队列"),
    ("stack", "栈"),
    ("tree", "树"),
    ("node", "节点"),
    ("graph", "图"),
    ("edge", "边"),
    ("pair", "对"),
    ("group", "组"),
    ("record", "记录"),
    ("table", "表"),
    ("row", "行"),
    ("column", "列"),
    ("field", "字段"),
    ("index", "索引"),
    ("position", "位置"),
    ("location", "位置"),
    ("source", "源"),
    ("target", "目标"),
    ("destination", "目的地"),
    ("input", "输入"),
    ("output", "输出"),
    ("internal", "内部"),
    ("external", "外部"),
    ("local", "本地"),
    ("remote", "远程"),
    ("global", "全局"),
    ("single", "单个"),
    ("multiple", "多个"),
    ("simple", "简单"),
    ("complex", "复杂"),
    ("unknown", "未知"),
    ("invalid", "无效"),
    ("valid", "有效"),
    ("empty", "空"),
    ("full", "满"),
    ("enabled", "启用"),
    ("disabled", "禁用"),
    ("active", "活跃"),
    ("inactive", "不活跃"),
    ("success", "成功"),
    ("failure", "失败"),
    ("warning", "警告"),
    ("danger", "危险"),
    ("secure", "安全"),
    ("unsecure", "不安全"),
    ("unique", "唯一"),
    ("common", "通用"),
    ("special", "特殊"),
    ("normal", "普通"),
    ("standard", "标准"),
    ("advanced", "高级"),
    ("basic", "基础"),
    ("primary", "主要"),
    ("secondary", "次要"),
    ("optional", "可选"),
    ("required", "必需"),
    ("mandatory", "必需"),
    ("generic", "泛型"),
    ("dynamic", "动态"),
    ("fixed", "固定"),
    ("random", "随机"),
];

/// crate 名 → 中文名特例表（常见 crate，保证输出准确）
const CRATE_NAME_EXCEPTIONS: &[(&str, &str)] = &[
    ("anyhow", "任意错误"),
    ("serde", "序列化"),
    ("serde_json", "JSON序列化"),
    ("serde_derive", "序列化推导"),
    ("toml", "TOML处理"),
    ("clap", "命令行解析"),
    ("tokio", "异步运行时"),
    ("thiserror", "错误推导"),
    ("log", "日志"),
    ("env_logger", "日志初始化"),
    ("tracing", "追踪日志"),
    ("reqwest", "HTTP客户端"),
    ("ureq", "轻量HTTP客户端"),
    ("rand", "随机数"),
    ("chrono", "日期时间"),
    ("regex", "正则表达式"),
    ("itertools", "迭代器工具"),
    ("rayon", "并行计算"),
    ("nom", "解析组合子"),
    ("syn", "语法树解析"),
    ("quote", "代码生成"),
    ("proc_macro2", "过程宏"),
    ("csv", "CSV处理"),
    ("html", "HTML处理"),
    ("url", "网址处理"),
    ("uuid", "UUID标识"),
    ("glob", "通配符匹配"),
    ("walkdir", "目录遍历"),
    ("zip", "ZIP压缩"),
    ("tar", "TAR归档"),
    ("flate2", "压缩解压"),
    ("sha2", "SHA2哈希"),
    ("hex", "十六进制"),
    ("base64", "Base64编码"),
    ("percent_encoding", "百分号编码"),
];

/// 按目标语言生成 crate 名称：zh 用特例表/拆词翻译，其他语言保留英文原名
pub fn generate_crate_localized_name(lang: &str, crate_name: &str) -> String {
    if lang == "zh" {
        generate_crate_chinese_name(crate_name)
    } else {
        crate_name.to_string()
    }
}

/// 生成 crate 的中文名（特例表优先，未命中拆词翻译，仍未知则保留原名）
pub fn generate_crate_chinese_name(crate_name: &str) -> String {
    if let Some((_, chinese)) = CRATE_NAME_EXCEPTIONS
        .iter()
        .find(|(en, _)| *en == crate_name)
    {
        return chinese.to_string();
    }
    let word_seq = split_words(crate_name);
    if word_seq.is_empty() {
        return crate_name.to_string();
    }
    let translations: Vec<String> = word_seq
        .iter()
        .map(|word| {
            WORD_TABLE
                .iter()
                .find(|(en, _)| *en == word)
                .map(|(_, zh)| zh.to_string())
                .unwrap_or_else(|| word.clone())
        })
        .collect();
    let result = translations.concat();
    if result == crate_name {
        crate_name.to_string()
    } else {
        result
    }
}

/// 按目标语言生成 API 名称：zh 走规则生成中文名，其他语言保留英文原名
pub fn rule_generate_localized_name(lang: &str, english_name: &str) -> String {
    if lang == "zh" {
        rule_generate_chinese_name(english_name)
    } else {
        english_name.to_string()
    }
}

/// 规则驱动：根据英文名生成中文名（整名特例 → 前缀规则 → 拆词翻译 → 原名）
pub fn rule_generate_chinese_name(english_name: &str) -> String {
    // 1. 整名特例
    if let Some((_, chinese)) = WHOLE_NAME_EXCEPTIONS
        .iter()
        .find(|(en, _)| *en == english_name)
    {
        return chinese.to_string();
    }
    // 2. 前缀规则（按长度降序匹配，如 get_value → 获取值）
    let mut prefix_candidates: Vec<&(&str, &str)> = PREFIX_RULES
        .iter()
        .filter(|(prefix, _)| english_name.starts_with(prefix) && english_name.len() > prefix.len())
        .collect();
    prefix_candidates.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
    if let Some((prefix, chinese)) = prefix_candidates.first() {
        let remainder = &english_name[prefix.len()..];
        return format!("{}{}", chinese, translate_words(remainder));
    }
    // 3. 拆词翻译（如 SerializeValue → 序列化值）
    let translation = translate_words(english_name);
    if !translation.is_empty() {
        return translation;
    }
    // 4. 兆底：保留原名
    english_name.to_string()
}

/// 把标识符拆成小写单词序列（支持 snake_case / 驼峰 / 连字符）
fn split_words(identifier: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_was_upper = false;
    for c in identifier.chars() {
        if c == '_' || c == '-' {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
            prev_was_upper = false;
            continue;
        }
        if c.is_ascii_uppercase() {
            if !current.is_empty() && !prev_was_upper {
                words.push(current.clone());
                current.clear();
            }
            current.push(c.to_ascii_lowercase());
            prev_was_upper = true;
        } else {
            current.push(c);
            prev_was_upper = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// 拆词后逐词查词表翻译并拼接
fn translate_words(english_name: &str) -> String {
    let word_seq = split_words(english_name);
    if word_seq.is_empty() {
        return String::new();
    }
    let all_known = word_seq
        .iter()
        .all(|w| WORD_TABLE.iter().any(|(en, _)| en == w));
    if !all_known {
        return String::new();
    }
    word_seq
        .iter()
        .map(|w| {
            WORD_TABLE
                .iter()
                .find(|(en, _)| en == w)
                .map(|(_, zh)| *zh)
                .unwrap_or(w)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_chinese_name() {
        assert_eq!(rule_generate_chinese_name("new"), "新建");
        assert_eq!(rule_generate_chinese_name("get_value"), "获取值");
        assert_eq!(rule_generate_chinese_name("set_name"), "设置名称");
        assert_eq!(rule_generate_chinese_name("is_empty"), "是否为空");
        assert_eq!(rule_generate_chinese_name("from_str"), "从字符串解析");
        assert_eq!(rule_generate_chinese_name("to_string"), "转字符串");
        assert_eq!(rule_generate_chinese_name("Error"), "错误");
        assert_eq!(rule_generate_chinese_name("Client"), "客户端");
        assert_eq!(rule_generate_chinese_name("Serialize"), "序列化");
        assert_eq!(rule_generate_chinese_name("Deserialize"), "反序列化");
        assert_eq!(rule_generate_chinese_name("未知标识符"), "未知标识符");
    }

    #[test]
    fn test_crate_chinese_name() {
        assert_eq!(generate_crate_chinese_name("anyhow"), "任意错误");
        assert_eq!(generate_crate_chinese_name("serde"), "序列化");
        assert_eq!(generate_crate_chinese_name("serde_json"), "JSON序列化");
        assert_eq!(generate_crate_chinese_name("tokio"), "异步运行时");
        // 未命中特例时拆词翻译
        assert_eq!(generate_crate_chinese_name("error_handler"), "错误处理器");
        // 无法翻译时保留原名
        assert_eq!(generate_crate_chinese_name("zxxyzq"), "zxxyzq");
    }

    #[test]
    fn test_split_words() {
        assert_eq!(split_words("get_value"), vec!["get", "value"]);
        assert_eq!(split_words("SerializeValue"), vec!["serialize", "value"]);
        assert_eq!(split_words("into_iter"), vec!["into", "iter"]);
        assert_eq!(split_words("JSON"), vec!["json"]);
    }

    /// 关键字冲突检测：中文名在 keywords.toml 中映射为不同英文时应报告冲突
    #[test]
    fn test_keyword_conflict_detection() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("keywords.toml"),
            "[\"类型\"]\n\"错误\" = \"Err\"\n\"结果\" = \"Result\"\n",
        )
        .unwrap();
        let chinese_name_table = vec![
            ("错误".to_string(), "Error".to_string()),
            ("结果".to_string(), "Result".to_string()),
            ("上下文".to_string(), "Context".to_string()),
        ];
        let conflicts = detect_keyword_conflicts(temp.path(), &chinese_name_table);
        assert_eq!(conflicts.len(), 1, "应只有 '错误' 冲突: {:?}", conflicts);
        assert_eq!(
            conflicts[0],
            ("错误".to_string(), "Err".to_string(), "Error".to_string())
        );
        // 语言包目录不存在时返回空
        assert!(
            detect_keyword_conflicts(&temp.path().join("不存在"), &chinese_name_table).is_empty()
        );
    }
}
