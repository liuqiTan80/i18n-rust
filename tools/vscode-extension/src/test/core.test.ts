/**
 * 单元测试：shell 引用 / 插入位置计算 / 语言注册表
 *
 * 使用 Node 内置测试运行器：npm test（先 npm run compile）
 */

import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { quoteCommandArg, quotePosixArg, quoteWindowsArg } from '../shell';
import { 应转换全角, 计算插入字符位置们, 扫描词法状态, 扫描词法状态们, 词法状态, 全角符号映射, 全角符号检测正则 } from '../fullwidth-convert';
import { 语言代码, 按代码查找, 方言语言表, 方言语言Id } from '../languages';
import { 选择光标诊断, 位置形状 } from '../diagnostic-pick';

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

test('命令名引用按平台分支（Windows 加 PowerShell 调用运算符前缀，cmd 亦兼容）', () => {
    const 路径 = 'C:\\Users\\t67\\.cargo\\bin\\rzc.EXE';
    if (process.platform === 'win32') {
        assert.equal(quoteCommandArg(路径), '& "C:\\Users\\t67\\.cargo\\bin\\rzc.EXE"');
    } else {
        // POSIX 分支与 quotePosixArg 一致（单引号），已由上方 POSIX 引用测试覆盖
        assert.equal(quoteCommandArg(路径), `'C:\\Users\\t67\\.cargo\\bin\\rzc.EXE'`);
    }
});

// ============================================================
// 全角转换决策（应转换全角）
// ============================================================

test('转换决策：代码区全角一律转换', () => {
    assert.equal(应转换全角(词法状态.代码, '，'), true);
    assert.equal(应转换全角(词法状态.代码, '“'), true);
    assert.equal(应转换全角(词法状态.代码, '”'), true);
});

test('转换决策：双引号字符串内中文引号仍转换（输入法配对），其余标点保留', () => {
    assert.equal(应转换全角(词法状态.双引号字符串, '“'), true);
    assert.equal(应转换全角(词法状态.双引号字符串, '”'), true);
    assert.equal(应转换全角(词法状态.双引号字符串, '，'), false);
    assert.equal(应转换全角(词法状态.双引号字符串, '。'), false);
});

test('转换决策：注释/字符/原始字符串内一律保留', () => {
    assert.equal(应转换全角(词法状态.行注释, '“'), false);
    assert.equal(应转换全角(词法状态.块注释, '”'), false);
    assert.equal(应转换全角(词法状态.单引号字符, '“'), false);
    assert.equal(应转换全角(词法状态.原始字符串, '”'), false);
});

