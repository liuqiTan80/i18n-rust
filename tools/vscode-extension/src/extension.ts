/**
 * i18n-rust VS Code 扩展
 *
 * 功能：
 * - 11 种方言（zh/en/ja/de/es/fr/pt/ru/ko/hi/ar）语法高亮与语言注册
 * - LSP 客户端连接 i18n-rust-lsp
 * - 提供 Run/Check/Eject 命令（终端复用、参数安全引用）
 * - 状态栏显示当前语言包
 * - 所有权错误可视化（变量移动/借用/再次使用位置颜色高亮）
 * - 全角符号自动转换（换行感知）
 * - AI 教学助手（SecretStorage 密钥、可取消流式对话）
 */

import * as vscode from 'vscode';
import * as path from 'path';
import * as cp from 'child_process';
import * as fs from 'fs';
import * as os from 'os';
import { promisify } from 'util';
import {
    CloseAction,
    ErrorAction,
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind
} from 'vscode-languageclient/node';
import { createProvider, listProviders } from './ai/provider-factory';
import {
    loadAIConfig,
    getSystemPrompt,
    currentLanguageName,
    initAISecrets
} from './ai/config-manager';
import { AIError } from './ai/types';
import { ProviderInterface } from './ai/provider-interface';
import { 全角符号映射, 扫描词法状态, 词法状态, 计算插入字符位置们 } from './fullwidth-convert';
import { 方言语言Id, 方言语言表, 语言代码 } from './languages';
import { quoteCommandArg, quoteShellArg } from './shell';
import { findInPath, 解析可执行文件 } from './executable';

const execFileAsync = promisify(cp.execFile);

/** 全部方言源码扩展名（用于 eject 输出路径推导） */
const 方言扩展名正则 = /\.(zh|en|ja|de|es|fr|pt|ru|ko|hi|ar)$/;

let client: LanguageClient | undefined;
let statusBarItem: vscode.StatusBarItem;
// LSP 自动重启计数（防止崩溃循环导致无限重启）
let 自动重启次数 = 0;
// 日志输出通道（替代 console.log，便于用户排查问题）
let 日志通道: vscode.OutputChannel;
// Run/Check 共用终端（避免每次命令新建终端刷屏）
let 命令终端: vscode.Terminal | undefined;
let 命令终端工作目录: string | undefined;
// 当前 AI 会话的中止器（新会话自动中止上一个，避免输出交错）
let 当前AI中止器: AbortController | undefined;

/**
 * 写入扩展日志（带时间戳）
 */
function 日志(消息: string): void {
    日志通道?.appendLine(`[${new Date().toISOString()}] ${消息}`);
}

// ============================================================
// 所有权错误可视化装饰器（诊断 data 中的 所有权详情 → 颜色高亮）
// ============================================================

// 移动/借用发生位置：黄色背景 + 黄色边框
let 移动位置装饰器: vscode.TextEditorDecorationType;
// 再次使用位置：红色背景 + 红色边框
let 再次使用装饰器: vscode.TextEditorDecorationType;
// 生命周期区间（冲突发生行到再次使用行）：浅绿色背景
let 生命周期装饰器: vscode.TextEditorDecorationType;

/**
 * 注册所有权错误可视化：
 * 监听诊断变化、编辑器切换与文档关闭，将诊断 data 中的
 * 所有权详情（变量名、移动/借用发生、再次使用位置）转换为
 * 不同颜色的装饰器，帮助学习者直观理解所有权规则。
 */
function 注册所有权可视化(context: vscode.ExtensionContext): void {
    // 创建装饰器类型
    移动位置装饰器 = vscode.window.createTextEditorDecorationType({
        backgroundColor: 'rgba(255, 255, 0, 0.3)',
        border: '1px solid yellow',
        borderRadius: '3px'
    });
    再次使用装饰器 = vscode.window.createTextEditorDecorationType({
        backgroundColor: 'rgba(255, 0, 0, 0.3)',
        border: '1px solid red',
        borderRadius: '3px'
    });
    生命周期装饰器 = vscode.window.createTextEditorDecorationType({
        backgroundColor: 'rgba(0, 255, 0, 0.1)'
    });
    context.subscriptions.push(移动位置装饰器, 再次使用装饰器, 生命周期装饰器);

    // 诊断更新时刷新（错误修复后诊断消失，装饰器自动清除）
    context.subscriptions.push(
        vscode.languages.onDidChangeDiagnostics(() => {
            刷新所有权装饰器();
        })
    );

    // 编辑器切换/可见编辑器变化时刷新（装饰器与编辑器绑定）
    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor(() => {
            刷新所有权装饰器();
        })
    );
    context.subscriptions.push(
        vscode.window.onDidChangeVisibleTextEditors(() => {
            刷新所有权装饰器();
        })
    );

    // 文档关闭时清除其装饰器（关闭后该编辑器已不在可见列表中，刷新即清理）
    context.subscriptions.push(
        vscode.workspace.onDidCloseTextDocument(() => {
            刷新所有权装饰器();
        })
    );

    // 初始应用（扩展激活时已有打开的文档）
    应用所有权装饰器();
}

/**
 * 清除所有可见编辑器上的所有权装饰器
 */
