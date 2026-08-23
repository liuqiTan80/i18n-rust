/**
 * 方言语言注册表（单一数据源）
 *
 * 与引擎支持的 10 种语言包一一对应（lang-packs/<code>/）。
 * package.json 的语言注册、when 条件与运行时逻辑均以本表为准；
 * 新增语言时同步更新本表、package.json 并重新运行 npm run gen:grammars。
 */

/**
 * 单个方言语言的元信息
 */
export interface 方言语言 {
    /** 语言包代码（= lang-packs 目录名 = 源码文件扩展名），如 zh */
    code: string;
    /** VS Code 语言 ID，如 rust-zh */
    languageId: string;
    /** 源码文件扩展名（不含点），如 zh */
    extension: string;
    /** 语言包自身语言的显示名（如 中文 / English / 日本語） */
    displayName: string;
}

/**
 * 全部 10 种受支持的方言语言（顺序即菜单展示顺序）
 */
export const 方言语言表: readonly 方言语言[] = [
    { code: 'zh', languageId: 'rust-zh', extension: 'zh', displayName: '中文' },
    { code: 'ja', languageId: 'rust-ja', extension: 'ja', displayName: '日本語' },
    { code: 'de', languageId: 'rust-de', extension: 'de', displayName: 'Deutsch' },
    { code: 'es', languageId: 'rust-es', extension: 'es', displayName: 'Español' },
    { code: 'fr', languageId: 'rust-fr', extension: 'fr', displayName: 'Français' },
    { code: 'pt', languageId: 'rust-pt', extension: 'pt', displayName: 'Português' },
    { code: 'ru', languageId: 'rust-ru', extension: 'ru', displayName: 'Русский' },
    { code: 'ko', languageId: 'rust-ko', extension: 'ko', displayName: '한국어' },
    { code: 'hi', languageId: 'rust-hi', extension: 'hi', displayName: 'हिन्दी' },
    { code: 'ar', languageId: 'rust-ar', extension: 'ar', displayName: 'العربية' }
];

/**
 * 所有方言语言 ID（用于文档 languageId 判断）
 */
export const 方言语言Id: readonly string[] = 方言语言表.map(语言 => 语言.languageId);

/**
 * 显示名 → 语言信息（语言包选择器回写配置后用于解析代码）
 */
const 显示名索引: ReadonlyMap<string, 方言语言> = new Map(
    方言语言表.map(语言 => [语言.displayName, 语言])
);

/**
 * 代码 → 语言信息
 */
const 代码索引: ReadonlyMap<string, 方言语言> = new Map(
    方言语言表.map(语言 => [语言.code, 语言])
);

/**
 * 按显示名查找语言（找不到返回 undefined）
 */
export function 按显示名查找(显示名: string): 方言语言 | undefined {
    return 显示名索引.get(显示名);
}

/**
 * 按代码查找语言（找不到返回 undefined）
 */
export function 按代码查找(code: string): 方言语言 | undefined {
    return 代码索引.get(code);
}

/**
 * 当前语言包配置值（显示名）对应的语言代码；
 * 非法配置值回退为 zh，保证下游永不拿到空代码。
 */
export function 语言代码(配置显示名: string): string {
    return 按显示名查找(配置显示名)?.code ?? 'zh';
}
