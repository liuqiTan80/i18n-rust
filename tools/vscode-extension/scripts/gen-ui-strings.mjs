/**
 * 扩展界面文案生成器
 *
 * 从 crates/engine/lang-packs/zh/ui.toml 的 ["界面消息"] 段读取扩展所需的
 * ext_* 键，生成 src/generated/ui-strings.ts，供 extension.ts 取用。
 * 语言包文案更新后重跑即刷新——扩展 UI 文案与引擎消息表单一真值，不再内联硬编码。
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
const ui文件 = path.join(仓库根, 'crates', 'engine', 'lang-packs', 'zh', 'ui.toml');
const 输出路径 = path.join(扩展目录, 'src', 'generated', 'ui-strings.ts');

/** 扩展消费的 ui.toml ["界面消息"] 键（键名沿用 ui.toml，不另立词表） */
const 消费的键 = [
    'ext_need_dialect_file',
    'ext_lsp_restarted',
    'ext_explain_need_file',
    'ext_explain_no_diag',
    'ext_explain_no_diag_nearby',
    'ext_add_dep_no_workspace',
];

function 主流程() {
    const 界面 = parseToml(fs.readFileSync(ui文件, 'utf8')).get('界面消息');
    if (!界面) {
        console.error('ui.toml 缺少 ["界面消息"] 段');
        process.exit(1);
    }
    const 行 = [
        '/* eslint-disable */',
        '/**',
        ' * 自动生成——请勿手改。',
        ' * 数据源：crates/engine/lang-packs/zh/ui.toml ["界面消息"] 的 ext_* 键。',
        ' * 刷新：npm run gen:all（scripts/gen-ui-strings.mjs）。',
        ' */',
        '',
        'export const 界面文案: Readonly<Record<string, string>> = {',
    ];
    for (const key of 消费的键) {
        const val = 界面.get(key);
        if (val === undefined) {
            console.error(`ui.toml 缺少键：${key}`);
            process.exit(1);
        }
        行.push(`    ${key}: ${JSON.stringify(val)},`);
    }
    行.push('};', '');

    fs.mkdirSync(path.dirname(输出路径), { recursive: true });
    fs.writeFileSync(输出路径, 行.join('\n'), 'utf8');
    console.log(`已生成 ${path.relative(扩展目录, 输出路径)}（${消费的键.length} 条文案）`);
}

主流程();
