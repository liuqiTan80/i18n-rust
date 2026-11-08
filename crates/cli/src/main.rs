//! rzc 命令行入口 —— 多语言 Rust 教学方言编译器
//!
//! 提供初始化、运行、检查、导出、语言包、映射等子命令，
//! 将母语 Rust 源码实时转译为标准 Rust 并调用 cargo 编译或运行。

use clap::{FromArgMatches, Parser, Subcommand};
use i18n_rust_engine::映射管理器::映射管理器;
use i18n_rust_engine::缓存::转译缓存;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::{Mutex, PoisonError};
use std::thread::{scope, sleep, spawn};
use std::time::Duration;

#[path = "临时守卫.rs"]
mod 临时守卫;
#[path = "内置语言.rs"]
mod 内置语言;
#[path = "包仓库.rs"]
mod 包仓库;
#[path = "安装.rs"]
mod 安装;
#[path = "映射检查.rs"]
mod 映射检查;
#[path = "映射生成/mod.rs"]
mod 映射生成;
#[path = "映射覆盖.rs"]
mod 映射覆盖;
#[path = "界面.rs"]
mod 界面;
#[path = "翻译缺口.rs"]
mod 翻译缺口;
#[path = "诊断输出.rs"]
mod 诊断输出;
#[path = "语言管理.rs"]
mod 语言管理;

use i18n_rust_engine::模块路径::注解非母语模块;
use 内置语言::{内置语言清单, 有内置语言包, 获取内置数据};
use 安装::{
    安装工具链, 安装语言服务, 显示安装向导, 诊断环境, 语言服务发布标签
};
use 映射检查::{运行检查, 运行生成骨架};
use 映射生成::{探测系统语言, 版本需求, 运行自动生成};
use 映射覆盖::运行覆盖检查;
use 诊断输出::{
    可用直接编译, 报告翻译缺口, 检查直接编译, 流翻译器, 翻译编译诊断, 诊断上下文, 诊断语言代码,
    载入错误管理器, 运行直接编译,
};
use 语言管理::{
    全局语言包子目录名, 全局语言目录, 全部可用扩展名, 列出语言, 安装语言, 搜索远程语言,
    查询扩展名映射, 移除语言, 语言来源, 静态扩展名映射,
};

// 非 ASCII 模块名 #[路径属性] 注解：命令行与语言服务共享同一引擎实现
// 语言服务镜像复用同一引擎入口

#[derive(Parser)]
#[command(name = "rzc", version)]
// 兜底文案（本地化命令行时会按界面语言覆盖）；用英文避免硬编码中文
#[command(about = "Multi-language Rust teaching dialect compiler")]
pub struct 参数组 {
    /// 静默教学检查提示（初学者代码风格告警；Unicode 混淆字符与全角标点告警不受影响）
    #[arg(long = "no-lint", global = true)]
    pub 无教学检查: bool,
    #[command(subcommand)]
    pub 子命令值: 命令组,
}

