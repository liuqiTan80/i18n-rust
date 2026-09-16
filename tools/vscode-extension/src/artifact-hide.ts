/**
 * 转译产物显示控制（VS Code 胶水）
 *
 * 设置 i18n-rust.hideGeneratedFiles 控制是否把两个产物匹配模式
 * （.rs 与 .rs.bak，见 artifact-rules.ts）写入工作区 files.exclude：
 * 开启=隐藏，关闭=移除条目恢复显示。命令
 * i18n-rust.toggleGeneratedFiles 提供免开设置面板的快捷切换；
 * 激活与配置变化时自动同步，保证开关与实际显示一致。
 */

import * as vscode from 'vscode';
import { 计算排除项, 排除项已一致 } from './artifact-rules';

/**
 * 注册产物显示控制（供 activate 调用）：
 * 1. 命令 i18n-rust.toggleGeneratedFiles：免开设置面板，随手切换；
 * 2. 激活时同步一次（首次使用/上次未生效的自愈）；
 * 3. 监听开关变化，写入或移除 files.exclude 条目。
 */
export function 注册产物隐藏(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('i18n-rust.toggleGeneratedFiles', async () => {
            if (!vscode.workspace.workspaceFolders?.length) {
                vscode.window.showWarningMessage('请先打开文件夹工作区，再切换转译产物的显示');
                return;
            }
            const 当前 = vscode.workspace
                .getConfiguration('i18n-rust')
                .get<boolean>('hideGeneratedFiles', false);
            const 目标 = !当前;
            try {
                await 写开关(目标);
                vscode.window.showInformationMessage(
                    目标
                        ? '已隐藏转译产物（.rs / .rs.bak）'
                        : '已恢复显示转译产物（.rs / .rs.bak）'
                );
            } catch (错误) {
                vscode.window.showErrorMessage(
                    `写入 hideGeneratedFiles 设置失败：${(错误 as Error).message}`
                );
            }
        })
    );

    // 激活时同步一次：覆盖「此前改了开关但未生效」「手工改过设置」等场景
    void 同步产物隐藏();

    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(事件 => {
            // 多根工作区 folder 级修改可能不改变窗口级合并值，逐根再判一次
            const 受影响 = 事件.affectsConfiguration('i18n-rust.hideGeneratedFiles')
                || (vscode.workspace.workspaceFolders ?? []).some(文件夹 =>
                    事件.affectsConfiguration('i18n-rust.hideGeneratedFiles', 文件夹.uri)
                );
            if (受影响) {
                void 同步产物隐藏();
            }
        })
    );
}

/**
 * 把开关值写入用户可见的设置位置：
 * 有 .code-workspace 时写工作区文件，否则逐文件夹写
 * （单根即 .vscode/settings.json）。
 */
async function 写开关(值: boolean): Promise<void> {
    if (vscode.workspace.workspaceFile) {
        await vscode.workspace
            .getConfiguration('i18n-rust')
            .update('hideGeneratedFiles', 值, vscode.ConfigurationTarget.Workspace);
        return;
    }
    for (const 文件夹 of vscode.workspace.workspaceFolders ?? []) {
        await vscode.workspace
            .getConfiguration('i18n-rust', 文件夹.uri)
            .update('hideGeneratedFiles', 值, vscode.ConfigurationTarget.WorkspaceFolder);
    }
}

/** 按开关的当前有效值刷新 files.exclude（开启写入、关闭移除，幂等） */
async function 同步产物隐藏(): Promise<void> {
    if (vscode.workspace.workspaceFile) {
        const 开启 = vscode.workspace
            .getConfiguration('i18n-rust')
            .get<boolean>('hideGeneratedFiles', false);
        await 更新排除项(
            vscode.workspace.getConfiguration('files'),
            开启,
            vscode.ConfigurationTarget.Workspace
        );
        return;
    }
    for (const 文件夹 of vscode.workspace.workspaceFolders ?? []) {
        const 开启 = vscode.workspace
            .getConfiguration('i18n-rust', 文件夹.uri)
            .get<boolean>('hideGeneratedFiles', false);
        await 更新排除项(
            vscode.workspace.getConfiguration('files', 文件夹.uri),
            开启,
            vscode.ConfigurationTarget.WorkspaceFolder
        );
    }
}

/**
 * 读出目标层级的现有条目，有变化才计算并写回。
 * 读取用 inspect 的层级值（而非合并值），避免把用户全局/默认条目
 * 固化进工作区设置文件。
 */
async function 更新排除项(
    config: vscode.WorkspaceConfiguration,
    开启: boolean,
    目标: vscode.ConfigurationTarget
): Promise<void> {
    const inspect = config.inspect<Record<string, boolean>>('exclude');
    const 现有 = 目标 === vscode.ConfigurationTarget.Workspace
        ? inspect?.workspaceValue
        : inspect?.workspaceFolderValue;
    if (排除项已一致(现有, 开启)) {
        return;
    }
    try {
        await config.update('exclude', 计算排除项(现有, 开启), 目标);
    } catch (错误) {
        // 只读工作区等场景：提示一次，不中断扩展其它功能
        vscode.window.showWarningMessage(
            `写入工作区 files.exclude 失败：${(错误 as Error).message}`
        );
    }
}