function 清除所有权装饰器(): void {
    for (const 编辑器 of vscode.window.visibleTextEditors) {
        编辑器.setDecorations(移动位置装饰器, []);
        编辑器.setDecorations(再次使用装饰器, []);
        编辑器.setDecorations(生命周期装饰器, []);
    }
}

/**
 * 为所有可见的方言编辑器应用所有权装饰器
 */
function 应用所有权装饰器(): void {
    for (const 编辑器 of vscode.window.visibleTextEditors) {
        if (!方言语言Id.includes(编辑器.document.languageId)) {
            continue;
        }
        const 诊断列表 = vscode.languages.getDiagnostics(编辑器.document.uri);
        const 范围 = 提取所有权范围(诊断列表, 编辑器.document);
        if (!范围) {
            continue;
        }
        编辑器.setDecorations(移动位置装饰器, 范围.移动);
        编辑器.setDecorations(再次使用装饰器, 范围.再次使用);
        编辑器.setDecorations(生命周期装饰器, 范围.生命周期);
    }
}

/**
 * 刷新所有权装饰器：先清除全部，再重新应用
 */
function 刷新所有权装饰器(): void {
    清除所有权装饰器();
    应用所有权装饰器();
}

/**
 * 从诊断列表中提取所有权错误的高亮范围
 *
 * 诊断 data 由 LSP 代理写入（JSON）：
 * { "变量名": "数据", "移动发生": {…}, "借用发生": {…}, "再次使用": {…} }
 * 位置对象含 起始行/起始列/结束行/结束列（1-based）。
 * 返回 null 表示没有所有权详情。
 */
function 提取所有权范围(
    诊断列表: readonly vscode.Diagnostic[],
    文档: vscode.TextDocument
): { 移动: vscode.Range[]; 再次使用: vscode.Range[]; 生命周期: vscode.Range[] } | null {
    const 移动: vscode.Range[] = [];
    const 再次使用: vscode.Range[] = [];
    const 生命周期: vscode.Range[] = [];

    for (const 诊断 of 诊断列表) {
        // vscode.Diagnostic 未暴露 data 字段（LSP 扩展字段），通过类型断言访问
        const 详情 = 获取所有权详情((诊断 as any).data);
        if (!详情 || !详情['变量名']) {
            continue;
        }
        const 移动位置 = 详情['移动发生'];
        const 借用位置 = 详情['借用发生'];
        const 再次位置 = 详情['再次使用'];

        // 冲突发生点（移动或借用）用黄色高亮
        if (移动位置) {
            移动.push(位置转范围(移动位置, 文档));
        }
        if (借用位置) {
            移动.push(位置转范围(借用位置, 文档));
        }
        // 再次使用点用红色高亮
        if (再次位置) {
            再次使用.push(位置转范围(再次位置, 文档));
        }
        // 生命周期：冲突发生行到再次使用行之间的整行浅色背景
        const 发生位置 = 移动位置 ?? 借用位置;
        if (发生位置 && 再次位置) {
            const 发生行 = 转行号(发生位置['起始行'], 文档);
            const 再次行 = 转行号(再次位置['起始行'], 文档);
            const 开始行 = Math.min(发生行, 再次行);
            const 结束行 = Math.max(发生行, 再次行);
            生命周期.push(new vscode.Range(开始行, 0, 结束行, 文档.lineAt(结束行).text.length));
        }
    }

    if (移动.length === 0 && 再次使用.length === 0 && 生命周期.length === 0) {
        return null;
    }
    return { 移动, 再次使用, 生命周期 };
}

/**
 * 从诊断 data 中获取所有权详情对象
 *
 * 兼容两种存储方式：data 直接是详情对象，或嵌套在 所有权详情 键下
 * （LSP 代理在已有 data（如代码操作数据）时采用嵌套方式）。
 */
function 获取所有权详情(data: unknown): any {
    if (!data || typeof data !== 'object' || Array.isArray(data)) {
        return null;
    }
    const 值 = data as any;
    if (值['所有权详情']) {
        return 值['所有权详情'];
    }
    return 值['变量名'] ? 值 : null;
}

/**
 * 将诊断位置（1-based 行列）转换为 vscode.Range（0-based）
 * 并裁剪到文档范围，防止越界。
 */
function 位置转范围(位置: any, 文档: vscode.TextDocument): vscode.Range {
    const 起始行 = 转行号(位置['起始行'], 文档);
    const 结束行 = 转行号(位置['结束行'], 文档);
    const 起始列 = Math.max((位置['起始列'] ?? 1) - 1, 0);
    const 结束列 = Math.max((位置['结束列'] ?? 起始列 + 1) - 1, 起始列);
    return new vscode.Range(起始行, 起始列, 结束行, 结束列);
}

/**
 * 将 1-based 行号转换为 0-based 并裁剪到文档范围内
 */
function 转行号(行号: any, 文档: vscode.TextDocument): number {
    const 行数 = 文档.lineCount;
    const 行 = ((行号 ?? 1) as number) - 1;
    return Math.min(Math.max(行, 0), Math.max(行数 - 1, 0));
}

/**
 * 判断文档中指定位置是否处于字符串或注释内
 * （从文档开头扫描到该位置，维护 Rust 词法状态）
 */
function 是否在字符串或注释(文档: vscode.TextDocument, 位置: vscode.Position): boolean {
    const 前缀 = 文档.getText(new vscode.Range(文档.positionAt(0), 位置));
    return 扫描词法状态(前缀) !== 词法状态.代码;
}

