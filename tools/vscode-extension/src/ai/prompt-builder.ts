/**
 * Prompt builder
 *
 * Generates the system prompt for the current language pack
 * (e.g. Chinese, Russian, Japanese):
 * - Reads keyword mappings, type/identifier mappings, and macro names
 *   from the language pack as examples
 * - Falls back to an English prompt when no template is available
 *
 * This module does not depend on the vscode API, making it easy to test and reuse.
 */

import * as fs from 'fs';
import * as path from 'path';

/**
 * Language pack data (structured result parsed from TOML files)
 */
export interface LanguagePackData {
    /** Display name of the language pack (e.g. 中文 / 俄语 / 日本語) */
    languageName: string;
    /** All tables of keywords.toml (key = dialect term, value = English) */
    keywordTables: Map<string, Map<string, string>>;
    /** [标识符] table of stdlib.toml (types, methods, etc.) */
    stdlibIdentifiers: Map<string, string>;
}

/**
 * Lightweight TOML parser (supports this project's language pack format):
 * - Table headers: ["表名"]
 * - Key-value: "键" = "值" or bare key = "值"
 * - Comments: lines starting with #, plus trailing comments after values
 * Returns an empty table on parse failure (caller falls back to English prompt).
 */
export function parseToml(content: string): Map<string, Map<string, string>> {
    const tables = new Map<string, Map<string, string>>();
    let currentTable: Map<string, string> | undefined;

    for (const rawLine of content.split('\n')) {
        const line = stripComment(rawLine).trim();
        if (!line) {
            continue;
        }
        // Table header: ["表名"] or [bare-key-table-name]
        const headerMatch = /^\[\s*(?:"(.+?)"|([A-Za-z0-9_-]+))\s*\]$/.exec(line);
        if (headerMatch) {
            currentTable = new Map<string, string>();
            tables.set(headerMatch[1] ?? headerMatch[2], currentTable);
            continue;
        }
        // Key-value: quoted key = quoted value (project convention; non-ASCII keys must be quoted)
        const kvMatch = /^"(.+?)"\s*=\s*"(.+?)"\s*$/.exec(line);
        if (kvMatch && currentTable) {
            currentTable.set(kvMatch[1], kvMatch[2]);
            continue;
        }
        // Key-value: bare key = quoted value (compatible with hand-written ASCII keys)
        const bareMatch = /^([A-Za-z0-9_-]+)\s*=\s*"(.+?)"\s*$/.exec(line);
        if (bareMatch && currentTable) {
            currentTable.set(bareMatch[1], bareMatch[2]);
        }
        // Other lines (arrays, multi-line strings) are ignored
    }
    return tables;
}

/** Strip trailing comments (# inside quotes is not a comment) */
function stripComment(line: string): string {
    let inQuotes = false;
    for (let i = 0; i < line.length; i++) {
        const ch = line[i];
        if (ch === '"') {
            inQuotes = !inQuotes;
        } else if (ch === '#' && !inQuotes) {
            return line.slice(0, i);
        }
    }
    return line;
}

/**
 * Load language pack data for a language code (<root>/<code>/ directory,
 * e.g. <root>/zh/).
 * Required files: keywords.toml, stdlib.toml
 * (missing files still return the usable part)
 * Returns null when the directory is missing or all files are missing.
 */
export function loadLanguagePack(rootDir: string, languageCode: string): LanguagePackData | null {
    const langDir = path.join(rootDir, languageCode);
    if (!fs.existsSync(langDir)) {
        return null;
    }
    const keywordTables = new Map<string, Map<string, string>>();
    const keywordFile = path.join(langDir, 'keywords.toml');
    if (fs.existsSync(keywordFile)) {
        try {
            const parsed = parseToml(fs.readFileSync(keywordFile, 'utf8'));
            for (const [tableName, entries] of parsed) {
                keywordTables.set(tableName, entries);
            }
        } catch {
            // Treat as empty on parse failure
        }
    }
    const stdlibIdentifiers = new Map<string, string>();
    const stdlibFile = path.join(langDir, 'stdlib.toml');
    if (fs.existsSync(stdlibFile)) {
        try {
            const parsed = parseToml(fs.readFileSync(stdlibFile, 'utf8'));
            const identTable = parsed.get('标识符');
            if (identTable) {
                for (const [zh, en] of identTable) {
                    stdlibIdentifiers.set(zh, en);
                }
            }
        } catch {
            // Treat as empty on parse failure
        }
    }
    if (keywordTables.size === 0 && stdlibIdentifiers.size === 0) {
        return null;
    }
    // 显示名从 lang_info.toml 的 ["语言包"] "名称" 读取，读不到时用代码兜底
    const languageName = readLanguageName(langDir) ?? languageCode;
    return { languageName, keywordTables, stdlibIdentifiers };
}

