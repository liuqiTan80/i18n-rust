/**
 * Shell 参数安全引用（纯逻辑，不依赖 VS Code API）
 *
 * 用于 vscode.Terminal.sendText 等必须经过 shell 解释的场景：
 * 文件路径中可能包含引号、反引号、$() 等元字符，直接字符串拼接会导致命令注入，
 * 必须先按「实际终端 shell」做针对性转义。
 *
 * 三类 shell 采用不同的字面量转义策略：
 * - POSIX（sh/bash/zsh）：单引号是字面字符串，仅需对内部单引号闭合-转义-重开；
 * - PowerShell：单引号是字面字符串，仅需对内部单引号加倍；
 * - cmd.exe：无真正的字面量引用机制，双引号内仍解释 ^ & | < > ( ) 等元字符，
 *   故逐字符加 ^ 前缀尽力字面化（% 的环境变量展开存在 cmd 固有残余，见下）。
 */

import * as os from 'os';

/** 终端 shell 种类 */
export type Shell种类 = 'posix' | 'powershell' | 'cmd';

/**
 * 根据 shell 可执行文件路径推断 shell 种类。
 * 缺省（undefined / 空串）时按平台推断：非 Windows 走 posix，
 * Windows 不假设 PowerShell（宁可多转义也不留下 cmd 注入面），保守走 cmd。
 */
export function 推断Shell种类(shell路径?: string): Shell种类 {
    if (os.platform() !== 'win32') {
        return 'posix';
    }
    const 路径 = (shell路径 ?? '').toLowerCase();
    if (路径.includes('powershell') || 路径.includes('pwsh')) {
        return 'powershell';
    }
    if (路径.includes('cmd.exe') || 路径.endsWith('cmd')) {
        return 'cmd';
    }
    // Git Bash / MSYS2 / Cygwin / WSL 等 POSIX 兼容 shell：反引号、$()、;
    // 等元字符与 posix 相同，须走单引号字面量转义。若误走 cmd 的 ^ 转义，
    // 反引号在 bash 中仍是命令替换，会遗留命令注入。
    if (/bash|zsh|fish|sh\.exe|wsl|msys|cygwin/i.test(路径)) {
        return 'posix';
    }
    return 'cmd';
}

/**
 * 将参数安全引用为 shell 字面量。
 * @param shell路径 终端 shell 可执行文件路径（如 vscode.env.shell），缺省按平台推断
 */
export function quoteShellArg(arg: string, shell路径?: string): string {
    switch (推断Shell种类(shell路径)) {
        case 'powershell':
            return quotePowerShellArg(arg);
        case 'cmd':
            return quoteCmdArg(arg);
        default:
            return quotePosixArg(arg);
    }
}

/**
 * 将可执行文件路径引用为命令名（命令行第一个 token）。
 * Windows（PowerShell）命令名在带引号时须加调用运算符 `& `，
 * 否则 `"路径" run ...` 会被解析为字符串表达式而报语法错误；
 * cmd 中行首 `&` 后跟命令同样合法（空命令被忽略），故统一加前缀。
 */
export function quoteCommandArg(arg: string, shell路径?: string): string {
    const 种类 = 推断Shell种类(shell路径);
    if (种类 === 'posix') {
        return quotePosixArg(arg);
    }
    return `& ${quoteShellArg(arg, shell路径)}`;
}

/**
 * POSIX 单引号引用（POSIX sh 规范：单引号内无转义，需断开重开）
 */
export function quotePosixArg(arg: string): string {
    return `'${arg.replace(/'/g, `'\\''`)}'`;
}

/**
 * PowerShell 单引号引用（PS 规范：单引号字符串是纯字面量，
 * 唯一需转义的是单引号本身，用加倍 ' ' 表示）
 */
export function quotePowerShellArg(arg: string): string {
    return `'${arg.replace(/'/g, "''")}'`;
}

/**
 * cmd.exe 引用（Windows 的传统命令提示符）。
 * cmd 不存在"字面量引用"机制：双引号内部 ^ & | < > ( ) 等元字符仍被解释，
 * 因此对它们逐个加 ^ 前缀使其字面化。% 与 ! 的动态展开在 cmd 中先于 ^ 转义发生，
 * 无法被 ^ 完全中和（尤其形如 %VAR% 的环境变量展开），这是 cmd 的固有局限；
 * 为最大限度降低注入面，VS Code 在 Windows 上默认终端为 PowerShell，
 * 那些走 PowerShell 字面量路径的场景（quotePowerShellArg）不在此列。
 */
export function quoteCmdArg(arg: string): string {
    const 转义 = arg
        .replace(/"/g, '""')
        .replace(/[\^&|<>()%!]/g, '^$&');
    return `"${转义}"`;
}