/**
 * 注册全角符号自动转换：
 * 在代码区（非字符串、非注释）输入全角符号时自动替换为半角符号；
 * 字符串与注释内的全角符号保持原样（内容可能是有意义的母语标点）。
 * 受配置 i18n-rust.autoConvertFullWidthSymbols 控制（默认开启）。
 */
function 注册全角符号转换(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.workspace.onDidChangeTextDocument(事件 => {
            // 仅处理单变更事件：多变更时各 range 处于不同文档快照坐标系，
            // 统一换算易出错；批量粘贴全角内容属低频场景，交由手动处理
            if (事件.contentChanges.length !== 1) {
                return;
            }
            const 文档 = 事件.document;
            if (!方言语言Id.includes(文档.languageId)) {
                return;
            }
            const 配置 = vscode.workspace.getConfiguration('i18n-rust');
            if (!配置.get<boolean>('autoConvertFullWidthSymbols', true)) {
                return;
            }
            // 只处理用户正在编辑的活动编辑器（避免批量工具写入时干扰）
            const 编辑器 = vscode.window.activeTextEditor;
            if (!编辑器 || 编辑器.document !== 文档) {
                return;
            }

            const 变更 = 事件.contentChanges[0];
            if (!变更.text) {
                return;
            }
            // 逐字符推进计算位置（换行感知），避免 translate(0, i) 跨行错位
            const 替换们: { 范围: vscode.Range; 文本: string }[] = [];
            const 位置们 = 计算插入字符位置们(变更.range.start.line, 变更.range.start.character, 变更.text);
            for (const { 索引, 行, 列 } of 位置们) {
                const 半角 = 全角符号映射[变更.text[索引]];
                if (!半角) {
                    continue;
                }
                const 插入位置 = new vscode.Position(行, 列);
                if (是否在字符串或注释(文档, 插入位置)) {
                    continue;
                }
                替换们.push({
                    范围: new vscode.Range(插入位置, 插入位置.translate(0, 1)),
                    文本: 半角
                });
            }
            if (替换们.length === 0) {
                return;
            }

            // 一次应用全部替换（全角/半角均为 1 个 UTF-16 单元，光标自动保持在原位）
            void 编辑器.edit(构建编辑 => {
                for (const 替换 of 替换们) {
                    构建编辑.replace(替换.范围, 替换.文本);
                }
            });
        })
    );
}

/**
 * 当前所有工作区根目录
 */
function 工作区根们(): string[] {
    return (vscode.workspace.workspaceFolders ?? []).map(文件夹 => 文件夹.uri.fsPath);
}

/**
 * 查找语言包目录路径
 *
 * 优先级：
 * 1. 配置 i18n-rust.languagePackPath（显式指定）
 * 2. 工作区中的语言包目录（lang-packs/<代码> 或主仓库单副本结构 crates/engine/lang-packs/<代码>）
 * 3. 全局安装目录 ~/.rz/lang-packs/<代码>（rzc 全局安装位置）
 * 4. LSP 二进制所在项目的语言包目录（沿 PATH 查找）
 * 5. 常见项目目录（~/code/zrRust 等）
 * 6. 找不到返回 undefined（LSP 使用默认内置映射）
 */
function 查找语言包路径(config: vscode.WorkspaceConfiguration): string | undefined {
    // 1. 显式配置
    const 显式路径 = config.get<string>('languagePackPath', '');
    if (显式路径) {
        return 显式路径;
    }
    const 语言 = 语言代码(config.get<string>('languagePack', '中文'));
    // 2. 工作区语言包（lang-packs/ 用户项目约定 + crates/engine/lang-packs/ 主仓库单副本）
    for (const 根 of 工作区根们()) {
        const 候选 = 语言包候选们(根, 语言);
        if (候选) {
            return 候选;
        }
    }
    // 3. 全局安装目录
    const 全局候选 = path.join(os.homedir(), '.rz', 'lang-packs', 语言);
    if (fs.existsSync(全局候选)) {
        return 全局候选;
    }
    // 4. LSP 二进制所在项目（沿 PATH 查找 i18n-rust-lsp，向上搜索语言包）
    const serverPath = config.get<string>('serverPath', 'i18n-rust-lsp');
    if (!path.isAbsolute(serverPath)) {
        const lspRealPath = findInPath(serverPath);
        if (lspRealPath) {
            const 候选 = 向上查找语言包(path.dirname(lspRealPath), 语言);
            if (候选) { return 候选; }
        }
    }
    // 5. 常见项目目录
    const home = os.homedir();
    for (const 项目名 of ['code/zrRust', 'zrRust']) {
        const 候选 = 语言包候选们(path.join(home, 项目名), 语言);
        if (候选) {
            return 候选;
        }
    }
    return undefined;
}

/**
 * 在指定基础目录下探测两种语言包布局：
 * lang-packs/<代码>（用户项目约定）与 crates/engine/lang-packs/<代码>（主仓库单一数据源）
 */
function 语言包候选们(基础目录: string, 语言: string): string | undefined {
    for (const 相对路径 of [
        path.join('lang-packs', 语言),
        path.join('crates', 'engine', 'lang-packs', 语言),
    ]) {
        const 候选 = path.join(基础目录, 相对路径);
        if (fs.existsSync(候选)) {
            return 候选;
        }
    }
    return undefined;
}

