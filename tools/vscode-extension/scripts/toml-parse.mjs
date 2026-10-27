/**
 * 轻量 TOML 解析（生成脚本共享）
 *
 * 与扩展内 prompt-builder / gen-grammars 同规则：仅处理
 *   ["表名"] 表头 / 裸 [表名] 表头，以及 "键" = "值" / 裸键 = "值"，
 * 其余行（数组、多行字符串、数字/布尔值等）忽略。返回 Map<表名, Map<键, 值>>。
 *
 * 抽成单一模块供 gen-grammars / gen-snippets / gen-ui-strings 复用，
 * 避免三处解析逻辑漂移（同一份 keywords.toml/ui.toml 必须被一致解读）。
 */

/** 去掉行尾注释（引号内的 # 不是注释） */
export function stripComment(line) {
    let inQuotes = false;
    for (let i = 0; i < line.length; i++) {
        const ch = line[i];
        if (ch === '"') { inQuotes = !inQuotes; }
        else if (ch === '#' && !inQuotes) { return line.slice(0, i); }
    }
    return line;
}

/**
 * 解析 TOML 为「表名 → (键 → 值)」的嵌套 Map。
 * 无表头的裸键（文件起始）归入空字符串表 ''。
 */
export function parseToml(content) {
    const tables = new Map();
    let current;
    for (const rawLine of content.split('\n')) {
        const line = stripComment(rawLine).trim();
        if (!line) { continue; }
        const header = /^\[\s*(?:"(.+?)"|([A-Za-z0-9_-]+))\s*\]$/.exec(line);
        if (header) {
            current = new Map();
            tables.set(header[1] ?? header[2], current);
            continue;
        }
        const kv = /^(?:"(.+?)"|([A-Za-z0-9_-]+))\s*=\s*"(.+?)"\s*$/.exec(line);
        if (kv && current) {
            current.set(kv[1] ?? kv[2], kv[3]);
        }
    }
    return tables;
}
