// 模块：命令行
// 功能：提供 i18n-rust 命令行教学工具

use clap::{Parser, Subcommand};
use i18n_rust_engine::映射管理::映射管理器;
use i18n_rust_engine::词法处理;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// i18n-rust 教学命令行工具
#[derive(Parser)]
#[command(name = "i18n")]
#[command(about = "多语言 Rust 教学方言编译器")]
struct 命令行参数 {
    #[command(subcommand)]
    命令: 命令,
}

#[derive(Subcommand)]
enum 命令 {
    /// 创建新的母语 Rust 项目
    Init {
        /// 项目名称
        项目名: String,
        /// 语言代码，例如 zh
        #[arg(short, long, default_value = "zh")]
        语言: String,
    },
    /// 翻译并运行单个母语脚本
    Run {
        /// 母语源码文件路径
        文件: PathBuf,
        /// 语言包路径（可选，默认使用内置中文）
        #[arg(short, long)]
        语言包: Option<PathBuf>,
    },
    /// 类型检查，输出中文诊断
    Check {
        /// 母语源码文件路径
        文件: PathBuf,
        /// 语言包路径（可选）
        #[arg(short, long)]
        语言包: Option<PathBuf>,
    },
    /// 导出标准 Rust 源代码
    Eject {
        /// 母语源码文件路径
        文件: PathBuf,
        /// 语言包路径（可选）
        #[arg(short, long)]
        语言包: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let 参数 = 命令行参数::parse();

    match 参数.命令 {
        命令::Init { 项目名, 语言 } => {
            创建项目(&项目名, &语言)?;
            Ok(())
        }
        命令::Run { 文件, 语言包 } => {
            let 源码 = fs::read_to_string(&文件)?;
            let 映射 = 加载映射(语言包)?;
            let 英文代码 = 词法处理::转译源码(&源码, 映射.获取映射表());

            // 将翻译后的代码写入临时文件并执行
            let 临时目录 = 文件.parent().unwrap_or_else(|| Path::new("."));
            let 临时源文件 = 临时目录.join("__temp_i18n.rs");
            let 可执行文件 = 临时目录.join("__temp_i18n_out");

            fs::write(&临时源文件, &英文代码)?;

            let 编译输出 = Command::new("rustc")
                .arg(&临时源文件)
                .arg("-o")
                .arg(&可执行文件)
                .output()
                .expect("rustc 执行失败");

            if 编译输出.status.success() {
                let 执行结果 = Command::new(&可执行文件).output()?;
                println!("{}", String::from_utf8_lossy(&执行结果.stdout));
            } else {
                eprintln!("编译错误：\n{}", String::from_utf8_lossy(&编译输出.stderr));
            }

            let _ = fs::remove_file(&临时源文件);
            let _ = fs::remove_file(&可执行文件);
            Ok(())
        }
               命令::Check { 文件, 语言包 } => {
            let 源码 = fs::read_to_string(&文件)?;
            let 映射 = 加载映射(语言包)?;

            // 1. 翻译母语代码
            let 英文代码 = 词法处理::转译源码(&源码, 映射.获取映射表());

            // 2. 调用 rustc 获取 JSON 错误输出
            let 临时目录 = 文件.parent().unwrap_or_else(|| Path::new("."));
            let 临时源文件 = 临时目录.join("__temp_check.rs");
            fs::write(&临时源文件, &英文代码)?;

            let 输出 = Command::new("rustc")
                .arg("--error-format=json")
                .arg(&临时源文件)
                .output()
                .expect("rustc 执行失败");

            let _ = fs::remove_file(&临时源文件);

            // 3. 解析 JSON 诊断输出
            let rustc输出 = String::from_utf8_lossy(&输出.stderr);
            use i18n_rust_engine::诊断::{解析诊断输出, 诊断翻译器, 错误翻译管理器};

            // 4. 加载错误消息翻译
            let 错误消息路径 = 文件.parent().unwrap_or_else(|| Path::new("."))
                .join("语言包/中文/错误消息.toml");
            let 翻译器 = if 错误消息路径.exists() {
                let 翻译管理器 = 错误翻译管理器::从文件加载(&错误消息路径)
                    .map_err(|e| anyhow::anyhow!("加载错误消息失败: {}", e))?;
                let 反向映射: HashMap<String, String> = 映射
                    .获取节映射("类型")
                    .map(|节映射| {
                        节映射
                            .iter()
                            .map(|(k, v)| (v.clone(), k.clone()))
                            .collect()
                    })
                    .unwrap_or_default();
                Some(诊断翻译器::新建(翻译管理器, 反向映射))
            } else {
                None
            };

            let 原始文件名 = 文件.file_name().unwrap_or_default().to_string_lossy().to_string();

            let mut 诊断列表 = 解析诊断输出(&rustc输出);
            // 只保留有错误码的 error/warning 级别诊断
            诊断列表.retain(|d| {
                d.代码.is_some() && (d.级别 == "error" || d.级别 == "warning")
            });
            // 按错误码去重（同一错误码只保留第一个）
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
                // 再次按错误码去重，避免 rustc 重复条目渗透
                let mut 已见教学错误码 = std::collections::HashSet::new();
                教学列表.retain(|t| {
                    t.错误码.as_ref().map_or(false, |码| 已见教学错误码.insert(码.clone()))
                });
                // 替换为中文文件名和源码行
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
                // 无翻译包时直接显示 rustc 输出
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
            let 英文代码 = 词法处理::转译源码(&源码, 映射.获取映射表());
            let 输出路径 = 文件.with_extension("rs");
            fs::write(&输出路径, 英文代码)?;
            println!("已导出到 {}", 输出路径.display());
            Ok(())
        }
    }
}

/// 创建新项目
fn 创建项目(项目名: &str, 语言: &str) -> anyhow::Result<()> {
    let 项目路径 = PathBuf::from(项目名);
    if 项目路径.exists() {
        anyhow::bail!("目录 {} 已存在", 项目名);
    }

    fs::create_dir_all(项目路径.join("src"))?;
    fs::create_dir_all(项目路径.join("语言包").join(语言))?;

    // 1. rust-toolchain.toml
    let 工具链内容 = r#"[toolchain]
channel = "1.75.0"
components = ["rustc", "cargo"]
"#;
    fs::write(项目路径.join("rust-toolchain.toml"), 工具链内容)?;

    // 2. Cargo.toml
    let cargo内容 = format!(
        r#"[package]
name = "{项目名}"
version = "0.1.0"
edition = "2021"

[dependencies]
"#
    );
    fs::write(项目路径.join("Cargo.toml"), cargo内容)?;

    // 3. 复制语言包（只复制关键字和错误消息，若无则创建最小版本）
    let 源语言包路径 = PathBuf::from("语言包/中文/关键字.toml");
    let 目标关键字路径 = 项目路径.join("语言包").join(语言).join("关键字.toml");
    if 源语言包路径.exists() {
        fs::copy(&源语言包路径, &目标关键字路径)?;
    } else {
        let 最小语言包 = r#"["声明"]
"函数" = "fn"
"让" = "let"
"可变" = "mut"
"#;
        fs::write(&目标关键字路径, 最小语言包)?;
    }

    let 源错误路径 = PathBuf::from("语言包/中文/错误消息.toml");
    let 目标错误路径 = 项目路径.join("语言包").join(语言).join("错误消息.toml");
    if 源错误路径.exists() {
        fs::copy(&源错误路径, &目标错误路径)?;
    }

    // 4. 示例中文源码
    let 示例源码 = r#"使用 std::io;

函数 主函数() {
    让 名字 = 字符串::从("世界");
    打印行!("你好，{}！", 名字);
}
"#;
    fs::write(项目路径.join("src/main.zh"), 示例源码)?;

    // 5. README
    let 自述文件内容 = format!(
        r#"# {项目名}

使用 **i18n-rust** 中文教学方言创建的 Rust 项目。

## 快速开始

1. 翻译并运行中文代码：

## 快速开始

1. 翻译并运行中文代码：
i18n run src/main.zh

2. 查看标准 Rust 代码：
i18n eject src/main.zh

3. 类型检查：
i18n check src/main.zh



## 手动编译

翻译后的代码也可用标准 cargo 构建（需将 .zh 内容翻译后复制到 main.rs）。

祝你学习愉快！
"#
    );
    fs::write(项目路径.join("README.md"), 自述文件内容)?;

    println!("✅ 项目 '{}' 创建成功！", 项目名);
    println!("请进入目录并尝试：cd {} && i18n run src/main.zh", 项目名);

    Ok(())
}

/// 加载语言包映射管理器
fn 加载映射(语言包路径: Option<PathBuf>) -> anyhow::Result<映射管理器> {
    match 语言包路径 {
        Some(路径) => {
            映射管理器::从文件加载(&路径).map_err(|e| anyhow::anyhow!("加载语言包失败: {}", e))
        }
        None => {
            let 默认路径 = PathBuf::from("语言包/中文/关键字.toml");
            if 默认路径.exists() {
                映射管理器::从文件加载(&默认路径)
                    .map_err(|e| anyhow::anyhow!("加载默认语言包失败: {}", e))
            } else {
                anyhow::bail!("未提供语言包，且默认中文语言包未找到")
            }
        }
    }
}

/// 从源码中根据行号提取一行（去除首尾空白）
fn 获取中文源码行(源码: &str, 行号: u32) -> Option<String> {
    if 行号 == 0 {
        return None;
    }
    源码.lines().nth((行号 - 1) as usize).map(|s| s.to_string())
}
