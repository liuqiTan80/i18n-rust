//! 规则驱动的中文名生成：整名特例 → 前缀规则 → 拆词翻译，完全离线；
//! 另含生成名与语言包关键字映射的冲突检测。

use std::fs;
use std::path::Path;

/// 检测中文名与语言包关键字映射的冲突。
///
/// 引擎的词法转译（关键字映射）先于别名替换执行，因此若生成的中文名
/// 在 keywords.toml 中已映射为不同英文（如 `错误` → `Err`），则本映射不会生效。
/// 返回冲突列表：(中文名, 关键字映射的英文, 本映射的英文)；语言包目录不存在时返回空。
pub fn 检测关键字冲突(
    语言包目录: &Path,
    中文名表: &[(String, String)],
) -> Vec<(String, String, String)> {
    let Ok(内容) = fs::read_to_string(语言包目录.join("keywords.toml")) else {
        return Vec::new();
    };
    // 复用引擎权威语义（按节名升序合并、后到覆盖），与运行时生效的
    // 关键字映射一致；否则按文档顺序先到先得会误报冲突
    let Ok(节表) = i18n_rust_engine::映射源::解析配置节(&内容) else {
        return Vec::new();
    };
    let 关键词映射表 = i18n_rust_engine::映射源::摊平节表(&节表);
    let mut 冲突表 = Vec::new();
    for (中文, 英文) in 中文名表 {
        if let Some(关键词英文) = 关键词映射表.get(中文)
            && 关键词英文 != 英文
        {
            冲突表.push((中文.clone(), 关键词英文.clone(), 英文.clone()));
        }
    }
    冲突表
}