/**
 * 读取语言包目录内 lang_info.toml 的显示名（["语言包"] "名称"），
 * 文件缺失或解析失败返回 null
 */
function readLanguageName(langDir: string): string | null {
    const infoFile = path.join(langDir, 'lang_info.toml');
    if (!fs.existsSync(infoFile)) {
        return null;
    }
    try {
        const tables = parseToml(fs.readFileSync(infoFile, 'utf8'));
        return tables.get('语言包')?.get('名称') ?? null;
    } catch {
        return null;
    }
}

/**
 * Build the system prompt:
 * - When the language pack is available, generate examples from
 *   keywords / types and identifiers / macros
 * - Otherwise fall back to the English prompt
 *
 * languageCode 为语言包目录名（如 zh / ru），extension 为源码扩展名。
 */
export function buildSystemPrompt(languageCode: string, rootDir?: string): string {
    const data = rootDir ? loadLanguagePack(rootDir, languageCode) : null;
    if (!data) {
        return fallbackEnglishPrompt();
    }
    const lines: string[] = [];

    // Keyword mappings (types and macros are separated into their own sections)
    const keywordLines: string[] = [];
    const typeLines: string[] = [];
    const macroLines: string[] = [];
    for (const [tableName, entries] of data.keywordTables) {
        for (const [zh, en] of entries) {
            if (tableName === '类型') {
                typeLines.push(`- ${zh} = ${en}`);
            } else if (tableName === '宏') {
                // Show macros with an exclamation mark (e.g. 打印行! = println!)
                macroLines.push(`- ${ensureExclamation(zh)} = ${ensureExclamation(en)}`);
            } else {
                keywordLines.push(`- ${zh} = ${en}`);
            }
        }
    }
    // Stdlib identifiers supplement the type section (first 20 as examples)
    const identifierLines = [...data.stdlibIdentifiers.entries()]
        .slice(0, 20)
        .map(([zh, en]) => `- ${zh} = ${en}`);

    lines.push(`你是一位精通 Rust 编程的教学助手，正在辅导使用「${data.languageName}」方言编写 Rust 代码（扩展名 .${languageCode}）的学生。`);
    lines.push('');
    lines.push('【方言关键字映射】');
    lines.push(...(keywordLines.length > 0 ? keywordLines : ['（无）']));
    lines.push('');
    lines.push('【类型与标准库标识符映射】');
    lines.push(...(typeLines.length > 0 || identifierLines.length > 0 ? [...typeLines, ...identifierLines] : ['（无）']));
    lines.push('');
    lines.push('【宏映射】');
    lines.push(...(macroLines.length > 0 ? macroLines : ['（无）']));
    lines.push('');
    lines.push('【回答要求】');
    lines.push(`1. 使用${data.languageName}回答用户的问题；`);
    lines.push('2. 代码示例使用方言书写，并附等价的 Rust 对照代码；');
    lines.push('3. 解释要通俗易懂，面向初学者。');
    return lines.join('\n');
}

/** English fallback prompt (used when the language pack is unavailable) */
function fallbackEnglishPrompt(): string {
    return [
        'You are a Rust programming teaching assistant helping students write Rust code in their native language dialect.',
        '',
        '【Requirements】',
        '1. Answer questions in the user\'s language when detectable, otherwise in English;',
        '2. When showing code, write examples in the user\'s language dialect and include the equivalent standard Rust code;',
        '3. Keep explanations simple and beginner-friendly.'
    ].join('\n');
}

/** Append an exclamation mark at the end (skip if already present) */
function ensureExclamation(name: string): string {
    return name.endsWith('!') ? name : `${name}!`;
}
