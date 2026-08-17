/**
 * 单元测试：prompt-builder（TOML 解析与系统提示词生成）
 *
 * 覆盖语言包定位修复：按语言代码目录（<root>/zh/）读取，
 * 显示名来自 lang_info.toml，缺失时回退英文提示词。
 */

import * as assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { parseToml, loadLanguagePack, buildSystemPrompt } from '../ai/prompt-builder';

// ============================================================
// TOML 解析
// ============================================================

test('parseToml：引号表头与中文键值', () => {
    const tables = parseToml(`# 注释\n["声明"]\n"函数" = "fn" # 行尾注释\n"让" = "let"\n`);
    assert.equal(tables.get('声明')?.get('函数'), 'fn');
    assert.equal(tables.get('声明')?.get('让'), 'let');
});

test('parseToml：裸键表头与裸键键名', () => {
    const tables = parseToml(`[english]\nfn = "fn"\n`);
    assert.equal(tables.get('english')?.get('fn'), 'fn');
});

test('parseToml：引号内 # 不是注释', () => {
    const tables = parseToml(`["表"]\n"键" = "值#不是注释"\n`);
    assert.equal(tables.get('表')?.get('键'), '值#不是注释');
});

// ============================================================
// 语言包加载与提示词生成
// ============================================================

/** 构造临时语言包根目录 <root>/zh/（模拟仓库 lang-packs 布局） */
function 构造语言包(): string {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'i18n-rust-test-'));
    const langDir = path.join(root, 'zh');
    fs.mkdirSync(langDir);
    fs.writeFileSync(path.join(langDir, 'keywords.toml'), [
        '["声明"]',
        '"函数" = "fn"',
        '["类型"]',
        '"整数" = "i32"',
        '["宏"]',
        '"打印行" = "println"',
        ''
    ].join('\n'), 'utf8');
    fs.writeFileSync(path.join(langDir, 'stdlib.toml'), [
        '["标识符"]',
        '"字符串" = "String"',
        ''
    ].join('\n'), 'utf8');
    fs.writeFileSync(path.join(langDir, 'lang_info.toml'), [
        '["语言包"]',
        '"名称" = "中文"',
        '"扩展名" = "zh"',
        ''
    ].join('\n'), 'utf8');
    return root;
}

test('loadLanguagePack：按代码目录读取，显示名来自 lang_info', () => {
    const root = 构造语言包();
    const data = loadLanguagePack(root, 'zh');
    assert.ok(data);
    assert.equal(data.languageName, '中文');
    assert.equal(data.keywordTables.get('声明')?.get('函数'), 'fn');
    assert.equal(data.stdlibIdentifiers.get('字符串'), 'String');
});

test('loadLanguagePack：目录不存在返回 null', () => {
    const root = 构造语言包();
    assert.equal(loadLanguagePack(root, 'ru'), null);
});

test('buildSystemPrompt：真实映射进入提示词（回归：不再静默回退英文）', () => {
    const root = 构造语言包();
    const prompt = buildSystemPrompt('zh', root);
    assert.ok(prompt.includes('- 函数 = fn'), '应包含关键字映射');
    assert.ok(prompt.includes('打印行! = println!'), '宏名应自动补 !');
    assert.ok(prompt.includes('扩展名 .zh'), '扩展名应按语言代码生成');
    assert.ok(prompt.includes('「中文」'), '显示名应来自 lang_info.toml');
});

test('buildSystemPrompt：语言包缺失回退英文', () => {
    const prompt = buildSystemPrompt('ru', undefined);
    assert.ok(prompt.includes('Rust programming teaching assistant'));
});