#[derive(Subcommand)]
pub enum 命令组 {
    #[command(name = "init")]
    初始化 {
        #[arg(value_name = "PROJECT_NAME")]
        项目名: String,
        // 缺省跟随系统区域设置（LC_ALL/LANG）：全球用户的第一个项目应是自己的母语。
        // 不用参数默认值类型：需要区分「用户显式指定」与「自动检测失败」，
        // 后者要提示回退或在终端里列出菜单
        #[arg(short, long = "lang")]
        语言: Option<String>,
        /// 同时生成 AGENTS.md（面向 AI 助手的母语编程指南，由当前语言包渲染）
        #[arg(long = "ai")]
        助手: bool,
    },
    #[command(name = "run")]
    运行 {
        #[arg(value_name = "FILE")]
        源文件值: PathBuf,
        #[arg(short, long = "lang-pack")]
        语言包: Option<PathBuf>,
    },
    #[command(name = "check")]
    检查 {
        #[arg(value_name = "FILE")]
        源文件值: PathBuf,
        #[arg(short, long = "lang-pack")]
        语言包: Option<PathBuf>,
        /// 自动修正全角标点（写回源文件；仅转换存在半角对应字符的那些）
        #[arg(long = "fix")]
        修复: bool,
    },
    #[command(name = "eject")]
    导出 {
        /// 要导出的方言源文件或目录；可重复（批量导出，失败项汇总报告）
        #[arg(value_name = "FILES", required = true, num_args = 1..)]
        文件们: Vec<PathBuf>,
        #[arg(short, long = "lang-pack")]
        语言包: Option<PathBuf>,
    },
    /// 转译预览：将方言源码转为标准 Rust 并打印到标准输出（不写任何文件）
    #[command(name = "transpile")]
    转译预览 {
        #[arg(value_name = "FILE")]
        源文件值: PathBuf,
        #[arg(short, long = "lang-pack")]
        语言包: Option<PathBuf>,
    },
    /// 为当前项目添加第三方依赖（封装 cargo 添加命令，并给出母语映射提示）
    #[command(name = "add")]
    添加依赖 {
        /// 第三方库名称或 名称@版本，可重复（例如可一次写多个第三方库名）
        #[arg(value_name = "CRATES", required = true)]
        依赖们: Vec<String>,
    },
    /// 语言包管理（列出 / 安装 / 移除）
    #[command(name = "lang")]
    语言包管理 {
        #[command(subcommand)]
        子命令值: 语言子命令,
    },
    /// 生成第三方库映射（从已安装的库中抽取接口）
    #[command(name = "mapping")]
    映射管理 {
        #[command(subcommand)]
        子命令值: 映射子命令,
    },
    /// 共享第三方库注册中心（搜索 / 安装 / 列出 / 移除 / 更新 / 发布）
    #[command(name = "crate")]
    库注册 {
        #[command(subcommand)]
        子命令值: 库子命令,
    },
    /// 安装配套组件（语言服务器等）
    #[command(name = "install")]
    安装组件 {
        #[command(subcommand)]
        子命令值: Option<安装子命令>,
    },
    /// 诊断工具链环境：内置工具链 / 环境变量路径 / 版本对比
    #[command(name = "doctor")]
    诊断环境,
    /// 查看 / 导出 / 清空待译清单（尚无母语翻译的编译器与 cargo 消息）
    #[command(name = "gaps")]
    待译清单 {
        /// 语言代码（默认为检测到的界面语言）
        #[arg(long = "lang")]
        语言: Option<String>,
        /// 将清单以三列形式（层级、代码、消息）导出到指定路径
        #[arg(long = "export")]
        导出路径: Option<PathBuf>,
        /// 清空清单（填完消息表后使用）
        #[arg(long = "clear")]
        清空清单: bool,
    },
    /// 母语与 Rust 映射速查表（关键字 / 模块路径 / 别名 / 派生特征）
    #[command(name = "cheat")]
    速查 {
        /// 内置语言代码（例如 zh）或语言包目录路径；省略时从系统语言自动检测
        #[arg(value_name = "LANG")]
        语言: Option<String>,
        /// 以 Markdown 表格输出，便于嵌入文档
        #[arg(long = "markdown")]
        标记: bool,
    },
    /// 母语编程指南：面向学习者与 AI 助手的写作规则与完整术语表
    #[command(name = "guide")]
    指南 {
        /// 内置语言代码（例如 zh）；省略时从系统语言自动检测
        #[arg(short, long = "lang")]
        语言: Option<String>,
        /// 将指南写入指定文件（例如 AGENTS.md）而非标准输出
        #[arg(short, long = "out")]
        输出文件: Option<PathBuf>,
    },
    /// 跨母语阅读：将方言或标准 Rust 项目重新转译为另一种方言
    ///（先把每个源文件转成标准 Rust，再本地化为目标方言；镜像出的项目保持
    /// 结构忠实，可用 `rzc run` 重新构建）。缺失的目标术语保留英文原文
    ///（登记为待翻译项）；一对多的反向查询会挑选一个确定的候选，
    /// 因此跨母语「阅读」无需与原词逐字一致。
    #[command(name = "relocalize")]
    反译重生 {
        /// 目标方言语言代码（必填，例如 ja）；即输出项目所使用的语言
        #[arg(long = "to")]
        目标语言: String,
        /// 源方言语言代码；省略时按每个文件的扩展名自动检测
        #[arg(long = "from")]
        源语言: Option<String>,
        /// 输出目录（镜像源布局并换用目标扩展名）；默认为源项目根旁的镜像目录
        #[arg(long = "out-dir")]
        输出目录: Option<PathBuf>,
        /// 覆盖源语言包目录（目标始终从目标语言参数解析）
        #[arg(short, long = "lang-pack")]
        语言包: Option<PathBuf>,
        /// 待转换的项目目录或源文件；默认为当前目录
        #[arg(value_name = "FILES", num_args = 0..)]
        文件们: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum 安装子命令 {
    /// 安装语言服务器（编辑器扩展的补全与诊断后端）
    #[command(name = "lsp")]
    语言服务 {
        /// 若已安装则强制覆盖
        #[arg(short = 'f', long = "force")]
        强制: bool,
    },
    /// 一键安装内置工具链（独立编译器与构建工具，无需 rustup）
    #[command(name = "toolchain")]
    工具链 {
        /// 工具链版本（默认为 rzc 锁定的版本，例如 1.98.0）
        #[arg(long = "version", default_value = i18n_rust_engine::工具链::锁定版本)]
        版本: String,
        /// 官方文档分析器发布标签（默认为锁定版本）
        #[arg(long = "ra-tag", default_value = 语言服务发布标签)]
        发布标签: String,
        /// 仅升级文档分析器（跳过约 300 兆字节的编译器与构建工具重新下载）
        #[arg(long = "ra-only")]
        仅分析器: bool,
        /// 若已安装则强制重装
        #[arg(short = 'f', long = "force")]
        强制: bool,
    },
}

#[derive(Subcommand)]
pub enum 映射子命令 {
    /// 自动生成第三方库映射文件：抽取库的公开接口；由 AI 或规则以目标语言生成名称与释义
    #[command(name = "auto")]
    自动生成 {
        /// 目标第三方库名称（须已安装或在注册源上可用）
        #[arg(value_name = "CRATE_NAME")]
        包名: String,
        /// 目标语言（语言包目录名，例如 zh、ru；默认从系统语言自动检测）
        #[arg(long = "lang")]
        语言: Option<String>,
        /// AI 提供方：深度求索（默认）/ 开放人工智能 / 兼容端点——三者均遵循开放人工智能协议，端点、密钥与模型通过 RZ_AI_BASE_URL / RZ_AI_API_KEY / RZ_AI_MODEL 设置——或规则（离线规则模式）
        #[arg(long = "provider", default_value = "deepseek")]
        服务商: String,
        /// 固定目标库版本（例如 2.11.5 精确；2.11 / 2 = 该系列最新）；省略时用最新版（不可复现）
        #[arg(long = "target-version")]
        目标版本: Option<String>,
        /// 输出文件路径（默认为项目语言包根目录：<lang>/crates/<crate_name>.toml）
        #[arg(long = "output")]
        输出路径: Option<PathBuf>,
        /// 生成映射后同时把该库加入当前项目的依赖清单
        #[arg(long = "install")]
        安装依赖: bool,
    },
    /// 校验第三方库映射质量：重复键 / 关键字规避 / 跨文件冲突 / 条目数一致性
    #[command(name = "check")]
    校验 {
        /// 内置语言代码（例如 zh）或语言包目录路径；省略时校验所有内置语言
        #[arg(value_name = "TARGET")]
        目标: Option<String>,
    },
    /// 语料覆盖率矩阵：对照真实后端源码校验语言包关键字与接口覆盖率；列出缺失的母语映射
    #[command(name = "coverage")]
    覆盖检查 {
        /// 内置语言代码（例如 zh）；省略时检查所有内置语言
        #[arg(long = "lang")]
        语言: Option<String>,
    },
    /// 从源语言的第三方库映射生成目标语言的翻译骨架（保留键待翻译，英文值不变）
    #[command(name = "scaffold")]
    脚手架 {
        /// 源语言代码（内置语言，例如 zh）
        #[arg(value_name = "SOURCE")]
        源语言: String,
        /// 目标语言代码（新语言包目录名，例如某语言代码）
        #[arg(value_name = "TARGET")]
        目标语言: String,
        /// 输出目录（默认为项目语言包根目录：<target>/crates/）
        #[arg(long = "output")]
        输出路径: Option<PathBuf>,
        /// 翻译模式：规则（默认；生成待办骨架供人工翻译）或深度求索 / 开放人工智能 / 兼容端点（AI 翻译键；端点、密钥与模型来自 AI 端点环境变量）
        #[arg(long = "provider", default_value = "rule")]
        服务商: String,
    },
}

#[derive(Subcommand)]
pub enum 库子命令 {
    /// 搜索注册中心里已发布的第三方库映射（可选关键字过滤）
    #[command(name = "search")]
    检索 {
        /// 关键字（匹配库名 / 语言 / 作者）；省略时列出全部（按下载量排序）
        #[arg(value_name = "KEYWORD")]
        关键词: Option<String>,
    },
    /// 安装单个第三方库映射：从注册中心复制到全局语言包
    #[command(name = "install")]
    安装映射 {
        /// 第三方库名称（连字符归一化为下划线）
        #[arg(value_name = "CRATE_NAME")]
        包名: String,
        /// 目标语言代码（例如 zh）
        #[arg(long = "lang")]
        语言: String,
        /// 若已安装则强制覆盖
        #[arg(short = 'f', long = "force")]
        强制: bool,
    },
    /// 列出已安装的社区映射
    #[command(name = "list")]
    列表,
    /// 移除已安装的社区映射（从全局语言包与清单中删除）
    #[command(name = "remove")]
    移除映射 {
        /// 第三方库名称
        #[arg(value_name = "CRATE_NAME")]
        包名: String,
        /// 语言代码
        #[arg(long = "lang")]
        语言: String,
    },
    /// 按已安装清单重新拉取全部映射（获取他人更新）
    #[command(name = "update")]
    重新拉取,
    /// 将本地映射发布到注册中心（先进行质量校验）
    #[command(name = "publish")]
    发布映射 {
        /// 第三方库名称
        #[arg(value_name = "CRATE_NAME")]
        包名: String,
        /// 语言代码
        #[arg(long = "lang")]
        语言: String,
        /// 映射文件路径（省略时在常见位置搜索：全局或项目语言包或当前目录）
        #[arg(long = "file")]
        映射文件: Option<PathBuf>,
        /// 译者署名（默认为 git 用户名）
        #[arg(long = "author")]
        作者: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum 语言子命令 {
    /// 列出所有已安装的语言包（内置 + 用户安装）
    #[command(name = "list")]
    列表,
    /// 安装语言包：本地目录路径直接复制；语言代码从远程仓库下载
    #[command(name = "install")]
    安装语言 {
        /// 本地语言包目录路径，或远程语言代码
        #[arg(value_name = "SOURCE")]
        源: String,
        /// 若已安装则强制覆盖
        #[arg(short = 'f', long = "force")]
        强制: bool,
    },
    /// 移除用户安装的语言包（内置语言包不可移除）
    #[command(name = "remove")]
    移除语言 {
        /// 语言代码（语言包目录名）
        #[arg(value_name = "LANG_CODE")]
        语言代码: String,
    },
    /// 浏览远程语言包市场（扫描远程仓库；可选关键字过滤）
    #[command(name = "search")]
    检索 {
        /// 关键字（匹配语言代码或显示名）；省略时列出所有远程语言包
        #[arg(value_name = "KEYWORD")]
        关键词: Option<String>,
    },
}

fn main() -> ExitCode {
    use std::io::{IsTerminal, Read};
    // 双击 rzc 可执行文件（无参数且标准输出是终端）：显示环境安装向导（逐项检查
    // 组件状态与解决方式）并停留，避免黑窗一闪而过。用标准输出检测（双击时标准输出
    // 连控制台）而非标准输入（双击场景标准输入句柄可能无效，读取立即结束导致秒关）；
    // 标准输入读取失败时停留数秒兜底。管道或重定向场景标准输出非终端，走正常流程。
    if env::args().len() == 1 && std::io::stdout().is_terminal() {
        显示安装向导();
        println!();
        println!("按任意键退出...");
        let mut 缓冲 = [0u8; 1];
        if std::io::stdin().read_exact(&mut 缓冲).is_err() {
            sleep(Duration::from_secs(8));
        }
        return ExitCode::SUCCESS;
    }
    match 运行() {
        Ok(退出码值) => 退出码值,
        Err(错误值) => {
            // 错误正文已由各处按界面语言生成，硬编码英文前缀会是教学
            // 输出里最后一处英文（清单预检等走的就是这条出口）
            eprintln!(
                "{}",
                界面::界面消息表::全局表()
                    .取带参消息("cli_error_prefix", &[&format!("{错误值:#}")])
            );
            ExitCode::FAILURE
        }
    }
}

fn 运行() -> anyhow::Result<ExitCode> {
    // 按当前界面语言本地化命令行帮助文本
    let 界面表 = 界面::界面消息表::全局表();
    // 同步引擎全局语言（错误、诊断、日志随界面语言输出）
    i18n_rust_engine::语言::设定语言(&界面::探测界面语言());
    let 命令行实例 = 本地化命令行(&界面表);
    // 命令行框架自身错误（帮助、版本输出、非法参数等）按其退出码直接退出
    let 参数值 = match 参数组::from_arg_matches(&命令行实例.get_matches()) {
        Ok(已匹配参数) => 已匹配参数,
        Err(命令行错误) => 命令行错误.exit(),
    };

    // 无教学检查：项目开发（非教学）场景静默教学检查（初学者代码风格提示，
    // 每次转译刷屏）；Unicode 混淆与全角标点告警不受影响
    if 参数值.无教学检查 {
        i18n_rust_engine::校验::设教学告警启用(false);
    }

    // 首次运行引导：终端交互场景下，首次执行教学核心命令时打印
    // 欢迎语与环境检查（编译器缺失提示与下一步建议），仅一次
    //（主目录下的首次运行标记文件）；持续集成或管道等非终端场景自动跳过。
    match &参数值.子命令值 {
        命令组::运行 { .. } | 命令组::检查 { .. } | 命令组::初始化 { .. } => {
            也许显示首次运行(&界面表);
        }
        _ => {}
    }

    match 参数值.子命令值 {
        命令组::初始化 {
            项目名, 语言, 助手
        } => {
            let 语言值 = 解析初始化语言(语言)?;
            i18n_rust_engine::语言::设定语言(&语言值);
            创建项目(&项目名, &语言值, 助手)?;
            Ok(ExitCode::SUCCESS)
        }
        命令组::运行 {
            源文件值, 语言包
        } => {
            let 界面表 = 按文件取界面(&源文件值, &语言包);
            确保源文件(&界面表, &源文件值)?;
            let 源码文本 = fs::read_to_string(&源文件值)?;
            let 映射管理器值 = 加载映射(语言包.clone(), Some(&源文件值))?;
            let 项目根 = 查找项目根(&源文件值)?;
            // 项目级声明上下文（跨文件声明豁免）：扫描源码目录全部方言文件与
            // 入口，收集模块名与声明名；入口文件、项目内文件与诊断重放
            // 共享同一上下文（否则跨文件调用的成员名被当库别名替换，报 E0599）
            let 项目上下文值 = 收集项目上下文(&项目根, &源文件值, &映射管理器值);
            // 入口文件写入源码目录的主程序产物作为编译目标；会话缓存贯穿入口文件与
            // 项目内其他文件（并行转译共享命中，见转译项目文件）
            let 源路径 = 入口产物路径(&项目根, &源文件值, &映射管理器值);
            // 经构建工具运行时只能运行默认入口：非默认入口
            //（示例目录、源码目录下其他文件、根目录脚本）的产物写在自身路径，
            // 无参数的运行会静默运行另一个程序，必须先明确提示
            let 直接编译 = 可用直接编译(&项目根, &源文件值);
            if !直接编译 && 源路径 != 项目根.join(主程序产物路径) {
                // 库入口单列一条消息：它不是「入口文件认错了吧」，而是库
                // 压根没有可执行程序。指人家用带名称参数的方式是让人去找一个
                // 不存在的程序，故这里给的实际动作只有「校验」与「补主程序」。
                if 入口产物默认(&源文件值, &项目根, &映射管理器值) == Some(库产物路径)
                {
                    let 路径文本 = 源文件值.display().to_string();
                    anyhow::bail!(
                        "{}",
                        界面表.取带参消息("run_lib_no_bin", &[&路径文本, &路径文本])
                    );
                }
                anyhow::bail!(
                    "{}",
                    界面表.取带参消息(
                        "run_not_default_entry",
                        &[
                            &源文件值.display().to_string(),
                            &源路径.display().to_string()
                        ]
                    )
                );
            }
            let 缓存 = Mutex::new(i18n_rust_engine::缓存::转译缓存::默认持久化());
            let 转译结果 = 项目缓存转译(
                &源码文本,
                &映射管理器值,
                &mut 缓存.lock().unwrap_or_else(PoisonError::into_inner),
                Some(&项目上下文值),
                &教学显示名(&源文件值, &项目根),
            );
            // 列映射：把编译器诊断的英文产物列号回译到母语源码列号
            let 列位映射 = i18n_rust_engine::列位映射::列位映射表::回放生成(
                &源码文本,
                &转译结果.流程映射,
            );
            // 写盘产物含模块路径注解插入行：保留行映射供诊断回译先行换算
            //（注解落点按托管布局解析：子模块产物在托管镜像目录下）
            let 入口扩展名 = 全部可用扩展名();
            let 是否入口 = 是否入口源(&源文件值, &项目根, &映射管理器值);
            let (注解后产物, 入口行映射) = 注解产物(
                &转译结果.输出,
                &项目根,
                &源文件值,
                &源路径,
                是否入口,
                &入口扩展名,
            );
            写出产物(&项目根, &源路径, &注解后产物, &界面表)?;
            // 入口产物落构建工具 包根，清托管目录里的旧布局残留；非入口产物已从
            // 源文件旁迁到托管目录，清掉迁走后同目录的遗留文件
            let 旧产物 = 源文件值.with_extension("rs");
            if 是否入口 {
                清理入口生成副本(&项目根, &源文件值, &源路径);
            } else {
                迁移旧产物(&旧产物, &源路径, &注解后产物, &界面表);
            }
            // 同步转译项目内其他方言文件，保证多文件项目的模块引用链可用
            转译项目文件(&项目根, &源文件值, &映射管理器值, &缓存, &项目上下文值)?;

            // 单文件项目直调编译器：绕开构建工具的索引与项目结构（教学单文件
            // 场景编译更快、无网络索引问题）；多文件或有依赖项目回退构建工具
            if 直接编译 {
                return 运行直接编译(
                    &界面表,
                    &项目根,
                    &源路径,
                    &语言包,
                    &映射管理器值,
                    &源码文本,
                    &源文件值,
                    &列位映射,
                    &入口行映射,
                    &项目上下文值,
                );
            }

            // 编译诊断（警告与错误）走 JSON 行翻译，
            // 程序自身标准输出与标准错误原样透传（构建工具不包装子进程输出），
            // 避免英文警告与程序输出混淆，也无需二次编译。
            清单预检(&项目根, &界面表)?;
            let mut 子进程 = Command::new(解析cargo())
                .args(["run", "--message-format=json"])
                .current_dir(&项目根)
                .stdin(Stdio::inherit())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|错误值| {
                    anyhow::anyhow!(
                        "{}",
                        界面表.取带参消息(
                            "cargo_run_failed",
                            &[&项目根.display().to_string(), &错误值.to_string()]
                        )
                    )
                })?;
            let 标准输出管道 = 子进程
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("cargo run stdout 管道不可用"))?;
            // 标准错误线程逐行翻译：构建进度与程序
            // 恐慌输出本地化，其余行（程序普通标准错误）原样透传
            let 标准错误管道 = 子进程
                .stderr
                .take()
                .ok_or_else(|| anyhow::anyhow!("cargo run stderr 管道不可用"))?;
            // 构建工具自身错误（清单、无目标、依赖拉不到等）是纯文本，
            // 逐行过消息表本地化；未命中行仍走原有进度行翻译，行为不劣于旧版
            let 工具根 = 项目根.clone();
            let 工具语言包 = 语言包.clone();
            let 缺口语言 = 诊断语言代码(&源文件值);
            let 标准错误句柄 = spawn(move || {
                let 界面表 = 界面::界面消息表::全局表();
                let 基础翻译器 = 流翻译器::新建();
                let mut 翻译器 =
                    match 载入错误管理器(&界面表, &工具语言包, &工具根, &缺口语言)
                    {
                        Some(管理器值) => 基础翻译器.带工具文本(管理器值, 缺口语言),
                        None => 基础翻译器,
                    };
                let 读取器 = BufReader::new(标准错误管道);
                for 行结果 in 读取器.lines() {
                    match 行结果 {
                        Ok(行文本) => eprintln!("{}", 翻译器.翻译(&行文本, &界面表)),
                        Err(_) => break,
                    }
                }
                // 流式线程里登记的英文残留也要落盘：无 JSON 诊断时主链路不报告
                报告翻译缺口(&界面表);
            });
            let 读取器 = BufReader::new(标准输出管道);
            // 收集构建工具 JSON 诊断行（含原因字段），其余行视为程序输出原样透传
            let mut 诊断行文本 = String::new();
            for 行结果 in 读取器.lines() {
                let 行文本 = 行结果.map_err(|错误值| {
                    anyhow::anyhow!(
                        "{}",
                        界面表.取带参消息(
                            "cargo_run_failed",
                            &[&项目根.display().to_string(), &错误值.to_string()]
                        )
                    )
                })?;
                if 行文本.starts_with('{')
                    && let Ok(解析结果) = serde_json::from_str::<serde_json::Value>(&行文本)
                    && 解析结果.get("reason").is_some()
                {
                    诊断行文本.push_str(&行文本);
                    诊断行文本.push('\n');
                    continue;
                }
                println!("{行文本}");
            }
            let 进程状态值 = 子进程.wait().map_err(|错误值| {
                anyhow::anyhow!(
                    "{}",
                    界面表.取带参消息(
                        "cargo_run_failed",
                        &[&项目根.display().to_string(), &错误值.to_string()]
                    )
                )
            })?;
            let _ = 标准错误句柄.join();

            // 编译诊断翻译（警告与错误均覆盖）；
            // 无诊断且成功时静默（程序已运行，不再提示编译状态）
            if !诊断行文本.is_empty() {
                let _ = 翻译编译诊断(
                    &诊断行文本,
                    "",
                    &诊断上下文 {
                        界面表: &界面表,
                        语言包参数: &语言包,
                        项目根: &项目根,
                        映射器: &映射管理器值,
                        源码: &源码文本,
                        源文件: &源文件值,
                        入口产物: 源路径.clone(),
                        列位映射: Some(&列位映射),
                        入口行映射: Some(&入口行映射),
                        项目上下文: Some(&项目上下文值),
                    },
                    进程状态值.success(),
                    true,
                    // 输出已实时透传：构建成功而程序运行失败（如恐慌）时
                    // 不补打「编译错误」标签
                    true,
                );
            }

            // 传播被运行程序的退出码（信号终止等无码场景回退失败码）
            Ok(进程状态值
                .code()
                .map(|代码值| ExitCode::from(代码值 as u8))
                .unwrap_or(ExitCode::FAILURE))
        }
        命令组::检查 {
            源文件值,
            语言包,
            修复,
        } => {
            let 界面表 = 按文件取界面(&源文件值, &语言包);
            确保源文件(&界面表, &源文件值)?;
            let mut 源码文本 = fs::read_to_string(&源文件值)?;
            // 全角标点教学修复：中文输入法下最常见的编译错误来源之一。
            // 修复开关打开时自动将代码位置（字符串与注释内除外）的全角标点改写为半角，
            // 并提示剩余需人工修改的字符（顿号、全角空格等）；
            // 不带修复开关时警告由转译管线自动输出。
            if 修复 {
                let (修正后文本, 修正数) =
                    i18n_rust_engine::全角标点::修复全角标点(&源码文本);
                if 修正数 > 0 {
                    fs::write(&源文件值, &修正后文本)?;
                    源码文本 = 修正后文本;
                    println!(
                        "{}",
                        界面表.取带参消息(
                            "fullwidth_fix_done",
                            &[&修正数.to_string(), &源文件值.display().to_string()]
                        )
                    );
                }
                let 剩余数 = i18n_rust_engine::全角标点::检测全角标点(&源码文本).len();
                if 剩余数 > 0 {
                    println!(
                        "{}",
                        界面表.取带参消息("fullwidth_fix_remaining", &[&剩余数.to_string()])
                    );
                }
            }
            let 映射管理器值 = 加载映射(语言包.clone(), Some(&源文件值))?;
            let 项目根 = 查找项目根(&源文件值)?;
            // 项目级声明上下文（跨文件声明豁免，同运行）
            let 项目上下文值 = 收集项目上下文(&项目根, &源文件值, &映射管理器值);
            let 源路径 = 入口产物路径(&项目根, &源文件值, &映射管理器值);
            let 缓存 = Mutex::new(i18n_rust_engine::缓存::转译缓存::默认持久化());
            let 转译结果 = 项目缓存转译(
                &源码文本,
                &映射管理器值,
                &mut 缓存.lock().unwrap_or_else(PoisonError::into_inner),
                Some(&项目上下文值),
                &教学显示名(&源文件值, &项目根),
            );
            // 列映射：把编译器诊断的英文产物列号回译到母语源码列号
            let 列位映射 = i18n_rust_engine::列位映射::列位映射表::回放生成(
                &源码文本,
                &转译结果.流程映射,
            );
            // 写盘产物含模块路径注解插入行：保留行映射供诊断回译先行换算
            //（注解落点按托管布局解析：子模块产物在托管镜像目录下）
            let 入口扩展名 = 全部可用扩展名();
            let 是否入口 = 是否入口源(&源文件值, &项目根, &映射管理器值);
            let (注解后产物, 入口行映射) = 注解产物(
                &转译结果.输出,
                &项目根,
                &源文件值,
                &源路径,
                是否入口,
                &入口扩展名,
            );
            写出产物(&项目根, &源路径, &注解后产物, &界面表)?;
            // 入口产物落构建工具 包根，清托管目录里的旧布局残留；非入口产物已从
            // 源文件旁迁到托管目录，清掉迁走后同目录的遗留文件
            let 旧产物 = 源文件值.with_extension("rs");
            if 是否入口 {
                清理入口生成副本(&项目根, &源文件值, &源路径);
            } else {
                迁移旧产物(&旧产物, &源路径, &注解后产物, &界面表);
            }
            // 同步转译项目内其他方言文件，保证多文件项目的模块引用链可用
            转译项目文件(&项目根, &源文件值, &映射管理器值, &缓存, &项目上下文值)?;

            // 单文件项目直调编译器（绕开构建工具）；多文件或有依赖项目回退构建工具
            if 可用直接编译(&项目根, &源文件值) {
                return 检查直接编译(
                    &界面表,
                    &项目根,
                    &源路径,
                    &语言包,
                    &映射管理器值,
                    &源码文本,
                    &源文件值,
                    &列位映射,
                    &入口行映射,
                    &项目上下文值,
                );
            }

            清单预检(&项目根, &界面表)?;
            let 输出值 = Command::new(解析cargo())
                .arg("check")
                .arg("--message-format=json")
                .current_dir(&项目根)
                .output()
                .map_err(|错误值| {
                    anyhow::anyhow!(
                        "{}",
                        界面表.取带参消息(
                            "cargo_check_failed",
                            &[&项目根.display().to_string(), &错误值.to_string()]
                        )
                    )
                })?;
            // 构建工具整体是否成功（决定最终退出码）
            let 退出码值 = if 输出值.status.success() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };

            // 结构化诊断翻译（与运行编译失败路径共用同一管线）
            let 标准错误文本 = String::from_utf8_lossy(&输出值.stderr).to_string();
            let 编译输出文本 = format!(
                "{}\n{}",
                String::from_utf8_lossy(&输出值.stdout),
                标准错误文本
            );
            let _ = 翻译编译诊断(
                &编译输出文本,
                &标准错误文本,
                &诊断上下文 {
                    界面表: &界面表,
                    语言包参数: &语言包,
                    项目根: &项目根,
                    映射器: &映射管理器值,
                    源码: &源码文本,
                    源文件: &源文件值,
                    入口产物: 源路径.clone(),
                    列位映射: Some(&列位映射),
                    入口行映射: Some(&入口行映射),
                    项目上下文: Some(&项目上下文值),
                },
                输出值.status.success(),
                false, // 检查场景：无诊断且成功时提示「编译成功」
                false, // 检查：诊断输出由本函数负责（非流式透传）
            );
            Ok(退出码值)
        }
        命令组::导出 {
            文件们, 语言包
        } => 导出目标们(&文件们, &语言包),
        命令组::转译预览 {
            源文件值, 语言包
        } => {
            // 转译预览：仅输出到标准输出，不产生任何文件（与导出互补）
            let 界面表 = 按文件取界面(&源文件值, &语言包);
            确保源文件(&界面表, &源文件值)?;
            let 源码文本 = fs::read_to_string(&源文件值)?;
            let 映射管理器值 = 加载映射(语言包, Some(&源文件值))?;
            // 与导出同一管线（项目上下文与模块注解）：预览产物即导出产物
            let 项目上下文值 = 收集导出上下文(&源文件值, &映射管理器值);
            let mut 缓存 = i18n_rust_engine::缓存::转译缓存::默认持久化();
            let 英文代码 = 导出转译(
                &源文件值,
                &源码文本,
                &映射管理器值,
                &项目上下文值,
                &mut 缓存,
                false,
                &源文件值.display().to_string(),
            );
            print!("{英文代码}");
            let _ = 界面表; // 标准输出模式无需额外提示
            Ok(ExitCode::SUCCESS)
        }
        命令组::反译重生 {
            目标语言,
            源语言,
            输出目录,
            语言包,
            文件们,
        } => 反译重生目标们(&目标语言, &源语言, &输出目录, &语言包, &文件们),
        命令组::安装组件 { 子命令值 } => {
            // 省略子命令时默认安装全部组件（当前仅语言服务器）
            match 子命令值.unwrap_or(安装子命令::语言服务 { 强制: false }) {
                安装子命令::语言服务 { 强制 } => 安装语言服务(&界面表, 强制)?,
                安装子命令::工具链 {
                    版本,
                    发布标签,
                    仅分析器,
                    强制,
                } => 安装工具链(&界面表, &版本, &发布标签, 强制, 仅分析器)?,
            }
            Ok(ExitCode::SUCCESS)
        }
        命令组::诊断环境 => 诊断环境().map(|()| ExitCode::SUCCESS),
        命令组::待译清单 {
            语言,
            导出路径,
            清空清单,
        } => {
            // 待译清单（未译全的编译器与构建工具消息）查看、导出、清零：
            // 参数文案保持英文（维护者命令，不占用语言包界面键）
            let 界面表 = 界面::界面消息表::全局表();
            翻译缺口::清单命令(&界面表, 语言, 导出路径, 清空清单)?;
            Ok(ExitCode::SUCCESS)
        }
        命令组::速查 { 语言, 标记 } => {
            let 语言码 = 语言.clone().unwrap_or_else(探测系统语言);
            // 速查表标题固定英文：全球用户的第一张卡不依赖界面语言
            i18n_rust_engine::语言::设定语言(&语言码);
            let 虚拟源 = PathBuf::from(format!("main.{语言码}"));
            // 借虚拟扩展名让映射加载走既有解析链（项目内、全局、内置语言包）
            let 映射管理器值 = 加载映射(None, Some(&虚拟源))?;
            print!(
                "{}",
                i18n_rust_engine::指南::渲染速查表(&映射管理器值, &语言码, 标记)
            );
            Ok(ExitCode::SUCCESS)
        }
        命令组::指南 {
            语言, 输出文件
        } => {
            let 语言码 = 语言.clone().unwrap_or_else(探测系统语言);
            // 指南正文随语言包界面消息切换，先同步引擎全局语言
            i18n_rust_engine::语言::设定语言(&语言码);
            let 虚拟源 = PathBuf::from(format!("main.{语言码}"));
            let 映射管理器值 = 加载映射(None, Some(&虚拟源))?;
            let 指南文本 = i18n_rust_engine::指南::渲染母语指南(&映射管理器值, &语言码);
            match 输出文件 {
                Some(输出路径) => {
                    // 写盘成功才报「已生成」：失败时让异常冒泡到顶层母语错误出口
                    fs::write(&输出路径, 指南文本)?;
                    println!(
                        "{}",
                        界面::界面消息表::全局表().取带参消息(
                            "agents_generated",
                            &[&输出路径.display().to_string()]
                        )
                    );
                }
                None => print!("{指南文本}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        命令组::语言包管理 { 子命令值 } => {
            处理语言子命令(子命令值).map(|()| ExitCode::SUCCESS)
        }
        命令组::添加依赖 { 依赖们 } => 处理添加子命令(&依赖们),
        命令组::映射管理 { 子命令值 } => match 子命令值 {
            映射子命令::自动生成 {
                包名,
                语言,
                服务商,
                目标版本,
                输出路径,
                安装依赖,
            } => {
                let 语言值 = 语言.unwrap_or_else(探测系统语言);
                i18n_rust_engine::语言::设定语言(&语言值);
                let 输出路径 = 输出路径.unwrap_or_else(|| {
                    // 默认写入项目语言包根：从当前目录向上找清单，
                    // 保证任意子目录下执行都落到项目本地语言包；
                    // 主仓库内落到引擎语言包目录（单一数据源），用户项目落语言包目录
                    let 基目录 = env::current_dir()
                        .ok()
                        .and_then(|当前目录值| 向上查找项目根(&当前目录值))
                        .unwrap_or_else(|| PathBuf::from("."));
                    语言包根目录(&基目录).join(format!("{}/crates/{}.toml", 语言值, 包名))
                });
                运行自动生成(&包名, &语言值, &服务商, &输出路径, 目标版本.as_deref())
                    .map(|()| ExitCode::SUCCESS)
                    .inspect(|_| {
                        // 安装依赖：生成成功后把库加入当前项目依赖（用户项目内执行时）
                        if 安装依赖 {
                            安装库到当前项目(&包名, 目标版本.as_deref());
                        }
                        // 生成后自动对所在语言包跑一次冲突检测（仅提示，不改变退出码：
                        // 语言包可能存在历史遗留问题，生成成功与否以写入结果为准）
                        if let Some(语言目录) = 输出路径.parent().and_then(|上级| 上级.parent())
                            && 语言目录.join("keywords.toml").exists()
                            && let Some(目录文本) = 语言目录.to_str()
                        {
                            let _ = 运行检查(Some(目录文本));
                        }
                    })
            }
            映射子命令::校验 { 目标 } => {
                // 校验输出的语言默认跟随系统语言
                let 语言值 = 探测系统语言();
                i18n_rust_engine::语言::设定语言(&语言值);
                match 运行检查(目标.as_deref()) {
                    Ok(检查通过) => {
                        let 结果码 = if 检查通过 {
                            ExitCode::SUCCESS
                        } else {
                            ExitCode::FAILURE
                        };
                        Ok(结果码)
                    }
                    Err(错误值) => Err(错误值),
                }
            }
            映射子命令::覆盖检查 { 语言 } => {
                // 覆盖率输出的语言默认跟随系统语言
                let 界面语言值 = 探测系统语言();
                i18n_rust_engine::语言::设定语言(&界面语言值);
                match 运行覆盖检查(语言.as_deref()) {
                    Ok(覆盖通过) => {
                        let 覆盖码 = if 覆盖通过 {
                            ExitCode::SUCCESS
                        } else {
                            ExitCode::FAILURE
                        };
                        Ok(覆盖码)
                    }
                    Err(错误值) => Err(错误值),
                }
            }
            映射子命令::脚手架 {
                源语言,
                目标语言,
                输出路径,
                服务商,
            } => {
                let 语言值 = 探测系统语言();
                i18n_rust_engine::语言::设定语言(&语言值);
                运行生成骨架(&源语言, &目标语言, 输出路径.as_deref(), &服务商)
                    .map(|()| ExitCode::SUCCESS)
            }
        },
        命令组::库注册 { 子命令值 } => match 子命令值 {
            库子命令::检索 { 关键词 } => {
                包仓库::检索(关键词.as_deref()).map(|()| ExitCode::SUCCESS)
            }
            库子命令::安装映射 {
                包名, 语言, 强制
            } => 包仓库::安装(&包名, &语言, 强制).map(|()| ExitCode::SUCCESS),
            库子命令::列表 => 包仓库::列出().map(|()| ExitCode::SUCCESS),
            库子命令::移除映射 { 包名, 语言 } => {
                包仓库::移除已装(&包名, &语言).map(|()| ExitCode::SUCCESS)
            }
            库子命令::重新拉取 => 包仓库::更新().map(|()| ExitCode::SUCCESS),
            库子命令::发布映射 {
                包名,
                语言,
                映射文件,
                作者,
            } => 包仓库::发布(&包名, &语言, 映射文件, 作者.as_deref()).map(|()| ExitCode::SUCCESS),
        },
    }
}

/// 首次运行引导：欢迎、编译器缺失提示与下一步建议
///
/// 仅交互终端（标准输出是终端）且标记文件不存在时显示；
/// 显示后写入标记文件，保证每个用户只看到一次。
fn 也许显示首次运行(界面表: &界面::界面消息表) {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return;
    }
    // 与语言管理的全局语言目录相同的跨平台主目录解析
    let 主目录 = env::var("HOME").or_else(|_| env::var("USERPROFILE")).ok();
    let 标记文件 = 主目录.map(|主目录值| PathBuf::from(主目录值).join(".rz").join("first-run"));
    if let Some(标记值) = &标记文件
        && 标记值.exists()
    {
        return;
    }
    println!();
    println!("{}", 界面表.取消息("first_run_hello"));
    // 编译器检测：内置工具链目录或环境变量路径（含可执行文件名）任一命中
    let 编译工具可用 = i18n_rust_engine::工具链::查找工具链程序("rustc").is_some()
        || env::var_os("PATH").is_some_and(|路径值| {
            env::split_paths(&路径值).any(|目录| {
                let 程序名 = if cfg!(windows) { "rustc.exe" } else { "rustc" };
                目录.join(程序名).exists()
            })
        });
    if !编译工具可用 {
        println!("{}", 界面表.取消息("first_run_rustc_missing"));
    }
    println!("{}", 界面表.取消息("first_run_next_steps"));
    println!();
    if let Some(标记值) = 标记文件 {
        if let Some(父级) = 标记值.parent() {
            let _ = std::fs::create_dir_all(父级);
        }
        let _ = std::fs::write(标记值, "");
    }
}

/// 处理语言包子命令
fn 处理语言子命令(子命令值: 语言子命令) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    match 子命令值 {
        语言子命令::列表 => {
            let 清单 = 列出语言();
            if 清单.is_empty() {
                println!("{}", 界面表.取消息("no_lang_installed"));
                return Ok(());
            }
            println!(
                "{}",
                界面表.取带参消息("installed_langs_count", &[&清单.len().to_string()])
            );
            for 信息项 in &清单 {
                let 标签 = match 信息项.语言来源 {
                    语言来源::内置 => 界面表.取消息("tag_builtin"),
                    语言来源::用户安装 => 界面表.取消息("tag_user"),
                };
                let 扩展文本 = 信息项
                    .扩展名
                    .as_deref()
                    .map(|扩展值| format!(".{}", 扩展值))
                    .unwrap_or_else(|| 界面表.取消息("unknown"));
                let 版本文本 = match 信息项.版本.as_deref() {
                    Some(版本值) => 版本值.to_string(),
                    None => 界面表.取消息("unknown"),
                };
                let 可移除文本 = if 信息项.语言来源 == 语言来源::内置 {
                    界面表.取消息("not_removable")
                } else {
                    String::new()
                };
                let 显示文本 = 信息项
                    .显示名称
                    .as_deref()
                    .map(|名称值| format!("{} ({})", 名称值, 信息项.语言代码))
                    .unwrap_or_else(|| 信息项.语言代码.clone());
                println!(
                    "{}",
                    界面表.取带参消息(
                        "lang_list_display",
                        &[&标签, &显示文本, &扩展文本, &版本文本, &可移除文本]
                    )
                );
            }
            println!(
                "{}",
                界面表.取带参消息("global_lang_dir", &[&全局语言目录().display().to_string()])
            );
            Ok(())
        }
        语言子命令::安装语言 { 源, 强制 } => 安装语言(&源, 强制),
        语言子命令::检索 { 关键词 } => {
            // 市场浏览：下载仓库压缩包扫描语言包（含显示名与版本），
            // 可选关键词过滤；与安装共用同一远程源回退策略
            println!(
                "{}",
                界面表.取带参消息(
                    "lang_search_header",
                    &[&界面表.取消息("lang_search_source")]
                )
            );
            let 找到 = 搜索远程语言(关键词.as_deref())?;
            if 找到.is_empty() {
                let 关键词文本 = 关键词.as_deref();
                if let Some(关键词值) = 关键词文本 {
                    if !关键词值.trim().is_empty() {
                        println!("{}", 界面表.取带参消息("lang_search_empty", &[关键词值]));
                    } else {
                        println!("{}", 界面表.取消息("lang_search_empty_all"));
                    }
                } else {
                    println!("{}", 界面表.取消息("lang_search_empty_all"));
                }
                return Ok(());
            }
            for 信息项 in &找到 {
                let 名称值 = 信息项.显示名称.as_deref().unwrap_or("—");
                let 版本文本 = 信息项.版本.as_deref().unwrap_or("—");
                println!("  {:<12} {:<16} {}", 信息项.语言代码, 名称值, 版本文本);
            }
            println!();
            println!("{}", 界面表.取消息("lang_search_hint"));
            Ok(())
        }
        语言子命令::移除语言 { 语言代码 } => 移除语言(&语言代码),
    }
}

/// 处理添加依赖子命令：封装构建工具的添加命令，成功后提示母语映射可用性
///
/// 失败时捕获构建工具的标准错误而非直接透传：初始化只生成方言入口
///（源码目录下的母语主文件），而构建工具只认标准 Rust 入口与清单里的目标声明，
/// 于是新建项目后第一条添加依赖命令必然在清单解析阶段失败，原文是构建器
/// 内部术语且不含任何可执行动作。
///
/// 命中提示要同时满足两个条件：项目确实无目标（[`有货物目标`]，与运行和
/// 检查的阻断同源）且构建工具确实报在清单解析上
///（[`清单错误判定`]）。缺后半个条件，包名为空这类真清单错误也会被
/// 指向检查子命令；缺前半个条件，网络失败、库名拼错都会多出一句无关指引。命中后
/// 先打母语指引，再把构建工具的标准输出与标准错误原样回显——不隐藏它的信息，退出码不变。
fn 处理添加子命令(依赖们: &[String]) -> anyhow::Result<ExitCode> {
    let 界面表 = 界面::界面消息表::全局表();
    let 当前目录值 = env::current_dir()?;
    let 项目根 = 向上查找项目根(&当前目录值)
        .ok_or_else(|| anyhow::anyhow!("{}", 界面表.取消息("add_no_project")))?;
    let 输出值 = Command::new(解析cargo())
        .arg("add")
        .args(依赖们)
        .current_dir(&项目根)
        .output()
        .map_err(|错误值| {
            anyhow::anyhow!(
                "{}",
                界面表.取带参消息("add_cargo_failed", &[&错误值.to_string()])
            )
        })?;
    if !输出值.status.success() {
        let 标准错误文本 = String::from_utf8_lossy(&输出值.stderr);
        if !有货物目标(&项目根) && 清单错误判定(&标准错误文本) {
            let 入口名 = 入口提示名(&项目根);
            // 模板里入口名出现两次（「缺哪个主文件」与「该检查哪个文件」），
            // 故同一实参传两次，与映射缺失提示的写法一致
            eprintln!(
                "{}",
                界面表.取带参消息("add_no_targets_hint", &[&入口名, &入口名])
            );
        }
        // 构建工具自身的错误详情原样回显，保持既有输出可见
        let 标准输出文本 = String::from_utf8_lossy(&输出值.stdout);
        print!("{标准输出文本}");
        eprint!("{标准错误文本}");
        return Ok(ExitCode::FAILURE);
    }
    let 标准输出文本 = String::from_utf8_lossy(&输出值.stdout);
    print!("{标准输出文本}");
    let 语言码 = 界面::探测界面语言();
    for 依赖项 in 依赖们 {
        // 依赖名取版本分隔符前段，并将连字符归一为下划线（代码中导入路径用下划线）
        let 包名 = 依赖项.split('@').next().unwrap_or(依赖项).replace('-', "_");
        match 查找库映射别名(&语言码, &项目根, &包名) {
            Some(别名) => println!(
                "{}",
                界面表.取带参消息("add_mapping_ready", &[&包名, &别名])
            ),
            None => println!(
                "{}",
                界面表.取带参消息("add_mapping_missing", &[&包名, &包名])
            ),
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// 构建工具的失败是否报在清单解析上（其输出不随界面语言本地化，故按英文特征）
fn 清单错误判定(标准错误文本: &str) -> bool {
    标准错误文本.contains("no targets specified in the manifest")
        || 标准错误文本.contains("failed to parse manifest")
}

/// 面向读者的方言入口名（源码目录下的母语主文件之类），仅用于提示文案
///
/// 优先入口主文件，其次按文件名排序取源码目录直属的第一个方言文件——不能依赖
/// 目录读取的任意顺序，否则同一项目两次失败可能指向不同文件。母语入口别名需要
/// 映射表才能识别，此处按排序结果给一个确定值，读者照着跑同样会生成入口产物。
fn 入口提示名(项目根: &Path) -> String {
    let 扩展名序列 = 全部可用扩展名();
    let 源码目录 = 项目根.join("src");
    let Ok(目录项集) = fs::read_dir(&源码目录) else {
        return "src/main.rs".to_string();
    };
    let mut 候选们: Vec<PathBuf> = 目录项集
        .flatten()
        .map(|目录项| 目录项.path())
        .filter(|路径值| 路径值.is_file() && 是否方言文件(路径值, &扩展名序列))
        .collect();
    候选们.sort();
    let 首选 = 候选们
        .iter()
        .find(|路径值| 路径值.file_stem().and_then(|词干值| 词干值.to_str()) == Some("main"))
        .or_else(|| 候选们.first());
    match 首选 {
        Some(路径值) => 路径值
            .strip_prefix(项目根)
            .unwrap_or(路径值.as_path())
            .display()
            .to_string()
            .replace('\\', "/"),
        None => "src/main.rs".to_string(),
    }
}

/// 查找第三方库在当前语言映射中的母语别名（项目或全局语言包优先于内置）
///
/// 扫描库映射目录的模块路径节：值的首段（双冒号分隔）与库名
/// 匹配即命中；命中时返回对应母语键作为示例提示。
fn 查找库映射别名(语言码: &str, 项目根: &Path, 包名: &str) -> Option<String> {
    // 1. 项目内语言包与全局用户语言包的库映射目录
    let 目录们 = [
        语言包根目录(项目根).join(语言码).join("crates"),
        全局语言目录().join(语言码).join("crates"),
    ];
    for 目录 in &目录们 {
        let Ok(目录项集) = fs::read_dir(目录) else {
            continue;
        };
        for 目录项 in 目录项集.flatten() {
            let 路径值 = 目录项.path();
            if 路径值.extension().and_then(|扩展值| 扩展值.to_str()) != Some("toml") {
                continue;
            }
            if let Ok(内容文本) = fs::read_to_string(&路径值)
                && let Some(别名) = 查找映射别名(&内容文本, 包名)
            {
                return Some(别名);
            }
        }
    }
    // 2. 内置语言包（未知语言代码自动回退中文）
    let 内置数据 = 获取内置数据(语言码);
    for (_, 内容文本) in 内置数据.三方库数据 {
        if let Some(别名) = 查找映射别名(内容文本, 包名) {
            return Some(别名);
        }
    }
    None
}

/// 在单个映射内容中查找第三方库对应的母语别名
fn 查找映射别名(内容文本: &str, 包名: &str) -> Option<String> {
    let 值对象: toml::Value = toml::from_str(内容文本).ok()?;
    let 路径表 = 值对象.get("模块路径")?.as_table()?;
    for (键名, 值项) in 路径表 {
        let Some(英文路径) = 值项.as_str() else {
            continue;
        };
        let 首段 = 英文路径.split("::").next().unwrap_or(英文路径);
        if 首段.replace('-', "_") == 包名 {
            return Some(键名.clone());
        }
    }
    None
}

/// 映射自动生成时的安装依赖：把库加入当前项目依赖（找不到项目时仅告警）
///
/// 指定目标版本时按同一版本需求添加（精确锁定或前缀），保证应用依赖与映射生成基准一致。
fn 安装库到当前项目(包名: &str, 目标版本: Option<&str>) {
    let 界面表 = 界面::界面消息表::全局表();
    let Some(根目录) = env::current_dir()
        .ok()
        .and_then(|当前目录值| 向上查找项目根(&当前目录值))
    else {
        println!("{}", 界面表.取消息("mapping_auto_install_no_project"));
        return;
    };
    let 规格文本 = match 目标版本.and_then(版本需求) {
        Some(需求值) => format!("{}@{}", 包名, 需求值),
        None => 包名.to_string(),
    };
    match Command::new(解析cargo())
        .arg("add")
        .arg(&规格文本)
        .current_dir(&根目录)
        .status()
    {
        Ok(进程状态值) => {
            if 进程状态值.success() {
                println!("{}", 界面表.取带参消息("mapping_auto_installed", &[包名]))
            } else {
                println!(
                    "{}",
                    界面表.取带参消息(
                        "mapping_auto_install_failed",
                        &[包名, &进程状态值.to_string()]
                    )
                )
            }
        }
        Err(错误值) => println!(
            "{}",
            界面表.取带参消息("mapping_auto_install_failed", &[包名, &错误值.to_string()])
        ),
    }
}

/// 根据源码文件定位项目根（包含清单文件的目录）
fn 查找项目根(源文件值: &Path) -> anyhow::Result<PathBuf> {
    let 文件目录 = if 源文件值.is_absolute() {
        源文件值
            .parent()
            .map(|父级| 父级.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        env::current_dir()?
            .join(源文件值)
            .parent()
            .map(|父级| 父级.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };

    if let Some(根目录) = 向上查找项目根(&文件目录) {
        return Ok(根目录);
    }
    // 未找到清单时明确报错：静默回退当前目录会在无关目录写入入口产物
    anyhow::bail!(
        "{}",
        界面::界面消息表::全局表()
            .取带参消息("no_project_root", &[&源文件值.display().to_string()])
    )
}

/// 计算运行与检查的入口产物路径。
///
/// 仅当源文件确为项目源码目录直属入口（父目录恰为项目的源码目录，且词干为
/// [`入口产物名`]，即主函数词或库词）时才写入构建工具自动
/// 发现的 包根——构建工具只认这两个默认位置，入口产物无法移出源码目录：
/// - 项目根的构建脚本入口则进托管目录，不把构建脚本覆盖为入口；
/// - 子目录或示例里恰好名为入口的文件，产物进托管目录，不越界覆盖宿主入口；
/// - 其余非入口文件一律进 [`托管目录`](引擎库::产物布局::托管目录)
///   镜像源码布局，教学源码目录里只留母语源文件与入口产物；源文件在项目之外时
///   退回旧行为（产物写在自身旁边）。
///
/// 库入口有一个例外：清单里显式声明了库路径时以声明为准
///（[`声明的库路径`]），否则升级后那些项目会静默编译一份不再更新的托管产物。
fn 入口产物路径(
    项目根: &Path, 源文件值: &Path, 管理器值: &映射管理器
) -> PathBuf {
    let 默认产物 = 入口产物默认(源文件值, 项目根, 管理器值);
    if 默认产物 == Some(库产物路径) {
        声明的库路径(项目根).unwrap_or_else(|| 项目根.join(库产物路径))
    } else if let Some(产物名) = 默认产物 {
        项目根.join(产物名)
    } else {
        i18n_rust_engine::产物布局::产物路径(项目根, 源文件值)
    }
}

/// 源文件是否为项目的固定入口（[`入口产物路径`] 的判定条件，
/// 产物注解需按同一条件选择子模块目录，故单独暴露）
fn 是否入口源(源文件值: &Path, 项目根: &Path, 管理器值: &映射管理器) -> bool {
    入口产物默认(源文件值, 项目根, 管理器值).is_some()
}

/// 按托管布局为方言源文件的产物补模块路径注解，并返回磁盘行到引擎直行映射
///
/// `是否根` 取 [`是否入口源`]：包根的子模块与源文件同目录，
/// 非根文件的子模块位于 `<自身词干>/`（编译器文件式布局）。
/// 落点解析（方言子模块到托管产物、手写标准 Rust 子模块到原地）由引擎统一
/// 实现，命令行写盘与诊断行映射重算必须走同一函数，否则注解行数不同、行号回译错位。
pub(crate) fn 注解产物(
    代码: &str,
    项目根: &Path,
    源文件: &Path,
    产物: &Path,
    是否根: bool,
    扩展名序列: &[String],
) -> (String, Vec<usize>) {
    let 父级 = 源文件.parent().unwrap_or(Path::new("."));
    let 子目录 = if 是否根 {
        父级.to_path_buf()
    } else {
        match 源文件.file_stem().and_then(|词干值| 词干值.to_str()) {
            Some(词干值) => 父级.join(词干值),
            None => 父级.to_path_buf(),
        }
    };
    let 产物目录 = 产物.parent().unwrap_or(Path::new("."));
    i18n_rust_engine::产物布局::注解产物模块路径并返回行映射(
        代码,
        &子目录,
        产物目录,
        项目根,
        扩展名序列,
    )
}

/// 旧布局遗留在源文件旁边的产物迁走后，清理同目录同名产物
///
/// 升级到托管布局后，源码目录里的旧产物已无人引用：留在母语目录里
/// 正是本次要解决的问题。安全口径与 [`写入转译产物`] 一致——内容与本次
/// 产物相同（就是我们的生成物）才删；不同则可能是用户手改过，备份为备份文件
/// 再删，绝不静默丢弃。
fn 迁移旧产物(
    旧产物: &Path, 产物: &Path, 内容文本: &str, 界面表: &crate::界面::界面消息表
) {
    if 旧产物 == 产物 || !旧产物.is_file() {
        return;
    }
    let 相同 = fs::read_to_string(旧产物).is_ok_and(|旧内容| 旧内容 == 内容文本);
    if 相同 {
        let _ = fs::remove_file(旧产物);
        return;
    }
    let 备份 = 旧产物.with_extension("rs.bak");
    if fs::rename(旧产物, &备份).is_ok() {
        println!(
            "{}",
            界面表.取带参消息(
                "legacy_product_moved",
                &[&旧产物.display().to_string(), &备份.display().to_string()]
            )
        );
    }
}

/// 入口文件写完后，清掉托管目录里同一源文件的旧产物
///
/// 库入口被纳入入口集合之前，产物落在托管目录；入口规则改后
/// 它写到 包根，托管目录里那份残留已无任何模块路径注解引用，却仍可能被
/// 编译器的默认模块查找命中——代码改了却不报错，比报错更糟。只删托管目录内、
/// 且不等于本次落点的那一个文件：清单用库路径指着托管路径时两者相同，不能删。
fn 清理入口生成副本(项目根: &Path, 源文件: &Path, 产物: &Path) {
    let 陈旧产物 = i18n_rust_engine::产物布局::产物路径(项目根, 源文件);
    if 陈旧产物 != 产物 && 陈旧产物.is_file() {
        let _ = fs::remove_file(陈旧产物);
    }
}

/// 文件父目录（规范化为绝对路径，纯比较不依赖文件必已存在）是否恰为
/// 项目源码目录——即该文件是否为项目真正的源码目录直属入口，用于防止子目录
/// 里恰好名为入口的文件越界聚合、覆盖宿主入口产物。
fn 直属源码目录(源文件值: &Path, 项目根: &Path) -> bool {
    let Some(父级) = 源文件值.parent() else {
        return false;
    };
    // 相对路径按当前目录补齐为绝对，再与已规范化的项目根对齐比较；
    // 路径可能尚未落盘（单元测试、新建文件），规范化失败时退回词法路径。
    let 绝对父级 = if 父级.is_absolute() {
        父级.to_path_buf()
    } else {
        env::current_dir()
            .map(|当前目录值| 当前目录值.join(父级))
            .unwrap_or_else(|_| 父级.to_path_buf())
    };
    let 规范父级 = 绝对父级.canonicalize().unwrap_or_else(|_| 绝对父级.clone());
    let 源码目录 = 项目根.join("src");
    let 规范源码 = 源码目录.canonicalize().unwrap_or_else(|_| 源码目录.clone());
    规范父级 == 规范源码
}

/// 构建工具自动发现的两个 包根产物路径（相对项目根）
const 主程序产物路径: &str = "src/main.rs";
const 库产物路径: &str = "src/lib.rs";

/// 词干是否为项目入口主函数名：字面主函数名，或语言包中映射到主函数名的
/// 母语词（中文的主函数、日文的主函数、俄文的主函数等）。
///
/// 初始化生成源码目录下的母语主文件，教程与示例同样使用该命名；
/// 早期项目还可能有母语词干入口——都须聚合到
/// 固定入口产物，否则构建工具报「没有可构建目标」。
fn 是否入口词干(词干: &str, 管理器值: &映射管理器) -> bool {
    词干 == "main"
        || 管理器值
            .取关键词表()
            .iter()
            .any(|(母语词, 英文)| 英文 == "main" && 母语词 == 词干)
}

/// 入口词干对应的默认 包根产物路径（相对项目根）；非入口词干返回 `无`
///
/// 构建工具只自动发现两个 包根：可执行程序与库。方言侧对应两组词干：
/// 主函数词及其语言包主函数词，和字面库词。
/// 库词不是 Rust 关键字、各语言包都不翻译它，因此它没有母语别名——写
/// 母语库文件仍然只是普通模块，这一点与主函数不对称，但正是构建工具的
/// 命名要求，造库要的是零配置而不是文件名本地化。
fn 入口产物名(词干: &str, 管理器值: &映射管理器) -> Option<&'static str> {
    if 是否入口词干(词干, 管理器值) {
        Some(主程序产物路径)
    } else if 词干 == "lib" {
        Some(库产物路径)
    } else {
        None
    }
}

/// 源文件按其词干应落到的默认 包根路径（[`入口产物名`] 与
/// [`直属源码目录`] 两级判定合一）；非入口返回 `无`
fn 入口产物默认(
    源文件值: &Path,
    项目根: &Path,
    管理器值: &映射管理器,
) -> Option<&'static str> {
    let 词干 = 源文件值.file_stem().and_then(|词干值| 词干值.to_str())?;
    入口产物名(词干, 管理器值).filter(|_| 直属源码目录(源文件值, 项目根))
}

/// 清单中库路径显式声明的库产物路径（绝对形式，项目内才返回）
///
/// 纳管库入口之前，造方言库的唯一配方是在清单里手写库路径
///（当时库入口的产物落在托管目录）。入口集合纳入库词后 rzc 默认改写
/// 包根，若无视既有声明，那些项目会继续编译一份不再被更新的托管产物——
/// 代码改了却不报错，比报错更糟。声明是用户意图的唯一来源，故以它为准；
/// 无声明、内容非法、或声明指向项目外时返回 `无` 回退默认位置，不猜意图、
/// 不往项目外写文件。
fn 声明的库路径(项目根: &Path) -> Option<PathBuf> {
    let 文本内容 = fs::read_to_string(项目根.join("Cargo.toml")).ok()?;
    let 清单值: toml::Value = toml::from_str(&文本内容).ok()?;
    let 相对路径 = 清单值.get("lib")?.get("path")?.as_str()?;
    // 词法归一后仍要在项目根内：前缀判断不消解上级目录，
    // 字面拼接看起来像在项目里，实际写到了项目外
    let 拼接路径 = 项目根.join(相对路径);
    let 内部路径 = i18n_rust_engine::产物布局::相对项目根路径(项目根, &拼接路径)?;
    Some(项目根.join(内部路径))
}

/// 从指定目录向上查找项目根（含清单文件的目录），未找到返回 `无`
pub(crate) fn 向上查找项目根(起始目录: &Path) -> Option<PathBuf> {
    let mut 当前目录值 = 起始目录
        .canonicalize()
        .unwrap_or_else(|_| 起始目录.to_path_buf());
    loop {
        if 当前目录值.join("Cargo.toml").exists() {
            return Some(当前目录值);
        }
        当前目录值 = 当前目录值.parent()?.to_path_buf();
    }
}

/// 同 [`向上查找项目根`]（兄弟模块使用的等价入口名）
pub(crate) fn 查找项目根向上(起始目录: &Path) -> Option<PathBuf> {
    向上查找项目根(起始目录)
}

/// 调用构建工具前的清单预检：把其自身高频新手错误（非 JSON 通道）拦在
/// 本地，用母语报出并给出下一步命令。
///
/// 只拦「构建工具一定会失败」且原因确定的两种形态：
/// - 清单不是合法配置（括号、引号、缩进写坏）；
/// - 未声明编译目标且两个默认入口都不存在（初始化被跳过，
///   或入口文件不在源码目录直属时最常见）。
///
/// 其余构建工具错误（依赖解析、工作区等）仍由翻译链处理。
fn 清单预检(项目根: &Path, 界面表: &界面::界面消息表) -> anyhow::Result<()> {
    let 路径值 = 项目根.join("Cargo.toml");
    let 文本内容 = fs::read_to_string(&路径值)?;
    toml::from_str::<toml::Value>(&文本内容).map_err(|错误值| {
        anyhow::anyhow!(
            "{}",
            界面表.取带参消息(
                "manifest_parse_error",
                &[&路径值.display().to_string(), &错误值.to_string()]
            )
        )
    })?;
    if !有货物目标(项目根) {
        anyhow::bail!("{}", 界面表.取消息("manifest_no_target"));
    }
    Ok(())
}

/// 项目是否有构建工具能读取的构建目标（[`清单预检`] 的判定核心）
///
/// 判定规则的唯一来源：运行与检查的阻断、添加依赖的「先跑一次检查」提示都走这里，
/// 两处语义不许分叉。
///
/// 目标声明：库、可执行程序、示例任一存在；或默认发现位置
/// 有内容——两个默认入口，或二进制、测试、基准目录非空
///（里面可能是子目录入口，故只判空不判后缀：误报会阻断正常项目，漏报仅退化为
/// 翻译链处理构建工具英文错）。清单读不到或不是合法内容时按「有目标」返回，
/// 让调用方去报各自更准确的错（解析失败不等于无目标）。
///
/// 诊断子命令的项目自检也走这里，判定口径不许分叉。
pub(crate) fn 有货物目标(项目根: &Path) -> bool {
    let Ok(文本内容) = fs::read_to_string(项目根.join("Cargo.toml")) else {
        return true;
    };
    let Ok(清单值) = toml::from_str::<toml::Value>(&文本内容) else {
        return true;
    };
    let 已声明 = 清单值.get("lib").is_some()
        || 清单值.get("bin").is_some()
        || 清单值.get("example").is_some();
    已声明
        || ["src/main.rs", "src/lib.rs"]
            .iter()
            .any(|相对路径段| 项目根.join(相对路径段).exists())
        || ["src/bin", "tests", "benches"].iter().any(|目录名| {
            let 路径值 = 项目根.join(目录名);
            路径值.is_dir()
                && fs::read_dir(&路径值)
                    .ok()
                    .is_some_and(|mut 目录项集| 目录项集.next().is_some())
        })
}

/// 项目内语言包根目录：主仓库为单副本结构，引擎语言包目录；
/// 普通用户项目仍沿用项目语言包目录约定（自定义覆盖）
pub(crate) fn 语言包根目录(基目录: &Path) -> PathBuf {
    let 引擎包目录 = 基目录.join("crates/engine/lang-packs");
    if 引擎包目录.is_dir() {
        引擎包目录
    } else {
        基目录.join("lang-packs")
    }
}

/// 测试辅助：无项目上下文的单文件转译（关键字与别名管线，非 ASCII 模块注解）
#[cfg(test)]
fn 转译为英文(源文本: &str, 管理器值: &映射管理器) -> String {
    注解非母语模块(&i18n_rust_engine::转译管线(源文本, 管理器值).输出)
}

/// 收集待导出源文件所在范围的声明上下文（导出与预览共用）
///
/// 单文件且能定位项目根时复用 [`收集项目上下文`]（扫描整个
/// 源码目录，跨文件声明豁免与运行、检查一致）；目录或项目外文件按「待导出文件
/// 集合」收集——导出的契约是零锁定毕业，可在任意目录导出教材示例，
/// 不要求目标位于项目内。
fn 收集导出上下文(
    源文件值: &Path,
    管理器值: &映射管理器,
) -> i18n_rust_engine::别名::项目上下文 {
    let 扩展名序列 = 全部可用扩展名();
    if 源文件值.is_dir() {
        return 按文件收集上下文(收集方言文件(源文件值, &扩展名序列), 管理器值);
    }
    if let Ok(项目根) = 查找项目根(源文件值) {
        let mut 上下文值 = 收集项目上下文(&项目根, 源文件值, 管理器值);
        // 导出可在源码目录之外（示例目录、项目根）：项目扫描只覆盖源码目录，
        // 用文件所在目录补齐兄弟文件（模块名与声明名，否则跨文件字段
        // 访问仍会被别名改写）
        let 扫描根 = 源文件值.parent().unwrap_or(Path::new("."));
        if !扫描根.starts_with(项目根.join("src")) {
            // 单文件补齐只扫同目录兄弟（不递归），避免遍历大目录子树
            let 额外 = 按文件收集上下文(收集同级方言文件(扫描根, &扩展名序列), 管理器值);
            上下文值.模块集.extend(额外.模块集);
            上下文值.名字集.extend(额外.名字集);
            上下文值.条目集.extend(额外.条目集);
        }
        return 上下文值;
    }
    let 扫描根 = 源文件值.parent().unwrap_or(Path::new("."));
    // 项目外单文件：上下文只看同目录兄弟（不递归整个父目录子树）
    let mut 文件们 = 收集同级方言文件(扫描根, &扩展名序列);
    // 目标文件必须纳入（父目录无方言文件等边界）
    if !文件们.iter().any(|路径值| 路径值 == 源文件值) {
        文件们.push(源文件值.to_path_buf());
    }
    按文件收集上下文(文件们, 管理器值)
}

/// 从给定源文件集合收集项目级声明上下文
///
/// 模块名取文件词干（路径段豁免的保守近似），声明名由引擎扫描各文件内容。
fn 按文件收集上下文(
    文件们: impl IntoIterator<Item = PathBuf>,
    管理器值: &映射管理器,
) -> i18n_rust_engine::别名::项目上下文 {
    let mut 模块集 = HashSet::new();
    let mut 源集合: Vec<String> = Vec::new();
    for 路径值 in 文件们 {
        if let Some(词干) = 路径值.file_stem().and_then(|词干值| 词干值.to_str()) {
            模块集.insert(词干.to_string());
        }
        if let Ok(源文本) = fs::read_to_string(&路径值) {
            源集合.push(源文本);
        }
    }
    i18n_rust_engine::别名::项目上下文::从源集合(
        模块集,
        源集合.iter().map(String::as_str),
        管理器值,
    )
}

/// 转译单个源文件为可导出的标准 Rust（项目上下文与模块路径注解）
///
/// 显示名是教学告警的文件归属名（导出与预览按调用方给的路径显示）。
/// 目录模式区分导出形态：目录批量导出时非入口文件的子模块位于
/// 自身词干目录下（编译器默认查找规则），注解须带词干前缀；单文件导出
/// 按 包根处理（与既有导出产物一致）。
fn 导出转译(
    路径值: &Path,
    源文本: &str,
    管理器值: &映射管理器,
    项目: &i18n_rust_engine::别名::项目上下文,
    缓存: &mut 转译缓存,
    目录模式: bool,
    显示名: &str,
) -> String {
    let 转译结果 = 项目缓存转译(源文本, 管理器值, 缓存, Some(项目), 显示名);
    注解导出模块(&转译结果.输出, 路径值, 管理器值, 目录模式)
}

/// 给导出产物的非 ASCII 模块名补模块路径注解
///
/// 仅 包根（主函数词或库词）与单文件导出按当前目录解析；
/// 目录模式的其他文件是子模块，须按自身词干下的子名注解。
/// `mod` 文件是例外：它的子模块与自身同目录，不能加 `mod/` 前缀
/// （否则生成 `#[path = "mod/子.rs"]`，目录模式下必然导出失败）。
fn 注解导出模块(
    代码: &str, 路径值: &Path, 管理器值: &映射管理器, 目录模式: bool
) -> String {
    let 词干 = 路径值
        .file_stem()
        .and_then(|词干值| 词干值.to_str())
        .unwrap_or("");
    // 包根词干按当前目录解析子模块，口径与 [`入口产物名`] 统一，
    // 不在注解侧另立一套入口判定
    if !目录模式 || 入口产物名(词干, 管理器值).is_some() || 词干 == "mod" {
        注解非母语模块(代码)
    } else {
        i18n_rust_engine::模块路径::注解嵌套模块并返回行映射(代码, 词干).0
    }
}

/// 校验输入是单个源文件（运行、检查、预览共用）
///
/// 目录输入在这三个子命令里无意义，且在某些平台上直接读取目录
/// 只会报拒绝访问；路径不存在
/// 时则只暴露系统层「文件不存在」错误。两者都给出可操作提示，把目录
/// 批量导出指向导出子命令。
fn 确保源文件(
    界面表: &crate::界面::界面消息表, 源文件值: &Path
) -> anyhow::Result<()> {
    if !源文件值.exists() {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("input_missing", &[&源文件值.display().to_string()])
        );
    }
    if 源文件值.is_dir() {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("input_is_directory", &[&源文件值.display().to_string()])
        );
    }
    Ok(())
}

/// 按语言分组待导出文件（组与组内均保持排序稳定）
///
/// 目录可能混有多种方言：按扩展名检测语言并分组，
/// 每组用自身语言包转译——单锚点管理器会把其他语言的关键字原样留在
/// 产物里。显式指定语言包目录时尊重用户选择，不分组。
fn 按语言分组文件<'甲>(
    文件们: &'甲 [PathBuf],
    显式语言包: Option<&Path>,
) -> Vec<Vec<&'甲 PathBuf>> {
    if 显式语言包.is_some() {
        return vec![文件们.iter().collect()];
    }
    let mut 分组表: BTreeMap<String, Vec<&PathBuf>> = BTreeMap::new();
    for 路径值 in 文件们 {
        let 扩展值 = 路径值
            .extension()
            .and_then(|扩展段| 扩展段.to_str())
            .unwrap_or("");
        let 语言码 = 扩展名转语言代码(扩展值).unwrap_or_else(|| 扩展值.to_string());
        分组表.entry(语言码).or_default().push(路径值);
    }
    分组表.into_values().collect()
}

