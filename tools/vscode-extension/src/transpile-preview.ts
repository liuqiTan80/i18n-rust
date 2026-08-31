/**
 * 转译预览：将方言源码实时转译为标准 Rust，并在并排 Webview 中对比展示。
 *
 * 数据流：读取源码 → `rzc transpile <file>`（stdout 输出转译结果，不写文件）
 * → 左右两栏渲染（左：母语源码；右：标准 Rust，带行号与轻量高亮）。
 * 保存源文件时自动重新转译；面板按文件路径复用，重复执行命令时聚焦已有面板。
 */

import * as vscode from 'vscode';
import * as path from 'path';
import * as cp from 'child_process';
import * as fs from 'fs';
import { promisify } from 'util';
import { 方言语言Id } from './languages';
import { 解析可执行文件 } from './executable';

const execFileAsync = promisify(cp.execFile);

/** 已打开的转译预览面板（文件路径 → 面板），供保存自动刷新与复用聚焦 */
const 预览面板们 = new Map<string, vscode.WebviewPanel>();

/** 标准 Rust 关键字（右侧转译结果高亮用；左侧方言关键字随语言包变化，不做关键字高亮） */
const RUST关键字 = new Set([
    'as', 'async', 'await', 'break', 'const', 'continue', 'crate', 'dyn', 'else',
    'enum', 'extern', 'false', 'fn', 'for', 'if', 'impl', 'in', 'let', 'loop',
    'match', 'mod', 'move', 'mut', 'pub', 'ref', 'return', 'self', 'Self',
    'static', 'struct', 'super', 'trait', 'true', 'type', 'unsafe', 'use',
    'where', 'while'
]);

/**
 * 注册转译预览命令与保存自动刷新：
 * - i18n-rust.transpilePreview：对当前方言文件打开/聚焦并排转译预览
 * - 方言文件保存时，若其预览面板已打开则自动重新转译
 */
export function 注册转译预览(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.transpilePreview', async () => {
            const 编辑器 = vscode.window.activeTextEditor;
            if (!编辑器 || !方言语言Id.includes(编辑器.document.languageId)) {
                vscode.window.showWarningMessage('请打开一个方言源码文件（.zh/.ja 等）再使用转译预览');
                return;
            }
            await 显示转译预览(编辑器.document.uri.fsPath);
        })
    );

    // 保存方言文件时自动刷新对应预览面板（若已打开）
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument(文档 => {
            const 面板 = 预览面板们.get(文档.uri.fsPath);
            if (面板) {
                void 刷新转译预览(面板, 文档.uri.fsPath);
            }
        })
    );
}

/**
 * 打开（或聚焦已有）指定文件的转译预览面板
 */
async function 显示转译预览(文件路径: string): Promise<void> {
    if (!fs.existsSync(文件路径)) {
        vscode.window.showErrorMessage(
            `文件不存在: ${文件路径}\n提示: 请先保存文件，或关闭此标签页后打开项目中的实际文件。`
        );
        return;
    }
    const 已有 = 预览面板们.get(文件路径);
    if (已有) {
        已有.reveal(vscode.ViewColumn.Beside);
        return;
    }

    const 面板 = vscode.window.createWebviewPanel(
        'i18n-rust.transpilePreview',
        `转译预览: ${path.basename(文件路径)}`,
        vscode.ViewColumn.Beside,
        { enableScripts: true, retainContextWhenHidden: true }
    );
    预览面板们.set(文件路径, 面板);
    面板.onDidDispose(() => {
        预览面板们.delete(文件路径);
    });
    面板.webview.onDidReceiveMessage(消息 => {
        if (消息.type === 'refresh') {
            void 刷新转译预览(面板, 文件路径);
        }
    });
    await 刷新转译预览(面板, 文件路径);
}

/**
 * 重新执行转译并渲染面板内容（源码 + 转译结果 + 状态/错误）
 */
async function 刷新转译预览(面板: vscode.WebviewPanel, 文件路径: string): Promise<void> {
    if (!fs.existsSync(文件路径)) {
        面板.webview.html = 渲染页面(文件路径, '', '', '文件不存在，可能已被删除');
        return;
    }
    const 源码 = fs.readFileSync(文件路径, 'utf-8');
    面板.webview.html = 渲染页面(文件路径, 源码, '', undefined, '正在转译…');

    const rzc路径 = await 解析rzc();
    if (!rzc路径) {
        return;
    }
    const 开始 = Date.now();
    try {
        const { stdout } = await execFileAsync(rzc路径, ['transpile', 文件路径], { timeout: 30000 });
        面板.webview.html = 渲染页面(文件路径, 源码, stdout, undefined, `转译耗时 ${Date.now() - 开始}ms`);
    } catch (err: any) {
        const 详情 = err?.stderr?.toString().trim() || err?.message || String(err);
        面板.webview.html = 渲染页面(文件路径, 源码, '', 详情, '转译失败');
    }
}

/**
 * 解析 rzc 可执行文件；找不到时提示用户并返回 undefined
 */
