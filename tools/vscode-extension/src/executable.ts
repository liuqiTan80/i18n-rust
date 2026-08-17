/**
 * 可执行文件解析（跨平台）
 *
 * 替代 Unix 专属的 `which`：按 PATH 逐目录扫描，
 * Windows 下追加 PATHEXT 后缀（.exe 等）探测。
 * 供 rzc CLI 与 i18n-rust-lsp 二进制定位共用。
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

/**
 * 当前平台下可执行文件可能的后缀（Windows 依赖 PATHEXT，其他平台为空）
 */
function 可执行后缀们(): string[] {
    if (process.platform !== 'win32') {
        return [''];
    }
    const pathExt = process.env.PATHEXT;
    const 后缀们 = pathExt ? pathExt.split(';').filter(s => s.length > 0) : ['.exe', '.cmd', '.bat'];
    // 确保 .exe 永远在探测列表里
    return 后缀们.some(s => s.toLowerCase() === '.exe') ? 后缀们 : ['.exe', ...后缀们];
}

/**
 * 判断给定路径是否为可用的可执行文件
 * （存在 + 是文件；POSIX 额外要求可执行位）
 */
function 是可执行文件(完整路径: string): boolean {
    try {
        const 状态 = fs.statSync(完整路径);
        if (!状态.isFile()) {
            return false;
        }
        if (process.platform === 'win32') {
            return true;
        }
        fs.accessSync(完整路径, fs.constants.X_OK);
        return true;
    } catch {
        return false;
    }
}

/**
 * 在 PATH 环境变量中查找可执行文件的真实路径
 * 找到符号链接时解析为真实路径（便于向上搜索语言包目录）
 */
export function findInPath(name: string): string | undefined {
    const pathEnv = process.env.PATH ?? '';
    const 分隔符 = process.platform === 'win32' ? ';' : ':';
    for (const 目录 of pathEnv.split(分隔符)) {
        if (!目录) {
            continue;
        }
        for (const 后缀 of 可执行后缀们()) {
            // 名称自带扩展名（如用户配置 xxx.exe）时不重复追加
            const 候选 = path.join(目录, 后缀 && !path.extname(name) ? name + 后缀 : name);
            if (是可执行文件(候选)) {
                try {
                    return fs.realpathSync(候选);
                } catch {
                    return 候选;
                }
            }
        }
    }
    return undefined;
}

/**
 * 解析用户配置的可执行文件路径：
 * 1. 绝对路径且存在 → 直接返回
 * 2. 相对路径 → 依次尝试各工作区根目录下的常见构建位置
 *    （target/release、target/debug、工作区根）
 * 3. 按名称在 PATH 中查找
 * 找不到返回 undefined（由调用方给出明确错误提示）。
 */
export function 解析可执行文件(
    配置值: string,
    工作区根们: readonly string[]
): string | undefined {
    const 名称 = 配置值.trim();
    if (!名称) {
        return undefined;
    }
    if (path.isAbsolute(名称)) {
        return 是可执行文件(名称) ? 名称 : undefined;
    }
    // 工作区内常见构建产物位置（release 优先）
    for (const 根 of 工作区根们) {
        for (const 相对 of [
            path.join('target', 'release', 名称),
            path.join('target', 'debug', 名称),
            名称
        ]) {
            for (const 后缀 of 可执行后缀们()) {
                const 候选 = path.join(根, 后缀 && !path.extname(相对) ? 相对 + 后缀 : 相对);
                if (是可执行文件(候选)) {
                    return 候选;
                }
            }
        }
    }
    return findInPath(名称);
}

/**
 * 用户主目录（语言包全局安装位置 ~/.rz/lang-packs 的基准）
 */
export function 用户主目录(): string {
    return os.homedir();
}
