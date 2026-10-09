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
import { after, test } from 'node:test';
import { parseToml, loadLanguagePack, buildSystemPrompt, resolveSystemPrompt, customPromptLacksPlaceholder, buildDialectGuard } from '../ai/prompt-builder';

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

/** 本次进程创建的全部临时语言包根目录（下方 after 钩子统一清理，避免 /tmp 残留） */
const 已建临时根目录: string[] = [];

/** 构造临时语言包根目录 <root>/zh/（模拟仓库 lang-packs 布局） */
function 构造语言包(): string {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'i18n-rust-test-'));
    已建临时根目录.push(root);
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
        '["模块路径"]',
        '"标准库" = "std"',
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

// 所有用例结束后统一删除临时目录（无论通过或断言失败都会执行）
after(() => {
    for (const root of 已建临时根目录) {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('loadLanguagePack：按代码目录读取，显示名来自 lang_info', () => {
    const root = 构造语言包();
    const data = loadLanguagePack(root, 'zh');
    assert.ok(data);
    assert.equal(data.languageName, '中文');
    assert.equal(data.keywordTables.get('声明')?.get('函数'), 'fn');
    assert.equal(data.stdlibIdentifiers.get('字符串'), 'String');
    assert.equal(data.stdlibModules.get('标准库'), 'std');
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

test('buildSystemPrompt：标准库模块路径进入提示词', () => {
    const root = 构造语言包();
    const prompt = buildSystemPrompt('zh', root);
    assert.ok(prompt.includes('【标准库模块路径映射'), '应含模块路径小节');
    assert.ok(prompt.includes('- 标准库 = std'), '标准库路径映射应进入提示词');
});

test('buildSystemPrompt：标识符不再被截断为 20 条（回归）', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'i18n-rust-test-'));
    已建临时根目录.push(root);
    const langDir = path.join(root, 'zh');
    fs.mkdirSync(langDir);
    fs.writeFileSync(path.join(langDir, 'keywords.toml'), '["声明"]\n"函数" = "fn"\n', 'utf8');
    const ids = Array.from({ length: 30 }, (_unused, i) => `"标识${i}" = "Item${i}"`).join('\n');
    fs.writeFileSync(path.join(langDir, 'stdlib.toml'), `["标识符"]\n${ids}\n`, 'utf8');
    const prompt = buildSystemPrompt('zh', root);
    assert.ok(prompt.includes('- 标识29 = Item29'), '第 30 条标识符也应进入提示词（旧实现截断为 20 会漏）');
});

test('buildSystemPrompt：含方言硬约束、缺词保留英文、围栏 rust-<code>', () => {
    const root = 构造语言包();
    const prompt = buildSystemPrompt('zh', root);
    assert.ok(prompt.includes('禁止输出'), '应含禁止英文关键字硬约束');
    assert.ok(prompt.includes('rust-zh'), '代码围栏标签应使用 rust-zh');
    assert.ok(prompt.includes('切勿臆造'), '应含缺词不臆造的诚实约束');
});

test('buildSystemPrompt：语言包缺失时给出明确告警（不再静默）', () => {
    const prompt = buildSystemPrompt('ru', undefined);
    assert.ok(prompt.includes('未能加载'), '应含中文告警');
    assert.ok(prompt.includes('could not be loaded'), '应含英文告警');
    assert.ok(prompt.includes('Rust programming teaching assistant'), '仍附英文兜底提示');
});

// ============================================================
// few-shot 示例注入 + 自定义 systemPrompt 占位符
// ============================================================

test('loadLanguagePack：读入 ai-examples.md 原文', () => {
    const root = 构造语言包();
    fs.writeFileSync(path.join(root, 'zh', 'ai-examples.md'), '示例正文', 'utf8');
    const data = loadLanguagePack(root, 'zh');
    assert.equal(data?.examplesText, '示例正文');
});

test('buildSystemPrompt：存在 ai-examples.md 时注入 few-shot 小节', () => {
    const root = 构造语言包();
    fs.writeFileSync(path.join(root, 'zh', 'ai-examples.md'), '示例正文：函数 主函数() {}', 'utf8');
    const prompt = buildSystemPrompt('zh', root);
    assert.ok(prompt.includes('【方言写法示例'), '应含 few-shot 小节标题');
    assert.ok(prompt.includes('函数 主函数() {}'), 'few-shot 内容应进入提示词');
});

test('buildSystemPrompt：无 ai-examples.md 时不出现 few-shot 小节', () => {
    const root = 构造语言包();
    const prompt = buildSystemPrompt('zh', root);
    assert.ok(!prompt.includes('【方言写法示例'), '缺示例文件应不注入该小节');
});

test('resolveSystemPrompt：自定义含占位符时注入当前语言包词表', () => {
    const custom = '你是我的助手。\n{{dialect_tables}}\n请简洁作答。';
    const out = resolveSystemPrompt(custom, '【方言关键字映射】…');
    assert.ok(out.includes('你是我的助手。'), '自定义指令应保留');
    assert.ok(out.includes('【方言关键字映射】…'), '占位符应被替换为语言包提示词');
    assert.ok(!out.includes('{{dialect_tables}}'), '占位符应被替换掉');
});

test('resolveSystemPrompt：自定义不含占位符时完全覆盖', () => {
    assert.equal(resolveSystemPrompt('只用英文回答', '词表'), '只用英文回答');
});

test('resolveSystemPrompt：自定义为空时使用语言包提示词', () => {
    assert.equal(resolveSystemPrompt('   ', '词表'), '词表');
});

test('customPromptLacksPlaceholder：非空且无占位符为 true', () => {
    assert.equal(customPromptLacksPlaceholder('自定义'), true, '非空无占位符→有错配风险');
    assert.equal(customPromptLacksPlaceholder('含 {{dialect_tables}} 的自定义'), false, '含占位符→无风险');
    assert.equal(customPromptLacksPlaceholder(''), false, '空→不适用（不弹提醒）');
});

test('buildDialectGuard：只收关键字与宏、跳过类型名，并建英→方反查表', () => {
    const root = 构造语言包(); // fixture：声明(函数=fn)、类型(整数=i32)、宏(打印行=println)
    const data = loadLanguagePack(root, 'zh');
    assert.ok(data);
    const 守卫 = buildDialectGuard(data!);
    assert.ok(守卫.reservedEnglish.has('fn'), '关键字 fn 应入保留集');
    assert.ok(守卫.reservedEnglish.has('println'), '宏 println 应入保留集');
    assert.ok(!守卫.reservedEnglish.has('i32'), '类型名 i32 不应入保留集（避免误报）');
    assert.equal(守卫.englishToDialect.get('fn'), '函数');
    assert.equal(守卫.englishToDialect.get('println'), '打印行');
});
