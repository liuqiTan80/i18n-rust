/**
 * 单元测试：版本比较（更新检查纯逻辑）
 *
 * 使用 Node 内置测试运行器：npm test（先 npm run compile）
 */

import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { 解析发行版版本, 是更新版本 } from '../version-compare';

test('解析发行版版本：v 前缀与格式校验', () => {
    assert.equal(解析发行版版本('v0.7.2'), '0.7.2');
    assert.equal(解析发行版版本('0.7.2'), '0.7.2');
    assert.equal(解析发行版版本(' v1.0.0 '), '1.0.0');
    assert.equal(解析发行版版本(undefined), undefined);
    assert.equal(解析发行版版本(null), undefined);
    assert.equal(解析发行版版本(''), undefined);
    assert.equal(解析发行版版本('release-0.7'), undefined);
    assert.equal(解析发行版版本('v0.8-beta.1'), '0.8-beta.1');
});

test('是更新版本：基本比较', () => {
    assert.equal(是更新版本('0.7.2', '0.7.1'), true);
    assert.equal(是更新版本('0.7.1', '0.7.2'), false);
    assert.equal(是更新版本('0.7.1', '0.7.1'), false);
    assert.equal(是更新版本('1.0.0', '0.9.9'), true);
    assert.equal(是更新版本('0.6.9', '0.7.0'), false);
});

test('是更新版本：段数不一致按短板补齐', () => {
    assert.equal(是更新版本('0.8', '0.7.1'), true);
    assert.equal(是更新版本('0.7', '0.7.0'), false);
    assert.equal(是更新版本('0.7.1.1', '0.7.1'), true);
    assert.equal(是更新版本('0.7.1', '0.7.1.0'), false);
});

test('是更新版本：预发布后缀先剥离，不误报', () => {
    assert.equal(是更新版本('0.7.2-beta.1', '0.7.2'), false);
    assert.equal(是更新版本('0.7.3-beta', '0.7.2'), true);
});

test('是更新版本：非法输入一律 false', () => {
    assert.equal(是更新版本('abc', '0.7.1'), false);
    assert.equal(是更新版本('0.7.x', '0.7.1'), false);
    assert.equal(是更新版本('', '0.7.1'), false);
    assert.equal(是更新版本('0.7.2', ''), false);
});
