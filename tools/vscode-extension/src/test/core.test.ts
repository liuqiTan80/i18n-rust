/**
 * 单元测试：shell 引用 / 插入位置计算 / 语言注册表
 *
 * 使用 Node 内置测试运行器：npm test（先 npm run compile）
 */

import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { quotePosixArg, quoteWindowsArg } from '../shell';
import { 计算插入字符位置们, 扫描词法状态, 词法状态 } from '../fullwidth-convert';
import { 语言代码, 按代码查找, 方言语言表, 方言语言Id } from '../languages';

// ============================================================
// shell 引用
// ============================================================

test('POSIX 引用：普通路径加单引号', () => {
    assert.equal(quotePosixArg('/home/user/main.zh'), `'/home/user/main.zh'`);
});

test('POSIX 引用：单引号闭合-转义-重开', () => {
    assert.equal(quotePosixArg(`/tmp/a'b.zh`), `'/tmp/a'\\''b.zh'`);
});

test('POSIX 引用：反引号与 $() 失去注入能力', () => {
    // 单引号内所有元字符均为字面量，无需额外转义
    assert.equal(quotePosixArg('/tmp/`rm -rf ~`$(x).zh'), `'/tmp/\`rm -rf ~\`$(x).zh'`);
});

test('Windows 引用：双引号加倍', () => {
    assert.equal(quoteWindowsArg('C:\\a "b".zh'), '"C:\\a ""b"".zh"');
});

test('Windows 引用：反引号与 % 转义', () => {
    assert.equal(quoteWindowsArg('a`b%c'), '"a``b^%c"');
});

// ============================================================
// 插入位置计算（全角转换换行感知）
// ============================================================

test('插入位置：单行文本按列推进', () => {
    const 位置们 = 计算插入字符位置们(3, 5, 'ab，');
    assert.deepEqual(位置们, [
        { 索引: 0, 行: 3, 列: 5 },
        { 索引: 1, 行: 3, 列: 6 },
        { 索引: 2, 行: 3, 列: 7 }
    ]);
});

test('插入位置：多行文本换行后行号递增、列归零', () => {
    const 位置们 = 计算插入字符位置们(0, 2, 'a，\n。b');
    // '，' 在第 0 行第 3 列；'。' 换行后在第 1 行第 0 列
    const 逗号 = 位置们.find(p => p.索引 === 1);
    const 句号 = 位置们.find(p => p.索引 === 3);
    assert.deepEqual(逗号, { 索引: 1, 行: 0, 列: 3 });
    assert.deepEqual(句号, { 索引: 3, 行: 1, 列: 0 });
});

test('插入位置：CRLF 视为一个换行', () => {
    const 位置们 = 计算插入字符位置们(0, 0, 'a\r\nb');
    // \r\n 占索引 1、2，'b' 在索引 3、第 1 行第 0 列
    assert.deepEqual(位置们.find(p => p.索引 === 3), { 索引: 3, 行: 1, 列: 0 });
    assert.equal(位置们.length, 2);
});

test('插入位置：换行符本身不出现在结果中', () => {
    const 位置们 = 计算插入字符位置们(0, 0, '\n\n，');
    assert.equal(位置们.length, 1);
    assert.deepEqual(位置们[0], { 索引: 2, 行: 2, 列: 0 });
});

// ============================================================
// 词法状态扫描
// ============================================================

test('词法扫描：代码区与字符串区分', () => {
    assert.equal(扫描词法状态('let x = 1;'), 词法状态.代码);
    assert.equal(扫描词法状态('let s = "abc'), 词法状态.双引号字符串);
    assert.equal(扫描词法状态('// 注释，'), 词法状态.行注释);
});

test('词法扫描：生命周期标注不误判为字符字面量', () => {
    assert.equal(扫描词法状态(`fn f<'a>(x: &'a str)`), 词法状态.代码);
});

// ============================================================
// 语言注册表
// ============================================================

test('语言表：11 种语言且 languageId 唯一', () => {
    assert.equal(方言语言表.length, 11);
    assert.equal(new Set(方言语言Id).size, 11);
    for (const 语言 of 方言语言表) {
        assert.equal(语言.languageId, `rust-${语言.code}`);
        assert.equal(语言.extension, 语言.code);
    }
});

test('语言代码：显示名映射与非法值回退', () => {
    assert.equal(语言代码('中文'), 'zh');
    assert.equal(语言代码('日本語'), 'ja');
    assert.equal(语言代码('العربية'), 'ar');
    assert.equal(语言代码('不存在的语言'), 'zh');
});

test('按代码查找：覆盖全部语言包目录名', () => {
    for (const code of ['zh', 'en', 'ja', 'de', 'es', 'fr', 'pt', 'ru', 'ko', 'hi', 'ar']) {
        assert.ok(按代码查找(code), `缺少语言 ${code}`);
    }
});
