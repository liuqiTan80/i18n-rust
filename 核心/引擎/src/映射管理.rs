// 模块：映射管理
// 功能：从 TOML 文件加载语言包，提供关键字（中文→英文）映射查询接口，支持按节名提取

use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct 映射管理器 {
    pub 关键字映射: HashMap<String, String>,
    节映射表: HashMap<String, HashMap<String, String>>,
}

impl 映射管理器 {
    pub fn 从文件加载(路径: &Path) -> Result<Self, String> {
        let 内容 = fs::read_to_string(路径)
            .map_err(|e| format!("无法读取语言包文件 {}: {}", 路径.display(), e))?;
        let 根: toml::Value = toml::from_str(&内容)
            .map_err(|e| format!("解析语言包 TOML 失败: {}", e))?;

        let mut 关键字映射 = HashMap::new();
        let mut 节映射表 = HashMap::new();

        if let toml::Value::Table(表) = 根 {
            for (节名, 节内容) in 表 {
                if let toml::Value::Table(条目表) = 节内容 {
                    let mut 节映射 = HashMap::new();
                    for (中文, 英文值) in 条目表 {
                        if let toml::Value::String(英文) = 英文值 {
                            节映射.insert(中文.clone(), 英文.clone());
                            关键字映射.insert(中文.clone(), 英文.clone());
                        }
                    }
                    节映射表.insert(节名.clone(), 节映射);
                }
            }
        }

        Ok(Self {
            关键字映射,
            节映射表,
        })
    }

    pub fn 查询(&self, 中文关键字: &str) -> Option<&String> {
        self.关键字映射.get(中文关键字)
    }

    pub fn 获取映射表(&self) -> &HashMap<String, String> {
        &self.关键字映射
    }

    /// 获取指定节的中文→英文映射
    pub fn 获取节映射(&self, 节名: &str) -> Option<&HashMap<String, String>> {
        self.节映射表.get(节名)
    }
}


#[cfg(test)]
mod 测试 {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn 创建测试文件(内容: &str) -> NamedTempFile {
        let mut 文件 = NamedTempFile::new().expect("创建临时文件失败");
        write!(文件, "{}", 内容).expect("写入临时文件失败");
        文件
    }

    #[test]
    fn 测试加载多节映射() {
        let toml内容 = r#"
["声明"]
"函数" = "fn"
"让" = "let"

["控制流"]
"如果" = "if"
"否则" = "else"

["类型"]
"整数" = "i32"
"文本" = "str"
"#;
        let 文件 = 创建测试文件(toml内容);
        let 管理器 = 映射管理器::从文件加载(文件.path()).unwrap();
        assert_eq!(管理器.查询("函数").unwrap(), "fn");
        assert_eq!(管理器.查询("否则").unwrap(), "else");
        assert_eq!(管理器.查询("整数").unwrap(), "i32");

        let 类型节 = 管理器.获取节映射("类型").unwrap();
        assert_eq!(类型节.get("整数").unwrap(), "i32");
    }
}