test('检测正则：覆盖映射表全部键，不误报母语文本与 ASCII', () => {
    for (const 符号 of Object.keys(全角符号映射)) {
        assert.ok(全角符号检测正则.test(`前缀${符号}后缀`), `未命中 ${符号}`);
    }
    assert.equal(全角符号检测正则.test('让 x = 1; // 中文注释'), false);
    assert.equal(全角符号检测正则.test('println!("hello");'), false);
    assert.equal(全角符号检测正则.test('日本語のコード'), false);
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

test('词法扫描多位置：单遍遍历记录各偏移状态', () => {
    // 索引：     0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19
    // 前缀：     l e t ' ' s ' ' = ' ' "  a  b  c  "  ;  ' ' /  /  ' ' 注 释
    const 前缀 = 'let s = "abc"; // 注释';
    const 状态们 = 扫描词法状态们(前缀, [3, 8, 9, 14, 18]);
    assert.deepEqual(状态们, [
        词法状态.代码,             // 偏移 3："let" 后
        词法状态.代码,             // 偏移 8：字符串引号前
        词法状态.双引号字符串,     // 偏移 9：字符串内（"abc）
        词法状态.代码,             // 偏移 14：字符串闭合后
        词法状态.行注释            // 偏移 18：// 注释内
    ]);
});

test('词法扫描多位置：偏移 0、重复偏移与末尾偏移', () => {
    const 前缀 = 'let x = 1;';
    // 偏移 0（文档开头）恒为代码；重复偏移 5 去重；末尾偏移 = 前缀长度
    const 状态们 = 扫描词法状态们(前缀, [0, 5, 5, 前缀.length]);
    assert.deepEqual(状态们, [词法状态.代码, 词法状态.代码, 词法状态.代码]);
    // 与单位置扫描在末尾偏移处结果一致（等价性）
    assert.equal(扫描词法状态们(前缀, [前缀.length])[0], 扫描词法状态(前缀));
});

test('词法扫描多位置：跳过转义/注释标记后状态仍正确', () => {
    // 索引：     0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20
    // 前缀：     l e t ' ' s ' ' = ' ' " a  \  "  b  "  ;  ' ' /  /  ' ' 注 释
    const 前缀 = 'let s = "a\\"b"; // 注释';
    const 状态们 = 扫描词法状态们(前缀, [10, 14, 18]);
    assert.equal(状态们[0], 词法状态.双引号字符串); // 偏移 10：转义符前（字符串未闭合）
    assert.equal(状态们[1], 词法状态.代码);         // 偏移 14：字符串闭合后（\" 被跳过）
    assert.equal(状态们[2], 词法状态.行注释);       // 偏移 18：// 注释内（第二个 / 被跳过）
});

// ============================================================
// 语言注册表
// ============================================================

test('语言表：10 种语言且 languageId 唯一', () => {
    assert.equal(方言语言表.length, 10);
    assert.equal(new Set(方言语言Id).size, 10);
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
    for (const code of ['zh', 'ja', 'de', 'es', 'fr', 'pt', 'ru', 'ko', 'hi', 'ar']) {
        assert.ok(按代码查找(code), `缺少语言 ${code}`);
    }
});

// ============================================================
// 诊断选择（AI 讲解当前诊断）
// ============================================================

// 构造假诊断（range.contains 可选：测试未提供时退化为距离策略）
function 假诊断(行: number, 列: number, 严重程度 = 1): any {
    return {
        range: { start: { line: 行, character: 列 } },
        message: `诊断@${行}:${列}`,
        severity: 严重程度
    };
}

test('诊断选择：空列表返回 undefined', () => {
    assert.equal(选择光标诊断([], { line: 0, character: 0 }), undefined);
});

test('诊断选择：无 contains 时取同行起点最近者', () => {
    const 诊断们 = [假诊断(0, 10), 假诊断(0, 3), 假诊断(1, 1)];
    const 选中 = 选择光标诊断(诊断们, { line: 0, character: 5 });
    assert.equal(选中, 诊断们[1]); // 列 3 距光标 5 最近
});

test('诊断选择：同距离时错误（severity 小）优先于警告', () => {
    const 警告 = 假诊断(0, 5, 1);
    const 错误 = 假诊断(0, 5, 0);
    const 选中 = 选择光标诊断([警告, 错误], { line: 0, character: 6 });
    assert.equal(选中, 错误);
});

test('诊断选择：光标行无诊断时取全文行差最近者', () => {
    const 诊断们 = [假诊断(1, 0), 假诊断(5, 0)];
    const 选中 = 选择光标诊断(诊断们, { line: 3, character: 0 });
    assert.equal(选中, 诊断们[0]); // 行 1 距光标 3 行最近
});

test('诊断选择：range.contains 覆盖光标时优先返回', () => {
    const 覆盖诊断 = {
        range: {
            start: { line: 0, character: 0 },
            contains(位置: 位置形状): boolean {
                return 位置.line === 0 && 位置.character >= 0 && 位置.character <= 20;
            }
        },
        message: '整行错误',
        severity: 0
    };
    const 近旁 = 假诊断(0, 18);
    const 选中 = 选择光标诊断([近旁, 覆盖诊断], { line: 0, character: 10 });
    assert.equal(选中, 覆盖诊断);
});
