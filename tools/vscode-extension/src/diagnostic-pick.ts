/**
 * 诊断选择纯函数：从编辑器诊断列表中选择"光标附近"的一条进行讲解
 *
 * 不依赖 vscode API（结构化类型兼容 vscode.Diagnostic / vscode.Range），
 * 便于单元测试与复用。选择策略：
 * 1. 覆盖光标位置的诊断优先（range.contains）；
 * 2. 否则取光标所在行内起点距光标最近者（同距离时错误优先于警告，
 *    severity 数值小者严重：Error=0 / Warning=1 / Information=2 / Hint=3）；
 * 3. 否则取全文起点距光标最近者（行差优先）。
 */

/** 位置形状（兼容 vscode.Position） */
export interface 位置形状 {
    line: number;
    character: number;
}

/** 范围形状（兼容 vscode.Range；contains 参数用 any 兼容 vscode.Position） */
export interface 范围形状 {
    start: 位置形状;
    contains?(位置: any): boolean;
}

/** 诊断形状（兼容 vscode.Diagnostic：range/message/severity） */
export interface 诊断形状 {
    range: 范围形状;
    message: string;
    severity?: number;
}

/**
 * 选择光标附近的一条诊断；找不到返回 undefined
 */
export function 选择光标诊断<T extends 诊断形状>(
    诊断们: readonly T[],
    光标: 位置形状
): T | undefined {
    if (诊断们.length === 0) {
        return undefined;
    }

    // 1. 覆盖光标位置（range.contains 由 vscode.Range 提供）
    for (const 诊断 of 诊断们) {
        if (诊断.range.contains && 诊断.range.contains(光标)) {
            return 诊断;
        }
    }

    // 2. 光标所在行内起点距离最近；同距离时错误优先（severity 数值小者）
    let 候选: T | undefined;
    let 最小距离 = Number.MAX_SAFE_INTEGER;
    for (const 诊断 of 诊断们) {
        if (诊断.range.start.line !== 光标.line) {
            continue;
        }
        const 距离 = Math.abs(诊断.range.start.character - 光标.character);
        if (距离 < 最小距离 || (距离 === 最小距离 && 候选 !== undefined
            && (诊断.severity ?? 3) < (候选.severity ?? 3))) {
            候选 = 诊断;
            最小距离 = 距离;
        }
    }
    if (候选 !== undefined) {
        return 候选;
    }

    // 3. 全文兜底：行差优先（×1000 权重），同行动比列差
    for (const 诊断 of 诊断们) {
        const 距离 = Math.abs(诊断.range.start.line - 光标.line) * 1000
            + Math.abs(诊断.range.start.character - 光标.character);
        if (距离 < 最小距离) {
            候选 = 诊断;
            最小距离 = 距离;
        }
    }
    return 候选;
}
