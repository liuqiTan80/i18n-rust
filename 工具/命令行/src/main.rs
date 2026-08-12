use clap::{Parser, Subcommand};
use i18n_rust_engine::词法处理;
use i18n_rust_engine::映射管理::映射管理器;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(name = "rzc")]
#[command(about = "多语言 Rust 教学方言编译器")]
struct 命令行参数 {
    #[command(subcommand)]
    命令: 命令,
}

#[derive(Subcommand)]
enum 命令 {
    Init {
        项目名: String,
        #[arg(short, long, default_value = "zh")]
        语言: String,
    },
    Run {
        文件: PathBuf,
        #[arg(short, long)]
        语言包: Option<PathBuf>,
    },
    Check {
        文件: PathBuf,
        #[arg(short, long)]
        语言包: Option<PathBuf>,
    },
    Eject {
        文件: PathBuf,
        #[arg(short, long)]
        语言包: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cargo路径 = "cargo";
    let 参数 = 命令行参数::parse();

    match 参数.命令 {
        命令::Init { 项目名, 语言 } => {
            创建项目(&项目名, &语言)?;
            Ok(())
        }
        命令::Run { 文件, 语言包 } => {
            let 源码 = fs::read_to_string(&文件)?;
            let 映射 = 加载映射(语言包)?;
            let mut 英文代码 = 词法处理::转译源码(&源码, 映射.获取映射表());
            if !映射.模块路径映射.is_empty() {
                英文代码 = i18n_rust_engine::模块路径替换::替换模块路径(&英文代码, &映射.模块路径映射);
            }
            if !映射.标识符别名映射.is_empty() {
                英文代码 = i18n_rust_engine::别名替换::替换别名(&英文代码, &映射.标识符别名映射);
            }

            let 项目根 = 查找项目根(&文件)?;
            let 源码路径 = 项目根.join("src/main.rs");
            fs::write(&源码路径, &英文代码)?;

            let 输出 = Command::new(cargo路径)
                .arg("run")
                .current_dir(&项目根)
                .output()
                .map_err(|e| anyhow::anyhow!("执行 cargo run 失败，项目根: {}，错误: {}", 项目根.display(), e))?;

            if 输出.status.success() {
                println!("{}", String::from_utf8_lossy(&输出.stdout));
            } else {
                eprintln!("编译错误：\n{}", String::from_utf8_lossy(&输出.stderr));
            }
            Ok(())
        }
        命令::Check { 文件, 语言包 } => {
            let 源码 = fs::read_to_string(&文件)?;
            let 映射 = 加载映射(语言包)?;
            let mut 英文代码 = 词法处理::转译源码(&源码, 映射.获取映射表());
            if !映射.模块路径映射.is_empty() {
                英文代码 = i18n_rust_engine::模块路径替换::替换模块路径(&英文代码, &映射.模块路径映射);
            }
            if !映射.标识符别名映射.is_empty() {
                英文代码 = i18n_rust_engine::别名替换::替换别名(&英文代码, &映射.标识符别名映射);
            }

            let 项目根 = 查找项目根(&文件)?;
            let 源码路径 = 项目根.join("src/main.rs");
            fs::write(&源码路径, &英文代码)?;

            let 输出 = Command::new(cargo路径)
                .arg("check")
                .arg("--message-format=json")
                .current_dir(&项目根)
                .output()
                .map_err(|e| anyhow::anyhow!("执行 cargo check 失败，项目根: {}，错误: {}", 项目根.display(), e))?;

            let rustc输出 = String::from_utf8_lossy(&输出.stderr);
            use i18n_rust_engine::诊断::{解析诊断输出, 诊断翻译器, 错误翻译管理器};

            let 错误消息路径 = 项目根.join("语言包/中文/错误消息.toml");
            let 翻译器 = if 错误消息路径.exists() {
                let 翻译管理器 = 错误翻译管理器::从文件加载(&错误消息路径)
                    .map_err(|e| anyhow::anyhow!("加载错误消息失败: {}", e))?;
                let 反向映射: HashMap<String, String> = 映射
                    .获取节映射("类型")
                    .map(|节映射| {
                        节映射.iter().map(|(k, v)| (v.clone(), k.clone())).collect()
                    })
                    .unwrap_or_default();
                Some(诊断翻译器::新建(翻译管理器, 反向映射))
            } else {
                None
            };

            let 原始文件名 = 文件.file_name().unwrap_or_default().to_string_lossy().to_string();
            let mut 诊断列表 = 解析诊断输出(&rustc输出);
            诊断列表.retain(|d| d.代码.is_some() && (d.级别 == "error" || d.级别 == "warning"));
            let mut 已见错误码 = std::collections::HashSet::new();
            诊断列表.retain(|d| {
                if let Some(ref 代码) = d.代码 {
                    已见错误码.insert(代码.代码.clone())
                } else {
                    false
                }
            });

            if 诊断列表.is_empty() {
                println!("✅ 编译成功，没有错误。");
                return Ok(());
            }

            if let Some(ref 翻译器) = 翻译器 {
                let mut 教学列表 = 翻译器.批量翻译(&诊断列表);
                let mut 已见教学错误码 = std::collections::HashSet::new();
                教学列表.retain(|t| {
                    t.错误码.as_ref().map_or(false, |码| 已见教学错误码.insert(码.clone()))
                });
                for 教学 in &mut 教学列表 {
                    教学.位置.iter_mut().for_each(|位置| {
                        位置.文件名 = 原始文件名.clone();
                        位置.源码文本 = 获取中文源码行(&源码, 位置.起始行);
                    });
                }
                if 教学列表.is_empty() {
                    println!("✅ 编译成功，没有错误。");
                } else {
                    println!("{}", i18n_rust_engine::诊断::教学诊断::批量格式化为文本(&教学列表));
                }
            } else {
                if !rustc输出.is_empty() {
                    for 行 in rustc输出.lines() {
                        if let Ok(原始) = serde_json::from_str::<serde_json::Value>(行) {
                            if let Some(消息) = 原始.get("message") {
                                println!("{}", 消息.as_str().unwrap_or(""));
                            }
                        } else {
                            println!("{}", 行);
                        }
                    }
                } else {
                    println!("✅ 编译成功，没有错误。");
                }
            }
            Ok(())
        }
        命令::Eject { 文件, 语言包 } => {
            let 源码 = fs::read_to_string(&文件)?;
            let 映射 = 加载映射(语言包)?;
            let mut 英文代码 = 词法处理::转译源码(&源码, 映射.获取映射表());
            if !映射.模块路径映射.is_empty() {
                英文代码 = i18n_rust_engine::模块路径替换::替换模块路径(&英文代码, &映射.模块路径映射);
            }
            if !映射.标识符别名映射.is_empty() {
                英文代码 = i18n_rust_engine::别名替换::替换别名(&英文代码, &映射.标识符别名映射);
            }
            let 输出路径 = 文件.with_extension("rs");
            fs::write(&输出路径, 英文代码)?;
            println!("已导出到 {}", 输出路径.display());
            Ok(())
        }
    }
}

/// 根据源码文件定位项目根（包含 Cargo.toml 的目录）
fn 查找项目根(文件: &Path) -> anyhow::Result<PathBuf> {
    // 先获取文件所在目录的绝对路径
    let 文件目录 = if 文件.is_absolute() {
        文件.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::current_dir()?.join(文件).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
    };

    let mut 当前 = 文件目录.canonicalize().unwrap_or(文件目录);
    loop {
        if 当前.join("Cargo.toml").exists() {
            return Ok(当前);
        }
        if let Some(父) = 当前.parent() {
            当前 = 父.to_path_buf();
        } else {
            break;
        }
    }
    // 如果找不到，回退到当前目录
    std::env::current_dir().map_err(|e| anyhow::anyhow!("无法确定项目根: {}", e))
}

fn 获取中文源码行(源码: &str, 行号: u32) -> Option<String> {
    if 行号 == 0 { return None; }
    源码.lines().nth((行号 - 1) as usize).map(|s| s.to_string())
}

fn 创建项目(项目名: &str, 语言: &str) -> anyhow::Result<()> {
    let 项目路径 = PathBuf::from(项目名);
    if 项目路径.exists() {
        anyhow::bail!("目录 {} 已存在", 项目名);
    }
    fs::create_dir_all(项目路径.join("src"))?;
    fs::write(项目路径.join("rust-toolchain.toml"), "[toolchain]\nchannel = \"1.75.0\"\ncomponents = [\"rustc\", \"cargo\"]\n")?;
    fs::write(项目路径.join("Cargo.toml"), format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n", 项目名))?;
    // 复制语言包（如果有默认语言包）
    let 源 = PathBuf::from("语言包/中文");
    let 目标 = 项目路径.join("语言包").join(语言);
    if 源.exists() && !目标.exists() {
        fs::create_dir_all(&目标)?;
        for 文件 in &["关键字.toml", "错误消息.toml", "模块路径.toml"] {
            let 源文件 = 源.join(文件);
            if 源文件.exists() {
                fs::copy(&源文件, 目标.join(文件))?;
            }
        }
        let 三方源 = 源.join("第三方库");
        if 三方源.exists() {
            let 三方目标 = 目标.join("第三方库");
            fs::create_dir_all(&三方目标)?;
            for 条目 in fs::read_dir(&三方源)? {
                let 条目 = 条目?;
                if 条目.path().is_file() {
                    fs::copy(条目.path(), 三方目标.join(条目.file_name()))?;
                }
            }
        }
    }
    fs::write(项目路径.join("src/main.zh"), "函数 主函数() {\n    打印行!(\"你好，世界！\");\n}\n")?;
    println!("✅ 项目 '{}' 创建成功！", 项目名);
    Ok(())
}

fn 加载映射(语言包路径: Option<PathBuf>) -> anyhow::Result<映射管理器> {
    if let Some(路径) = 语言包路径 {
        return 映射管理器::从目录加载(&路径)
            .map_err(|e| anyhow::anyhow!("加载语言包失败: {}", e));
    }
    let 默认路径 = PathBuf::from("语言包/中文");
    if 默认路径.exists() {
        映射管理器::从目录加载(&默认路径)
            .map_err(|e| anyhow::anyhow!("加载默认语言包失败: {}", e))
    } else {
        anyhow::bail!("未找到语言包。请在项目下创建 语言包/中文 目录，或通过 --语言包 参数指定。")
    }
}