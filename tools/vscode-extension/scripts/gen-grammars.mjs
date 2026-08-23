/**
 * 语法文件生成器
 *
 * 以 syntaxes/rust-zh.tmLanguage.json 为模板，读取仓库根目录
 * lang-packs/<code>/keywords.toml 与 lang_info.toml，
 * 为全部 10 种方言生成 syntaxes/rust-<code>.tmLanguage.json。
 *
 * 用法：node scripts/gen-grammars.mjs（npm run gen:grammars）
 *
 * 分类映射（keywords.toml 表 → TextMate scope）：
 *   声明                → keyword.declaration
 *   控制流              → keyword.control
 *   内存 / 不安全       → keyword.other
 *   逻辑值              → constant.language.boolean
 *   特殊值              → constant.language
 *   类型                → storage.type（数值型为 storage.type.numeric，
 *                          Some/None/Ok/Err 为 constant.other）
 *   宏                  → entity.name.function.macro（自动补 !）
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const 脚本目录 = path.dirname(fileURLToPath(import.meta.url));
const 扩展目录 = path.resolve(脚本目录, '..');
const 仓库根 = path.resolve(扩展目录, '..', '..');
const 语言包根 = path.join(仓库根, 'crates', 'engine', 'lang-packs');
const 语法目录 = path.join(扩展目录, 'syntaxes');
const 模板路径 = path.join(语法目录, 'rust-zh.tmLanguage.json');

/** 全部方言代码（与 src/languages.ts 保持一致） */
const 代码们 = ['zh', 'ja', 'de', 'es', 'fr', 'pt', 'ru', 'ko', 'hi', 'ar'];

/** 表名 → 生成目标分类 */
const 表分类映射 = {
    '声明': 'declaration',
    '控制流': 'control',
    '内存': 'other',
    '不安全': 'other',
    '逻辑值': 'boolean',
    '特殊值': 'special',
    '类型': 'type',
    '宏': 'macro'
};

/**
 * 轻量 TOML 解析（与扩展内 prompt-builder 同规则）：
 * 仅处理 ["表名"] / "键" = "值" / 裸键表头与键，忽略其余行
 */
