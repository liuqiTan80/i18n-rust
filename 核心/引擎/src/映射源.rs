//! 映射源模块 - 从 TOML 文件加载映射表
//!
//! 提供映射数据的加载和管理，支持按类别组织：
//! - 关键字映射（词法处理阶段）
//! - 标准库映射（语义处理阶段）
//! - 第三方库映射（按需加载）

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value;

/// 映射表分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum 映射分类 {
    /// 关键字（词法处理阶段）
    关键字,
    /// 标准库（语义处理阶段）
    标准库,
    /// 第三方库（按需加载）
    第三方库,
}

impl 映射分类 {
    /// 获取分类对应的默认文件名
    pub fn 默认文件名(&self) -> &'static str {
        match self {
            映射分类::关键字 => "关键字.toml",
            映射分类::标准库 => "标准库.toml",
            映射分类::第三方库 => "第三方库.toml",
        }
    }

    /// 获取分类名称
    pub fn 名称(&self) -> &'static str {
        match self {
            映射分类::关键字 => "关键字",
            映射分类::标准库 => "标准库",
            映射分类::第三方库 => "第三方库",
        }
    }
}

/// 映射表加载器 - 从 TOML 文件加载映射数据
#[derive(Debug, Clone)]
pub struct 映射表加载器 {
    /// 映射表根目录
    根目录: PathBuf,
    /// 已加载的映射表（按分类和子分类组织）
    映射表: HashMap<映射分类, HashMap<String, HashMap<String, String>>>,
}

impl 映射表加载器 {
    /// 创建新的加载器
    pub fn 新建<P: AsRef<Path>>(语言包路径: P) -> Self {
        let 根目录 = 语言包路径.as_ref().join("映射表");
        Self {
            根目录,
            映射表: HashMap::new(),
        }
    }

    /// 加载指定分类的映射表
    pub fn 加载(&mut self, 分类: 映射分类) -> Result<(), String> {
        // 第三方库是目录，包含多个文件
        if 分类 == 映射分类::第三方库 {
            return self.加载第三方库目录();
        }
        
        let 文件路径 = self.根目录.join(分类.默认文件名());
        
        if !文件路径.exists() {
            return Err(format!("映射表文件不存在: {:?}", 文件路径));
        }

        let 内容 = fs::read_to_string(&文件路径)
            .map_err(|e| format!("读取映射表失败: {}", e))?;

        let 值: Value = 内容.parse()
            .map_err(|e| format!("解析映射表失败: {}", e))?;

        let 分类映射 = self.解析_toml值(值)?;
        self.映射表.insert(分类, 分类映射);
        Ok(())
    }

    /// 解析 TOML 值为映射表
    fn 解析_toml值(&self, 值: Value) -> Result<HashMap<String, HashMap<String, String>>, String> {
        let mut 分类映射 = HashMap::new();

        if let Value::Table(表) = 值 {
            for (子分类名, 子分类值) in 表 {
                if let Value::Table(子表) = 子分类值 {
                    let mut 子映射 = HashMap::new();
                    for (中文, 英文值) in 子表 {
                        if let Value::String(英文) = 英文值 {
                            子映射.insert(中文, 英文.clone());
                        }
                    }
                    if !子映射.is_empty() {
                        分类映射.insert(子分类名, 子映射);
                    }
                }
            }
        }

        Ok(分类映射)
    }

    /// 加载第三方库目录下的所有 TOML 文件
    fn 加载第三方库目录(&mut self) -> Result<(), String> {
        let 目录路径 = self.根目录.join("第三方库");
        
        if !目录路径.exists() {
            // 目录不存在，静默返回
            return Ok(());
        }

        let mut 合并映射 = HashMap::new();

        // 遍历目录中的所有 .toml 文件
        let 条目 = fs::read_dir(&目录路径)
            .map_err(|e| format!("读取第三方库目录失败: {}", e))?;

        for 条目 in 条目 {
            let 条目 = 条目.map_err(|e| format!("读取目录项失败: {}", e))?;
            let 路径 = 条目.path();
            
            if 路径.extension().and_then(|s| s.to_str()) == Some("toml") {
                // 获取文件名（不含扩展名）作为分类标识
                let 文件名 = 路径.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let 内容 = fs::read_to_string(&路径)
                    .map_err(|e| format!("读取映射表 {:?} 失败: {}", 路径, e))?;

                let 值: Value = 内容.parse()
                    .map_err(|e| format!("解析映射表 {:?} 失败: {}", 路径, e))?;

                // 将文件中的映射合并，加上文件分类前缀
                if let Value::Table(表) = 值 {
                    for (子分类名, 子分类值) in 表 {
                        if let Value::Table(子表) = 子分类值 {
                            let mut 子映射 = HashMap::new();
                            for (中文, 英文值) in 子表 {
                                if let Value::String(英文) = 英文值 {
                                    子映射.insert(中文, 英文.clone());
                                }
                            }
                            if !子映射.is_empty() {
                                // 使用 "文件名/子分类" 作为键
                                let 分类键 = format!("{}/{}", 文件名, 子分类名);
                                合并映射.insert(分类键, 子映射);
                            }
                        }
                    }
                }
            }
        }

        self.映射表.insert(映射分类::第三方库, 合并映射);
        Ok(())
    }

