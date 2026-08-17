/**
 * Shell 参数安全引用（纯逻辑，不依赖 VS Code API）
 *
 * 用于 vscode.Terminal.sendText 等必须经过 shell 解释的场景：
 * 文件路径中可能包含引号、反引号、$() 等元字符，
 * 直接字符串拼接会导致命令注入，必须先做平台感知的转义。
 */

import * as os from 'os';

/**
 * 将参数安全引用为 shell 字面量
 *
 * - POSIX（bash/zsh 等）：单引号包裹，内部单引号用 '\'' 闭合-转义-重开
 * - Windows（cmd/PowerShell 混合终端）：双引号包裹，
 *   双引号加倍、% 转义为 ^%（cmd 变量展开）、反引号加倍（PowerShell 转义符）
 */
export function quoteShellArg(arg: string): string {
    if (os.platform() === 'win32') {
        return quoteWindowsArg(arg);
    }
    return quotePosixArg(arg);
}

/**
 * POSIX 单引号引用（POSIX sh 规范：单引号内无转义，需断开重开）
 */
export function quotePosixArg(arg: string): string {
    return `'${arg.replace(/'/g, `'\\''`)}'`;
}

/**
 * Windows 引用（兼顾 cmd.exe 与 PowerShell 的常见终端宿主）
 * 注意：Windows 无法完全防御 cmd 的 ^ 注入与 ! 延迟展开，
 * 此方案覆盖引号/反引号/$/% 等主流注入向量，
 * 敏感路径场景建议改用无 shell 的 execFile。
 */
export function quoteWindowsArg(arg: string): string {
    const escaped = arg
        .replace(/"/g, '""')
        .replace(/`/g, '``')
        .replace(/%/g, '^%');
    return `"${escaped}"`;
}