/// 整名特例（优先于一切规则，保证常见 API 的中文名准确直观）
const 整名特例表: &[(&str, &str)] = &[
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
const 前缀规则表: &[(&str, &str)] = &[
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
const 单词翻译表: &[(&str, &str)] = &[
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
const 库名特例表: &[(&str, &str)] = &[
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
pub fn 生成库名本地化(语言: &str, 库名: &str) -> String {
    if 语言 == "zh" {
        生成库名中文名(库名)
    } else {
        库名.to_string()
    }
}

/// 生成 crate 的中文名（特例表优先，未命中拆词翻译，仍未知则保留原名）
pub fn 生成库名中文名(库名: &str) -> String {
    if let Some((_, 中文)) = 库名特例表.iter().find(|(英文, _)| *英文 == 库名) {
        return 中文.to_string();
    }
    let 词序列 = 拆分单词(库名);
    if 词序列.is_empty() {
        return 库名.to_string();
    }
    let 译文表: Vec<String> = 词序列
        .iter()
        .map(|词| {
            单词翻译表
                .iter()
                .find(|(英文, _)| *英文 == 词)
                .map(|(_, 中文)| 中文.to_string())
                .unwrap_or_else(|| 词.clone())
        })
        .collect();
    let 拼接结果 = 译文表.concat();
    if 拼接结果 == 库名 {
        库名.to_string()
    } else {
        拼接结果
    }
}

/// 按目标语言生成 API 名称：zh 走规则生成中文名，其他语言保留英文原名
pub fn 规则生成本地化名(语言: &str, 英文名: &str) -> String {
    if 语言 == "zh" {
        规则生成中文名(英文名)
    } else {
        英文名.to_string()
    }
}

/// 规则驱动：根据英文名生成中文名（整名特例 → 前缀规则 → 拆词翻译 → 原名）
pub fn 规则生成中文名(英文名: &str) -> String {
    // 1. 整名特例
    if let Some((_, 中文)) = 整名特例表.iter().find(|(英文, _)| *英文 == 英文名) {
        return 中文.to_string();
    }
    // 2. 前缀规则（按长度降序匹配，如 get_value → 获取值）
    let mut 前缀候选: Vec<&(&str, &str)> = 前缀规则表
        .iter()
        .filter(|(前缀, _)| 英文名.starts_with(前缀) && 英文名.len() > 前缀.len())
        .collect();
    前缀候选.sort_by_key(|(前缀, _)| std::cmp::Reverse(前缀.len()));
    if let Some((前缀, 中文)) = 前缀候选.first() {
        let 剩余串 = &英文名[前缀.len()..];
        return format!("{}{}", 中文, 翻译单词(剩余串));
    }
    // 3. 拆词翻译（如 SerializeValue → 序列化值）
    let 译文 = 翻译单词(英文名);
    if !译文.is_empty() {
        return 译文;
    }
    // 4. 兆底：保留原名
    英文名.to_string()
}

/// 把标识符拆成小写单词序列（支持 snake_case / 驼峰 / 连字符）
fn 拆分单词(原名: &str) -> Vec<String> {
    let mut 词列表 = Vec::new();
    let mut 当前词 = String::new();
    let mut 先前大写 = false;
    for 每字符 in 原名.chars() {
        if 每字符 == '_' || 每字符 == '-' {
            if !当前词.is_empty() {
                词列表.push(当前词.clone());
                当前词.clear();
            }
            先前大写 = false;
            continue;
        }
        if 每字符.is_ascii_uppercase() {
            if !当前词.is_empty() && !先前大写 {
                词列表.push(当前词.clone());
                当前词.clear();
            }
            当前词.push(每字符.to_ascii_lowercase());
            先前大写 = true;
        } else {
            当前词.push(每字符);
            先前大写 = false;
        }
    }
    if !当前词.is_empty() {
        词列表.push(当前词);
    }
    词列表
}

/// 拆词后逐词查词表翻译并拼接
fn 翻译单词(英文名: &str) -> String {
    let 词序列 = 拆分单词(英文名);
    if 词序列.is_empty() {
        return String::new();
    }
    let 全部已知 = 词序列
        .iter()
        .all(|词| 单词翻译表.iter().any(|(英文, _)| 英文 == 词));
    if !全部已知 {
        return String::new();
    }
    词序列
        .iter()
        .map(|词| {
            单词翻译表
                .iter()
                .find(|(英文, _)| 英文 == 词)
                .map(|(_, 中文)| *中文)
                .unwrap_or(词)
        })
        .collect()
}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    #[test]
    fn 测试规则中文名() {
        assert_eq!(规则生成中文名("new"), "新建");
        assert_eq!(规则生成中文名("get_value"), "获取值");
        assert_eq!(规则生成中文名("set_name"), "设置名称");
        assert_eq!(规则生成中文名("is_empty"), "是否为空");
        assert_eq!(规则生成中文名("from_str"), "从字符串解析");
        assert_eq!(规则生成中文名("to_string"), "转字符串");
        assert_eq!(规则生成中文名("Error"), "错误");
        assert_eq!(规则生成中文名("Client"), "客户端");
        assert_eq!(规则生成中文名("Serialize"), "序列化");
        assert_eq!(规则生成中文名("Deserialize"), "反序列化");
        assert_eq!(规则生成中文名("未知标识符"), "未知标识符");
    }

    #[test]
    fn 测试库名中文名() {
        assert_eq!(生成库名中文名("anyhow"), "任意错误");
        assert_eq!(生成库名中文名("serde"), "序列化");
        assert_eq!(生成库名中文名("serde_json"), "JSON序列化");
        assert_eq!(生成库名中文名("tokio"), "异步运行时");
        // 未命中特例时拆词翻译
        assert_eq!(生成库名中文名("error_handler"), "错误处理器");
        // 无法翻译时保留原名
        assert_eq!(生成库名中文名("zxxyzq"), "zxxyzq");
    }

    #[test]
    fn 测试拆分单词() {
        assert_eq!(拆分单词("get_value"), vec!["get", "value"]);
        assert_eq!(拆分单词("SerializeValue"), vec!["serialize", "value"]);
        assert_eq!(拆分单词("into_iter"), vec!["into", "iter"]);
        assert_eq!(拆分单词("JSON"), vec!["json"]);
    }

    /// 关键字冲突检测：中文名在 keywords.toml 中映射为不同英文时应报告冲突
    #[test]
    fn 测试关键字冲突检测() {
        let 临时 = tempfile::tempdir().unwrap();
        fs::write(
            临时.path().join("keywords.toml"),
            "[\"类型\"]\n\"错误\" = \"Err\"\n\"结果\" = \"Result\"\n",
        )
        .unwrap();
        let 中文名表 = vec![
            ("错误".to_string(), "Error".to_string()),
            ("结果".to_string(), "Result".to_string()),
            ("上下文".to_string(), "Context".to_string()),
        ];
        let 冲突表 = 检测关键字冲突(临时.path(), &中文名表);
        assert_eq!(冲突表.len(), 1, "应只有 '错误' 冲突: {:?}", 冲突表);
        assert_eq!(
            冲突表[0],
            ("错误".to_string(), "Err".to_string(), "Error".to_string())
        );
        // 语言包目录不存在时返回空
        assert!(检测关键字冲突(&临时.path().join("不存在"), &中文名表).is_empty());
    }
}