async function 解析rzc(): Promise<string | undefined> {
    const config = vscode.workspace.getConfiguration('i18n-rust');
    const 工作区根们 = (vscode.workspace.workspaceFolders ?? []).map(文件夹 => 文件夹.uri.fsPath);
    const rzc路径 = 解析可执行文件(config.get<string>('rzcPath', 'rzc'), 工作区根们);
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
 * 渲染并排预览页面：工具栏 + 左右两栏（行号 + 轻量高亮）
 */
function 渲染页面(
    文件路径: string,
    源码: string,
    转译: string,
    错误?: string,
    状态?: string
): string {
    const nonce = 生成Nonce();
    const 左列 = 高亮代码(源码, new Set<string>());
    const 右列 = 错误
        ? `<div class="error">${转义Html(错误)}</div>`
        : 高亮代码(转译, RUST关键字);
    const 文件名 = path.basename(文件路径);

    return `<!DOCTYPE html>
<html lang="zh">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'nonce-${nonce}';">
<style>
    body { font-family: var(--vscode-font-family); margin: 0; padding: 10px; box-sizing: border-box; color: var(--vscode-foreground); }
    .toolbar { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; }
    .title { font-weight: 600; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .status { font-size: 12px; opacity: 0.7; }
    button { background: var(--vscode-button-background); color: var(--vscode-button-foreground); border: none; padding: 4px 12px; cursor: pointer; border-radius: 2px; }
    button:hover { background: var(--vscode-button-hoverBackground); }
    .columns { display: flex; gap: 10px; height: calc(100vh - 52px); }
    .col { flex: 1; min-width: 0; display: flex; flex-direction: column; }
    .col-header { font-size: 12px; opacity: 0.8; padding: 4px 8px; background: var(--vscode-editor-lineHighlightBackground, transparent); border: 1px solid var(--vscode-panel-border, #444); border-bottom: none; }
    .code-wrap { flex: 1; overflow: auto; border: 1px solid var(--vscode-panel-border, #444); }
    pre { margin: 0; padding: 6px 0; font-family: var(--vscode-editor-font-family, monospace); font-size: var(--vscode-editor-font-size, 13px); line-height: 1.5; }
    .line { display: flex; }
    .line-num { color: var(--vscode-editorLineNumber-foreground, #888); text-align: right; padding: 0 8px; user-select: none; min-width: 3em; flex-shrink: 0; }
    .code { white-space: pre; padding-right: 12px; }
    .tok-comment { color: #6a9955; }
    .tok-string { color: #ce9178; }
    body.vscode-light .tok-comment { color: #008000; }
    body.vscode-light .tok-string { color: #a31515; }
    .tok-keyword { color: #569cd6; }
    body.vscode-light .tok-keyword { color: #0000ff; }
    .error { color: var(--vscode-errorForeground, #f14c4c); white-space: pre-wrap; padding: 8px; font-family: var(--vscode-editor-font-family, monospace); font-size: 13px; }
</style>
</head>
<body>
    <div class="toolbar">
        <span class="title">${转义Html(文件名)}（转译预览）</span>
        <span class="status">${状态 ? 转义Html(状态) : ''}</span>
        <button id="refresh">⟳ 重新转译</button>
    </div>
    <div class="columns">
        <div class="col">
            <div class="col-header">母语源码</div>
            <div class="code-wrap" id="left"><pre>${左列}</pre></div>
        </div>
        <div class="col">
            <div class="col-header">转译后的标准 Rust</div>
            <div class="code-wrap" id="right"><pre>${右列}</pre></div>
        </div>
    </div>
    <script nonce="${nonce}">
        const vscodeApi = acquireVsCodeApi();
        document.getElementById('refresh').addEventListener('click', () => {
            vscodeApi.postMessage({ type: 'refresh' });
        });
        // 左右两栏同步滚动：任一方向滚动时带动另一侧
        const 左 = document.getElementById('left');
        const 右 = document.getElementById('right');
        左.addEventListener('scroll', () => { 右.scrollTop = 左.scrollTop; 右.scrollLeft = 左.scrollLeft; });
        右.addEventListener('scroll', () => { 左.scrollTop = 右.scrollTop; 左.scrollLeft = 右.scrollLeft; });
    </script>
</body>
</html>`;
}

/**
 * 逐 token 高亮并生成带行号的代码行（注释/字符串/关键字分级配色）
 */
function 高亮代码(全文: string, 关键字们: Set<string>): string {
    if (!全文) {
        return '';
    }
    const 结果: string[] = [];
    let 行号 = 1;
    let 当前行 = '';
    const 收尾行 = () => {
        结果.push(`<div class="line"><span class="line-num">${行号}</span><span class="code">${当前行 || ' '}</span></div>`);
        行号++;
        当前行 = '';
    };
    // 按顺序消费 token：行注释 / 块注释（可跨行）/ 字符串 / 标识符 / 换行 / 其他
    const 正则 = /\/\/[^\n]*|\/\*[\s\S]*?\*\/|"(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*'|\b[a-zA-Z_][a-zA-Z0-9_]*\b|\n|[^\n]+/g;

    for (const 匹配 of 全文.matchAll(正则)) {
        const token = 匹配[0];
        if (token === '\n') {
            收尾行();
            continue;
        }
        当前行 += 包装令牌(token, 关键字们);
    }
    if (当前行 || 全文.endsWith('\n')) {
        收尾行();
    }
    return 结果.join('\n');
}

/**
 * 单个 token 转义并按类别包装高亮 span
 */
function 包装令牌(token: string, 关键字们: Set<string>): string {
    const 转义 = 转义Html(token);
    if (token.startsWith('//') || token.startsWith('/*')) {
        return `<span class="tok-comment">${转义}</span>`;
    }
    if (token.startsWith('"') || token.startsWith("'")) {
        return `<span class="tok-string">${转义}</span>`;
    }
    if (关键字们.has(token)) {
        return `<span class="tok-keyword">${转义}</span>`;
    }
    return 转义;
}

/**
 * HTML 转义（防注入；源码/转译结果均为不可信文本）
 */
function 转义Html(文本: string): string {
    return 文本
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

/**
 * 生成 CSP nonce（每次渲染随机）
 */
function 生成Nonce(): string {
    const 字符 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    let 文本 = '';
    for (let i = 0; i < 32; i++) {
        文本 += 字符.charAt(Math.floor(Math.random() * 字符.length));
    }
    return 文本;
}
