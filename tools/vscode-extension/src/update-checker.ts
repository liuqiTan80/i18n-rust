/**
 * 扩展更新检查（GitHub Releases）
 *
 * 每天最多检查一次（时间戳存 globalState），激活后延迟执行不阻塞启动；
 * 网络异常静默忽略，不影响正常使用；可在设置 i18n-rust.checkUpdates 中关闭。
 */

import * as https from 'https';
import * as vscode from 'vscode';
import { 解析发行版版本, 是更新版本 } from './version-compare';

/** 发布仓库（公开仓库，GitHub API 无需认证） */
const 仓库 = 'liuqiTan80/i18n-rust';

/** 两次检查的最小间隔：24 小时 */
const 检查间隔毫秒 = 24 * 60 * 60 * 1000;

/** 激活后延迟检查：先让 LSP 与界面完成启动 */
const 延迟毫秒 = 10000;

const 状态键_上次检查 = 'updateCheck.lastAt';
const 状态键_忽略版本 = 'updateCheck.skippedVersion';

interface 发行版信息 {
    版本: string;
    页面地址: string;
}

/**
 * 注册更新检查（供 activate 调用）。
 *
 * 未启用配置或距上次检查不足 24 小时时直接返回。
 */
export function 注册更新检查(context: vscode.ExtensionContext): void {
    const 已启用 = vscode.workspace
        .getConfiguration('i18n-rust')
        .get<boolean>('checkUpdates', true);
    if (!已启用) {
        return;
    }
    const 上次检查 = context.globalState.get<number>(状态键_上次检查, 0);
    if (Date.now() - 上次检查 < 检查间隔毫秒) {
        return;
    }
    const 定时器 = setTimeout(() => {
        void 执行检查(context);
    }, 延迟毫秒);
    context.subscriptions.push(new vscode.Disposable(() => clearTimeout(定时器)));
}

async function 执行检查(context: vscode.ExtensionContext): Promise<void> {
    const 发行版 = await 获取最新发行版();
    // 无论成功与否都记录检查时间，避免网络异常时每次启动都重试
    await context.globalState.update(状态键_上次检查, Date.now());
    if (!发行版) {
        return;
    }
    const 当前版本 = String(context.extension.packageJSON.version ?? '0.0.0');
    if (!是更新版本(发行版.版本, 当前版本)) {
        return;
    }
    if (context.globalState.get<string>(状态键_忽略版本) === 发行版.版本) {
        return;
    }

    const 选择 = await vscode.window.showInformationMessage(
        `i18n-rust 有新版本 v${发行版.版本}（当前 v${当前版本}），是否查看？`,
        '查看更新',
        '忽略此版本',
        '不再提醒'
    );
    if (选择 === '查看更新') {
        void vscode.env.openExternal(vscode.Uri.parse(发行版.页面地址));
    } else if (选择 === '忽略此版本') {
        await context.globalState.update(状态键_忽略版本, 发行版.版本);
    } else if (选择 === '不再提醒') {
        await vscode.workspace
            .getConfiguration('i18n-rust')
            .update('checkUpdates', false, vscode.ConfigurationTarget.Global);
    }
}

/** 请求 GitHub API 获取最新正式发行版；失败（网络/限流/无发行版）返回 undefined */
function 获取最新发行版(): Promise<发行版信息 | undefined> {
    return new Promise(resolve => {
        const 请求 = https.get(
            {
                hostname: 'api.github.com',
                path: `/repos/${仓库}/releases/latest`,
                headers: {
                    // GitHub API 要求带 User-Agent
                    'User-Agent': 'i18n-rust-vscode-extension',
                    Accept: 'application/vnd.github+json'
                },
                timeout: 8000
            },
            响应 => {
                if (响应.statusCode !== 200) {
                    响应.resume();
                    resolve(undefined);
                    return;
                }
                let 数据 = '';
                响应.setEncoding('utf8');
                响应.on('data', 块 => {
                    数据 += 块;
                });
                响应.on('end', () => {
                    try {
                        const 应答 = JSON.parse(数据) as { tag_name?: string; html_url?: string };
                        const 版本 = 解析发行版版本(应答.tag_name);
                        if (版本 && 应答.html_url) {
                            resolve({ 版本, 页面地址: 应答.html_url });
                        } else {
                            resolve(undefined);
                        }
                    } catch {
                        resolve(undefined);
                    }
                });
            }
        );
        请求.on('timeout', () => {
            请求.destroy();
            resolve(undefined);
        });
        请求.on('error', () => {
            resolve(undefined);
        });
    });
}
