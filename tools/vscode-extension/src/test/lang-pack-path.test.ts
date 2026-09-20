/**
 * 单元测试：语言包目录探测（两种布局 + 向上搜索）
 */

import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { 探测语言包根, 探测语言目录, 向上探测语言目录, 语言包根相对路径们 } from '../lang-pack-path';

/** 创建临时基础目录，测试后清理 */
function 临时目录(): string {
    return fs.mkdtempSync(path.join(os.tmpdir(), 'lp-'));
}

test('根相对路径：两种约定布局已声明', () => {
    assert.deepEqual(语言包根相对路径们, ['lang-packs', path.join('crates', 'engine', 'lang-packs')]);
});

test('探测语言包根：命中 lang-packs 布局', () => {
    const 根 = 临时目录();
    try {
        fs.mkdirSync(path.join(根, 'lang-packs'), { recursive: true });
        assert.equal(探测语言包根(根), path.join(根, 'lang-packs'));
    } finally {
        fs.rmSync(根, { recursive: true, force: true });
    }
});

test('探测语言包根：命中 crates/engine/lang-packs 布局', () => {
    const 根 = 临时目录();
    try {
        fs.mkdirSync(path.join(根, 'crates', 'engine', 'lang-packs'), { recursive: true });
        assert.equal(探测语言包根(根), path.join(根, 'crates', 'engine', 'lang-packs'));
    } finally {
        fs.rmSync(根, { recursive: true, force: true });
    }
});

test('探测语言包根：两种都不存在返回 undefined', () => {
    const 根 = 临时目录();
    try {
        assert.equal(探测语言包根(根), undefined);
    } finally {
        fs.rmSync(根, { recursive: true, force: true });
    }
});

test('探测语言目录：仅当具体语言子目录真实存在才命中', () => {
    const 根 = 临时目录();
    try {
        // 根存在但 zh 子目录不存在 → 不命中
        fs.mkdirSync(path.join(根, 'lang-packs'), { recursive: true });
        assert.equal(探测语言目录(根, 'zh'), undefined);

        // 补建 zh 子目录后命中
        fs.mkdirSync(path.join(根, 'lang-packs', 'zh'), { recursive: true });
        assert.equal(探测语言目录(根, 'zh'), path.join(根, 'lang-packs', 'zh'));
    } finally {
        fs.rmSync(根, { recursive: true, force: true });
    }
});

test('向上探测：从深层目录向上命中', () => {
    const 根 = 临时目录();
    try {
        fs.mkdirSync(path.join(根, 'lang-packs', 'zh'), { recursive: true });
        const 深层 = path.join(根, 'a', 'b', 'c');
        fs.mkdirSync(深层, { recursive: true });
        assert.equal(向上探测语言目录(深层, 'zh'), path.join(根, 'lang-packs', 'zh'));
    } finally {
        fs.rmSync(根, { recursive: true, force: true });
    }
});

test('向上探测：超过 5 级仍未命中返回 undefined', () => {
    const 根 = 临时目录();
    try {
        const 深层 = path.join(根, ...Array.from({ length: 6 }, (_, i) => `d${i}`));
        fs.mkdirSync(深层, { recursive: true });
        assert.equal(向上探测语言目录(深层, 'zh'), undefined);
    } finally {
        fs.rmSync(根, { recursive: true, force: true });
    }
});