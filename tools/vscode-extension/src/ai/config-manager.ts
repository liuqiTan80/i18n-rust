/**
 * Configuration manager
 *
 * Reads i18n-rust.ai.* settings, locates the language pack directory,
 * builds the final system prompt, and manages the API key in
 * VS Code SecretStorage (encrypted, never synced as plain settings).
 * Only this module depends on the vscode API; other AI modules stay pure TS.
 */

import * as vscode from 'vscode';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { AIConfig, ProviderId } from './types';
import { buildSystemPrompt } from './prompt-builder';
import { 语言代码 } from '../languages';
import { 探测语言包根 } from '../lang-pack-path';

/** SecretStorage 中 API 密钥的存储键 */
const 密钥存储键 = 'i18n-rust.ai.apiKey';

/** 激活时注入的 SecretStorage 引用（命令执行前必然已初始化） */
let secretStore: vscode.SecretStorage | undefined;

/**
 * 初始化密钥存储并完成一次性迁移：
 * 旧版本将密钥明文写在 i18n-rust.ai.apiKey 设置中，
 * 激活时若该设置非空且 SecretStorage 中尚无密钥，
 * 则迁入 SecretStorage 并清空设置。
 */
export async function initAISecrets(context: vscode.ExtensionContext): Promise<void> {
    secretStore = context.secrets;
    const config = vscode.workspace.getConfiguration('i18n-rust.ai');
    const settingsKey = config.get<string>('apiKey', '').trim();
    if (!settingsKey) {
        return;
    }
    const existing = await secretStore.get(密钥存储键);
    if (!existing) {
        await secretStore.store(密钥存储键, settingsKey);
    }
    // 清空明文设置：仅清实际存在密钥的层级（避免对无值层级写空串污染设置文件，
    // 也避免 update 传 undefined 抛异常导致后续层级被跳过）。逐层 try/catch，
    // 单层写失败不阻塞其它层——密钥已安全落入 SecretStorage，明文清理属尽力而为。
    const 检查 = config.inspect<string>('apiKey');
    const 层级们: vscode.ConfigurationTarget[] = [];
    if (检查?.globalValue != null && 检查.globalValue !== '') {
        层级们.push(vscode.ConfigurationTarget.Global);
    }
    if (检查?.workspaceValue != null && 检查.workspaceValue !== '') {
        层级们.push(vscode.ConfigurationTarget.Workspace);
    }
    for (const 层级 of 层级们) {
        try {
            await config.update('apiKey', '', 层级);
        } catch {
            // 忽略：明文残留不影响功能，密钥已存入 SecretStorage
        }
    }
}

/**
 * 读取当前 API 密钥：SecretStorage 优先，未初始化时回退明文设置
 */
export async function readApiKey(): Promise<string> {
    if (secretStore) {
        return (await secretStore.get(密钥存储键)) ?? '';
    }
    return vscode.workspace.getConfiguration('i18n-rust.ai').get<string>('apiKey', '');
}

/** 受限为「仅用户级」的 AI 设置键（工作区不可覆盖，见 package.json scope: application） */
const 仅用户级键 = ['provider', 'baseUrl', 'apiKey'] as const;

/**
 * 检测工作区层级对 AI 关键设置的覆盖。
 *
 * 安全背景：若 apiKey / baseUrl / provider 可被工作区 settings.json 覆盖，
 * 打开不可信仓库时该仓库即可把 baseUrl 指向第三方服务器，使后续请求把
 * 明文密钥发送到攻击者地址。故三者限定为 application 作用域（VS Code 忽略
 * 工作区层级取值）。本函数为历史遗留的工作区配置给出明确提示，避免用户
 * 误以为其仍在生效。返回提示文案列表（无覆盖时为空）。
 */
export function 检测AI设置作用域覆盖(): string[] {
    const config = vscode.workspace.getConfiguration('i18n-rust.ai');
    const 提示: string[] = [];
    for (const 键 of 仅用户级键) {
        const 检查 = config.inspect<string>(键);
        const 覆盖项: Array<[string, string | undefined]> = [
            ['工作区', 检查?.workspaceValue],
            ['工作区文件夹', 检查?.workspaceFolderValue]
        ];
        for (const [层级, 值] of 覆盖项) {
            if (!值 || 值 === 检查?.globalValue) {
                continue;
            }
            提示.push(
                `i18n-rust.ai.${键} 在${层级}设置中被设为「${值}」，出于安全考虑该层级取值已被忽略；实际生效值来自用户设置。`
            );
        }
    }
    return 提示;
}

/**
 * Read AI-related configuration (i18n-rust.ai.*), using defaults when unset
 */
export async function loadAIConfig(): Promise<AIConfig> {
    const config = vscode.workspace.getConfiguration('i18n-rust.ai');
    return {
        provider: config.get<string>('provider', 'openai') as ProviderId,
        apiKey: await readApiKey(),
        baseUrl: config.get<string>('baseUrl', '').trim(),
        model: config.get<string>('model', '').trim(),
        temperature: config.get<number>('temperature', 0.1),
        maxTokens: config.get<number>('maxTokens', 2048),
        systemPrompt: config.get<string>('systemPrompt', ''),
        timeout: config.get<number>('timeout', 60)
    };
}

/**
 * Current language pack display name (i18n-rust.languagePack, default: 中文)
 */
export function currentLanguageName(): string {
    return vscode.workspace.getConfiguration('i18n-rust').get<string>('languagePack', '中文');
}

/**
 * Current language pack code (目录名，如 zh / ru；非法配置回退 zh)
 */
export function currentLanguageCode(): string {
    return 语言代码(currentLanguageName());
}

/**
 * Locate the language pack root directory (containing language-code
 * subdirectories, e.g. <root>/zh/, <root>/ru/)
 * Search order:
 * 1. Configuration i18n-rust.languagePackPath (explicit user setting)
 * 2. Workspace language packs: lang-packs/ (user project convention) or
 *    crates/engine/lang-packs/ (main repo single-source layout)
 * 3. Global user directory ~/.rz/lang-packs (rzc's global install location)
 * Returns undefined when none exists (prompt builder falls back to English).
 */
export function findLanguagePackRoot(): string | undefined {
    const 显式 = vscode.workspace.getConfiguration('i18n-rust').get<string>('languagePackPath', '');
    if (显式 && fs.existsSync(显式)) {
        return 显式;
    }
    for (const folder of vscode.workspace.workspaceFolders ?? []) {
        const 候选 = 探测语言包根(folder.uri.fsPath);
        if (候选) {
            return 候选;
        }
    }
    const 全局 = path.join(os.homedir(), '.rz', 'lang-packs');
    if (fs.existsSync(全局)) {
        return 全局;
    }
    return undefined;
}

/**
 * Get the final system prompt:
 * - Uses the custom systemPrompt when configured (full override)
 * - Otherwise generates one from the current language pack
 *   (falls back to English when the pack is unavailable)
 */
export function getSystemPrompt(): string {
    const config = vscode.workspace.getConfiguration('i18n-rust.ai');
    const custom = config.get<string>('systemPrompt', '').trim();
    if (custom) {
        return custom;
    }
    return buildSystemPrompt(currentLanguageCode(), findLanguagePackRoot());
}