/// 导出单处方言源码（单目标与批量共用的唯一入口）
///
/// 路径不存在时直接给可操作提示：否则锚点落到无扩展名路径，
/// 映射加载会报「无法识别源文件扩展名」这类无关错误。
fn 导出单个(源文件值: &Path, 语言包: Option<&Path>) -> anyhow::Result<Vec<PathBuf>> {
    if !源文件值.exists() {
        let 界面表 = 按文件取界面(源文件值, &语言包.map(Path::to_path_buf));
        anyhow::bail!(
            "{}",
            界面表.取带参消息("input_missing", &[&源文件值.display().to_string()])
        );
    }
    // 目录模式的语言包解析锚点：目录内第一个方言文件（目录本身无
    // 扩展名，直接用于映射加载会报「无法识别源文件扩展名」）
    let 锚点 = if 源文件值.is_dir() {
        let 扩展名序列 = 全部可用扩展名();
        收集方言文件(源文件值, &扩展名序列)
            .into_iter()
            .next()
            .unwrap_or_else(|| 源文件值.to_path_buf())
    } else {
        源文件值.to_path_buf()
    };
    let 界面表 = 按文件取界面(&锚点, &语言包.map(Path::to_path_buf));
    // 目录可能混有多种方言：导出源们内按扩展名分组、每组用
    // 自身语言包（单锚点管理器会把其他语言的关键字原样留在
    // 产物里）；显式指定语言包时统一使用指定语言包目录
    导出源们(&界面表, 源文件值, 语言包)
}

