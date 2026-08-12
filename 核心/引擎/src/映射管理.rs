use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct 映射管理器 {
    pub 关键字映射: HashMap<String, String>,
    节映射表: HashMap<String, HashMap<String, String>>,
    pub 模块路径映射: HashMap<String, String>,
    pub 标识符别名映射: HashMap<String, String>,
}

impl 映射管理器 {
    /// 从语言包目录加载全部映射（推荐）
    pub fn 从目录加载(语言包目录: &Path) -> Result<Self, String> {
        let 关键字路径 = 语言包目录.join("关键字.toml");
        if !关键字路径.exists() {
            return Err(format!("关键字文件不存在: {}", 关键字路径.display()));
        }

        // 1. 加载关键字映射
        let 关键字内容 = fs::read_to_string(&关键字路径)
            .map_err(|e| format!("无法读取关键字文件: {}", e))?;
        let 根: toml::Value = toml::from_str(&关键字内容)
            .map_err(|e| format!("解析关键字 TOML 失败: {}", e))?;

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

        // 2. 加载模块路径映射（来自 模块路径.toml）
        let 模块路径文件 = 语言包目录.join("模块路径.toml");
        let mut 模块路径映射 = HashMap::new();
        if 模块路径文件.exists() {
            let 内容 = fs::read_to_string(&模块路径文件)
                .map_err(|e| format!("无法读取模块路径文件: {}", e))?;
            let 根: toml::Value = toml::from_str(&内容)
                .map_err(|e| format!("解析模块路径 TOML 失败: {}", e))?;
            if let toml::Value::Table(表) = 根 {
                if let Some(路径节) = 表.get("模块路径") {
                    if let toml::Value::Table(条目表) = 路径节 {
                        for (中文, 英文值) in 条目表 {
                            if let toml::Value::String(英文) = 英文值 {
                                模块路径映射.insert(中文.clone(), 英文.clone());
                            }
                        }
                    }
                }
            }
        }

        // 3. 扫描第三方库目录
        let 第三方库目录 = 语言包目录.join("第三方库");
        let mut 标识符别名映射 = HashMap::new();
        if 第三方库目录.exists() && 第三方库目录.is_dir() {
            if let Ok(条目) = fs::read_dir(&第三方库目录) {
                for 文件结果 in 条目 {
                    if let Ok(文件) = 文件结果 {
                        let 文件路径 = 文件.path();
                        if 文件路径.extension().and_then(|e| e.to_str()) == Some("toml") {
                            if let Ok(内容) = fs::read_to_string(&文件路径) {
                                if let Ok(根) = toml::from_str::<toml::Value>(&内容) {
                                    if let toml::Value::Table(表) = 根 {
                                        // 提取 ["模块路径"] -> 模块路径映射
                                        if let Some(模块节) = 表.get("模块路径") {
                                            if let toml::Value::Table(条目表) = 模块节 {
                                                for (中文, 英文值) in 条目表 {
                                                    if let toml::Value::String(英文) = 英文值 {
                                                        模块路径映射.insert(中文.clone(), 英文.clone());
                                                    }
                                                }
                                            }
                                        }
                                        // 提取 ["标识符"] -> 标识符别名映射
                                        if let Some(标识符节) = 表.get("标识符") {
                                            if let toml::Value::Table(条目表) = 标识符节 {
                                                for (中文, 英文值) in 条目表 {
                                                    if let toml::Value::String(英文) = 英文值 {
                                                        标识符别名映射.insert(中文.clone(), 英文.clone());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(Self {
            关键字映射,
            节映射表,
            模块路径映射,
            标识符别名映射,
        })
    }

    /// 向后兼容：从单个关键字文件加载（委托给从目录加载）
    pub fn 从文件加载(路径: &Path) -> Result<Self, String> {
        let 目录 = 路径.parent().unwrap_or(Path::new("."));
        Self::从目录加载(目录)
    }

    pub fn 查询(&self, 中文关键字: &str) -> Option<&String> {
        self.关键字映射.get(中文关键字)
    }

    pub fn 获取映射表(&self) -> &HashMap<String, String> {
        &self.关键字映射
    }

    pub fn 获取节映射(&self, 节名: &str) -> Option<&HashMap<String, String>> {
        self.节映射表.get(节名)
    }

    pub fn 获取模块路径映射(&self) -> &HashMap<String, String> {
        &self.模块路径映射
    }

    pub fn 获取标识符别名映射(&self) -> &HashMap<String, String> {
        &self.标识符别名映射
    }
}