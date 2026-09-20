/**
 * 单元测试：LSP 自动重启时间窗口判定
 */

import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { 更新退出时间戳, 重启窗口毫秒, 重启窗口内上限 } from '../restart-window';

test('空历史：记录本次退出，次数为 1', () => {
    const 现在 = 1_000_000;
    const { 修剪后, 次数 } = 更新退出时间戳([], 现在);
    assert.deepEqual(修剪后, [现在]);
    assert.equal(次数, 1);
});

test('窗口内累计：次数递增且旧记录保留', () => {
    const 现在 = 1_000_000;
    const 历史 = [现在 - 1000, 现在 - 2000];
    const { 修剪后, 次数 } = 更新退出时间戳(历史, 现在);
    assert.equal(次数, 3);
    assert.equal(修剪后.length, 3);
    assert.equal(修剪后[修剪后.length - 1], 现在);
});

test('窗口边界：恰好等于窗口时长的旧记录被裁剪', () => {
    const 现在 = 1_000_000;
    const 边界旧 = 现在 - 重启窗口毫秒;
    const { 修剪后, 次数 } = 更新退出时间戳([边界旧], 现在);
    // 时间差 == 窗口毫秒，不满足 < 窗口毫秒，应被过滤
    assert.equal(次数, 1);
    assert.deepEqual(修剪后, [现在]);
});

test('窗口外：跨周月的偶发崩溃自动过期', () => {
    const 现在 = Date.now();
    const 一周前 = 现在 - 7 * 24 * 60 * 60 * 1000;
    const { 修剪后, 次数 } = 更新退出时间戳([一周前, 一周前 + 1, 现在 - 100], 现在);
    // 两个一周前的记录被裁剪，仅保留 现在-100 与本次
    assert.equal(次数, 2);
    assert.equal(修剪后[0], 现在 - 100);
});

test('达上限：窗口内次数等于上限时不再累加判定由调用方处理', () => {
    const 现在 = Date.now();
    const 历史 = Array.from({ length: 重启窗口内上限 - 1 }, (_, i) => 现在 - (i + 1) * 1000);
    const { 修剪后, 次数 } = 更新退出时间戳(历史, 现在);
    assert.equal(次数, 重启窗口内上限);
    assert.equal(修剪后.length, 重启窗口内上限);
});