/// 导出子命令入口：单目标保持历史行为，多目标批量导出并汇总
///
/// 批量模式复用 [`导出单个`] 逐目标导出：单个目标失败不中断其余目标，
/// 最后打印成功与失败汇总并以非零码退出，减少逐个手动导出造成的遗漏
/// 与滞后事故。单目标失败仍直接冒泡为顶层错误（与历史行为一致）。
fn 导出目标们(文件们: &[PathBuf], 语言包: &Option<PathBuf>) -> anyhow::Result<ExitCode> {
    let 界面表 = 文件们
        .first()
        .map(|单个文件| 按文件取界面(单个文件, 语言包))
        .unwrap_or_else(界面::界面消息表::全局表);
    // 参数校验：同一目标重复出现只会重复写同一份产物（后写覆盖前写），
    // 多半是命令拼写失误，直接报错而非静默去重
    let mut 已见 = HashSet::new();
    if let Some(重复项) = 文件们
        .iter()
        .find(|单个文件| !已见.insert(单个文件.as_os_str().to_owned()))
    {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("eject_args_duplicate", &[&重复项.display().to_string()])
        );
    }

    if let Some(单个目标) = 文件们.first().filter(|_| 文件们.len() == 1) {
        for 输出路径 in 导出单个(单个目标, 语言包.as_deref())? {
            println!(
                "{}",
                界面表.取带参消息("exported_to", &[&输出路径.display().to_string()])
            );
        }
        return Ok(ExitCode::SUCCESS);
    }

    let mut 成功数 = 0usize;
    let mut 失败数 = 0usize;
    for 单个文件 in 文件们 {
        match 导出单个(单个文件, 语言包.as_deref()) {
            Ok(输出们) => {
                成功数 += 1;
                for 输出路径 in 输出们 {
                    println!(
                        "{}",
                        界面表.取带参消息("exported_to", &[&输出路径.display().to_string()])
                    );
                }
            }
            Err(错误值) => {
                失败数 += 1;
                eprintln!(
                    "{}",
                    界面表.取带参消息(
                        "eject_failed",
                        &[&单个文件.display().to_string(), &format!("{错误值:#}")]
                    )
                );
            }
        }
    }
    if 失败数 == 0 {
        println!(
            "{}",
            界面表.取带参消息("eject_summary_ok", &[&成功数.to_string()])
        );
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!(
            "{}",
            界面表.取带参消息(
                "eject_summary_partial",
                &[&成功数.to_string(), &失败数.to_string()]
            )
        );
        Ok(ExitCode::FAILURE)
    }
}

