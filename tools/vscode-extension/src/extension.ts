/**
 * i18n-rust VS Code 扩展
 * 
 * 功能：
 * - 中文 Rust 语法高亮
 * - LSP 客户端连接 i18n-rust-lsp
 * - 提供 Run/Check 命令
 * - 状态栏显示当前语言包
 * - 所有权错误可视化（变量移动/借用/再次使用位置颜色高亮）
 */

import * as vscode from 'vscode';
import * as path from 'path';
import * as cp from 'child_process';
import * as fs from 'fs';
import {
    CloseAction,
    ErrorAction,
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind
} from 'vscode-languageclient/node';
import { createProvider, listProviders } from './ai/provider-factory';
import { loadAIConfig, getSystemPrompt, currentLanguageName } from './ai/config-manager';
import { AIError } from './ai/types';
import { ProviderInterface } from './ai/provider-interface';

/**
 * 所有受支持的方言语言 ID（对应 package.json 中注册的语言）
 * .zh/.en/.de 均通过 LSP 代理翻译后交给 rust-analyzer
 */
const 方言语言Id: readonly string[] = ['rust-zh', 'rust-en', 'rust-de'];

/**
 * 语言包显示名 → lang-packs 目录名映射
 */
const 语言包目录映射: Record<string, string> = {
    '中文': 'zh',
    'English': 'en',
    '日本語': 'ja'
};

let client: LanguageClient | undefined;
let statusBarItem: vscode.StatusBarItem;
// LSP 自动重启计数（防止崩溃循环导致无限重启）
let 自动重启次数 = 0;

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
 * 查找语言包目录路径
 *
 * 优先级：
 * 1. 配置 i18n-rust.languagePackPath（显式指定）
 * 2. 工作区中的 lang-packs/<代码> 目录（如 lang-packs/zh）
 * 3. 找不到返回 undefined（LSP 使用默认内置映射）
 */
function 查找语言包路径(config: vscode.WorkspaceConfiguration): string | undefined {
    // 1. 显式配置
    const 显式路径 = config.get<string>('languagePackPath', '');
    if (显式路径) {
        return 显式路径;
    }
    // 2. 工作区 lang-packs/<代码>
    const 语言代码 = 语言包目录映射[config.get<string>('languagePack', '中文')] ?? 'zh';
    for (const 文件夹 of vscode.workspace.workspaceFolders ?? []) {
        const 候选 = path.join(文件夹.uri.fsPath, 'lang-packs', 语言代码);
        if (fs.existsSync(候选)) {
            return 候选;
        }
    }
    return undefined;
}

/**
 * 扩展激活入口
 */
export function activate(context: vscode.ExtensionContext): void {
    console.log('i18n-rust 扩展已激活');

    // 创建状态栏项
    statusBarItem = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Right,
        100
    );
    statusBarItem.command = 'i18n-rust.selectLanguagePack';
    context.subscriptions.push(statusBarItem);
    更新状态栏();

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

    // 监听文件保存
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument(doc => {
            if (方言语言Id.includes(doc.languageId)) {
                // 可选：保存时自动检查
            }
        })
    );

    // 注册所有权错误可视化（诊断 data 中的 所有权详情 → 颜色装饰器）
    注册所有权可视化(context);

    // 显示欢迎信息（首次激活）
    const 已显示 = context.globalState.get<boolean>('welcomeShown');
    if (!已显示) {
        vscode.window.showInformationMessage(
            '欢迎使用 i18n-rust！按 Ctrl+Shift+R 运行中文 Rust 代码。',
            '了解更多'
        ).then(selection => {
            if (selection === '了解更多') {
                vscode.env.openExternal(
                    vscode.Uri.parse('https://gitcode.com/tan80/zrRust')
                );
            }
        });
        context.globalState.update('welcomeShown', true);
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
                vscode.window.showWarningMessage('请打开一个 .zh/.en/.de 方言文件');
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
                vscode.window.showWarningMessage('请打开一个 .zh/.en/.de 方言文件');
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
                vscode.window.showWarningMessage('请打开一个 .zh/.en/.de 方言文件');
                return;
            }

            const 文件路径 = 编辑器.document.uri.fsPath;
            await 导出文件(文件路径);
        })
    );

    // 选择语言包命令
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.selectLanguagePack', async () => {
            const 语言列表 = ['中文', 'English', '日本語'];
            const 选择 = await vscode.window.showQuickPick(语言列表, {
                placeHolder: '选择语言包'
            });
            if (选择) {
                const config = vscode.workspace.getConfiguration('i18n-rust');
                await config.update('languagePack', 选择, vscode.ConfigurationTarget.Global);
                更新状态栏();
                // 语言包变化后重启 LSP，使新语言包立即生效
                await 重启服务器(context);
                vscode.window.showInformationMessage(`语言包已切换为: ${选择}，语言服务器已重启`);
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

    // AI 相关命令（对话 / 选择提供商 / 模型列表）
    注册AI命令(context);
}

/**
 * 注册 AI 相关命令：
 * - i18n-rust.aiChat：AI 对话（携带当前 .zh 文件内容作为上下文，流式输出）
 * - i18n-rust.aiSelectProvider：选择 AI 提供商
 * - i18n-rust.aiListModels：获取当前提供商可用模型列表
 */