    /// 加载所有默认映射表
    pub fn 加载全部(&mut self) -> Result<(), String> {
        self.加载(映射分类::关键字)?;
        self.加载(映射分类::标准库)?;
        // 第三方库可选加载
        if self.根目录.join("第三方库").is_dir() {
            let _ = self.加载(映射分类::第三方库);
        }
        Ok(())
    }

    /// 获取指定分类的完整映射表（扁平化）
    pub fn 获取映射(&self, 分类: 映射分类) -> HashMap<String, String> {
        let mut 结果 = HashMap::new();
        
        if let Some(分类映射) = self.映射表.get(&分类) {
            for 子映射 in 分类映射.values() {
                for (中文, 英文) in 子映射 {
                    结果.insert(中文.clone(), 英文.clone());
                }
            }
        }
        
        结果
    }

    /// 获取指定分类和子分类的映射表
    pub fn 获取子映射(&self, 分类: 映射分类, 子分类: &str) -> Option<&HashMap<String, String>> {
        self.映射表
            .get(&分类)
            .and_then(|m| m.get(子分类))
    }

    /// 查询映射
    pub fn 查询(&self, 分类: 映射分类, 中文: &str) -> Option<String> {
        if let Some(分类映射) = self.映射表.get(&分类) {
            for 子映射 in 分类映射.values() {
                if let Some(英文) = 子映射.get(中文) {
                    return Some(英文.clone());
                }
            }
        }
        None
    }

    /// 反向查询（从英文查中文）
    pub fn 反向查询(&self, 分类: 映射分类, 英文: &str) -> Option<String> {
        if let Some(分类映射) = self.映射表.get(&分类) {
            for 子映射 in 分类映射.values() {
                for (中文, e) in 子映射 {
                    if e == 英文 {
                        return Some(中文.clone());
                    }
                }
            }
        }
        None
    }