/// 导出一处方言源码为标准 Rust 文件，返回全部产物路径
///
/// 目录模式：递归导出目录下全部方言文件（直接读取目录在某些平台会报
/// 拒绝访问，必须先按文件与目录分流）。单文件与目录共用项目级声明上下文：否则
/// 模块声明的声明位受保护而导入路径段与字段访问被别名改写，声明与
/// 引用错位。
///
/// 混合语言目录按扩展名分组，每组独立加载语言包与上下文。写盘统一走
/// [`写入转译产物`]：已有同名产物且内容不同时先备份
///（与运行、检查同口径），避免渐进迁移中手写产物被静默覆盖。
fn 导出源们(
    界面表: &crate::界面::界面消息表,
    源文件值: &Path,
    显式语言包: Option<&Path>,
) -> anyhow::Result<Vec<PathBuf>> {
    let 目录模式 = 源文件值.is_dir();
    let 文件们 = if 目录模式 {
        收集方言文件(源文件值, &全部可用扩展名())
    } else {
        vec![源文件值.to_path_buf()]
    };
    if 文件们.is_empty() {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("eject_no_source", &[&源文件值.display().to_string()])
        );
    }
    let mut 缓存 = i18n_rust_engine::缓存::转译缓存::默认持久化();
    let mut 输出们 = Vec::with_capacity(文件们.len());
    for 分组 in 按语言分组文件(&文件们, 显式语言包) {
        let 锚点: &Path = 分组
            .first()
            .map(|路径值| 路径值.as_path())
            .unwrap_or(源文件值);
        let 映射管理器值 = 加载映射(显式语言包.map(Path::to_path_buf), Some(锚点))?;
        // 目录模式：组内文件共享上下文；单文件：项目级上下文（含 包根探测）
        let 项目上下文值 = if 目录模式 {
            按文件收集上下文(
                分组.iter().map(|路径值| (*路径值).to_path_buf()),
                &映射管理器值,
            )
        } else {
            收集导出上下文(源文件值, &映射管理器值)
        };
        for 路径值 in 分组 {
            let 源文本 = fs::read_to_string(路径值)?;
            // 导出前扫一遍冲突（无教学检查时一并关闭）：字段名与声明名命中词条会被
            // 改写、会恐慌的词条被当文本用——只提示不阻断，避免导出后才发现语义已变
            if i18n_rust_engine::校验::教学告警已启用() {
                for 提示 in
                    i18n_rust_engine::导出冲突提示::扫描冲突(&源文本, &映射管理器值)
                {
                    i18n_rust_engine::log_warn!(
                        "eject_hint",
                        "{}：{}",
                        路径值.display(),
                        提示.转提示文本()
                    );
                }
            }
            let 代码 = 导出转译(
                路径值,
                &源文本,
                &映射管理器值,
                &项目上下文值,
                &mut 缓存,
                目录模式,
                &路径值.display().to_string(),
            );
            let 输出路径 = 路径值.with_extension("rs");
            写入转译产物(&输出路径, &代码, 界面表)?;
            输出们.push(输出路径);
        }
    }
    Ok(输出们)
}

/// 反译重生命令：把方言或标准 Rust 工程反译重生为目标方言（跨母语阅读）
///
/// 逐文件管线与导出同源但更纯粹：源方言经正向转译管线到
/// 标准 Rust，再经引擎反译重生到目标方言；镜像写出到输出目录（默认源工程
/// 根旁的镜像目录），复制清单与工具链锁定文件，并用目标
/// 语言包渲染 AGENTS.md。
///
/// 产物是纯方言源（不含模块路径注解——那些由 rzc 正向构建时按托管布局
/// 再补），结构随源保持一致，可再次构建编译。保真边界：目标语言包缺该
/// 词条（尤其带双冒号的多段模块路径）时该段保留英文；一词多母语反查取排序最小
/// 的确定候选，故跨母语「阅读」不保证逐字节还原某一种子方言的原词。
fn 反译重生目标们(
    目标语言: &str,
    源语言: &Option<String>,
    输出目录: &Option<PathBuf>,
    语言包: &Option<PathBuf>,
    文件们: &[PathBuf],
) -> anyhow::Result<ExitCode> {
    // 锚点：优先显式路径，否则当前目录（默认整目录遍历）
    let 锚点 = 文件们
        .first()
        .cloned()
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let 界面表 = 按文件取界面(&PathBuf::from(format!("main.{目标语言}")), &None);
    if !锚点.exists() {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("input_missing", &[&锚点.display().to_string()])
        );
    }
    // 源工程根：目录即扫描根（并向上定位项目根），文件则定位其项目根
    let (扫描根, 项目根) = if 锚点.is_dir() {
        let 根目录 = 向上查找项目根(&锚点).unwrap_or_else(|| 锚点.clone());
        (锚点.clone(), 根目录)
    } else {
        let 根目录 = 查找项目根(&锚点)?;
        (根目录.clone(), 根目录)
    };
    // 目标语言包管理器（英文到目标母语反查），借虚拟扩展名走既有解析链
    let 目标管理器 = 加载映射(None, Some(&PathBuf::from(format!("main.{目标语言}"))))?;
    // 输出目录：显式优先，否则源工程根旁的镜像目录
    let 输出根 = 输出目录
        .clone()
        .unwrap_or_else(|| 项目根.join(format!("relocalized.{目标语言}")));

    // 收集候选源文件：方言文件与标准 Rust 文件（把标准扩展名临时登记为方言扩展名以复用遍历）
    let mut 扩展名序列 = 全部可用扩展名();
    扩展名序列.push("rs".to_string());
    let 源文件们 = 收集方言文件(&扫描根, &扩展名序列);
    if 源文件们.is_empty() {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("eject_no_source", &[&扫描根.display().to_string()])
        );
    }

    // 源语言管理器缓存（按语言代码）；显式源语言时统一用该语言
    let mut 管理器缓存: HashMap<String, 映射管理器> = HashMap::new();
    let mut 计数 = 0usize;
    let mut 写了入口 = false;
    for 路径值 in &源文件们 {
        if 输出根.is_absolute() && 路径值.starts_with(&输出根) {
            continue; // 跳过上一次运行写出的目录（幂等重跑不自我吞噬）
        }
        let 扩展值 = 路径值
            .extension()
            .and_then(|扩展段| 扩展段.to_str())
            .unwrap_or("");
        let 源文本 = fs::read_to_string(路径值)?;
        let 英文文本 = if 扩展值 == "rs" {
            源文本 // 已是标准 Rust：正向不转译，直接反译
        } else {
            // 方言源：按其语言（或显式源语言）正向转译到标准 Rust
            let 语言码 = 源语言
                .clone()
                .unwrap_or_else(|| 扩展名转语言代码(扩展值).unwrap_or_else(|| 扩展值.to_string()));
            if !管理器缓存.contains_key(&语言码) {
                let 虚拟源 = PathBuf::from(format!("main.{语言码}"));
                let 管理器值 = 加载映射(语言包.clone(), Some(&虚拟源))?;
                管理器缓存.insert(语言码.clone(), 管理器值);
            }
            i18n_rust_engine::转译管线(&源文本, &管理器缓存[&语言码]).输出
        };
        let 方言文本 = i18n_rust_engine::反译重生::反译重生(&英文文本, &目标管理器);
        // 镜像写出：相对扫描根的路径，扩展名换为目标语言代码
        let 相对路径 = 路径值.strip_prefix(&扫描根).unwrap_or(路径值.as_path());
        let 输出路径 = 输出根.join(相对路径).with_extension(目标语言);
        if let Some(父级) = 输出路径.parent() {
            fs::create_dir_all(父级)?;
        }
        fs::write(&输出路径, 方言文本)?;
        计数 += 1;
        let 词干 = 输出路径
            .file_stem()
            .and_then(|词干值| 词干值.to_str())
            .unwrap_or("");
        if 是否入口词干(词干, &目标管理器) {
            写了入口 = true;
        }
    }

    // 复制清单与工具链锁定（源工程根存在才复制）
    for 清单名 in ["Cargo.toml", "rust-toolchain.toml", "rust-toolchain"] {
        let 源路径 = 项目根.join(清单名);
        if 源路径.is_file() {
            let _ = fs::copy(&源路径, 输出根.join(清单名));
        }
    }
    // 目标方言的协作说明书（与指南打通，复用引擎渲染；失败不阻断产物写出）
    let _ = fs::write(
        输出根.join("AGENTS.md"),
        i18n_rust_engine::指南::渲染母语指南(&目标管理器, 目标语言),
    );

    if !写了入口 {
        i18n_rust_engine::log_warn!(
            "relocalize",
            "{}",
            界面表.取消息("relocalize_no_module_entry")
        );
    }
    println!(
        "{}",
        界面表.取带参消息(
            "relocalize_done",
            &[&计数.to_string(), &输出根.display().to_string()]
        )
    );
    Ok(ExitCode::SUCCESS)
}

/// 带项目上下文的缓存转译，附项目级声明上下文（跨文件声明豁免），
/// 教学告警按文件归属输出
///
/// 缓存语境指纹由引擎并入项目上下文指纹：
/// 项目声明集合变化时相关缓存自动失效。
fn 项目缓存转译(
    源文本: &str,
    管理器值: &映射管理器,
    缓存: &mut i18n_rust_engine::缓存::转译缓存,
    项目: Option<&i18n_rust_engine::别名::项目上下文>,
    显示名: &str,
) -> i18n_rust_engine::缓存::转译输出 {
    let 产物 = i18n_rust_engine::静默带项目转译源文本(源文本, 管理器值, 缓存, 项目).unwrap_or_else(
        |_错误值| {
            // 缓存失败不阻断转译：回退无缓存静默管线（告警统一由下方输出，不重复）
            i18n_rust_engine::log_warn!(
                "cli",
                "{}",
                i18n_rust_engine::语言::取消息("log_transpile_cache_fallback")
            );
            i18n_rust_engine::静默带项目转译管线(源文本, 管理器值, 项目)
        },
    );
    按文件输出教学告警(源文本, 显示名, 管理器值);
    产物
}

/// 教学告警的文件归属名：项目内文件相对源码目录显示，与 [`转译项目文件`]
/// 给兄弟文件算的显示名同口径；源码目录之外（根目录脚本、目录导出）按给定路径显示
fn 教学显示名(源文件值: &Path, 项目根: &Path) -> String {
    let 绝对路径 = 源文件值
        .canonicalize()
        .unwrap_or_else(|_| 源文件值.to_path_buf());
    let 源码目录 = 项目根.join("src");
    绝对路径
        .strip_prefix(&源码目录)
        .or_else(|_| 绝对路径.strip_prefix(项目根))
        .unwrap_or(绝对路径.as_path())
        .display()
        .to_string()
}

/// 解析构建工具可执行文件：内置工具链优先，环境变量路径回退；
/// 找不到时返回其命令名由系统报错（保持与旧行为一致的报错信息）
pub fn 解析cargo() -> PathBuf {
    i18n_rust_engine::工具链::查找工具链程序("cargo").unwrap_or_else(|| PathBuf::from("cargo"))
}

/// 解析编译器可执行文件：内置工具链优先，环境变量路径回退
pub fn 解析rustc() -> PathBuf {
    i18n_rust_engine::工具链::查找工具链程序("rustc").unwrap_or_else(|| PathBuf::from("rustc"))
}

/// 解析与 [`解析rustc`] 同工具链的文档工具：优先内建或环境变量路径的独立文档工具，
/// 缺失时改用编译器的系统根精确定位其配套文档工具（两者同
/// 系统根，版本一致），否则回退环境变量路径的文档工具名由系统报错。
///
/// 映射自动生成时手动调用文档工具生成 JSON 必须与构建工具编译库文件用同一
/// 编译器，否则跨版本链接触发版本不匹配错误。
pub fn 解析文档工具() -> PathBuf {
    if let Some(路径值) = i18n_rust_engine::工具链::查找工具链程序("rustdoc") {
        return 路径值;
    }
    if let Ok(输出值) = Command::new(解析rustc())
        .arg("--print")
        .arg("sysroot")
        .output()
    {
        let 系统根 = String::from_utf8_lossy(&输出值.stdout).trim().to_string();
        if !系统根.is_empty() {
            let 程序路径 = PathBuf::from(系统根)
                .join("bin")
                .join(format!("rustdoc{}", env::consts::EXE_SUFFIX));
            if 程序路径.is_file() {
                return 程序路径;
            }
        }
    }
    PathBuf::from("rustdoc")
}

/// 递归收集目录下全部方言源文件（路径排序保证确定性）
///
/// 支持多层模块布局：模块文件可放在以父模块
/// 命名的子目录中，与编译器的「父模块文件加父模块目录」规则一致。
fn 收集方言文件(目录: &Path, 扩展名序列: &[String]) -> Vec<PathBuf> {
    let mut 输出们 = Vec::new();
    递归收集方言文件(目录, 扩展名序列, &mut 输出们);
    输出们.sort();
    输出们
}

fn 递归收集方言文件(
    目录: &Path, 扩展名序列: &[String], 输出们: &mut Vec<PathBuf>
) {
    let Ok(目录项集) = fs::read_dir(目录) else {
        return;
    };
    for 目录项 in 目录项集.flatten() {
        let 路径值 = 目录项.path();
        let Ok(文件类型值) = 目录项.file_type() else {
            continue;
        };
        if 文件类型值.is_dir() {
            // 构建产物目录不会出现在源码树内；无特殊排除
            递归收集方言文件(&路径值, 扩展名序列, 输出们);
        } else if 文件类型值.is_file() && 是否方言文件(&路径值, 扩展名序列) {
            输出们.push(路径值);
        }
    }
}

/// 判断路径是否是已注册的方言源文件（扩展名可带点或不带点）
fn 是否方言文件(路径值: &Path, 扩展名序列: &[String]) -> bool {
    路径值
        .extension()
        .and_then(|扩展段| 扩展段.to_str())
        .is_some_and(|扩展段| {
            扩展名序列
                .iter()
                .any(|单个| 单个 == 扩展段 || 单个 == &format!(".{扩展段}"))
        })
}

/// 收集目录直接子文件中的方言源文件（不递归，路径排序保证确定性）
///
/// 单文件导出的上下文补齐只关心同目录兄弟模块；若沿用递归收集，在用户
/// 主目录等大目录下会无谓遍历整棵子树。
fn 收集同级方言文件(目录: &Path, 扩展名序列: &[String]) -> Vec<PathBuf> {
    let Ok(目录项集) = fs::read_dir(目录) else {
        return Vec::new();
    };
    let mut 输出们: Vec<PathBuf> = 目录项集
        .flatten()
        .filter_map(|目录项| {
            let 路径值 = 目录项.path();
            let 文件类型值 = 目录项.file_type().ok()?;
            (文件类型值.is_file() && 是否方言文件(&路径值, 扩展名序列)).then_some(路径值)
        })
        .collect();
    输出们.sort();
    输出们
}

/// 收集项目级声明上下文（跨文件声明豁免）
///
/// 递归扫描源码目录下全部层级的方言文件与入口文件（可能在项目根），收集：
/// - 模块名：各方言文件词干（含嵌套层级，供多层路径链豁免）；
/// - 声明名：各文件的项名与结构体字段（裸使用处豁免）。
///
/// 与 [`转译项目文件`] 扫描范围保持一致（源码目录全层级递归），
/// 入口文件不在源码目录时单独补扫；读取失败的文件跳过（转译阶段会报错）。
fn 收集项目上下文(
    项目根: &Path,
    入口文件: &Path,
    管理器值: &映射管理器,
) -> i18n_rust_engine::别名::项目上下文 {
    use std::collections::HashSet;
    let 扩展名序列 = 全部可用扩展名();
    let 入口规范 = 入口文件.canonicalize().ok();
    let mut 模块集 = HashSet::new();
    let mut 源集合: Vec<String> = Vec::new();
    let mut 入口已见 = false;

    let 源码目录 = 项目根.join("src");
    for 路径值 in 收集方言文件(&源码目录, &扩展名序列) {
        if let Some(词干) = 路径值.file_stem().and_then(|词干值| 词干值.to_str()) {
            模块集.insert(词干.to_string());
        }
        if 入口规范.is_some() && 路径值.canonicalize().ok() == 入口规范 {
            入口已见 = true;
        }
        if let Ok(源文本) = fs::read_to_string(&路径值) {
            源集合.push(源文本);
        }
    }
    // 入口文件不在源码目录（如项目根的自定义路径）时单独补扫
    if !入口已见 && 入口文件.is_file() {
        if let Some(词干) = 入口文件.file_stem().and_then(|词干值| 词干值.to_str()) {
            模块集.insert(词干.to_string());
        }
        if let Ok(源文本) = fs::read_to_string(入口文件) {
            源集合.push(源文本);
        }
    }

    i18n_rust_engine::别名::项目上下文::从源集合(
        模块集,
        源集合.iter().map(String::as_str),
        管理器值,
    )
}