/**
 * 从指定目录向上搜索语言包目录（最多向上 5 级，两种布局均探测）
 */
function 向上查找语言包(startDir: string, 语言: string): string | undefined {
    let dir = startDir;
    for (let i = 0; i < 5; i++) {
        const 候选 = 语言包候选们(dir, 语言);
        if (候选) {
            return 候选;
        }
        const parent = path.dirname(dir);
        if (parent === dir) { break; }
        dir = parent;
    }
    return undefined;
}

/**
 * 扩展激活入口
 */
export function activate(context: vscode.ExtensionContext): void {
    // 日志通道（替代 console.log）
    日志通道 = vscode.window.createOutputChannel('i18n-rust 日志');
    context.subscriptions.push(日志通道);
    日志('i18n-rust 扩展已激活');

    // AI 密钥迁移到 SecretStorage（一次性）
    void initAISecrets(context).catch(错误 => 日志(`AI 密钥迁移失败: ${(错误 as Error).message}`));

    // 创建状态栏项
    statusBarItem = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Right,
        100
    );
    statusBarItem.command = 'i18n-rust.selectLanguagePack';
    context.subscriptions.push(statusBarItem);
    更新状态栏();

    // 终端关闭时重置复用引用
    context.subscriptions.push(
        vscode.window.onDidCloseTerminal(终端 => {
            if (终端 === 命令终端) {
                命令终端 = undefined;
                命令终端工作目录 = undefined;
            }
        })
    );

    // 注册命令
    注册命令(context);

    // 启动 LSP 客户端
    启动语言服务器(context);

    // 监听配置变化
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(e => {
            if (e.affectsConfiguration('i18n-rust.languagePack')) {
                更新状态栏();
            }
        })
    );

    // 注册所有权错误可视化（诊断 data 中的 所有权详情 → 颜色装饰器）
    注册所有权可视化(context);

    // 注册全角符号自动转换（代码区全角 → 半角，字符串/注释内不转换）
    注册全角符号转换(context);

    // 显示欢迎信息（首次激活）
    const 已显示 = context.globalState.get<boolean>('welcomeShown');
    if (!已显示) {
        vscode.window.showInformationMessage(
            '欢迎使用 i18n-rust！按 Ctrl+Shift+R 运行母语 Rust 代码。',
            '了解更多'
        ).then(selection => {
            if (selection === '了解更多') {
                vscode.env.openExternal(
                    vscode.Uri.parse('https://gitcode.com/tan80/zrRust')
                );
            }
        });
        void context.globalState.update('welcomeShown', true);
    }
}

/**
 * 扩展停用
 */
export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}

/**
 * 注册所有命令
 */
function 注册命令(context: vscode.ExtensionContext): void {
    // i18n Run 命令
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.run', async () => {
            const 编辑器 = vscode.window.activeTextEditor;
            if (!编辑器 || !方言语言Id.includes(编辑器.document.languageId)) {
                vscode.window.showWarningMessage('请打开一个方言源码文件（.zh/.en/.ja 等）');
                return;
            }

            const 文件路径 = 编辑器.document.uri.fsPath;
            await 运行文件(文件路径);
        })
    );

    // i18n Check 命令
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.check', async () => {
            const 编辑器 = vscode.window.activeTextEditor;
            if (!编辑器 || !方言语言Id.includes(编辑器.document.languageId)) {
                vscode.window.showWarningMessage('请打开一个方言源码文件（.zh/.en/.ja 等）');
                return;
            }

            const 文件路径 = 编辑器.document.uri.fsPath;
            await 检查文件(文件路径);
        })
    );

    // i18n Eject 命令
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.eject', async () => {
            const 编辑器 = vscode.window.activeTextEditor;
            if (!编辑器 || !方言语言Id.includes(编辑器.document.languageId)) {
                vscode.window.showWarningMessage('请打开一个方言源码文件（.zh/.en/.ja 等）');
                return;
            }

            const 文件路径 = 编辑器.document.uri.fsPath;
            await 导出文件(文件路径);
        })
    );

    // 选择语言包命令
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.selectLanguagePack', async () => {
            const 选项们 = 方言语言表.map(语言 => ({
                label: 语言.displayName,
                description: `.${语言.extension}`
            }));
            const 选择 = await vscode.window.showQuickPick(选项们, {
                placeHolder: '选择语言包'
            });
            if (选择) {
                const config = vscode.workspace.getConfiguration('i18n-rust');
                await config.update('languagePack', 选择.label, vscode.ConfigurationTarget.Global);
                更新状态栏();
                // 语言包变化后重启 LSP，使新语言包立即生效
                await 重启服务器(context);
                vscode.window.showInformationMessage(`语言包已切换为: ${选择.label}，语言服务器已重启`);
            }
        })
    );

    // 重启服务器命令
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.restartServer', async () => {
            await 重启服务器(context);
            vscode.window.showInformationMessage('语言服务器已重启');
        })
    );

    // 映射工具命令（校验 / 翻译脚手架 / 安装语言包，依赖 rzc ≥ 0.3.3）
    注册映射工具命令(context);

    // AI 相关命令（对话 / 选择提供商 / 模型列表）
    注册AI命令(context);
}

