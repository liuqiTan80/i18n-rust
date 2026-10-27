/**
 * 方言代码片段生成器
 *
 * 以「英文关键字」为唯一真值描述片段结构，读取各语言包 keywords.toml，
 * 反向映射（英文 → 母语词）为全部 10 种方言生成 snippets/rust-<code>.json。
 * 语言包更新（关键字增删改）后重跑即刷新，方言间片段结构自动一致——
 * 消除此前 rust-zh.json 手工维护、其余 9 方言缺失的漂移。
 *
 * 用法：node scripts/gen-snippets.mjs（npm run gen:snippets）
 *
 * 说明：
 * - 触发前缀与代码体中的关键字取自当前方言 keywords.toml（英文反查母语，
 *   同英文多母语词时按「更短 → 字典序」确定，保证跨次生成幂等）；
 * - 片段描述与占位符提示为语言中立的英文元数据（生成器持有，非再造词条表）；
 * - 某关键字在该方言缺失时回落英文原名（本就是合法 Rust，片段仍可编译）。
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseToml } from './toml-parse.mjs';

const 脚本目录 = path.dirname(fileURLToPath(import.meta.url));
const 扩展目录 = path.resolve(脚本目录, '..');
const 仓库根 = path.resolve(扩展目录, '..', '..');
const 语言包根 = path.join(仓库根, 'crates', 'engine', 'lang-packs');
const 片段目录 = path.join(扩展目录, 'snippets');

/** 全部方言代码（与 gen-grammars.mjs / src/languages.ts 保持一致） */
const 代码们 = ['zh', 'ja', 'de', 'es', 'fr', 'pt', 'ru', 'ko', 'hi', 'ar'];

/**
 * 从 keywords.toml 内容构建「英文 → 母语」反向表。
 * 正向为 母语 → 英文；同一英文可能有多个母语词，取「更短 → 字典序」者，确定性。
 */
function 反向关键字表(关键字表) {
    const byEn = new Map();
    for (const 条目们 of 关键字表.values()) {
        for (const [母语, 英文] of 条目们) {
            if (母语 === 英文) { continue; } // 恒等映射（如 "if let"="if let"）跳过
            if (!byEn.has(英文)) { byEn.set(英文, []); }
            byEn.get(英文).push(母语);
        }
    }
    const reverse = new Map();
    for (const [英文, 母语们] of byEn) {
        母语们.sort((a, b) => (a.length - b.length) || (a < b ? -1 : a > b ? 1 : 0));
        reverse.set(英文, 母语们[0]);
    }
    return reverse;
}

/** VS Code 占位符：${n:hint}（hint 为语言中立英文） */
function P(n, hint) { return '${' + n + ':' + hint + '}'; }

/**
 * 片段定义：每项以英文关键字为结构骨架，渲染时替换为当前方言母语词。
 * w(英文) 返回母语词（缺失回落英文原名）。返回 VS Code 片段对象。
 */
function 构建片段(w) {
    const obj = {};
    const push = (prefix, body, description) => { obj[prefix] = { prefix, body, description }; };

    push(w('main'),
        [`${w('fn')} ${w('main')}() {`, '    $0', '}'],
        'main function entry');
    push(w('println'),
        [`${w('println')}!(${P(1, 'msg')});`],
        'print a line of text');
    push(`${w('println')}fmt`,
        [`${w('println')}!("${P(1, 'format')}", ${P(2, 'args')});`],
        'print formatted text');
    push(w('let'),
        [`${w('let')} ${P(1, 'name')} = ${P(2, 'value')};`],
        'declare an immutable binding');
    push(`${w('let')}${w('mut')}`,
        [`${w('let')} ${w('mut')} ${P(1, 'name')} = ${P(2, 'value')};`],
        'declare a mutable binding');
    push(w('if'),
        [`${w('if')} ${P(1, 'cond')} {`, '    $0', '}'],
        'if statement');
    push(`${w('if')}${w('else')}`,
        [`${w('if')} ${P(1, 'cond')} {`, '    $2', `} ${w('else')} {`, '    $0', '}'],
        'if/else statement');
    push(w('for'),
        [`${w('for')} ${P(1, 'item')} ${w('in')} ${P(2, 'iter')} {`, '    $0', '}'],
        'for loop');
    push(w('while'),
        [`${w('while')} ${P(1, 'cond')} {`, '    $0', '}'],
        'while loop');
    push(w('loop'),
        [`${w('loop')} {`, '    $0', '}'],
        'infinite loop');
    push(w('match'),
        [`${w('match')} ${P(1, 'expr')} {`, `    ${P(2, 'pattern')} => ${P(3, 'result')},`, '    $0', '}'],
        'match expression');
    push(w('struct'),
        [`${w('struct')} ${P(1, 'Name')} {`, `    ${P(2, 'field')}: ${P(3, 'Type')},`, '    $0', '}'],
        'struct definition');
    push(w('enum'),
        [`${w('enum')} ${P(1, 'Name')} {`, `    ${P(2, 'Variant')},`, '    $0', '}'],
        'enum definition');
    push(w('trait'),
        [`${w('trait')} ${P(1, 'Name')} {`, `    ${w('fn')} ${P(2, 'method')}(${P(3, 'args')}) -> ${P(4, 'Ret')};`, '    $0', '}'],
        'trait definition');
    push(w('impl'),
        [`${w('impl')} ${P(1, 'Type')} {`, `    ${w('fn')} ${P(2, 'method')}(${P(3, 'args')}) -> ${P(4, 'Ret')} {`, '        $0', '    }', '}'],
        'impl block');
    push(w('vec'),
        [`${w('vec')}![${P(1, 'elem')}]`],
        'create a vector');
    push(w('Some'), [`${w('Some')}(${P(1, 'value')})`], 'Option::Some');
    push(w('None'), [w('None')], 'Option::None');
    push(w('Ok'), [`${w('Ok')}(${P(1, 'value')})`], 'Result::Ok');
    push(w('Err'), [`${w('Err')}(${P(1, 'msg')})`], 'Result::Err');

    return obj;
}

function 主流程() {
    let 生成数 = 0;
    for (const code of 代码们) {
        const 关键字文件 = path.join(语言包根, code, 'keywords.toml');
        if (!fs.existsSync(关键字文件)) {
            console.error(`跳过 ${code}：缺少 ${关键字文件}`);
            continue;
        }
        const 关键字表 = parseToml(fs.readFileSync(关键字文件, 'utf8'));
        const reverse = 反向关键字表(关键字表);
        const w = (en) => reverse.get(en) ?? en;
        const 片段 = 构建片段(w);

        const 输出路径 = path.join(片段目录, `rust-${code}.json`);
        fs.writeFileSync(输出路径, JSON.stringify(片段, null, 2) + '\n', 'utf8');
        console.log(`已生成 ${path.relative(扩展目录, 输出路径)}（${Object.keys(片段).length} 条片段）`);
        生成数++;
    }
    if (生成数 !== 代码们.length) {
        console.error(`警告：仅生成 ${生成数}/${代码们.length} 份片段`);
        process.exit(1);
    }
}

主流程();