/// 同步转译项目源码目录下的全部方言源文件（入口文件除外）为对应产物文件，
/// 使多文件项目的模块引用链可用；非已注册方言扩展名的文件（如手写标准 Rust）跳过。
///
/// 转译在线程作用域中并行执行（教学项目文件相互独立，无共享可变
/// 状态）；共享缓存用互斥锁保护——查询与插入为短临界区，转译本身在锁外
/// 并行，文件多时与串行相比显著提速（缓存命中时仅查表，开销可忽略）。
fn 转译项目文件(
    项目根: &Path,
    入口文件: &Path,
    管理器值: &映射管理器,
    缓存: &Mutex<i18n_rust_engine::缓存::转译缓存>,
    项目: &i18n_rust_engine::别名::项目上下文,
) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    let 源码目录 = 项目根.join("src");
    let 扩展名序列 = 全部可用扩展名();
    // 无源码目录时没有项目内模块要转译（由构建工具自行报错），但根目录方言文件
    // 的产物也写在托管目录，失效残留的清理照旧要跑
    if !源码目录.is_dir() {
        清理失效产物(项目根, &扩展名序列, &界面表);
        return Ok(());
    }
    // 源码目录顶层的主函数入口词干在这里跳过：
    // 入口产物只由本次请求的文件写到自动发现的 包根，而所有主函数入口
    // 共用同一份入口产物——兄弟入口也走一遍会互相覆盖刚写好的那份。
    // 库入口不跳过：它的产物是托管目录里自己那份，与主程序产物
    // 无冲突；纳管库之前它就是这么转译的，而清单用库路径指着托管
    // 路径的老项目仍要靠这条路径拿到新鲜产物（跳过的话构建工具会编译一份陈旧的）。
    // 嵌套层级中的同名文件不是入口（编译器模块树按路径区分），照常转译。
    let 入口绝对 = 入口文件.canonicalize().ok();
    let mut 文件们 = Vec::new();
    for 路径值 in 收集方言文件(&源码目录, &扩展名序列) {
        if 入口绝对.as_ref() == Some(&路径值) {
            continue;
        }
        let 位于源码顶层 = 路径值.parent() == Some(源码目录.as_path());
        if 位于源码顶层
            && 路径值
                .file_stem()
                .and_then(|词干值| 词干值.to_str())
                .is_some_and(|词干| 是否入口词干(词干, 管理器值))
        {
            continue;
        }
        let Some(扩展值) = 路径值.extension().and_then(|扩展段| 扩展段.to_str()) else {
            continue;
        };
        if 扩展名转语言代码(扩展值).is_none() {
            continue;
        }
        文件们.push(路径值);
    }

    // 语境指纹只算一次（全部文件共享同一语言包与项目上下文）；
    // 项目上下文指纹并入后，跨文件声明变化时旧缓存自动失效。
    // 必须与引擎静默带项目转译源文本使用同一组合函数：
    // 入口文件与项目内其它文件共用同一个缓存实例，
    // 两处指纹算法若不同，同一语境会算出两个键，缓存互相不可见。
    let 指纹值 = i18n_rust_engine::缓存::转译缓存::组合指纹(
        管理器值.取语境指纹(),
        Some(项目.指纹()),
    );
    let 首个错误: Mutex<Option<anyhow::Error>> = Mutex::new(None);
    scope(|作用域句柄| {
        let mut 句柄们 = Vec::with_capacity(文件们.len());
        for 路径值 in 文件们 {
            // 界面表与首个错误遮蔽为引用：移动闭包捕获的是可复制的共享引用
            let 界面引用 = &界面表;
            let 错误引用 = &首个错误;
            let 扩展名引用: &[String] = &扩展名序列;
            // 相对源码目录的显示名在闭包外算好（避免路径缓冲被移动进线程）
            let 显示名 = 路径值
                .strip_prefix(&源码目录)
                .unwrap_or(&路径值)
                .display()
                .to_string();
            let 句柄 = 作用域句柄.spawn(move || {
                if 错误引用
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .is_some()
                {
                    return; // 已有失败文件：跳过剩余工作
                }
                let 源文本 = match fs::read_to_string(&路径值) {
                    Ok(内容文本) => 内容文本,
                    Err(错误值) => {
                        *错误引用.lock().unwrap_or_else(PoisonError::into_inner) =
                            Some(anyhow::anyhow!(
                                "{}",
                                界面引用.取带参消息(
                                    "transpile_file_failed",
                                    &[&路径值.display().to_string(), &错误值.to_string()]
                                )
                            ));
                        return;
                    }
                };
                // 短临界区：查询缓存（命中直接复用产物）
                let 已缓存 = 缓存
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .查询(&源文本, 指纹值)
                    .cloned();
                let 产物 = match 已缓存 {
                    Some(缓存产物) => 缓存产物,
                    None => {
                        // 锁外并行转译，完成后短临界区写回缓存；
                        // 静默管线：教学告警由匹配之外带文件名输出（多文件项目
                        // 中裸行列无法定位到具体文件）；项目上下文保证
                        // 跨文件调用的成员名与声明侧一致
                        let 新产物 = i18n_rust_engine::静默带项目转译管线(
                            &源文本,
                            管理器值,
                            Some(项目),
                        );
                        缓存.lock().unwrap_or_else(PoisonError::into_inner).插入(
                            &源文本,
                            指纹值,
                            新产物.clone(),
                        );
                        新产物
                    }
                };
                // 告警挂在匹配之外：命中缓存同样要提示。旧版把它放在「未命中」
                // 分支里，同一份源码第二次检查一条告警都不出
                按文件输出教学告警(&源文本, &显示名, 管理器值);
                // 非入口产物写入托管目录并镜像源码布局：模块路径注解落点按同一布局解析，
                // 托管树内的相对位置与旧布局逐字一致（子模块在自身词干目录
                // 下，注解相对当前文件所在目录解析），手写标准 Rust 子模块
                // 则跨出托管目录指回源码目录。
                // 这里不按入口规则落 包根：兄弟入口若也落 包根，等于
                // 替用户凭空造出一个编译目标。
                // 主函数词干已在收集阶段跳过；库词干没跳过，产物照旧落托管目录，
                // 声明库路径的老项目靠它拿新鲜产物。
                let 产物路径值 = i18n_rust_engine::产物布局::产物路径(项目根, &路径值);
                let 产物文本 =
                    注解产物(&产物.输出, 项目根, &路径值, &产物路径值, false, 扩展名引用).0;
                if let Err(错误值) = 写出产物(项目根, &产物路径值, &产物文本, 界面引用)
                {
                    *错误引用.lock().unwrap_or_else(PoisonError::into_inner) = Some(错误值);
                } else if 入口产物默认(&路径值, 项目根, 管理器值).is_none() {
                    // 旧布局把产物写在源文件旁：已迁走，
                    // 同目录遗留文件删掉后源码目录才真的只剩母语源文件。
                    // 入口源文件除外：库入口旁边的产物不是旧布局
                    // 遗留，而是 包根产物。
                    // 当兄弟文件顺带清理会把库产物删掉，可执行程序加库项目于是变成
                    // 「先检查库、再检查主程序，库产物消失，主程序报导入失败」，
                    // 恰好废掉库入口指的那条路（实机复现过）。
                    迁移旧产物(
                        &路径值.with_extension("rs"),
                        &产物路径值,
                        &产物文本,
                        界面引用,
                    );
                }
            });
            句柄们.push(句柄);
        }
        for 句柄 in 句柄们 {
            let _ = 句柄.join();
        }
    });
    match 首个错误.into_inner().unwrap_or(None) {
        Some(错误值) => Err(错误值),
        None => {
            // 全部产物写入完成后清失效残留
            清理失效产物(项目根, &扩展名序列, &界面表);
            Ok(())
        }
    }
}

/// 清走托管目录中失去源文件的产物，有清理时向用户报一句数量
///
/// 判定只看源文件在不在（且不碰托管目录以外路径），实现见引擎
/// [`清理失效产物`](引擎库::产物布局::清理失效产物)：
/// 残留会让编译器按默认模块查找命中旧产物，被删掉的代码仍在参与编译。
fn 清理失效产物(
    项目根: &Path, 扩展名序列: &[String], 界面表: &界面::界面消息表
) {
    let 已清理 = i18n_rust_engine::产物布局::清理失效产物(项目根, 扩展名序列);
    if !已清理.is_empty() {
        println!(
            "{}",
            界面表.取带参消息("stale_products_pruned", &[&已清理.len().to_string()])
        );
    }
}

/// 输出单个方言文件的教学告警（Unicode 混淆、全角标点、教学检查），带文件名归属
///
/// 多文件项目中项目内文件分散在多个方言文件，裸行列无法定位具体文件。扫描集合
/// 与出口都在引擎（[`引擎库::输出教学告警`]），入口文件与
/// 兄弟文件共用同一份口径——早先这里自己拼了一遍扫描器，少调英文回落与保留字
/// 撞名两类：同一份代码走入口路径会提示、走兄弟路径不提示。
/// 输出时机与缓存无关（每次转译都提示）。
fn 按文件输出教学告警(源文本: &str, 显示名: &str, 管理器值: &映射管理器) {
    i18n_rust_engine::输出教学告警(源文本, 管理器值, Some(显示名));
}

/// 拒绝写入符号链接路径（含根以下的中间组件）
///
/// 恶意仓库可把 `.rz`、`.rz/gen` 或目标文件换成指向用户文件的符号链接，
/// 转译写入就会跟随链接覆盖任意文件。逐级检查已存在组件：任何一级是
/// 符号链接即拒绝；尚不存在的新路径不检查（创建后即为普通目录/文件）。
/// 根自身不检查：用户可能本来就通过符号链接打开项目。
fn 检查写入路径无符号链接(根: &Path, 目标: &Path) -> anyhow::Result<()> {
    let mut 当前 = 目标.to_path_buf();
    loop {
        if 当前
            .symlink_metadata()
            .map(|元数据值| 元数据值.file_type().is_symlink())
            .unwrap_or(false)
        {
            anyhow::bail!("拒绝写入符号链接路径：{}", 当前.display());
        }
        let Some(父级) = 当前.parent() else {
            break;
        };
        if !父级.starts_with(根) {
            break;
        }
        当前 = 父级.to_path_buf();
    }
    Ok(())
}

/// 写转译产物，写入语义按落点分道
///
/// 托管目录是 rzc 专属地盘（用户在此无手写内容，源码目录的手写标准 Rust 也
/// 不会被复制进来），改版覆盖就是正常状态，备份只会在托管目录里留下
/// 无人认领的备份文件；而入口产物（可能是学习者手写
/// 英文 Rust）与项目外源文件旁的产物仍走 [`写入转译产物`] 的
/// 「内容不同先备份」护栏。
fn 写出产物(
    项目根: &Path,
    路径值: &Path,
    内容文本: &str,
    界面表: &crate::界面::界面消息表,
) -> anyhow::Result<()> {
    if i18n_rust_engine::产物布局::是否为托管产物(项目根, 路径值) {
        检查写入路径无符号链接(项目根, 路径值)?;
        // 内容未变则不写盘：cargo 以文件修改时间判定改动，重复写入会让每次
        // run/check 都重编译整个包
        if let Ok(既有内容) = fs::read_to_string(路径值)
            && 既有内容 == 内容文本
        {
            return Ok(());
        }
        if let Some(父级) = 路径值.parent() {
            fs::create_dir_all(父级)?;
        }
        return Ok(fs::write(路径值, 内容文本)?);
    }
    写入转译产物(路径值, 内容文本, 界面表)
}

/// 写转译产物：目标已存在且内容不同时先备份为备份文件，绝不静默覆盖用户文件。
///
/// 幂等重跑（内容一致）不产生备份；无同名手写文件时行为与直接写入完全一致。
fn 写入转译产物(
    路径值: &Path,
    内容文本: &str,
    界面表: &crate::界面::界面消息表,
) -> anyhow::Result<()> {
    // 目标若是符号链接，读写都会落到链接指向的文件（可能是用户配置等）；
    // 方言源旁的产物只应是普通文件，退化为普通文件前先拒绝
    if 路径值
        .symlink_metadata()
        .map(|元数据值| 元数据值.file_type().is_symlink())
        .unwrap_or(false)
    {
        anyhow::bail!("拒绝写入符号链接路径：{}", 路径值.display());
    }
    // 方言文件直接放在项目根等场景下源码目录可能尚不存在，先创建父目录
    //（否则写入报底层的「文件不存在」错误，语言服务侧翻译缓存已有同款处理）
    if let Some(父级) = 路径值.parent() {
        fs::create_dir_all(父级)?;
    }
    if let Ok(既有内容) = fs::read_to_string(路径值) {
        if 既有内容 == 内容文本 {
            // 内容一致：不写盘，保留修改时间，避免 cargo 误判改动
            return Ok(());
        }
        let 备份 = 路径值.with_extension("rs.bak");
        fs::rename(路径值, &备份)?;
        println!(
            "{}",
            界面表.取带参消息(
                "transpile_backup",
                &[&路径值.display().to_string(), &备份.display().to_string()]
            )
        );
    }
    fs::write(路径值, 内容文本)?;
    Ok(())
}

/// 探测本机当前生效工具链的通道号（如版本号或每夜版）
///
/// 解析编译器版本输出的第二段，取主次版本号作为通道；每夜版与测试版通道原样返回。
/// 编译器不在环境变量路径或输出格式异常时返回 `无`（调用方跳过生成锁定文件）。
fn 探测工具链通道() -> Option<String> {
    let 输出值 = Command::new(解析rustc()).arg("--version").output().ok()?;
    if !输出值.status.success() {
        return None;
    }
    let 标准输出文本 = String::from_utf8_lossy(&输出值.stdout);
    let 版本文本 = 标准输出文本.split_whitespace().nth(1)?;
    if 版本文本.contains("nightly") {
        return Some("nightly".to_string());
    }
    if 版本文本.contains("beta") {
        return Some("beta".to_string());
    }
    // 版本号取主次版本（通道只保留主次版本，补丁版本由工具链自行解析）
    let mut 分段 = 版本文本.split('.');
    let 主版本 = 分段.next()?;
    let 次版本 = 分段.next()?;
    if 主版本.chars().all(|单字| 单字.is_ascii_digit())
        && 次版本.chars().all(|单字| 单字.is_ascii_digit())
    {
        Some(format!("{主版本}.{次版本}"))
    } else {
        None
    }
}

/// 初始化子命令无法识别系统语言时的回退语言（也是语言菜单的回车默认值）
const 初始化缺省语言: &str = "zh";

/// 解析初始化子命令的目标语言
///
/// 显式语言参数必须对应可用语言包：内置、项目内语言包目录
/// 或全局安装包；否则直接报错——静默回退会生成扩展名未知的主文件，
/// 而用户已经看到「创建成功」，首次运行才报「未知扩展名」。
///
/// 未显式指定时跟随系统区域设置；区域设置不在已支持列表时，终端交互
/// 场景列出菜单让用户自选（回车默认中文），非交互场景（持续集成或管道）
/// 打印回退提示后使用中文，不再静默降级。
fn 解析初始化语言(显式: Option<String>) -> anyhow::Result<String> {
    let 界面表 = 界面::界面消息表::全局表();
    if let Some(语言码) = 显式 {
        let 语言值 = 语言码.trim().to_lowercase();
        if !语言值.is_empty() {
            if 语言包可用(&语言值) {
                return Ok(语言值);
            }
            anyhow::bail!(
                "{}",
                界面表.取带参消息("init_unknown_lang", &[&语言值, &可用语言代码文本()])
            );
        }
    }
    if let Some(语言码) = 界面::探测系统语言标签() {
        return Ok(语言码);
    }
    if let Some(语言码) = 语言菜单提示(&界面表) {
        return Ok(语言码);
    }
    let 区域原文 = 界面::系统区域标签().unwrap_or_else(|| "-".to_string());
    println!(
        "{}",
        界面表.取带参消息("init_lang_fallback", &[&区域原文, 初始化缺省语言])
    );
    Ok(初始化缺省语言.to_string())
}

/// 语言参数是否可用：内置、全局安装、当前目录的项目内覆盖包
fn 语言包可用(语言码: &str) -> bool {
    if 有内置语言包(语言码) {
        return true;
    }
    if 全局语言包子目录名().iter().any(|名称值| 名称值 == 语言码) {
        return true;
    }
    let 当前目录值 = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    语言包根目录(&当前目录值)
        .join(语言码)
        .join("keywords.toml")
        .is_file()
}

/// 错误提示里的候选语言列表（内置与已安装，按字母序）
fn 可用语言代码文本() -> String {
    let mut 代码们: Vec<String> = 内置语言清单().iter().map(|代码| 代码.to_string()).collect();
    for 名称值 in 全局语言包子目录名() {
        if !代码们.contains(&名称值) {
            代码们.push(名称值);
        }
    }
    代码们.sort();
    代码们.join(", ")
}

/// 交互式语言菜单：序号、语言代码或空行（默认中文），最多重试 3 次
///
/// 仅在标准输入与标准输出都是终端时调用（非交互场景下读不到输入）
fn 语言菜单提示(界面表: &界面::界面消息表) -> Option<String> {
    use std::io::{IsTerminal, Write};
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return None;
    }
    let 代码们: Vec<String> = 内置语言清单().iter().map(|代码| 代码.to_string()).collect();
    println!();
    println!("{}", 界面表.取消息("init_lang_menu_title"));
    for (序号, 代码) in 代码们.iter().enumerate() {
        let 名称值 = 界面::母语语言名(代码).unwrap_or(代码);
        println!("  {:>2}) {} ({})", 序号 + 1, 名称值, 代码);
    }
    for _ in 0..3 {
        let 缺省提示 = format!(
            "{} ({})",
            界面::母语语言名(初始化缺省语言).unwrap_or(初始化缺省语言),
            初始化缺省语言
        );
        print!(
            "{}",
            界面表.取带参消息("init_lang_menu_prompt", &[&缺省提示])
        );
        let _ = std::io::stdout().flush();
        let mut 行文本 = String::new();
        if std::io::stdin().read_line(&mut 行文本).unwrap_or(0) == 0 {
            // 输入结束（组合键）：交给调用方回退，不再重复提问
            println!();
            return None;
        }
        if let Some(语言码) = 界面::解析语言选择(&行文本, &代码们, 初始化缺省语言)
        {
            return Some(语言码);
        }
        println!("{}", 界面表.取消息("init_lang_menu_invalid"));
    }
    None
}

fn 创建项目(项目名: &str, 语言码: &str, 生成助手: bool) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::按语言建表(语言码);
    i18n_rust_engine::语言::设定语言(语言码);
    let 项目路径 = PathBuf::from(项目名);
    if 项目路径.exists() {
        anyhow::bail!("{}", 界面表.取带参消息("dir_exists", &[项目名]));
    }
    // 包名取路径最后一段（支持传入绝对或相对路径），并将构建工具不允许的字符替换为下划线
    let 原始包名 = 项目路径
        .file_name()
        .and_then(|名称值| 名称值.to_str())
        .unwrap_or(项目名);
    let 包名: String = 原始包名
        .chars()
        .map(|单字| {
            if 单字.is_ascii_alphanumeric() || 单字 == '-' || 单字 == '_' {
                单字
            } else {
                '_'
            }
        })
        .collect();
    fs::create_dir_all(项目路径.join("src"))?;
    // 版本锁定：固定到本机当前工具链版本（动态探测，避免硬编码随时间过时，
    // 导致文档分析器等工具报「工具链过于陈旧」）；探测失败时不生成锁定文件，
    // 项目跟随系统默认工具链。组件含文档分析器与源码供编辑器使用。
    if let Some(通道值) = 探测工具链通道() {
        fs::write(
            项目路径.join("rust-toolchain.toml"),
            format!(
                "[toolchain]\nchannel = \"{通道值}\"\ncomponents = [\"rustc\", \"cargo\", \"rust-analyzer\", \"rust-src\"]\n"
            ),
        )?;
    }

    fs::write(
        项目路径.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\n[workspace]\n",
            包名
        ),
    )?;
    // 语言包已内置到 rzc 可执行文件中，无需复制；主文件模板随语言参数变化
    fs::write(
        项目路径.join(format!("src/main.{}", 语言码)),
        界面表.取消息("template_main"),
    )?;
    fs::write(
        项目路径.join("README.md"),
        界面表.取带参消息("readme_template", &[项目名, 语言码]),
    )?;
    // 生成物不入库：托管目录收托管产物（模块转译结果），备份文件是旧布局
    // 产物迁移时的安全备份——两者都是机器产物而非源码；入口产物
    // 不列入（标准目标，让克隆后不改方言直接构建也能跑）
    fs::write(项目路径.join(".gitignore"), "/target\n.rz/gen/\n*.rs.bak\n")?;
    // AI 协作说明书：由当前语言包实时渲染（书写规则与全量词条表），
    // 不是一次性静态文案——语言包更新后可刷新；
    // 主流 AI 编程助手会阅读 AGENTS.md 作为项目上下文
    if 生成助手 {
        let 映射管理器值 = 加载映射(None, Some(&PathBuf::from(format!("main.{语言码}"))))?;
        fs::write(
            项目路径.join("AGENTS.md"),
            i18n_rust_engine::指南::渲染母语指南(&映射管理器值, 语言码),
        )?;
        println!("{}", 界面表.取带参消息("agents_generated", &["AGENTS.md"]));
    }
    println!("{}", 界面表.取带参消息("project_created", &[项目名]));
    println!("{}", 界面表.取带参消息("project_created_hint", &[语言码]));
    println!("{}", 界面表.取带参消息("project_run_hint", &[语言码]));
    Ok(())
}