function parseToml(content) {
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

/** 去掉行尾注释（引号内的 # 不是注释） */
function stripComment(line) {
    let inQuotes = false;
    for (let i = 0; i < line.length; i++) {
        const ch = line[i];
        if (ch === '"') { inQuotes = !inQuotes; }
        else if (ch === '#' && !inQuotes) { return line.slice(0, i); }
    }
    return line;
}

/** 正则转义 */
function 转义(文本) {
    return 文本.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** 词法上可作为关键字的词（排除 & / * / () 等符号映射；\p{M} 覆盖天城文/阿拉伯文组合符号） */
function 是关键字词(词) {
    return /^[\p{L}\p{M}\p{N}_]+(?:[\s-][\p{L}\p{M}\p{N}_]+)*$/u.test(词);
}

/** 数值型判断（i8..u128/f32/f64/isize/usize） */
function 是数值型(值) {
    return /^(?:[iu](?:8|16|32|64|128|size)|f32|f64)$/.test(值);
}

/** 构造 \b(词1|词2)\b 模式（按长度降序，长词优先匹配） */
function 词组模式(词们) {
    const 去重 = [...new Set(词们)].sort((a, b) => b.length - a.length);
    return `\\b(${去重.map(转义).join('|')})\\b`;
}

/**
 * 从语言包数据构建 关键字/类型/宏 三个 repository 条目
 */
function 构建语法节(关键字表) {
    const 分类词们 = { declaration: [], control: [], other: [], boolean: [], special: [] };
    const 数值型们 = [];
    const 常量们 = [];
    const 类型们 = [];
    const 宏们 = [];

    for (const [表名, 条目们] of 关键字表) {
        const 分类 = 表分类映射[表名];
        if (!分类) { continue; }
        for (const [词, 值] of 条目们) {
            if (!是关键字词(词)) { continue; }
            if (分类 === 'macro') {
                宏们.push(词);
            } else if (分类 === 'type') {
                if (['Some', 'None', 'Ok', 'Err'].includes(值)) {
                    常量们.push(词);
                } else if (是数值型(值)) {
                    数值型们.push(词);
                } else {
                    类型们.push(词);
                }
            } else {
                分类词们[分类].push(词);
            }
        }
    }

    const 关键字模式们 = [];
    if (分类词们.control.length > 0) {
        关键字模式们.push({ name: 'SCOPE.control.rust-zh', match: 词组模式(分类词们.control) });
    }
    if (分类词们.declaration.length > 0) {
        关键字模式们.push({ name: 'SCOPE.declaration.rust-zh', match: 词组模式(分类词们.declaration) });
    }
    if (分类词们.other.length > 0) {
        关键字模式们.push({ name: 'SCOPE.other.rust-zh', match: 词组模式(分类词们.other) });
    }
    if (分类词们.boolean.length > 0) {
        关键字模式们.push({ name: 'SCOPE.boolean.rust-zh', match: 词组模式(分类词们.boolean) });
    }
    if (分类词们.special.length > 0) {
        关键字模式们.push({ name: 'SCOPE.special.rust-zh', match: 词组模式(分类词们.special) });
    }

    const 类型模式们 = [];
    if (数值型们.length > 0) {
        类型模式们.push({ name: 'SCOPE.numeric.rust-zh', match: 词组模式(数值型们) });
    }
    if (类型们.length > 0) {
        类型模式们.push({ name: 'SCOPE.type.rust-zh', match: 词组模式(类型们) });
    }
    if (常量们.length > 0) {
        类型模式们.push({ name: 'SCOPE.const.rust-zh', match: 词组模式(常量们) });
    }

    const 宏模式们 = [];
    if (宏们.length > 0) {
        const 去重 = [...new Set(宏们)].sort((a, b) => b.length - a.length).map(转义).join('|');
        宏模式们.push({ name: 'SCOPE.macro.rust-zh', match: `\\b(${去重})!` });
    }
    return { 关键字模式们, 类型模式们, 宏模式们 };
}

/** scope 占位符 → 具体 scope 前缀 */
const SCOPE替换表 = {
    'SCOPE.control': 'keyword.control',
    'SCOPE.declaration': 'keyword.declaration',
    'SCOPE.other': 'keyword.other',
    'SCOPE.boolean': 'constant.language.boolean',
    'SCOPE.special': 'constant.language',
    'SCOPE.numeric': 'storage.type.numeric',
    'SCOPE.type': 'storage.type',
    'SCOPE.const': 'constant.other',
    'SCOPE.macro': 'entity.name.function.macro'
};

/**
 * 递归把所有字符串值中的 rust-zh 后缀与占位 scope 替换为目标语言
 */
function 替换节点(节点, code) {
    if (Array.isArray(节点)) {
        return 节点.map(项 => 替换节点(项, code));
    }
    if (节点 && typeof 节点 === 'object') {
        const 结果 = {};
        for (const [键, 值] of Object.entries(节点)) {
            结果[键] = 替换节点(值, code);
        }
        return 结果;
    }
    if (typeof 节点 === 'string') {
        let 值 = 节点;
        for (const [占位, 真实] of Object.entries(SCOPE替换表)) {
            值 = 值.split(占位).join(真实);
        }
        return 值.split('rust-zh').join(`rust-${code}`);
    }
    return 节点;
}

function main() {
    const 模板 = JSON.parse(fs.readFileSync(模板路径, 'utf8'));
    let 生成数 = 0;
    for (const code of 代码们) {
        const 语言目录 = path.join(语言包根, code);
        const 关键字文件 = path.join(语言目录, 'keywords.toml');
        const 信息文件 = path.join(语言目录, 'lang_info.toml');
        if (!fs.existsSync(关键字文件)) {
            console.error(`跳过 ${code}：缺少 ${关键字文件}`);
            continue;
        }
        const 关键字表 = parseToml(fs.readFileSync(关键字文件, 'utf8'));
        const 信息表 = fs.existsSync(信息文件)
            ? parseToml(fs.readFileSync(信息文件, 'utf8'))
            : new Map();
        const 名称 = 信息表.get('语言包')?.get('名称') ?? code;

        const { 关键字模式们, 类型模式们, 宏模式们 } = 构建语法节(关键字表);
        const 语法 = 替换节点(模板, code);
        语法.name = `Rust (${名称})`;
        语法.scopeName = `source.rust-${code}`;
        语法.repository['关键字'].patterns = 替换节点(关键字模式们, code);
        语法.repository['类型'].patterns = 替换节点(类型模式们, code);
        语法.repository['宏'].patterns = 替换节点(宏模式们, code);

        const 输出路径 = path.join(语法目录, `rust-${code}.tmLanguage.json`);
        fs.writeFileSync(输出路径, JSON.stringify(语法, null, 2) + '\n', 'utf8');
        console.log(`已生成 ${path.relative(扩展目录, 输出路径)}（${名称}，关键字 ${关键字模式们.length} 组 / 类型 ${类型模式们.length} 组 / 宏 ${宏模式们.length} 组）`);
        生成数++;
    }
    if (生成数 !== 代码们.length) {
        console.error(`警告：仅生成 ${生成数}/${代码们.length} 份语法`);
        process.exit(1);
    }
}

main();
