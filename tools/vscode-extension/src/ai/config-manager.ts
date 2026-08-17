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
    // 清空明文设置（全局与工作区两级都尝试）
    await config.update('apiKey', undefined, vscode.ConfigurationTarget.Global);
    await config.update('apiKey', undefined, vscode.ConfigurationTarget.Workspace);
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
 * 2. lang-packs/ under the current workspace
 * 3. Global user directory ~/.rz/lang-packs (rzc's global install location)
 * Returns undefined when none exists (prompt builder falls back to English).
 */
export function findLanguagePackRoot(): string | undefined {
    const candidates = [
        vscode.workspace.getConfiguration('i18n-rust').get<string>('languagePackPath', ''),
        ...(vscode.workspace.workspaceFolders ?? []).map(folder =>
            path.join(folder.uri.fsPath, 'lang-packs')
        ),
        path.join(os.homedir(), '.rz', 'lang-packs')
    ];
    for (const candidate of candidates) {
        if (candidate && fs.existsSync(candidate)) {
            return candidate;
        }
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

/**
 * Subscribe to AI config or language pack changes
 * (provider/baseUrl/languagePack changes trigger the callback)
 */
export function subscribeConfigChange(callback: () => void): vscode.Disposable {
    return vscode.workspace.onDidChangeConfiguration(change => {
        if (change.affectsConfiguration('i18n-rust.ai') || change.affectsConfiguration('i18n-rust.languagePack')) {
            callback();
        }
    });
}