/**
 * 注册 AI 相关命令：
 * - i18n-rust.aiChat：AI 对话（携带当前方言文件内容作为上下文，流式输出，可取消）
 * - i18n-rust.aiSelectProvider：选择 AI 提供商
 * - i18n-rust.aiListModels：获取当前提供商可用模型列表
 */
function 注册AI命令(context: vscode.ExtensionContext): void {
    // 输出面板：所有 AI 对话/模型结果统一输出到这里
    const AI输出 = vscode.window.createOutputChannel('i18n-rust AI');
    context.subscriptions.push(AI输出);

    // AI 对话命令
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.aiChat', async () => {
            const 配置 = await loadAIConfig();
            // 云端服务需要密钥；Ollama / 自定义地址可无密钥
            if (!配置.apiKey && 配置.provider !== 'ollama' && 配置.provider !== 'custom') {
                const 操作 = await vscode.window.showWarningMessage(
                    `尚未配置 API 密钥（提供商：${配置.provider}）。请在设置中填写 i18n-rust.ai.apiKey（将安全存入 SecretStorage）。`,
                    '打开设置'
                );
                if (操作 === '打开设置') {
                    vscode.commands.executeCommand('workbench.action.openSettings', 'i18n-rust.ai');
                }
                return;
            }

            const 问题 = await vscode.window.showInputBox({
                prompt: '向 AI 提问（关于当前方言文件或 Rust 教学）',
                placeHolder: '例如：解释这段代码的所有权移动过程'
            });
            if (!问题) {
                return;
            }

            // 携带当前打开的方言文件内容作为上下文
            const 编辑器 = vscode.window.activeTextEditor;
            const 上下文 = 编辑器 && 方言语言Id.includes(编辑器.document.languageId)
                ? `当前文件 ${编辑器.document.fileName}：\n\`\`\`rust\n${编辑器.document.getText()}\n\`\`\`\n\n`
                : '';

            let 提供商: ProviderInterface;
            try {
                提供商 = createProvider(配置);
            } catch (错误) {
                vscode.window.showErrorMessage((错误 as Error).message);
                return;
            }

            // 新会话中止上一个会话，避免输出交错
            当前AI中止器?.abort();
            const 中止器 = new AbortController();
            当前AI中止器 = 中止器;

            AI输出.show(true);
            AI输出.appendLine(`── AI 对话（${配置.provider} / ${配置.model || '默认模型'}，语言包：${currentLanguageName()}）──`);
            AI输出.appendLine(`问：${问题}\n`);

            await vscode.window.withProgress(
                {
                    location: vscode.ProgressLocation.Notification,
                    title: `i18n-rust AI 对话中（${配置.provider}）`,
                    cancellable: true
                },
                async (_进度, 取消令牌) => {
                    取消令牌.onCancellationRequested(() => 中止器.abort());
                    try {
                        await 提供商.streamChat(
                            [
                                { role: 'system', content: getSystemPrompt() },
                                { role: 'user', content: 上下文 + 问题 }
                            ],
                            chunk => AI输出.append(chunk),
                            中止器.signal
                        );
                        AI输出.appendLine('\n── 对话结束 ──');
                    } catch (错误) {
                        if (错误 instanceof AIError) {
                            if (错误.category === '已取消') {
                                AI输出.appendLine('\n── 已取消 ──');
                            } else {
                                AI输出.appendLine(`\n[${错误.category}] ${错误.message}`);
                                vscode.window.showErrorMessage(`${错误.category}：${错误.message}`);
                            }
                        } else {
                            AI输出.appendLine(`\n[未知错误] ${(错误 as Error).message}`);
                        }
                    } finally {
                        if (当前AI中止器 === 中止器) {
                            当前AI中止器 = undefined;
                        }
                    }
                }
            );
        })
    );

    // 选择 AI 提供商命令
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.aiSelectProvider', async () => {
            const 选项们 = listProviders().map(预设 => ({
                label: 预设.displayName,
                description: 预设.defaultBaseUrl || '需自定义地址',
                detail: 预设.requiresApiKey ? '需要 API 密钥' : '无需密钥',
                预设
            }));
            const 选择 = await vscode.window.showQuickPick(选项们, {
                placeHolder: '选择 AI 提供商'
            });
            if (!选择) {
                return;
            }
            const 配置 = vscode.workspace.getConfiguration('i18n-rust.ai');
            await 配置.update('provider', 选择.预设.id, vscode.ConfigurationTarget.Global);
            const 提示 = `AI 提供商已切换为「${选择.预设.displayName}」（默认地址：${选择.预设.defaultBaseUrl || '需手动填写 baseUrl'}）`;
            vscode.window.showInformationMessage(提示, '打开设置').then(操作 => {
                if (操作 === '打开设置') {
                    vscode.commands.executeCommand('workbench.action.openSettings', 'i18n-rust.ai');
                }
            });
        })
    );

    // 获取模型列表命令
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.aiListModels', async () => {
            const 配置 = await loadAIConfig();
            let 提供商: ProviderInterface;
            try {
                提供商 = createProvider(配置);
            } catch (错误) {
                vscode.window.showErrorMessage((错误 as Error).message);
                return;
            }
            AI输出.show(true);
            AI输出.appendLine(`── 模型列表（${配置.provider} / ${配置.baseUrl || '默认地址'}）──`);
            try {
                const 模型们 = await 提供商.listModels();
                AI输出.appendLine(模型们.join('\n'));
                AI输出.appendLine(`── 共 ${模型们.length} 个模型 ──`);
            } catch (错误) {
                if (错误 instanceof AIError) {
                    AI输出.appendLine(`[${错误.category}] ${错误.message}`);
                }
            }
        })
    );
}