fn 加载映射(
    语言包路径: Option<PathBuf>,
    源文件: Option<&Path>,
) -> anyhow::Result<映射管理器> {
    // 映射数据来源日志（语言包开发调试：设置日志环境变量后可见）
    // 注意：映射加载先于转译管线执行，日志需在此提前初始化（幂等）
    i18n_rust_engine::日志::初始化();
    let 界面表 = 按文件取界面(源文件.unwrap_or(Path::new("")), &语言包路径);
    // 1. 如果用户通过语言包参数指定了外部目录，强制使用
    if let Some(路径值) = 语言包路径 {
        i18n_rust_engine::log_info!(
            "映射加载",
            "{}",
            界面表.取带参消息("mapping_source_explicit", &[&路径值.display().to_string()])
        );
        return 映射管理器::从目录载入(&路径值).map_err(|错误值| {
            anyhow::anyhow!(
                "{}",
                界面表.取带参消息("load_lang_pack_failed", &[&错误值.to_string()])
            )
        });
    }
    // 2. 根据源文件扩展名确定语言代码
    let 扩展值 = 源文件
        .and_then(|文件值| 文件值.extension())
        .and_then(|扩展段| 扩展段.to_str())
        .unwrap_or("");
    let 语言码 = 扩展名转语言代码(扩展值).ok_or_else(|| {
        let 可用列表 = 全部可用扩展名();
        let 可用文本 = if 可用列表.is_empty() {
            界面表.取消息("no_available_ext")
        } else {
            可用列表
                .iter()
                .map(|扩展段| format!(".{}", 扩展段))
                .collect::<Vec<String>>()
                .join(", ")
        };
        anyhow::anyhow!(
            "{}",
            界面表.取带参消息("unknown_extension", &[扩展值, &可用文本])
        )
    })?;
    // 3. 项目内语言包目录存在时优先使用（自定义覆盖）：
    //    主仓库为引擎语言包目录（单一数据源），用户项目为项目语言包目录；
    //    项目根从源文件向上查找；源文件不在项目内时回退当前目录，兼容旧用法
    let mut 本地候选们: Vec<PathBuf> = Vec::new();
    if let Some(文件值) = 源文件
        && let Some(父级) = 文件值.parent()
        && let Some(根目录) = 向上查找项目根(父级)
    {
        本地候选们.push(语言包根目录(&根目录).join(&语言码));
    }
    let 当前目录值 = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    本地候选们.push(语言包根目录(&当前目录值).join(&语言码));
    for 本地路径 in &本地候选们 {
        if 本地路径.exists() {
            i18n_rust_engine::log_info!(
                "映射加载",
                "{}",
                界面表
                    .取带参消息("mapping_source_project", &[&本地路径.display().to_string()])
            );
            return 映射管理器::从目录载入(本地路径).map_err(|错误值| {
                anyhow::anyhow!(
                    "{}",
                    界面表.取带参消息("load_local_lang_pack_failed", &[&错误值.to_string()])
                )
            });
        }
    }
    // 4. 全局用户语言包目录
    let 全局路径 = 全局语言目录().join(&语言码);
    if 全局路径.exists() {
        i18n_rust_engine::log_info!(
            "映射加载",
            "{}",
            界面表.取带参消息("mapping_source_global", &[&全局路径.display().to_string()])
        );
        return 映射管理器::从目录载入(&全局路径).map_err(|错误值| {
            anyhow::anyhow!(
                "{}",
                界面表.取带参消息("load_global_lang_pack_failed", &[&错误值.to_string()])
            )
        });
    }
    // 5. 回退到内置语言包（未内置的语言提示用户安装，避免静默使用中文）
    if !有内置语言包(&语言码) {
        return Err(anyhow::anyhow!(
            "{}",
            界面表.取带参消息("lang_not_builtin", &[&语言码, &语言码])
        ));
    }
    let 内置数据 = 获取内置数据(&语言码);
    i18n_rust_engine::log_info!(
        "映射加载",
        "{}",
        界面表.取带参消息("mapping_source_builtin", &[&语言码])
    );
    映射管理器::从内置载入(
        内置数据.关键字内容,
        内置数据.模块路径内容,
        内置数据.标准库内容,
        内置数据.三方库数据,
    )
    .map_err(|错误值| {
        anyhow::anyhow!(
            "{}",
            界面表.取带参消息("load_builtin_lang_pack_failed", &[&错误值.to_string()])
        )
    })
}

/// 根据源文件与语言包参数选择界面消息语言
///
/// 显式语言包目录优先；否则按源文件扩展名确定语言代码；
/// 无法识别时回退界面语言环境变量、系统语言或中文。
fn 按文件取界面(源文件值: &Path, 语言包: &Option<PathBuf>) -> 界面::界面消息表 {
    if let Some(路径值) = 语言包 {
        let 界面表 = 界面::界面消息表::按显式目录建表(路径值);
        // 同步引擎全局语言（目录名即语言代码）
        let 语言码 = 路径值
            .file_name()
            .and_then(|名称值| 名称值.to_str())
            .unwrap_or("zh")
            .to_string();
        i18n_rust_engine::语言::设定语言(&语言码);
        return 界面表;
    }
    let 语言码 = 源文件值
        .extension()
        .and_then(|扩展段| 扩展段.to_str())
        .and_then(扩展名转语言代码)
        .unwrap_or_else(界面::探测界面语言);
    i18n_rust_engine::语言::设定语言(&语言码);
    界面::界面消息表::按语言建表(&语言码)
}

/// 按当前界面语言本地化命令行帮助文本
///
/// 利用命令工厂生成命令后逐项覆盖简介与帮助，
/// 使帮助输出与各子命令帮助均使用目标语言。
fn 本地化命令行(界面表: &界面::界面消息表) -> clap::Command {
    use clap::CommandFactory;
    参数组::command()
        .about(界面表.取消息("cli_about"))
        .mut_arg("无教学检查", |参数值| {
            参数值.help(界面表.取消息("arg_no_lint_help"))
        })
        .mut_subcommand("init", |命令值| {
            命令值
                .about(界面表.取消息("cmd_init_about"))
                .mut_arg("语言", |参数值| {
                    参数值.help(界面表.取消息("arg_lang_help"))
                })
                .mut_arg("助手", |参数值| {
                    参数值.help(界面表.取消息("cmd_init_ai_help"))
                })
        })
        .mut_subcommand("run", |命令值| 命令值.about(界面表.取消息("cmd_run_about")))
        .mut_subcommand("check", |命令值| {
            命令值
                .about(界面表.取消息("cmd_check_about"))
                .mut_arg("修复", |参数值| {
                    参数值.help(界面表.取消息("arg_check_fix_help"))
                })
        })
        .mut_subcommand("eject", |命令值| {
            命令值
                .about(界面表.取消息("cmd_eject_about"))
                .mut_arg("文件们", |参数值| {
                    参数值.help(界面表.取消息("arg_eject_files_help"))
                })
        })
        .mut_subcommand("transpile", |命令值| {
            命令值.about(界面表.取消息("cmd_transpile_about"))
        })
        .mut_subcommand("add", |命令值| {
            命令值
                .about(界面表.取消息("cmd_add_about"))
                .mut_arg("依赖们", |参数值| {
                    参数值.help(界面表.取消息("arg_add_crates_help"))
                })
        })
        .mut_subcommand("install", |命令值| {
            命令值
                .about(界面表.取消息("cmd_install_about"))
                .mut_subcommand("lsp", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_install_lsp_about"))
                        .mut_arg("强制", |参数值| {
                            参数值.help(界面表.取消息("arg_force_help"))
                        })
                })
                .mut_subcommand("toolchain", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("tc_install_help"))
                        .mut_arg("版本", |参数值| {
                            参数值.help(界面表.取消息("arg_tc_version_help"))
                        })
                        .mut_arg("发布标签", |参数值| {
                            参数值.help(界面表.取消息("arg_tc_ra_tag_help"))
                        })
                        .mut_arg("仅分析器", |参数值| {
                            参数值.help(界面表.取消息("arg_tc_ra_only_help"))
                        })
                        .mut_arg("强制", |参数值| {
                            参数值.help(界面表.取消息("arg_force_help"))
                        })
                })
        })
        .mut_subcommand("doctor", |命令值| {
            命令值.about(界面表.取消息("tc_doctor_help"))
        })
        .mut_subcommand("gaps", |命令值| {
            命令值
                .about(界面表.取消息("cmd_gaps_about"))
                .mut_arg("语言", |参数值| {
                    参数值.help(界面表.取消息("arg_gaps_lang_help"))
                })
                .mut_arg("导出路径", |参数值| {
                    参数值.help(界面表.取消息("arg_gaps_export_help"))
                })
                .mut_arg("清空清单", |参数值| {
                    参数值.help(界面表.取消息("arg_gaps_clear_help"))
                })
        })
        .mut_subcommand("cheat", |命令值| {
            命令值
                .about(界面表.取消息("cmd_cheat_about"))
                .mut_arg("语言", |参数值| {
                    参数值.help(界面表.取消息("arg_cheat_lang_help"))
                })
                .mut_arg("标记", |参数值| {
                    参数值.help(界面表.取消息("arg_markdown_help"))
                })
        })
        .mut_subcommand("guide", |命令值| {
            命令值
                .about(界面表.取消息("cmd_guide_about"))
                .mut_arg("语言", |参数值| {
                    参数值.help(界面表.取消息("arg_guide_lang_help"))
                })
                .mut_arg("输出文件", |参数值| {
                    参数值.help(界面表.取消息("arg_guide_out_help"))
                })
        })
        .mut_subcommand("relocalize", |命令值| {
            命令值
                .about(界面表.取消息("cmd_relocalize_about"))
                .mut_arg("目标语言", |参数值| {
                    参数值.help(界面表.取消息("arg_relocalize_to_help"))
                })
                .mut_arg("源语言", |参数值| {
                    参数值.help(界面表.取消息("arg_relocalize_from_help"))
                })
                .mut_arg("输出目录", |参数值| {
                    参数值.help(界面表.取消息("arg_relocalize_out_dir_help"))
                })
        })
        .mut_subcommand("lang", |命令值| {
            命令值
                .about(界面表.取消息("cmd_lang_about"))
                .mut_subcommand("list", |子命令实例| {
                    子命令实例.about(界面表.取消息("cmd_lang_list_about"))
                })
                .mut_subcommand("install", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_lang_install_about"))
                        .mut_arg("源", |参数值| {
                            参数值.help(界面表.取消息("arg_lang_source_help"))
                        })
                        .mut_arg("强制", |参数值| {
                            参数值.help(界面表.取消息("arg_force_help"))
                        })
                })
                .mut_subcommand("search", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_lang_search_about"))
                        .mut_arg("关键词", |参数值| {
                            参数值.help(界面表.取消息("arg_lang_search_keyword_help"))
                        })
                })
                .mut_subcommand("remove", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_lang_remove_about"))
                        .mut_arg("语言代码", |参数值| {
                            参数值.help(界面表.取消息("arg_lang_code_help"))
                        })
                })
        })
        .mut_subcommand("mapping", |命令值| {
            命令值
                .about(界面表.取消息("cmd_mapping_about"))
                .mut_subcommand("auto", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_mapping_auto_about"))
                        .mut_arg("包名", |参数值| {
                            参数值.help(界面表.取消息("arg_mapping_auto_crate_help"))
                        })
                        .mut_arg("语言", |参数值| {
                            参数值.help(界面表.取消息("arg_lang_help"))
                        })
                        .mut_arg("服务商", |参数值| {
                            参数值.help(界面表.取消息("arg_provider_help"))
                        })
                        .mut_arg("目标版本", |参数值| {
                            参数值.help(界面表.取消息("arg_target_version_help"))
                        })
                        .mut_arg("输出路径", |参数值| {
                            参数值.help(界面表.取消息("arg_output_help"))
                        })
                        .mut_arg("安装依赖", |参数值| {
                            参数值.help(界面表.取消息("arg_install_help"))
                        })
                })
                .mut_subcommand("check", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_mapping_check_about"))
                        .mut_arg("目标", |参数值| {
                            参数值.help(界面表.取消息("cmd_mapping_check_target_help"))
                        })
                })
                .mut_subcommand("scaffold", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_mapping_scaffold_about"))
                        .mut_arg("源语言", |参数值| {
                            参数值.help(界面表.取消息("cmd_mapping_scaffold_source_help"))
                        })
                        .mut_arg("目标语言", |参数值| {
                            参数值.help(界面表.取消息("cmd_mapping_scaffold_target_help"))
                        })
                        .mut_arg("输出路径", |参数值| {
                            参数值.help(界面表.取消息("arg_output_help"))
                        })
                        .mut_arg("服务商", |参数值| {
                            参数值.help(界面表.取消息("cmd_mapping_scaffold_provider_help"))
                        })
                })
                .mut_subcommand("coverage", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_mapping_coverage_about"))
                        .mut_arg("语言", |参数值| {
                            参数值.help(界面表.取消息("arg_coverage_lang_help"))
                        })
                })
        })
        .mut_subcommand("crate", |命令值| {
            命令值
                .about(界面表.取消息("cmd_crate_about"))
                .mut_subcommand("search", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_crate_search_about"))
                        .mut_arg("关键词", |参数值| {
                            参数值.help(界面表.取消息("arg_crate_keyword_help"))
                        })
                })
                .mut_subcommand("install", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_crate_install_about"))
                        .mut_arg("包名", |参数值| {
                            参数值.help(界面表.取消息("arg_crate_name_normalize_help"))
                        })
                        .mut_arg("语言", |参数值| {
                            参数值.help(界面表.取消息("arg_lang_help"))
                        })
                        .mut_arg("强制", |参数值| {
                            参数值.help(界面表.取消息("arg_force_help"))
                        })
                })
                .mut_subcommand("list", |子命令实例| {
                    子命令实例.about(界面表.取消息("cmd_crate_list_about"))
                })
                .mut_subcommand("remove", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_crate_remove_about"))
                        .mut_arg("包名", |参数值| {
                            参数值.help(界面表.取消息("arg_crate_name_help"))
                        })
                        .mut_arg("语言", |参数值| {
                            参数值.help(界面表.取消息("arg_lang_help"))
                        })
                })
                .mut_subcommand("update", |子命令实例| {
                    子命令实例.about(界面表.取消息("cmd_crate_update_about"))
                })
                .mut_subcommand("publish", |子命令实例| {
                    子命令实例
                        .about(界面表.取消息("cmd_crate_publish_about"))
                        .mut_arg("包名", |参数值| {
                            参数值.help(界面表.取消息("arg_crate_name_help"))
                        })
                        .mut_arg("语言", |参数值| {
                            参数值.help(界面表.取消息("arg_lang_help"))
                        })
                        .mut_arg("映射文件", |参数值| {
                            参数值.help(界面表.取消息("arg_crate_file_help"))
                        })
                        .mut_arg("作者", |参数值| {
                            参数值.help(界面表.取消息("arg_crate_author_help"))
                        })
                })
        })
}

/// 根据源码文件扩展名获取语言代码
///
/// 优先查询动态映射表，未命中时回退静态映射。
fn 扩展名转语言代码(扩展值: &str) -> Option<String> {
    if let Some(语言码) = 查询扩展名映射(扩展值) {
        return Some(语言码);
    }
    静态扩展名映射().get(扩展值).cloned()
}

