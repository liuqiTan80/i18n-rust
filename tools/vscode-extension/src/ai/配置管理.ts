/**
 * 配置管理
 *
 * 负责读取 i18n-rust.ai.* 配置、查找语言包目录、生成最终系统提示词，
 * 以及订阅配置变化。仅本模块依赖 vscode API，其余 AI 模块保持纯 TS。
 */

import * as vscode from 'vscode';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { AI配置, 提供商标识 } from './类型定义';
import { 构建系统提示词 } from './提示词构建';

/**
 * 读取 AI 相关配置（i18n-rust.ai.*），未设置时使用默认值
 */
export function 读取AI配置(): AI配置 {
    const 配置 = vscode.workspace.getConfiguration('i18n-rust.ai');
    return {
        provider: 配置.get<string>('provider', 'openai') as 提供商标识,
        apiKey: 配置.get<string>('apiKey', ''),
        baseUrl: 配置.get<string>('baseUrl', '').trim(),
        model: 配置.get<string>('model', '').trim(),
        temperature: 配置.get<number>('temperature', 0.1),
        maxTokens: 配置.get<number>('maxTokens', 2048),
        systemPrompt: 配置.get<string>('systemPrompt', ''),
        timeout: 配置.get<number>('timeout', 60)
    };
}

/**
 * 当前语言包名称（i18n-rust.languagePack，默认中文）
 */
export function 当前语言名(): string {
    return vscode.workspace.getConfiguration('i18n-rust').get<string>('languagePack', '中文');
}

/**
 * 查找语言包根目录（含各语言子目录，如 <根>/中文/、<根>/俄语/）
 * 查找顺序：
 * 1. 配置 i18n-rust.languagePackPath（用户显式指定）
 * 2. 工作区下的 lang-packs/ 目录
 * 3. 用户全局目录 ~/.rz/lang-packs（rzc 的全局语言包安装位置）
 * 全部不存在时返回 undefined（提示词构建将回退英文）。
 */
export function 查找语言包根目录(): string | undefined {
    const 候选列表 = [
        vscode.workspace.getConfiguration('i18n-rust').get<string>('languagePackPath', ''),
        ...(vscode.workspace.workspaceFolders ?? []).map(文件夹 =>
            path.join(文件夹.uri.fsPath, 'lang-packs')
        ),
        path.join(os.homedir(), '.rz', 'lang-packs')
    ];
    for (const 候选 of 候选列表) {
        if (候选 && fs.existsSync(候选)) {
            return 候选;
        }
    }
    return undefined;
}

/**
 * 获取最终系统提示词：
 * - 配置了自定义 systemPrompt 时直接使用（完全覆盖）
 * - 否则按当前语言包生成（语言包不可用时回退英文）
 */
export function 获取系统提示词(): string {
    const 配置 = 读取AI配置();
    const 自定义 = 配置.systemPrompt.trim();
    if (自定义) {
        return 自定义;
    }
    return 构建系统提示词(当前语言名(), 查找语言包根目录());
}

/**
 * 订阅 AI 配置或语言包变化（provider/baseUrl/语言包等变更时触发回调）
 */
export function 订阅AI配置变化(回调: () => void): vscode.Disposable {
    return vscode.workspace.onDidChangeConfiguration(变化 => {
        if (变化.affectsConfiguration('i18n-rust.ai') || 变化.affectsConfiguration('i18n-rust.languagePack')) {
            回调();
        }
    });
}
