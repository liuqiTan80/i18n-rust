/**
 * 提示词构建
 *
 * 根据当前语言包（如中文、俄语、日语）生成对应的系统提示词：
 * - 读取语言包的关键字映射、类型/标识符映射、宏名映射作为示例
 * - 语言包未提供模板或无法读取时，回退到英文提示词
 *
 * 本模块不依赖 vscode API，便于单元测试与复用。
 */

import * as fs from 'fs';
import * as path from 'path';

/**
 * 语言包数据（从 TOML 文件解析后的结构化结果）
 */
export interface 语言包数据 {
    /** 语言包显示名称（如 中文 / 俄语 / 日本語） */
    语言名: string;
    /** 关键字.toml 的全部表（键 = 中文，值 = 英文） */
    关键字表: Map<string, Map<string, string>>;
    /** 标准库.toml [标识符] 表（类型、方法等） */
    标准库标识符: Map<string, string>;
}

/**
 * 轻量 TOML 解析器（支持本项目语言包格式）：
 * - 表头：["表名"]
 * - 键值："键" = "值" 或 裸键 = "值"
 * - 注释：# 开头的行，以及值后的行尾注释
 * 解析失败时返回空表（调用方回退英文提示词）。
 */
export function 解析TOML(内容: string): Map<string, Map<string, string>> {
    const 表们 = new Map<string, Map<string, string>>();
    let 当前表: Map<string, string> | undefined;

    for (const 原始行 of 内容.split('\n')) {
        const 行 = 去除注释(原始行).trim();
        if (!行) {
            continue;
        }
        // 表头：["表名"] 或 [裸键表名]
        const 表头匹配 = /^\[\s*(?:"(.+?)"|([A-Za-z0-9_-]+))\s*\]$/.exec(行);
        if (表头匹配) {
            当前表 = new Map<string, string>();
            表们.set(表头匹配[1] ?? 表头匹配[2], 当前表);
            continue;
        }
        // 键值：双引号键 = 双引号值（项目惯例，中文键必须加引号）
        const 键值匹配 = /^"(.+?)"\s*=\s*"(.+?)"\s*$/.exec(行);
        if (键值匹配 && 当前表) {
            当前表.set(键值匹配[1], 键值匹配[2]);
            continue;
        }
        // 键值：裸键 = 双引号值（兼容用户手写的 ASCII 键）
        const 裸键匹配 = /^([A-Za-z0-9_-]+)\s*=\s*"(.+?)"\s*$/.exec(行);
        if (裸键匹配 && 当前表) {
            当前表.set(裸键匹配[1], 裸键匹配[2]);
        }
        // 其余行（如数组、多行字符串）忽略
    }
    return 表们;
}

/** 去掉行内注释（# 前的内容保留，引号内的 # 不视为注释） */
function 去除注释(行: string): string {
    let 在引号内 = false;
    for (let i = 0; i < 行.length; i++) {
        const 字符 = 行[i];
        if (字符 === '"') {
            在引号内 = !在引号内;
        } else if (字符 === '#' && !在引号内) {
            return 行.slice(0, i);
        }
    }
    return 行;
}

/**
 * 读取指定语言的语言包数据（<语言包根>/<语言名>/ 目录）
 * 必需文件：关键字.toml、标准库.toml（缺少时仍返回可用的部分）
 * 目录不存在或全部文件缺失时返回 null。
 */
