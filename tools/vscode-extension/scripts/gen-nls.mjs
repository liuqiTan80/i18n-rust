/**
 * 扩展 contributes 文案 nls 生成器（VS Code 原生 %placeholder% 机制）
 *
 * 单一真值：待译文案存于 crates/engine/lang-packs/<lang>/ui.toml 的 ["界面消息"] 段
 * 的 ext_* 键（11 语言）。本脚本以 package.json 中出现的 %ext_…% 占位符为准，
 * 从各语言 ui.toml 取同名词条，产出：
 *   - package.nls.json          默认英文（VS Code 无匹配显示语言时回落此文件）
 *   - package.nls.<loc>.json     各 locale（按 VS Code 显示语言后缀命名）
 * 从而让非中文用户在命令面板 / 设置界面 / 语言别名处看到本地化文案，而非硬编码中文。
 *
 * locale 映射（pack 码 → VS Code nls 后缀）：
 *   en→默认 package.nls.json（不出 package.nls.en.json，避免与默认重复）
 *   zh→zh-cn  pt→pt-br  ar/de/es/fr/ja/ko/ru→同名  hi→hi
 * 取舍：zh-tw 不发（避免向繁体用户显示简体），自动回落默认英文；
 *       hi 虽生成 package.nls.hi.json，但 VS Code 无 Hindi 显示语言，实际回落默认英文。
 *
 * 自校验（让生成器自带正确性，缺失/多余即非零退出）：
 *   ① package.json 每个 %key% 必须在每种语言的生成物中出现；
 *   ② 每个生成物的键集合必须恰等于 package.json 引用的键集合（无孤儿、无遗漏）；
 *   ③ 各语言 ui.toml 缺键即报错；
 *   ④ 每个 JSON 产出后可被解析。
 *
 * 用法：node scripts/gen-nls.mjs（npm run gen:nls，并入 npm run gen:all）
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseToml } from './toml-parse.mjs';

const 脚本目录 = path.dirname(fileURLToPath(import.meta.url));
const 扩展目录 = path.resolve(脚本目录, '..');
const 仓库根 = path.resolve(扩展目录, '..', '..');
const 语言包根 = path.join(仓库根, 'crates', 'engine', 'lang-packs');
const package路径 = path.join(扩展目录, 'package.json');

/** 支持的语言码（与 lang-packs 目录一致）。 */
const 语言列表 = ['ar', 'de', 'en', 'es', 'fr', 'hi', 'ja', 'ko', 'pt', 'ru', 'zh'];

/** pack 码 → VS Code nls 文件后缀；en 走默认 package.nls.json，故不列于此表。 */
const LOCALE后缀 = {
    ar: 'ar', de: 'de', es: 'es', fr: 'fr', ja: 'ja',
    ko: 'ko', ru: 'ru', zh: 'zh-cn', pt: 'pt-br', hi: 'hi',
};

/** 默认（回落）语言：英文，写入 package.nls.json。 */
const 默认语言 = 'en';

function 失败(msg) {
    console.error(`gen-nls: ${msg}`);
    process.exit(1);
}

/** 从 package.json 扫描全部 %ext_…% 占位符（去重、排序，保证生成物确定性）。 */
function 读取引用键() {
    const 原文 = fs.readFileSync(package路径, 'utf8');
    const 匹配 = 原文.matchAll(/%([a-z0-9_]+)%/g);
    const 键集 = new Set();
    for (const m of 匹配) {
        if (m[1].startsWith('ext_')) { 键集.add(m[1]); }
    }
    return [...键集].sort();
}

/** 读取某语言 ui.toml ["界面消息"] 的指定键，缺键即退出。 */
function 读取语言文案(语言, 键列表) {
    const ui文件 = path.join(语言包根, 语言, 'ui.toml');
    if (!fs.existsSync(ui文件)) { 失败(`${语言}/ui.toml 不存在`); }
    const 界面 = parseToml(fs.readFileSync(ui文件, 'utf8')).get('界面消息');
    if (!界面) { 失败(`${语言}/ui.toml 缺少 ["界面消息"] 段`); }
    const 文案 = {};
    for (const key of 键列表) {
        const val = 界面.get(key);
        if (val === undefined) { 失败(`${语言}/ui.toml 缺少键：${key}`); }
        if (typeof val !== 'string' || val.trim() === '') { 失败(`${语言}/${key} 值为空`); }
        文案[key] = val;
    }
    return 文案;
}

/** 生成并落盘一份 nls JSON；内含双向自校验。 */
function 写nls(文件名, 文案, 键集) {
    const 路径 = path.join(扩展目录, 文件名);
    const 产出键 = Object.keys(文案).sort();
    // ② 键集合须恰等于引用集合（无孤儿、无遗漏）
    if (产出键.length !== 键集.length || 产出键.some((k, i) => k !== 键集[i])) {
        失败(`${文件名} 键集合与 package.json 引用不一致`);
    }
    const json = JSON.stringify(文案, null, 2) + '\n';
    JSON.parse(json); // ④ 可解析
    fs.writeFileSync(路径, json, 'utf8');
    return 路径;
}

function 主流程() {
    const 键列表 = 读取引用键();
    if (键列表.length === 0) { 失败('package.json 未发现任何 %ext_…% 占位符'); }
    const 键集 = 键列表; // 已排序

    const 生成文件 = [];
    // 默认 package.nls.json（英文）
    生成文件.push(写nls('package.nls.json', 读取语言文案(默认语言, 键列表), 键集));
    // 各 locale（en 已作默认，跳过）
    for (const 语言 of 语言列表) {
        if (语言 === 默认语言) { continue; }
        const 后缀 = LOCALE后缀[语言];
        if (!后缀) { 失败(`语言 ${语言} 无 locale 后缀映射`); }
        生成文件.push(写nls(`package.nls.${后缀}.json`, 读取语言文案(语言, 键列表), 键集));
    }

    // ① 复核：每个 %key% 均已进入默认文件（等价于全部，因各文件同键集）
    const 默认 = JSON.parse(fs.readFileSync(path.join(扩展目录, 'package.nls.json'), 'utf8'));
    for (const key of 键集) {
        if (默认[key] === undefined) { 失败(`默认 nls 缺键：${key}`); }
    }

    const 相对 = 生成文件.map((p) => path.relative(扩展目录, p));
    console.log(`已生成 ${生成文件.length} 份 nls 文件，每份 ${键集.length} 键：`);
    for (const r of 相对) { console.log(`  - ${r}`); }
}

主流程();
