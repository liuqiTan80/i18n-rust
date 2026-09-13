/**
 * 版本比较（纯逻辑，不依赖 VS Code API）
 *
 * 供更新检查使用：解析 GitHub Releases 的 tag 并判断是否比当前安装版本新。
 */

/**
 * 从发行版 tag 解析版本号：去掉可选的 v/V 前缀并做基础格式校验。
 *
 * 无法解析（空值、不以数字开头、缺少点分段）时返回 undefined。
 */
export function 解析发行版版本(tag: string | undefined | null): string | undefined {
    if (!tag) {
        return undefined;
    }
    const 版本 = String(tag).trim().replace(/^v/i, '');
    return /^\d+\.\d+/.test(版本) ? 版本 : undefined;
}

/**
 * 远端版本是否比当前版本新。
 *
 * 只比较数字版段：预发布后缀（-beta.1 等）先剥离；
 * 段数不一致时按短板补齐（"0.8" 与 "0.8.0" 等价）；
 * 任一段不是纯数字或版本为空时返回 false（宁可不提示，不误提示）。
 */
export function 是更新版本(远端: string, 当前: string): boolean {
    const 段 = (版本: string): number[] | undefined => {
        const 数字们: number[] = [];
        for (const 段文本 of 版本.split('-')[0].split('.')) {
            const 规范段 = 段文本.trim();
            if (!/^\d+$/.test(规范段)) {
                return undefined;
            }
            数字们.push(parseInt(规范段, 10));
        }
        return 数字们.length > 0 ? 数字们 : undefined;
    };
    const 远端段 = 段(远端);
    const 当前段 = 段(当前);
    if (!远端段 || !当前段) {
        return false;
    }
    for (let i = 0; i < Math.max(远端段.length, 当前段.length); i++) {
        const 远端第 = 远端段[i] ?? 0;
        const 当前第 = 当前段[i] ?? 0;
        if (远端第 !== 当前第) {
            return 远端第 > 当前第;
        }
    }
    return false;
}
