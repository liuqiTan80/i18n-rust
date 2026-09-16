/**
 * 单元测试：转译产物隐藏规则（files.exclude 计算与一致性判断）
 *
 * 使用 Node 内置测试运行器：npm test（先 npm run compile）
 */

import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { 产物匹配模式们, 计算排除项, 排除项已一致 } from '../artifact-rules';

// ============================================================
// 计算排除项
// ============================================================

test('计算排除项：开启时写入两个产物模式并保留其它条目', () => {
    const 结果 = 计算排除项({ '**/.git': true, '自定义/**': true }, true);
    assert.deepEqual(结果, {
        '**/.git': true,
        '自定义/**': true,
        '**/*.rs': true,
        '**/*.rs.bak': true
    });
});

test('计算排除项：关闭时只移除产物模式，保留其它条目', () => {
    const 结果 = 计算排除项({ '**/.git': true, '**/*.rs': true, '**/*.rs.bak': true }, false);
    assert.deepEqual(结果, { '**/.git': true });
});

test('计算排除项：不修改传入对象（无副作用）', () => {
    const 现有 = { '**/*.rs': true };
    计算排除项(现有, false);
    assert.deepEqual(现有, { '**/*.rs': true });
});

test('计算排除项：空输入', () => {
    assert.deepEqual(计算排除项(undefined, true), {
        '**/*.rs': true,
        '**/*.rs.bak': true
    });
    assert.deepEqual(计算排除项(undefined, false), {});
});

test('计算排除项：重复应用幂等', () => {
    const 一次 = 计算排除项({ '**/.git': true }, true);
    const 两次 = 计算排除项(一次, true);
    assert.deepEqual(两次, 一次);
    const 关一次 = 计算排除项(两次, false);
    const 关两次 = 计算排除项(关一次, false);
    assert.deepEqual(关两次, { '**/.git': true });
});

// ============================================================
// 排除项已一致
// ============================================================

test('排除项已一致：开启判定', () => {
    assert.equal(排除项已一致({ '**/*.rs': true, '**/*.rs.bak': true }, true), true);
    assert.equal(排除项已一致({ '**/*.rs': true, '**/*.rs.bak': true, x: true }, true), true);
    assert.equal(排除项已一致({ '**/*.rs': true }, true), false);
    assert.equal(排除项已一致({ '**/*.rs': false, '**/*.rs.bak': true }, true), false);
    assert.equal(排除项已一致(undefined, true), false);
});

test('排除项已一致：关闭判定', () => {
    assert.equal(排除项已一致({ '**/.git': true }, false), true);
    assert.equal(排除项已一致(undefined, false), true);
    assert.equal(排除项已一致({ '**/*.rs': true }, false), false);
    assert.equal(排除项已一致({ '**/*.rs.bak': false }, false), false);
});

test('产物匹配模式：覆盖 .rs 与 .rs.bak', () => {
    assert.deepEqual(产物匹配模式们, ['**/*.rs', '**/*.rs.bak']);
});