export function 读取语言包(语言包根目录: string, 语言名: string): 语言包数据 | null {
    const 语言目录 = path.join(语言包根目录, 语言名);
    if (!fs.existsSync(语言目录)) {
        return null;
    }
    const 关键字表 = new Map<string, Map<string, string>>();
    const 关键字文件 = path.join(语言目录, '关键字.toml');
    if (fs.existsSync(关键字文件)) {
        try {
            const 解析结果 = 解析TOML(fs.readFileSync(关键字文件, 'utf8'));
            for (const [表名, 键值] of 解析结果) {
                关键字表.set(表名, 键值);
            }
        } catch {
            // 解析失败时按空处理
        }
    }
    const 标准库标识符 = new Map<string, string>();
    const 标准库文件 = path.join(语言目录, '标准库.toml');
    if (fs.existsSync(标准库文件)) {
        try {
            const 解析结果 = 解析TOML(fs.readFileSync(标准库文件, 'utf8'));
            const 标识符表 = 解析结果.get('标识符');
            if (标识符表) {
                for (const [中文, 英文] of 标识符表) {
                    标准库标识符.set(中文, 英文);
                }
            }
        } catch {
            // 解析失败时按空处理
        }
    }
    if (关键字表.size === 0 && 标准库标识符.size === 0) {
        return null;
    }
    return { 语言名, 关键字表, 标准库标识符 };
}

/**
 * 构建系统提示词：
 * - 语言包可用时，按 关键字 / 类型与标识符 / 宏 三部分生成示例
 * - 语言包不可用时回退英文提示词
 */
export function 构建系统提示词(语言名: string, 语言包根目录?: string): string {
    const 数据 = 语言包根目录 ? 读取语言包(语言包根目录, 语言名) : null;
    if (!数据) {
        return 英文回退提示词();
    }
    const 行们: string[] = [];

    // 关键字映射（排除类型与宏，单独成节）
    const 关键字行: string[] = [];
    const 类型行: string[] = [];
    const 宏行: string[] = [];
    for (const [表名, 键值] of 数据.关键字表) {
        for (const [中文, 英文] of 键值) {
            if (表名 === '类型') {
                类型行.push(`- ${中文} = ${英文}`);
            } else if (表名 === '宏') {
                // 宏名展示时补充感叹号（如 打印行! = println!），更符合调用形态
                宏行.push(`- ${补叹号(中文)} = ${补叹号(英文)}`);
            } else {
                关键字行.push(`- ${中文} = ${英文}`);
            }
        }
    }
    // 标准库标识符补充类型节（数量较多时取前 20 条示意）
    const 标识符行 = [...数据.标准库标识符.entries()].slice(0, 20).map(([中文, 英文]) => `- ${中文} = ${英文}`);

    行们.push(`你是一位精通 Rust 编程的教学助手，正在辅导使用「${数据.语言名}」方言编写 Rust 代码（扩展名 ${数据.语言名 === '中文' ? '.zh' : '对应语言包扩展名'}）的学生。`);
    行们.push('');
    行们.push('【方言关键字映射】');
    行们.push(...(关键字行.length > 0 ? 关键字行 : ['（无）']));
    行们.push('');
    行们.push('【类型与标准库标识符映射】');
    行们.push(...(类型行.length > 0 || 标识符行.length > 0 ? [...类型行, ...标识符行] : ['（无）']));
    行们.push('');
    行们.push('【宏映射】');
    行们.push(...(宏行.length > 0 ? 宏行 : ['（无）']));
    行们.push('');
    行们.push('【回答要求】');
    行们.push(`1. 使用${数据.语言名}回答用户的问题；`);
    行们.push('2. 代码示例使用方言书写，并附等价的 Rust 对照代码；');
    行们.push('3. 解释要通俗易懂，面向初学者。');
    return 行们.join('\n');
}

/** 英文回退提示词（语言包不可用或未提供模板时使用） */
function 英文回退提示词(): string {
    return [
        'You are a Rust programming teaching assistant helping students write Rust code in their native language dialect.',
        '',
        '【Requirements】',
        '1. Answer questions in the user\'s language when detectable, otherwise in English;',
        '2. When showing code, write examples in the user\'s language dialect and include the equivalent standard Rust code;',
        '3. Keep explanations simple and beginner-friendly.'
    ].join('\n');
}

/** 末尾补感叹号（已带时不重复） */
function 补叹号(名称: string): string {
    return 名称.endsWith('!') ? 名称 : `${名称}!`;
}
