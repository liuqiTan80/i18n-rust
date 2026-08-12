/**
 * i18n-rust VS Code 扩展
 * 
 * 功能：
 * - 中文 Rust 语法高亮
 * - LSP 客户端连接 i18n-rust-lsp
 * - 提供 Run/Check 命令
 * - 状态栏显示当前语言包
 */

import * as vscode from 'vscode';
import * as path from 'path';
import * as cp from 'child_process';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let statusBarItem: vscode.StatusBarItem;

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
            if (doc.languageId === 'rust-zh') {
                // 可选：保存时自动检查
            }
        })
    );

    // 显示欢迎信息（首次激活）
    const 已显示 = context.globalState.get<boolean>('welcomeShown');
    if (!已显示) {
        vscode.window.showInformationMessage(
            '欢迎使用 i18n-rust！按 Ctrl+Shift+R 运行中文 Rust 代码。',
            '了解更多'
        ).then(selection => {
            if (selection === '了解更多') {
                vscode.env.openExternal(
                    vscode.Uri.parse('https://github.com/i18n-rust/i18n-rust')
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
            if (!编辑器 || 编辑器.document.languageId !== 'rust-zh') {
                vscode.window.showWarningMessage('请打开一个 .zh 文件');
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
            if (!编辑器 || 编辑器.document.languageId !== 'rust-zh') {
                vscode.window.showWarningMessage('请打开一个 .zh 文件');
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
            if (!编辑器 || 编辑器.document.languageId !== 'rust-zh') {
                vscode.window.showWarningMessage('请打开一个 .zh 文件');
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
                vscode.window.showInformationMessage(`语言包已切换为: ${选择}`);
            }
        })
    );

    // 重启服务器命令
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.restartServer', async () => {
            if (client) {
                await client.stop();
            }
            启动语言服务器(context);
            vscode.window.showInformationMessage('语言服务器已重启');
        })
    );
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

    // 服务器选项
    const serverOptions: ServerOptions = {
        run: {
            command: serverPath,
            transport: TransportKind.stdio
        },
        debug: {
            command: serverPath,
            transport: TransportKind.stdio
        }
    };

    // 客户端选项
    const clientOptions: LanguageClientOptions = {
        documentSelector: [
            { scheme: 'file', language: 'rust-zh' }
        ],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.zh')
        },
        diagnosticCollectionName: 'i18n-rust',
        outputChannelName: 'i18n-rust LSP',
        traceOutputChannel: vscode.window.createOutputChannel('i18n-rust LSP Trace'),
        initializationOptions: {
            languagePack: config.get<string>('languagePack', '中文')
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
    终端.sendText(`i18n run "${文件路径}"`);
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
    终端.sendText(`i18n check "${文件路径}"`);
}

/**
 * 导出为 .rs 文件
 */
async function 导出文件(文件路径: string): Promise<void> {
    const 输出路径 = 文件路径.replace(/\.zh$/, '.rs');
    
    try {
        cp.execSync(`i18n eject "${文件路径}"`);
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
