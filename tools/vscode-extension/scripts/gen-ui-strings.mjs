/**
 * 扩展界面文案生成器
 *
 * 从 crates/engine/lang-packs/<lang>/ui.toml 的 ["界面消息"] 段读取扩展所需的
 * ext_* 键，为 11 种语言各生成一份，汇总成 src/generated/ui-strings.ts 的
 * 「语言码 → (键 → 文案)」表，供 extension.ts 按 VS Code 显示语言（vscode.env.language）
 * 取用。语言包文案更新后重跑即刷新——扩展 UI 文案与引擎消息表单一真值，不再内联硬编码，
 * 也不再只出中文（此前只读 zh，非中文用户看到的是中文提示）。
 *
 * 用法：node scripts/gen-ui-strings.mjs（npm run gen:ui）
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseToml } from './toml-parse.mjs';

const 脚本目录 = path.dirname(fileURLToPath(import.meta.url));
const 扩展目录 = path.resolve(脚本目录, '..');
const 仓库根 = path.resolve(扩展目录, '..', '..');
const 语言包根 = path.join(仓库根, 'crates', 'engine', 'lang-packs');
const 输出路径 = path.join(扩展目录, 'src', 'generated', 'ui-strings.ts');

/** 支持的语言码（与 lang-packs 目录一致），按字母序输出保证生成物确定性。 */
const 语言列表 = ['ar', 'de', 'en', 'es', 'fr', 'hi', 'ja', 'ko', 'pt', 'ru', 'zh'];

/** 兜底语言：用户显示语言无对应语言包时用之（英文为通用 lingua franca）。 */
const 回退语言 = 'en';

/** 扩展消费的 ui.toml ["界面消息"] 键（键名沿用 ui.toml，不另立词表） */
const 消费的键 = [
    'ext_need_dialect_file',
    'ext_lsp_restarted',
    'ext_explain_need_file',
    'ext_explain_no_diag',
    'ext_explain_no_diag_nearby',
    'ext_add_dep_no_workspace',
    'ext_lsp_binary_missing',
    'ext_lsp_start_failed',
];

function 读取语言文案(语言) {
    const ui文件 = path.join(语言包根, 语言, 'ui.toml');
    const 界面 = parseToml(fs.readFileSync(ui文件, 'utf8')).get('界面消息');
    if (!界面) {
        console.error(`${语言}/ui.toml 缺少 ["界面消息"] 段`);
        process.exit(1);
    }
    const 文案 = {};
    for (const key of 消费的键) {
        const val = 界面.get(key);
        if (val === undefined) {
            console.error(`${语言}/ui.toml 缺少键：${key}`);
            process.exit(1);
        }
        文案[key] = val;
    }
    return 文案;
}

function 主流程() {
    const 行 = [
        '/* eslint-disable */',
        '/**',
        ' * 自动生成——请勿手改。',
        ' * 数据源：crates/engine/lang-packs/<lang>/ui.toml ["界面消息"] 的 ext_* 键（11 语言）。',
        ' * 取用：extension.ts 按 vscode.env.language 前缀匹配语言码，无匹配回退 en。',
        ' * 刷新：npm run gen:all（scripts/gen-ui-strings.mjs）。',
        ' */',
        '',
        'export const 界面文案表: Readonly<Record<string, Readonly<Record<string, string>>>> = {',
    ];
    for (const 语言 of 语言列表) {
        const 文案 = 读取语言文案(语言);
        行.push(`    ${语言}: {`);
        for (const key of 消费的键) {
            行.push(`        ${key}: ${JSON.stringify(文案[key])},`);
        }
        行.push('    },');
    }
    行.push('};', '');
    行.push(`export const 回退语言 = ${JSON.stringify(回退语言)};`, '');

    fs.mkdirSync(path.dirname(输出路径), { recursive: true });
    fs.writeFileSync(输出路径, 行.join('\n'), 'utf8');
    console.log(`已生成 ${path.relative(扩展目录, 输出路径)}（${语言列表.length} 语言 × ${消费的键.length} 条文案）`);
}

主流程();