    /// 获取所有子分类名称
    pub fn 获取子分类(&self, 分类: 映射分类) -> Vec<String> {
        self.映射表
            .get(&分类)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// 统计映射条目数
    pub fn 条目数(&self, 分类: 映射分类) -> usize {
        self.获取映射(分类).len()
    }
}

/// 便捷函数：加载关键字映射
pub fn 加载关键字映射<P: AsRef<Path>>(语言包路径: P) -> Result<HashMap<String, String>, String> {
    let mut 加载器 = 映射表加载器::新建(语言包路径);
    加载器.加载(映射分类::关键字)?;
    Ok(加载器.获取映射(映射分类::关键字))
}

/// 便捷函数：加载标准库映射
pub fn 加载标准库映射<P: AsRef<Path>>(语言包路径: P) -> Result<HashMap<String, String>, String> {
    let mut 加载器 = 映射表加载器::新建(语言包路径);
    加载器.加载(映射分类::标准库)?;
    Ok(加载器.获取映射(映射分类::标准库))
}

/// 便捷函数：加载所有映射
pub fn 加载所有映射<P: AsRef<Path>>(语言包路径: P) -> Result<HashMap<映射分类, HashMap<String, String>>, String> {
    let mut 加载器 = 映射表加载器::新建(语言包路径);
    加载器.加载全部()?;
    
    let mut 结果 = HashMap::new();
    结果.insert(映射分类::关键字, 加载器.获取映射(映射分类::关键字));
    结果.insert(映射分类::标准库, 加载器.获取映射(映射分类::标准库));
    结果.insert(映射分类::第三方库, 加载器.获取映射(映射分类::第三方库));
    
    Ok(结果)
}

/// 创建默认的关键字映射（内置备用）
pub fn 创建内置关键字映射() -> HashMap<String, String> {
    let mut 映射 = HashMap::new();
    
    // 声明关键字
    映射.insert("函数".into(), "fn".into());
    映射.insert("变量".into(), "let".into());
    映射.insert("可变".into(), "mut".into());
    映射.insert("常量".into(), "const".into());
    映射.insert("结构体".into(), "struct".into());
    映射.insert("枚举".into(), "enum".into());
    映射.insert("实现".into(), "impl".into());
    映射.insert("特征".into(), "trait".into());
    映射.insert("类型".into(), "type".into());
    映射.insert("模块".into(), "mod".into());
    映射.insert("公开".into(), "pub".into());
    映射.insert("使用".into(), "use".into());
    映射.insert("作为".into(), "as".into());
    映射.insert("包".into(), "crate".into());
    映射.insert("超级".into(), "super".into());
    映射.insert("外部".into(), "extern".into());
    映射.insert("静态".into(), "static".into());
    
    // 控制流
    映射.insert("如果".into(), "if".into());
    映射.insert("否则".into(), "else".into());
    映射.insert("匹配".into(), "match".into());
    映射.insert("循环".into(), "loop".into());
    映射.insert("当".into(), "while".into());
    映射.insert("对于".into(), "for".into());
    映射.insert("在".into(), "in".into());
    映射.insert("中断".into(), "break".into());
    映射.insert("继续".into(), "continue".into());
    映射.insert("返回".into(), "return".into());
    
    // 基本类型
    映射.insert("整数".into(), "i32".into());
    映射.insert("长整数".into(), "i64".into());
    映射.insert("浮点数".into(), "f64".into());
    映射.insert("单精度浮点数".into(), "f32".into());
    映射.insert("文本".into(), "str".into());
    映射.insert("布尔".into(), "bool".into());
    映射.insert("字符".into(), "char".into());
    映射.insert("字节".into(), "u8".into());
    
    // 特殊值
    映射.insert("真".into(), "true".into());
    映射.insert("假".into(), "false".into());
    映射.insert("空".into(), "()".into());
    映射.insert("自我".into(), "self".into());
    映射.insert("自身".into(), "Self".into());
    
    // 错误处理
    映射.insert("结果".into(), "Result".into());
    映射.insert("选项".into(), "Option".into());
    映射.insert("有些".into(), "Some".into());
    映射.insert("无".into(), "None".into());
    映射.insert("成功".into(), "Ok".into());
    映射.insert("错误".into(), "Err".into());
    
    // 内存
    映射.insert("引用".into(), "&".into());
    映射.insert("解引用".into(), "*".into());
    映射.insert("移动".into(), "move".into());
    映射.insert("盒子".into(), "Box".into());
    
    // 异步
    映射.insert("异步".into(), "async".into());
    映射.insert("等待".into(), "await".into());
    映射.insert("不安全".into(), "unsafe".into());
    映射.insert("动态".into(), "dyn".into());
    
    映射
}

#[cfg(test)]
mod 测试 {
    use super::*;

    #[test]
    fn 测试_加载关键字映射() {
        // 创建临时测试目录
        let 临时目录 = std::env::temp_dir().join("i18n_映射表测试");
        let 映射表目录 = 临时目录.join("映射表");
        fs::create_dir_all(&映射表目录).unwrap();
        
        // 写入测试映射表
        let 测试内容 = r#"
["声明"]
"函数" = "fn"
"变量" = "let"

["控制流"]
"如果" = "if"
"否则" = "else"
"#;
        fs::write(映射表目录.join("关键字.toml"), 测试内容).unwrap();
        
        // 测试加载
        let mut 加载器 = 映射表加载器::新建(&临时目录);
        assert!(加载器.加载(映射分类::关键字).is_ok());
        
        // 测试查询
        assert_eq!(加载器.查询(映射分类::关键字, "函数"), Some("fn".to_string()));
        assert_eq!(加载器.查询(映射分类::关键字, "如果"), Some("if".to_string()));
        assert_eq!(加载器.查询(映射分类::关键字, "不存在"), None);
        
        // 测试反向查询
        assert_eq!(加载器.反向查询(映射分类::关键字, "fn"), Some("函数".to_string()));
        
        // 测试子分类
        let 子分类 = 加载器.获取子分类(映射分类::关键字);
        assert!(子分类.contains(&"声明".to_string()));
        assert!(子分类.contains(&"控制流".to_string()));
        
        // 测试条目数
        assert_eq!(加载器.条目数(映射分类::关键字), 4);
        
        // 清理
        fs::remove_dir_all(&临时目录).ok();
    }

    #[test]
    fn 测试_获取扁平化映射() {
        let 临时目录 = std::env::temp_dir().join("i18n_映射表测试2");
        let 映射表目录 = 临时目录.join("映射表");
        fs::create_dir_all(&映射表目录).unwrap();
        
        let 测试内容 = r#"
["分类A"]
"甲" = "alpha"
"乙" = "beta"

["分类B"]
"丙" = "gamma"
"#;
        fs::write(映射表目录.join("标准库.toml"), 测试内容).unwrap();
        
        let mut 加载器 = 映射表加载器::新建(&临时目录);
        加载器.加载(映射分类::标准库).unwrap();
        
        let 映射 = 加载器.获取映射(映射分类::标准库);
        assert_eq!(映射.len(), 3);
        assert_eq!(映射.get("甲"), Some(&"alpha".to_string()));
        assert_eq!(映射.get("丙"), Some(&"gamma".to_string()));
        
        fs::remove_dir_all(&临时目录).ok();
    }

    #[test]
    fn 测试_内置关键字映射() {
        let 映射 = 创建内置关键字映射();
        
        assert_eq!(映射.get("函数"), Some(&"fn".to_string()));
        assert_eq!(映射.get("如果"), Some(&"if".to_string()));
        assert_eq!(映射.get("整数"), Some(&"i32".to_string()));
        assert!(映射.len() > 30);
    }
}
