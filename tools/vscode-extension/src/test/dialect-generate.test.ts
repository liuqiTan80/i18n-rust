/**
 * dialect-generate.ts 单测：代码抽取 + 生成→校验→回喂闭环（依赖注入假件）
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
    extractDialectCode,
    buildFixFeedback,
    runDialectGenerate,
    findEnglishKeywordTokens,
    DialectValidation
} from '../ai/dialect-generate';
import { ChatMessage } from '../ai/types';

// ============================================================
// extractDialectCode
// ============================================================

test('extractDialectCode：抽取 ```rust-zh 围栏内代码', () => {
    const text = '前言\n```rust-zh\n函数 main() {\n}\n```\n结语';
    const code = extractDialectCode(text);
    assert.ok(code.includes('函数 main()'));
    assert.ok(!code.includes('前言'));
    assert.ok(!code.includes('```'));
});

test('extractDialectCode：多个代码块以空行拼接', () => {
    const text = '```rust\nfn a()\n```\n中间说明\n```rust\nfn b()\n```';
    const code = extractDialectCode(text);
    assert.ok(code.includes('fn a()'));
    assert.ok(code.includes('fn b()'));
    assert.ok(!code.includes('中间说明'));
});

test('extractDialectCode：无围栏时回退整段文本', () => {
    const code = extractDialectCode('   这是一段纯代码没有围栏   ');
    assert.equal(code, '这是一段纯代码没有围栏\n');
});

test('extractDialectCode：空输入返回空串', () => {
    assert.equal(extractDialectCode('   \n  '), '');
});

// ============================================================
// buildFixFeedback
// ============================================================

test('buildFixFeedback：包含报错原文与只输出方言代码的要求', () => {
    const fb = buildFixFeedback('error: 未定义符号 fn');
    assert.ok(fb.includes('error: 未定义符号 fn'));
    assert.ok(fb.includes('rzc check'));
    assert.ok(fb.includes('禁止英文原形'));
    assert.ok(fb.includes('切勿臆造'));
});

// ============================================================
// runDialectGenerate：闭环编排
// ============================================================

/** 记录每次 chat 收到的消息，返回可编排的假 chat */
function 假chat(回复序列: string[]): { chat: (m: ChatMessage[], s?: AbortSignal) => Promise<string>; 调用记录: ChatMessage[][] } {
    const 调用记录: ChatMessage[][] = [];
    let 第几次 = 0;
    return {
        调用记录,
        chat: async (m) => {
            调用记录.push(m.map(x => ({ ...x })));
            const 回复 = 回复序列[Math.min(第几次, 回复序列.length - 1)];
            第几次++;
            return 回复;
        }
    };
}

test('runDialectGenerate：首轮即通过，不触发回喂', async () => {
    const { chat, 调用记录 } = 假chat(['```rust\nfn ok()\n```']);
    const validate = async (): Promise<DialectValidation> => ({ ok: true, report: '' });
    const 结果 = await runDialectGenerate({
        chat, validate, systemPrompt: 'SYS', userRequest: '需求', maxFixRounds: 2
    });
    assert.equal(结果.ok, true);
    assert.equal(结果.rounds, 0);
    assert.equal(调用记录.length, 1);
    assert.ok(结果.code.includes('fn ok()'));
});

test('runDialectGenerate：失败一次后回喂诊断并修正成功', async () => {
    const { chat, 调用记录 } = 假chat([
        '```rust\nfn 英文fn()\n```',   // 首轮（有英文关键字）
        '```rust\n函数 fn()\n```'          // 修正轮
    ]);
    let 校验次数 = 0;
    const validate = async (): Promise<DialectValidation> => {
        校验次数++;
        return 校验次数 === 1 ? { ok: false, report: 'error: 期望方言词' } : { ok: true, report: '' };
    };
    const 结果 = await runDialectGenerate({
        chat, validate, systemPrompt: 'SYS', userRequest: '需求', maxFixRounds: 2
    });
    assert.equal(结果.ok, true);
    assert.equal(结果.rounds, 1);
    assert.equal(调用记录.length, 2);
    // 第二次 chat 的消息里应带上含报错原文的回喂用户消息
    const 第二轮消息 = 调用记录[1];
    const 回喂 = 第二轮消息.find(m => m.role === 'user' && m.content.includes('error: 期望方言词'));
    assert.ok(回喂, '应把 rzc check 报错回喂给模型');
    assert.ok(结果.code.includes('函数 fn()'));
});

test('runDialectGenerate：始终失败时按上限收敛，不无限循环', async () => {
    const { chat, 调用记录 } = 假chat(['```rust\n坏代码\n```']);
    const validate = async (): Promise<DialectValidation> => ({ ok: false, report: '一直错' });
    const 结果 = await runDialectGenerate({
        chat, validate, systemPrompt: 'SYS', userRequest: '需求', maxFixRounds: 2
    });
    assert.equal(结果.ok, false);
    assert.equal(结果.rounds, 2);
    // 1 次生成 + 2 次修正 = 3 次 chat，有界
    assert.equal(调用记录.length, 3);
});

test('runDialectGenerate：解析不到代码块时校验判失败并回喂', async () => {
    const { chat } = 假chat(['抱歉，我无法生成。']);
    // 复刻生产校验：空/无代码块时 ok=false
    const validate = async (code: string): Promise<DialectValidation> =>
        code.trim().length > 0 && code.includes('函数') ? { ok: true, report: '' } : { ok: false, report: '未解析出代码块' };
    const 结果 = await runDialectGenerate({
        chat, validate, systemPrompt: 'SYS', userRequest: '需求', maxFixRounds: 1
    });
    assert.equal(结果.ok, false);
});

test('runDialectGenerate：已取消时不进入修正循环', async () => {
    const 中止器 = new AbortController();
    中止器.abort();
    const { chat, 调用记录 } = 假chat(['```rust\nx\n```']);
    const validate = async (): Promise<DialectValidation> => ({ ok: false, report: 'err' });
    const 结果 = await runDialectGenerate({
        chat, validate, systemPrompt: 'SYS', userRequest: '需求', maxFixRounds: 2, signal: 中止器.signal
    });
    assert.equal(结果.ok, false);
    assert.equal(结果.rounds, 0);
    assert.equal(调用记录.length, 1); // 只有首轮生成
});

// ============================================================
// findEnglishKeywordTokens：方言纯度闸
// ============================================================

test('findEnglishKeywordTokens：命中代码区的英文关键字/宏', () => {
    const 保留 = new Set(['fn', 'let', 'println', 'mut']);
    const hits = findEnglishKeywordTokens('fn main() { let x = 1; println!("hi"); }', 保留);
    assert.deepEqual(hits, ['fn', 'let', 'println']);
    assert.ok(!hits.includes('main'), '非保留词不应被拦');
});

test('findEnglishKeywordTokens：屏蔽字符串与注释中的英文', () => {
    const 保留 = new Set(['fn', 'println']);
    const code = '// fn 在注释里\nlet s = "println fn 在字符串里";\n';
    assert.deepEqual(findEnglishKeywordTokens(code, 保留), [], '字符串/注释内的英文词不应被误判');
});

test('findEnglishKeywordTokens：保留集为空时直接返回空', () => {
    assert.deepEqual(findEnglishKeywordTokens('fn main() {}', new Set()), []);
});