/**
 * 停止并重新启动 LSP 语言服务器
 */
async function 重启服务器(context: vscode.ExtensionContext): Promise<void> {
    if (client) {
        await client.stop();
        client = undefined;
    }
    自动重启次数 = 0;
    启动语言服务器(context);
}

/**
 * 启动 LSP 语言服务器
 */
function 启动语言服务器(context: vscode.ExtensionContext): void {
    const config = vscode.workspace.getConfiguration('i18n-rust');

    // 跨平台解析 LSP 二进制（绝对路径 / 工作区构建目录 / PATH）
    const 服务器路径 = 解析可执行文件(
        config.get<string>('serverPath', 'i18n-rust-lsp'),
        工作区根们()
    );
    if (!服务器路径) {
        const 消息 = '未找到 i18n-rust-lsp 可执行文件。请运行 `rzc install lsp`（或 `cargo install i18n-rust-lsp`）安装语言服务器，或在设置 i18n-rust.serverPath 中指定二进制路径（高亮/补全/诊断功能暂不可用）。';
        日志(消息);
        vscode.window.showErrorMessage(消息, '打开设置').then(操作 => {
            if (操作 === '打开设置') {
                vscode.commands.executeCommand('workbench.action.openSettings', 'i18n-rust.serverPath');
            }
        });
        return;
    }
    日志(`LSP 二进制: ${服务器路径}`);

    // 服务器选项：显式传递语言包路径（LSP 默认相对路径依赖启动目录，通常找不到）
    const args: string[] = [];
    const 语言包路径 = 查找语言包路径(config);
    if (语言包路径) {
        args.push('--language-pack', 语言包路径);
        日志(`语言包目录: ${语言包路径}`);
    }
    const serverOptions: ServerOptions = {
        run: {
            command: 服务器路径,
            args,
            transport: TransportKind.stdio
        },
        debug: {
            command: 服务器路径,
            args,
            transport: TransportKind.stdio
        }
    };

    // 客户端选项
    const trace通道 = vscode.window.createOutputChannel('i18n-rust LSP Trace');
    context.subscriptions.push(trace通道);
    const clientOptions: LanguageClientOptions = {
        documentSelector: 方言语言Id.map(语言 => ({ scheme: 'file', language: 语言 })),
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.{zh,en,ja,de,es,fr,pt,ru,ko,hi,ar}')
        },
        diagnosticCollectionName: 'i18n-rust',
        outputChannelName: 'i18n-rust LSP',
        traceOutputChannel: trace通道,
        initializationOptions: {
            languagePack: config.get<string>('languagePack', '中文')
        },
        // LSP 进程异常退出时自动重启（最多 5 次，防止崩溃循环无限重启）
        errorHandler: {
            error: (_error, _message, count) => ({
                action: (count ?? 0) >= 3 ? ErrorAction.Shutdown : ErrorAction.Continue
            }),
            closed: () => {
                if (自动重启次数 < 5) {
                    自动重启次数++;
                    vscode.window.showWarningMessage(
                        `i18n-rust 语言服务器异常退出，正在自动重启（第 ${自动重启次数} 次）...`
                    );
                    return { action: CloseAction.Restart };
                }
                vscode.window.showErrorMessage(
                    'i18n-rust 语言服务器多次异常退出，已停止自动重启。请执行「i18n: 重启语言服务器」命令或查看输出面板「i18n-rust LSP」日志。'
                );
                return { action: CloseAction.DoNotRestart };
            }
        }
    };

    // 创建客户端
    client = new LanguageClient(
        'i18n-rust',
        'i18n-rust 语言服务器',
        serverOptions,
        clientOptions
    );

    // 启动客户端
    client.start().catch(err => {
        日志(`LSP 启动失败: ${err.message}`);
        vscode.window.showErrorMessage(
            `i18n-rust LSP 启动失败: ${err.message}。请确保 i18n-rust-lsp 已安装并可用。`
        );
    });

    context.subscriptions.push(client);
}

/**
 * 注册映射工具命令：
 * - mappingCheck：第三方库映射质量校验（全部内置语言或工作区指定语言包目录）
 * - mappingScaffold：新语言翻译脚手架（rule 骨架 / deepseek AI 翻译）
 * - langInstall：安装语言包（远程语言代码或本地目录）
 */