function 注册AI命令(context: vscode.ExtensionContext): void {
    // 输出面板：所有 AI 对话/模型结果统一输出到这里
    const AI输出 = vscode.window.createOutputChannel('i18n-rust AI');

    // AI 对话命令
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.aiChat', async () => {
            const 配置 = loadAIConfig();
            // 云端服务需要密钥；Ollama / 自定义地址可无密钥
            if (!配置.apiKey && 配置.provider !== 'ollama' && 配置.provider !== 'custom') {
                const 操作 = await vscode.window.showWarningMessage(
                    `尚未配置 API 密钥（提供商：${配置.provider}）。请在设置中填写 i18n-rust.ai.apiKey。`,
                    '打开设置'
                );
                if (操作 === '打开设置') {
                    vscode.commands.executeCommand('workbench.action.openSettings', 'i18n-rust.ai');
                }
                return;
            }

            const 问题 = await vscode.window.showInputBox({
                prompt: '向 AI 提问（关于当前 .zh 文件或 Rust 教学）',
                placeHolder: '例如：解释这段代码的所有权移动过程'
            });
            if (!问题) {
                return;
            }

            // 携带当前打开的方言文件内容作为上下文
            const 编辑器 = vscode.window.activeTextEditor;
            const 上下文 = 编辑器 && 方言语言Id.includes(编辑器.document.languageId)
                ? `当前文件 ${编辑器.document.fileName}：\n\`\`\`zh\n${编辑器.document.getText()}\n\`\`\`\n\n`
                : '';

            let 提供商: ProviderInterface;
            try {
                提供商 = createProvider(配置);
            } catch (错误) {
                vscode.window.showErrorMessage((错误 as Error).message);
                return;
            }

            AI输出.show(true);
            AI输出.appendLine(`── AI 对话（${配置.provider} / ${配置.model || '默认模型'}，语言包：${currentLanguageName()}）──`);
            AI输出.appendLine(`问：${问题}\n`);
            try {
                await 提供商.streamChat(
                    [
                        { role: 'system', content: getSystemPrompt() },
                        { role: 'user', content: 上下文 + 问题 }
                    ],
                    chunk => AI输出.append(chunk)
                );
                AI输出.appendLine('\n── 对话结束 ──');
            } catch (错误) {
                if (错误 instanceof AIError) {
                    AI输出.appendLine(`\n[${错误.category}] ${错误.message}`);
                    vscode.window.showErrorMessage(`${错误.category}：${错误.message}`);
                } else {
                    AI输出.appendLine(`\n[未知错误] ${(错误 as Error).message}`);
                }
            }
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
            const 配置 = loadAIConfig();
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
    }
    自动重启次数 = 0;
    启动语言服务器(context);
}

/**
 * 启动 LSP 语言服务器
 */
function 启动语言服务器(context: vscode.ExtensionContext): void {
    const config = vscode.workspace.getConfiguration('i18n-rust');
    let serverPath = config.get<string>('serverPath', 'i18n-rust-lsp');

    // 如果路径是相对的，尝试在常见位置查找
    if (!path.isAbsolute(serverPath)) {
        const workspaceFolders = vscode.workspace.workspaceFolders;
        if (workspaceFolders) {
            const 可能路径 = [
                path.join(workspaceFolders[0].uri.fsPath, 'target', 'debug', serverPath),
                path.join(workspaceFolders[0].uri.fsPath, serverPath),
            ];
            for (const p of 可能路径) {
                if (require('fs').existsSync(p)) {
                    serverPath = p;
                    break;
                }
            }
        }
    }

    // 服务器选项：显式传递语言包路径（LSP 默认相对路径依赖启动目录，通常找不到）
    const args: string[] = [];
    const 语言包路径 = 查找语言包路径(config);
    if (语言包路径) {
        args.push('--language-pack', 语言包路径);
    }
    const serverOptions: ServerOptions = {
        run: {
            command: serverPath,
            args,
            transport: TransportKind.stdio
        },
        debug: {
            command: serverPath,
            args,
            transport: TransportKind.stdio
        }
    };

    // 客户端选项
    const clientOptions: LanguageClientOptions = {
        documentSelector: 方言语言Id.map(语言 => ({ scheme: 'file', language: 语言 })),
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.{zh,en,de}')
        },
        diagnosticCollectionName: 'i18n-rust',
        outputChannelName: 'i18n-rust LSP',
        traceOutputChannel: vscode.window.createOutputChannel('i18n-rust LSP Trace'),
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
        vscode.window.showErrorMessage(
            `i18n-rust LSP 启动失败: ${err.message}。请确保 i18n-rust-lsp 已安装并在 PATH 中。`
        );
    });

    context.subscriptions.push(client);
}

/**
 * 运行 .zh 文件
 */
async function 运行文件(文件路径: string): Promise<void> {
    const 终端 = vscode.window.createTerminal({
        name: 'i18n Run',
        cwd: path.dirname(文件路径)
    });
    终端.show();
    终端.sendText(`rzc run "${文件路径}"`);
}

/**
 * 检查 .zh 文件
 */
async function 检查文件(文件路径: string): Promise<void> {
    const 终端 = vscode.window.createTerminal({
        name: 'i18n Check',
        cwd: path.dirname(文件路径)
    });
    终端.show();
    终端.sendText(`rzc check "${文件路径}"`);
}

/**
 * 导出为 .rs 文件
 */
async function 导出文件(文件路径: string): Promise<void> {
    const 输出路径 = 文件路径.replace(/\.(zh|en|de)$/, '.rs');
    
    try {
        cp.execSync(`rzc eject "${文件路径}"`);
        vscode.window.showInformationMessage(
            `已导出到 ${输出路径}`,
            '打开文件'
        ).then(selection => {
            if (selection === '打开文件') {
                vscode.workspace.openTextDocument(输出路径).then(doc => {
                    vscode.window.showTextDocument(doc);
                });
            }
        });
    } catch (err: any) {
        vscode.window.showErrorMessage(`导出失败: ${err.message}`);
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
