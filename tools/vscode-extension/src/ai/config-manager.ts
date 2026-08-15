/**
 * Configuration manager
 *
 * Reads i18n-rust.ai.* settings, locates the language pack directory,
 * builds the final system prompt, and subscribes to configuration changes.
 * Only this module depends on the vscode API; other AI modules stay pure TS.
 */

import * as vscode from 'vscode';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { AIConfig, ProviderId } from './types';
import { buildSystemPrompt } from './prompt-builder';

/**
 * Read AI-related configuration (i18n-rust.ai.*), using defaults when unset
 */
export function loadAIConfig(): AIConfig {
    const config = vscode.workspace.getConfiguration('i18n-rust.ai');
    return {
        provider: config.get<string>('provider', 'openai') as ProviderId,
        apiKey: config.get<string>('apiKey', ''),
        baseUrl: config.get<string>('baseUrl', '').trim(),
        model: config.get<string>('model', '').trim(),
        temperature: config.get<number>('temperature', 0.1),
        maxTokens: config.get<number>('maxTokens', 2048),
        systemPrompt: config.get<string>('systemPrompt', ''),
        timeout: config.get<number>('timeout', 60)
    };
}

/**
 * Current language pack name (i18n-rust.languagePack, default: 中文)
 */
export function currentLanguageName(): string {
    return vscode.workspace.getConfiguration('i18n-rust').get<string>('languagePack', '中文');
}

/**
 * Locate the language pack root directory (containing language subdirectories,
 * e.g. <root>/中文/, <root>/俄语/)
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
    const config = loadAIConfig();
    const custom = config.systemPrompt.trim();
    if (custom) {
        return custom;
    }
    return buildSystemPrompt(currentLanguageName(), findLanguagePackRoot());
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