function 注册映射工具命令(context: vscode.ExtensionContext): void {
    // 映射校验：工作区存在 lang-packs/ 时可选具体语言包目录，否则校验全部内置语言
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.mappingCheck', async () => {
            const rzc路径 = await 解析rzc();
            if (!rzc路径) {
                return;
            }
            const 工作区根 = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
            let 目标参数 = '';
            if (工作区根) {
                // 两种布局均探测：lang-packs/（用户项目约定）与
                // crates/engine/lang-packs/（主仓库单一数据源）
                const langPacks目录 = [path.join(工作区根, 'lang-packs'),
                    path.join(工作区根, 'crates', 'engine', 'lang-packs')]
                    .find(目录 => fs.existsSync(目录));
                if (langPacks目录) {
                    const 子目录们 = fs.readdirSync(langPacks目录).filter(名 =>
                        fs.existsSync(path.join(langPacks目录, 名, 'keywords.toml'))
                    );
                    const 选项们: { label: string; description: string }[] = [
                        { label: '全部内置语言', description: 'rzc mapping check（含跨语言一致性对比）' }
                    ];
                    for (const 名 of 子目录们) {
                        选项们.push({ label: 名, description: path.relative(工作区根, path.join(langPacks目录, 名)) });
                    }
                    const 选择 = await vscode.window.showQuickPick(选项们, {
                        placeHolder: '选择校验目标'
                    });
                    if (!选择) {
                        return;
                    }
                    if (选择.label !== '全部内置语言') {
                        目标参数 = ` ${quoteShellArg(path.join(langPacks目录, 选择.label))}`;
                    }
                }
            }
            const 终端 = 获取命令终端(工作区根 ?? '.');
            终端.show();
            终端.sendText(`${quoteCommandArg(rzc路径)} mapping check${目标参数}`);
        })
    );

    // 翻译脚手架：选源语言 → 输入目标代码 → 选翻译方式 → 终端执行
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.mappingScaffold', async () => {
            const rzc路径 = await 解析rzc();
            if (!rzc路径) {
                return;
            }
            // 源语言必须是内置语言；en 无 crates 映射不可作源
            const 源选项们 = 方言语言表
                .filter(语言 => 语言.code !== 'en')
                .map(语言 => ({ label: 语言.code, description: 语言.displayName }));
            const 源选择 = await vscode.window.showQuickPick(源选项们, {
                placeHolder: '选择源语言（从其 crates 映射生成骨架）'
            });
            if (!源选择) {
                return;
            }
            const 目标 = await vscode.window.showInputBox({
                prompt: '目标语言代码（新语言包目录名）',
                placeHolder: '如 vi、th、tr',
                validateInput: 值 =>
                    /^[a-z]{2,3}(-[A-Za-z]{2,4})?$/.test(值.trim())
                        ? undefined
                        : '语言代码格式无效（如 vi、pt-BR）'
            });
            if (!目标) {
                return;
            }
            const 方式选择 = await vscode.window.showQuickPick(
                [
                    {
                        label: 'rule',
                        description: '生成 TODO 骨架，键由人工翻译（默认）'
                    },
                    {
                        label: 'deepseek',
                        description: 'AI 自动翻译键名（需环境变量 DEEPSEEK_API_KEY）'
                    }
                ],
                { placeHolder: '选择翻译方式' }
            );
            if (!方式选择) {
                return;
            }
            const 工作区根 = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
            const 终端 = 获取命令终端(工作区根 ?? '.');
            终端.show();
            终端.sendText(
                `${quoteCommandArg(rzc路径)} mapping scaffold ${quoteShellArg(源选择.label)} `
                + `${quoteShellArg(目标.trim())} --provider ${方式选择.label}`
            );
        })
    );

    // 安装语言包：远程语言代码或本地目录路径
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.langInstall', async () => {
            const rzc路径 = await 解析rzc();
            if (!rzc路径) {
                return;
            }
            const 来源 = await vscode.window.showInputBox({
                prompt: '安装语言包：输入远程语言代码或本地语言包目录路径',
                placeHolder: '如 vi（远程，可用 RZ_LANG_REPO 指定仓库）或 /path/to/lang-pack',
                validateInput: 值 => (值.trim() ? undefined : '不能为空')
            });
            if (!来源) {
                return;
            }
            const 工作区根 = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
            const 终端 = 获取命令终端(工作区根 ?? '.');
            终端.show();
            终端.sendText(`${quoteCommandArg(rzc路径)} lang install ${quoteShellArg(来源.trim())}`);
        })
    );

    // 添加依赖：LSP 快捷修复（unresolved import）注入的代码动作入口，
    // 参数为 crate 名（可带 @version）；无参时弹输入框手动指定。
    // 优先 rzc add（附带母语映射提示），找不到 rzc 退化 cargo add
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.cargoAdd', async (crate名?: string) => {
            if (!crate名) {
                crate名 = await vscode.window.showInputBox({
                    prompt: '添加依赖：输入 crate 名（可带 @版本）',
                    placeHolder: '如 serde_json 或 tokio@1',
                    validateInput: 值 => (值.trim() ? undefined : '不能为空')
                });
            }
            if (!crate名) {
                return;
            }
            const 工作区根 = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
            if (!工作区根) {
                vscode.window.showWarningMessage('未打开工作区，无法添加依赖');
                return;
            }
            const 终端 = 获取命令终端(工作区根);
            终端.show();
            const config = vscode.workspace.getConfiguration('i18n-rust');
            const rzc路径 = 解析可执行文件(config.get<string>('rzcPath', 'rzc'), 工作区根们());
            if (rzc路径) {
                终端.sendText(`${quoteCommandArg(rzc路径)} add ${quoteShellArg(crate名.trim())}`);
            } else {
                终端.sendText(`cargo add ${quoteShellArg(crate名.trim())}`);
            }
        })
    );
}