#[cfg(test)]
mod 母语测试 {
    use super::{
        入口产物路径, 写入转译产物, 扩展名转语言代码, 按语言分组文件, 探测工具链通道,
        收集同级方言文件, 收集项目上下文, 是否入口源, 查找映射别名, 检查写入路径无符号链接,
        注解导出模块, 注解非母语模块, 清理入口生成副本, 转译为英文, 转译项目文件, 项目缓存转译,
    };
    // 引擎注解器直测（产物写盘已改走产物布局的托管布局版）
    use i18n_rust_engine::模块路径::注解非母语模块并返回行映射;

    /// 加载内置中文映射管理器（测试转译管线用）
    fn 中文管理器() -> i18n_rust_engine::映射管理器::映射管理器 {
        let 内置数据 = crate::内置语言::获取内置数据("zh");
        i18n_rust_engine::映射管理器::映射管理器::从内置载入(
            内置数据.关键字内容,
            内置数据.模块路径内容,
            内置数据.标准库内容,
            内置数据.三方库数据,
        )
        .expect("内置中文语言包应可加载")
    }

    /// 混合语言目录按扩展名分组：同语言文件归组，显式语言包不再分组
    ///
    /// 持环境变量锁：分组走扩展名动态映射（扫描全局语言包目录，受
    /// 语言目录环境变量影响），语言管理的环境变量测试并发修改该变量时
    /// 会令日文解不出语言码、组序错位。
    #[test]
    fn 按语言分组测试() {
        let _锁 = crate::语言管理::tests::环境锁();
        use std::path::{Path, PathBuf};
        let 文件们 = vec![
            PathBuf::from("a.zh"),
            PathBuf::from("b.ja"),
            PathBuf::from("c.zh"),
        ];
        let 分组们 = 按语言分组文件(&文件们, None);
        // 有序映射按语言码排序：日文组在前，中文组保留文件顺序
        assert_eq!(分组们.len(), 2);
        assert_eq!(分组们[0].len(), 1);
        assert!(分组们[0][0].ends_with("b.ja"));
        assert_eq!(分组们[1].len(), 2);
        assert!(分组们[1][0].ends_with("a.zh"));
        assert!(分组们[1][1].ends_with("c.zh"));
        // 显式语言包：不分组，全部文件交给指定语言包
        let 显式分组 = 按语言分组文件(&文件们, Some(Path::new("lang-packs/ja")));
        assert_eq!(显式分组.len(), 1);
        assert_eq!(显式分组[0].len(), 3);
    }

    /// 目录模式导出：mod 文件的子模块与自身同目录，注解不得带 mod/ 前缀
    #[test]
    fn 目录导出模块注解测试() {
        use std::path::PathBuf;
        let 管理器值 = 中文管理器();
        let 源文本 = "mod 零件;\n";
        let 模块代码 = 注解导出模块(
            源文本,
            &PathBuf::from("src/响应映射/mod.zh"),
            &管理器值,
            true,
        );
        assert!(模块代码.contains("#[path = \"零件.rs\"]"));
        assert!(!模块代码.contains("mod/零件.rs"));
        // 普通子模块仍按自身词干前缀（rustc 对「名.rs」的子模块查找规则）
        let 普通代码 = 注解导出模块(源文本, &PathBuf::from("src/领域.zh"), &管理器值, true);
        assert!(普通代码.contains("#[path = \"领域/零件.rs\"]"));
    }

    /// 单文件上下文补齐只扫同目录直接子文件：不递归子目录
    #[test]
    fn 同级文件不递归测试() {
        let 目录 = tempfile::tempdir().expect("创建临时目录失败");
        std::fs::write(目录.path().join("a.zh"), "").unwrap();
        std::fs::write(目录.path().join("b.ja"), "").unwrap();
        std::fs::write(目录.path().join("c.rs"), "").unwrap();
        std::fs::create_dir(目录.path().join("sub")).unwrap();
        std::fs::write(目录.path().join("sub/d.zh"), "").unwrap();
        let 扩展名序列 = ["zh".to_string(), "ja".to_string()];
        let 文件们 = 收集同级方言文件(目录.path(), &扩展名序列);
        let 名称们: Vec<String> = 文件们
            .iter()
            .map(|路径值| 路径值.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(名称们, vec!["a.zh".to_string(), "b.ja".to_string()]);
    }

    /// 入口产物路径：入口词干（主函数词或语言包主函数词）写入 包根，
    /// 非入口文件进托管目录（镜像源码布局），不占用入口产物
    ///（构建脚本不得覆盖入口产物）
    ///
    /// 项目根必须是真实落盘目录：合成路径在某些平台上不具
    /// 绝对性，且与项目根连接出的源码目录不在同一比较空间，
    /// 直属源码目录判定会恒为假，使母语词干入口这条断言在任何平台上都测不到真实行为。
    #[test]
    fn 入口产物路径测试() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path().join("proj");
        std::fs::create_dir_all(根.join("src")).unwrap();
        let 管理器值 = 中文管理器();
        // 主文件词干聚合到固定入口
        assert_eq!(
            入口产物路径(&根, &根.join("src/main.zh"), &管理器值),
            根.join("src/main.rs")
        );
        // 母语词干入口（映射主函数，旧项目命名）同样聚合到入口产物
        //（回归：此前仅认字面主函数名，母语主文件产物写自身名致构建工具找不到目标）
        assert_eq!(
            入口产物路径(&根, &根.join("src/主函数.zh"), &管理器值),
            根.join("src/main.rs")
        );
        // 项目根的构建脚本（词干非入口）进托管目录，绝不触碰入口产物
        assert_eq!(
            入口产物路径(&根, &根.join("build.zh"), &管理器值),
            根.join(".rz/gen/build.rs")
        );
        // 源码目录下非入口模块进托管目录镜像
        assert_eq!(
            入口产物路径(&根, &根.join("src/helper.zh"), &管理器值),
            根.join(".rz/gen/src/helper.rs")
        );
    }

    /// 库入口产物路径：库入口落自动发现的库 包根，
    /// 让「造方言库」不再需要手写库路径声明。
    ///
    /// 三条边界都要钉住：只有源码目录直属的库词才是入口（子目录或项目根的库文件
    /// 是普通模块，落点若错到 包根会凭空给项目加一个编译目标）；清单
    /// 显式声明库路径时以声明为准；声明指向项目外时不猜意图。
    #[test]
    fn 库入口产物路径测试() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path().join("proj");
        std::fs::create_dir_all(根.join("src/sub")).unwrap();
        let 管理器值 = 中文管理器();
        // 无清单即无声明即默认 包根
        assert_eq!(
            入口产物路径(&根, &根.join("src/lib.zh"), &管理器值),
            根.join("src/lib.rs")
        );
        assert!(是否入口源(&根.join("src/lib.zh"), &根, &管理器值));
        // 子目录与项目根的库文件不是入口（与主函数同规则，越界写出防护）
        assert_eq!(
            入口产物路径(&根, &根.join("src/sub/lib.zh"), &管理器值),
            根.join(".rz/gen/src/sub/lib.rs")
        );
        assert_eq!(
            入口产物路径(&根, &根.join("lib.zh"), &管理器值),
            根.join(".rz/gen/lib.rs")
        );
        assert!(!是否入口源(&根.join("src/sub/lib.zh"), &根, &管理器值));
        // 库词没有母语别名（它不是关键字，各语言包都不翻译它）：
        // 写成母语库名只是普通模块，不会变成 包根
        assert_eq!(
            入口产物路径(&根, &根.join("src/库.zh"), &管理器值),
            根.join(".rz/gen/src/库.rs")
        );

        // 旧配方：库路径指向托管目录，产物跟着声明走，不再另写默认 包根
        std::fs::write(
            根.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n[lib]\npath = \".rz/gen/src/lib.rs\"\n",
        )
        .unwrap();
        assert_eq!(
            入口产物路径(&根, &根.join("src/lib.zh"), &管理器值),
            根.join(".rz/gen/src/lib.rs")
        );
        // 声明指向项目外时回退默认位置，不往项目外写文件
        std::fs::write(
            根.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n[lib]\npath = \"../outside.rs\"\n",
        )
        .unwrap();
        assert_eq!(
            入口产物路径(&根, &根.join("src/lib.zh"), &管理器值),
            根.join("src/lib.rs")
        );
        // 主入口不受库路径影响（声明的是库，不是可执行程序）
        assert_eq!(
            入口产物路径(&根, &根.join("src/main.zh"), &管理器值),
            根.join("src/main.rs")
        );
    }

    /// 入口产物落 包根后，托管目录里同一源文件的旧产物要清掉
    ///
    /// 只清「本次请求的入口文件」那一份：兄弟模块的托管产物仍在被模块路径注解引用。
    #[test]
    fn 清理入口生成副本测试() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path().join("proj");
        std::fs::create_dir_all(根.join(".rz/gen/src")).unwrap();
        let 生成库 = 根.join(".rz/gen/src/lib.rs");
        let 生成模块 = 根.join(".rz/gen/src/数学.rs");
        std::fs::write(&生成库, "陈旧产物").unwrap();
        std::fs::write(&生成模块, "仍被引用").unwrap();
        清理入口生成副本(&根, &根.join("src/lib.zh"), &根.join("src/lib.rs"));
        assert!(!生成库.exists());
        assert!(生成模块.exists());
        // 落点就是托管路径时（清单声明了库路径）不能删——那是本次产物
        清理入口生成副本(&根, &根.join("src/lib.zh"), &生成库);
        std::fs::write(&生成库, "本次产物").unwrap();
        清理入口生成副本(&根, &根.join("src/lib.zh"), &生成库);
        assert!(生成库.exists());
    }

    /// 越界写出回归：入口词干但不在项目源码目录直属的文件（子目录或示例
    /// 里恰好名为入口）不得聚合覆盖宿主入口产物，产物进托管目录的同名镜像路径。
    /// 对照：同时断言源码目录直属入口确实聚合（证明本测试真的在区分「直属」与
    /// 「子目录」，而非因目录不存在而两边都不聚合、造成假通过）。
    #[test]
    fn 源码外入口不越界测试() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path().join("proj");
        std::fs::create_dir_all(根.join("src/sub")).unwrap();
        std::fs::create_dir_all(根.join("examples")).unwrap();
        let 管理器值 = 中文管理器();
        // 对照：源码目录直属入口确实聚合
        assert_eq!(
            入口产物路径(&根, &根.join("src/main.zh"), &管理器值),
            根.join("src/main.rs")
        );
        // 子目录入口不覆盖宿主入口
        assert_eq!(
            入口产物路径(&根, &根.join("src/sub/main.zh"), &管理器值),
            根.join(".rz/gen/src/sub/main.rs")
        );
        // 示例入口不占用宿主入口
        assert_eq!(
            入口产物路径(&根, &根.join("examples/main.zh"), &管理器值),
            根.join(".rz/gen/examples/main.rs")
        );
        // 母语主函数词但位于子目录同样不越界
        assert_eq!(
            入口产物路径(&根, &根.join("src/sub/主函数.zh"), &管理器值),
            根.join(".rz/gen/src/sub/主函数.rs")
        );
    }

    /// 正向聚合：真实磁盘项目的源码目录直属入口（英文词干与母语词干）都聚合到
    /// 包根；相对词法路径（新建未落盘项目）按词法比较同样判定
    #[test]
    fn 直属入口聚合测试() {
        let 管理器值 = 中文管理器();
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path().join("proj");
        std::fs::create_dir_all(根.join("src/sub")).unwrap();

        // 已落盘：真实目录的规范化比较
        assert_eq!(
            入口产物路径(&根, &根.join("src/main.zh"), &管理器值),
            根.join("src/main.rs")
        );
        assert_eq!(
            入口产物路径(&根, &根.join("src/主函数.zh"), &管理器值),
            根.join("src/main.rs")
        );
        // 同目录的非入口模块不聚合，且产物进托管目录
        assert_eq!(
            入口产物路径(&根, &根.join("src/工具.zh"), &管理器值),
            根.join(".rz/gen/src/工具.rs")
        );

        // 未落盘：源码目录尚不存在，规范化双双失败退回词法路径，
        // 词干为入口时仍应聚合（编辑器里新建项目骨架的场景）
        let 幽灵根 = 临时.path().join("ghost-proj");
        assert_eq!(
            入口产物路径(&幽灵根, &幽灵根.join("src/main.zh"), &管理器值),
            幽灵根.join("src/main.rs")
        );
        assert_eq!(
            入口产物路径(&幽灵根, &幽灵根.join("src/sub/main.zh"), &管理器值),
            幽灵根.join(".rz/gen/src/sub/main.rs")
        );
    }

    /// 工具链通道探测：开发与持续集成环境必有编译器；主次版本号形如版本号，通道词原样
    #[test]
    fn 探测工具链通道测试() {
        let 通道值 = 探测工具链通道().expect("测试环境应有 rustc");
        let 是版本号 = 通道值.split_once('.').is_some_and(|(甲, 乙)| {
            !甲.is_empty()
                && !乙.is_empty()
                && 甲
                    .chars()
                    .chain(乙.chars())
                    .all(|单字| 单字.is_ascii_digit())
        });
        assert!(
            是版本号 || matches!(通道值.as_str(), "nightly" | "beta"),
            "意外的通道格式：{通道值}"
        );
    }

    /// 已内置的语言包扩展名可解析出语言代码
    ///
    /// 持环境变量锁：扩展名映射会扫描全局语言包目录（受语言目录环境变量影响），
    /// 语言管理的环境变量测试并发修改该变量时会污染本测试
    #[test]
    fn 内置扩展名语言代码测试() {
        let _锁 = crate::语言管理::tests::环境锁();
        assert_eq!(扩展名转语言代码("zh").as_deref(), Some("zh"));
        assert_eq!(扩展名转语言代码("de").as_deref(), Some("de"));
        assert_eq!(扩展名转语言代码("ru").as_deref(), Some("ru"));
        assert_eq!(扩展名转语言代码("ja").as_deref(), Some("ja"));
        assert_eq!(扩展名转语言代码("hi").as_deref(), Some("hi"));
    }

    /// 未知扩展名返回 `无`（同样受语言目录环境变量影响，需持锁）
    #[test]
    fn 未知扩展名语言代码测试() {
        let _锁 = crate::语言管理::tests::环境锁();
        assert_eq!(扩展名转语言代码("xyz"), None);
    }

    /// 统一转译管线：中文关键字转为标准 Rust
    #[test]
    fn 中文关键字转译测试() {
        let 管理器值 = 中文管理器();
        let 输出文本 = 转译为英文("公开 函数 help() {}\n", &管理器值);
        assert!(输出文本.contains("pub fn help()"), "实际输出：{输出文本}");
    }

    /// 多文件转译：源码目录下的其他方言文件转译到托管目录，
    /// 入口与手写标准 Rust 不受影响；旧布局遗留在源文件旁的产物被清走；
    /// 跨文件声明豁免：其他文件声明的与映射词同名的成员不被替换
    #[test]
    fn 项目文件转译测试() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        std::fs::create_dir_all(根.join("src")).unwrap();
        let 入口 = 根.join("src/main.zh");
        // 跨文件调用本项目的函数（撞映射词）：
        // 项目上下文让调用位与声明侧一致（不被替换出英文原名）
        std::fs::write(
            &入口,
            "模组 辅助;\n\n函数 main() {\n    让 x = 辅助::新建();\n}\n",
        )
        .unwrap();
        std::fs::write(根.join("src/辅助.zh"), "公开 函数 新建() {}\n").unwrap();
        std::fs::write(根.join("src/manual.rs"), "// 手写文件不覆盖\n").unwrap();
        // 旧布局遗留：本次已迁到托管目录，内容不同的同目录产物得备份后删除
        std::fs::write(根.join("src/辅助.rs"), "// 上一版旧产物\n").unwrap();

        let 管理器值 = 中文管理器();
        let 缓存 = std::sync::Mutex::new(i18n_rust_engine::缓存::转译缓存::新建(8));
        // 项目级声明上下文：模块名（文件词干）与声明名（入口与源码目录全扫）
        let 上下文值 = 收集项目上下文(根, &入口, &管理器值);
        assert!(上下文值.模块集.contains("辅助"), "模块名应含文件名词干");
        assert!(
            上下文值.名字集.contains("新建"),
            "声明名应含其他文件的函数名"
        );
        转译项目文件(根, &入口, &管理器值, &缓存, &上下文值).unwrap();

        // 其他方言文件已转译到托管目录（镜像源码布局）
        let 辅助产物 = std::fs::read_to_string(根.join(".rz/gen/src/辅助.rs")).unwrap();
        assert!(辅助产物.contains("pub fn 新建()"), "实际输出：{辅助产物}");
        // 旧布局遗留产物已从源码目录清走，旧内容备份
        assert!(
            !根.join("src/辅助.rs").exists(),
            "旧布局遗留产物应已从源码目录迁走"
        );
        assert_eq!(
            std::fs::read_to_string(根.join("src/辅助.rs.bak")).unwrap(),
            "// 上一版旧产物\n",
            "内容与本次产物不同时得先备份"
        );
        // 手写标准 Rust 不被触碰
        let 手写产物 = std::fs::read_to_string(根.join("src/manual.rs")).unwrap();
        assert!(手写产物.contains("手写文件不覆盖"));
        // 入口文件未被重复转译（无入口产物产生，由调用方单独写入）
        assert!(!根.join("src/main.rs").exists());

        // 入口转译（带项目上下文）：跨文件调用位不被替换
        let 入口产物文本 = 项目缓存转译(
            &std::fs::read_to_string(&入口).unwrap(),
            &管理器值,
            &mut 缓存.lock().unwrap(),
            Some(&上下文值),
            "main.zh",
        )
        .输出;
        assert!(
            入口产物文本.contains("辅助::新建()"),
            "跨文件调用位应与声明侧一致：{入口产物文本}"
        );
    }

    /// 写入转译产物的备份语义（条件守卫）：目标存在且内容不同才备份为备份文件；
    /// 内容相同（幂等重跑）不产生备份，绝不静默覆盖用户文件
    #[test]
    fn 转译产物备份测试() {
        let 目录 = tempfile::tempdir().unwrap();
        let 路径值 = 目录.path().join("main.rs");
        let 界面表 = crate::界面::界面消息表::按语言建表("zh");

        // 首次写入：文件不存在，直接写盘，无备份
        写入转译产物(&路径值, "v1", &界面表).unwrap();
        assert_eq!(std::fs::read_to_string(&路径值).unwrap(), "v1");
        assert!(!路径值.with_extension("rs.bak").exists());

        // 内容相同：幂等重跑，不产生备份，文件保持原内容
        写入转译产物(&路径值, "v1", &界面表).unwrap();
        assert!(!路径值.with_extension("rs.bak").exists());

        // 内容不同：旧内容备份，文件更新为新内容
        写入转译产物(&路径值, "v2", &界面表).unwrap();
        assert_eq!(std::fs::read_to_string(&路径值).unwrap(), "v2");
        assert_eq!(
            std::fs::read_to_string(路径值.with_extension("rs.bak")).unwrap(),
            "v1"
        );
    }

    /// 托管产物路径含符号链接时拒绝写入：恶意仓库可用 `.rz/gen` 链接
    /// 把转译产物写到项目外（仅 Unix 跑：Windows 建符号链接需特权）
    #[cfg(unix)]
    #[test]
    fn 托管写入拒绝符号链接路径() {
        let 根 = tempfile::tempdir().unwrap();
        let 根路径 = 根.path();
        // 正常的新路径（尚不存在）应放行
        assert!(
            检查写入路径无符号链接(
                &根路径,
                &根路径.join(".rz").join("gen").join("x.rs")
            )
            .is_ok(),
            "普通新路径不应被拒绝"
        );
        let 受害者 = 根路径.join("victim.txt");
        std::fs::write(&受害者, "keep").unwrap();
        // `.rz` 指向临时目录本身：后续每一级路径都落在链接之后
        std::os::unix::fs::symlink(&根路径, &根路径.join(".rz")).unwrap();
        assert!(
            检查写入路径无符号链接(
                &根路径,
                &根路径.join(".rz").join("gen").join("x.rs")
            )
            .is_err(),
            "中间组件是符号链接时必须拒绝"
        );

        // 目标文件自身是符号链接：写入转译产物应拒绝，受害者内容不变
        let 链 = 根路径.join("main.rs");
        std::os::unix::fs::symlink(&受害者, &链).unwrap();
        let 界面表 = crate::界面::界面消息表::按语言建表("zh");
        assert!(
            写入转译产物(&链, "overwrite", &界面表).is_err(),
            "目标是符号链接时必须拒绝"
        );
        assert_eq!(std::fs::read_to_string(&受害者).unwrap(), "keep");
    }

    /// 非 ASCII 文件式模块声明补模块路径注解（绕过编译器标识符限制）
    #[test]
    fn 非母语模块注解测试() {
        let 输出文本 = 注解非母语模块("mod 数学;");
        assert_eq!(输出文本, "#[path = \"数学.rs\"]\nmod 数学;");
    }

    /// 带公开可见性时注解插在可见性关键字之前
    #[test]
    fn 公开模块注解测试() {
        let 输出文本 = 注解非母语模块("pub mod 数学;");
        assert_eq!(输出文本, "#[path = \"数学.rs\"]\npub mod 数学;");
    }

    /// 英文模块名与内联模块块不处理
    #[test]
    fn 跳过英文与内联模块测试() {
        assert_eq!(注解非母语模块("mod math;"), "mod math;");
        assert_eq!(注解非母语模块("mod 数学 { }"), "mod 数学 { }");
    }

    /// 已有模块路径注解时不重复添加；缩进保持
    #[test]
    fn 已有注解与缩进测试() {
        let 源文本 = "#[path = \"数学.rs\"]\nmod 数学;";
        assert_eq!(注解非母语模块(源文本), 源文本);
        let 输出文本 = 注解非母语模块("函数 main() {\n    mod 数学;\n}");
        assert_eq!(
            输出文本,
            "函数 main() {\n    #[path = \"数学.rs\"]\n    mod 数学;\n}"
        );
    }

    /// 注解行映射：磁盘行到引擎直出行（从零起算），注解行归属其模块声明行，
    /// 多处注解时偏移逐处累积（对应多模块入口的诊断行号换算）
    #[test]
    fn 注解行映射测试() {
        // 单处注解：磁盘第 0、1 行均归属引擎第 0 行
        let (输出文本, 行映射) = 注解非母语模块并返回行映射("mod 数学;\nfn main() {}");
        assert_eq!(输出文本, "#[path = \"数学.rs\"]\nmod 数学;\nfn main() {}");
        assert_eq!(行映射, vec![0, 0, 1]);

        // 无注解时映射恒等
        let (_, 行映射二) = 注解非母语模块并返回行映射("fn main() {}\nfn f() {}");
        assert_eq!(行映射二, vec![0, 1]);

        // 两处注解：后续行偏移为 2
        let 两处 = "mod 数学;\nmod 物理;\nfn main() {}";
        let (输出二, 行映射三) = 注解非母语模块并返回行映射(两处);
        assert_eq!(
            输出二,
            "#[path = \"数学.rs\"]\nmod 数学;\n#[path = \"物理.rs\"]\nmod 物理;\nfn main() {}"
        );
        assert_eq!(行映射三, vec![0, 0, 1, 1, 2]);
    }

    /// 映射内容中按库名首段查找母语别名；连字符库名归一匹配
    #[test]
    fn 映射别名查找测试() {
        let 映射文本 =
            "[\"模块路径\"]\n\"HTTP客户端\" = \"reqwest\"\n\"时间\" = \"chrono::prelude\"\n";
        assert_eq!(
            查找映射别名(映射文本, "reqwest").as_deref(),
            Some("HTTP客户端")
        );
        assert_eq!(查找映射别名(映射文本, "chrono").as_deref(), Some("时间"));
        assert_eq!(查找映射别名(映射文本, "tokio"), None);
        assert_eq!(
            查找映射别名(
                "[\"模块路径\"]\n\"序列化\" = \"serde-json\"\n",
                "serde_json"
            )
            .as_deref(),
            Some("序列化")
        );
    }
}