/**
 * 解析 rzc 可执行文件；找不到时提示用户并返回 undefined
 */
async function 解析rzc(): Promise<string | undefined> {
    const config = vscode.workspace.getConfiguration('i18n-rust');
    const rzc路径 = 解析可执行文件(config.get<string>('rzcPath', 'rzc'), 工作区根们());
    if (!rzc路径) {
        const 操作 = await vscode.window.showErrorMessage(
            '未找到 rzc 命令行工具。请安装 rzc（cargo install 或从 Releases 下载），或在设置 i18n-rust.rzcPath 中指定路径。',
            '打开设置'
        );
        if (操作 === '打开设置') {
            vscode.commands.executeCommand('workbench.action.openSettings', 'i18n-rust.rzcPath');
        }
        return undefined;
    }
    return rzc路径;
}

/**
 * 获取复用终端：cwd 与上次一致则复用，否则销毁重建
 */
function 获取命令终端(cwd: string): vscode.Terminal {
    if (命令终端 && !命令终端.exitStatus && 命令终端工作目录 === cwd) {
        return 命令终端;
    }
    if (!命令终端 || 命令终端.exitStatus) {
        // 旧终端已退出：安全重建并复用引用
        命令终端 = vscode.window.createTerminal({ name: 'i18n-rust', cwd });
        命令终端工作目录 = cwd;
        return 命令终端;
    }
    // cwd 变了但旧终端仍在运行（可能有长时间编译/运行中的程序）：
    // 不 dispose 避免杀死进程，新建独立终端（不覆盖复用引用）
    return vscode.window.createTerminal({ name: 'i18n-rust', cwd });
}

/**
 * 运行方言源码文件
 */
async function 运行文件(文件路径: string): Promise<void> {
    // 检查文件是否存在
    if (!fs.existsSync(文件路径)) {
        vscode.window.showErrorMessage(
            `文件不存在: ${文件路径}\n`
            + `提示: 请先保存文件，或关闭此标签页后打开项目中的实际文件。`
        );
        return;
    }

    // 警告临时目录
    if (文件路径.startsWith('/tmp/') || 文件路径.startsWith('/private/tmp/')) {
        const 选择 = await vscode.window.showWarningMessage(
            `当前文件位于临时目录: ${文件路径}\n建议使用项目目录中的文件。`,
            '继续运行', '取消'
        );
        if (选择 !== '继续运行') {
            return;
        }
    }

    const rzc路径 = await 解析rzc();
    if (!rzc路径) {
        return;
    }
    const 终端 = 获取命令终端(path.dirname(文件路径));
    终端.show();
    // 参数安全引用，防止路径中的引号/反引号等导致命令注入
    终端.sendText(`${quoteCommandArg(rzc路径)} run ${quoteShellArg(文件路径)}`);
}

/**
 * 检查方言源码文件
 */
async function 检查文件(文件路径: string): Promise<void> {
    // 检查文件是否存在
    if (!fs.existsSync(文件路径)) {
        vscode.window.showErrorMessage(
            `文件不存在: ${文件路径}\n`
            + `提示: 请先保存文件，或关闭此标签页后打开项目中的实际文件。`
        );
        return;
    }

    const rzc路径 = await 解析rzc();
    if (!rzc路径) {
        return;
    }
    const 终端 = 获取命令终端(path.dirname(文件路径));
    终端.show();
    终端.sendText(`${quoteCommandArg(rzc路径)} check ${quoteShellArg(文件路径)}`);
}

/**
 * 导出为 .rs 文件（无 shell 介入的 execFile，参数数组传递）
 */
async function 导出文件(文件路径: string): Promise<void> {
    if (!fs.existsSync(文件路径)) {
        vscode.window.showErrorMessage(
            `文件不存在: ${文件路径}\n提示: 请先保存文件。`
        );
        return;
    }
    const rzc路径 = await 解析rzc();
    if (!rzc路径) {
        return;
    }
    const 输出路径 = 文件路径.replace(方言扩展名正则, '.rs');

    try {
        await vscode.window.withProgress(
            { location: vscode.ProgressLocation.Notification, title: 'i18n-rust: 导出为 Rust 源码...' },
            () => execFileAsync(rzc路径, ['eject', 文件路径])
        );
        if (!fs.existsSync(输出路径)) {
            vscode.window.showWarningMessage(`rzc eject 执行成功，但未找到预期输出文件: ${输出路径}`);
            return;
        }
        const 选择 = await vscode.window.showInformationMessage(`已导出到 ${输出路径}`, '打开文件');
        if (选择 === '打开文件') {
            const doc = await vscode.workspace.openTextDocument(输出路径);
            await vscode.window.showTextDocument(doc);
        }
    } catch (err: any) {
        const 详情 = err?.stderr?.toString().trim() || err?.message || String(err);
        日志(`导出失败: ${详情}`);
        vscode.window.showErrorMessage(`导出失败: ${详情}`);
    }
}

/**
 * 更新状态栏
 */
function 更新状态栏(): void {
    const config = vscode.workspace.getConfiguration('i18n-rust');
    const 语言包 = config.get<string>('languagePack', '中文');

    statusBarItem.text = `$(globe) ${语言包}`;
    statusBarItem.tooltip = `i18n-rust 语言包: ${语言包}\n点击切换语言包`;
    statusBarItem.show();
}
