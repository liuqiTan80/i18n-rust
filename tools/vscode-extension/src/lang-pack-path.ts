/**
 * 语言包目录探测（纯逻辑，仅依赖 fs/path，便于单测）
 *
 * 统一两种语言包布局的探测：
 * - lang-packs/（用户项目约定）
 * - crates/engine/lang-packs/（主仓库单一数据源）
 *
 * 供 LSP 启动（extension.ts，需具体语言目录）与 AI 提示词
 * （ai/config-manager.ts，需语言包根目录）共享，避免布局漂移。
 */

import * as fs from 'fs';
import * as path from 'path';

/** 语言包根目录相对基础目录的两种布局 */
export const 语言包根相对路径们: readonly string[] = [
    'lang-packs',
    path.join('crates', 'engine', 'lang-packs')
];

/**
 * 探测基础目录下的语言包根目录，返回第一个存在的布局，找不到返回 undefined。
 */
export function 探测语言包根(基础目录: string): string | undefined {
    for (const 相对 of 语言包根相对路径们) {
        const 候选 = path.join(基础目录, 相对);
        if (fs.existsSync(候选)) {
            return 候选;
        }
    }
    return undefined;
}

/**
 * 探测基础目录下某语言的目录（要求 <布局>/<语言> 真实存在），失败返回 undefined。
 */
export function 探测语言目录(基础目录: string, 语言: string): string | undefined {
    for (const 相对 of 语言包根相对路径们) {
        const 候选 = path.join(基础目录, 相对, 语言);
        if (fs.existsSync(候选)) {
            return 候选;
        }
    }
    return undefined;
}

/**
 * 从 startDir 向上（最多 5 级）探测某语言的目录，失败返回 undefined。
 */
export function 向上探测语言目录(startDir: string, 语言: string): string | undefined {
    let dir = startDir;
    for (let i = 0; i < 5; i++) {
        const 候选 = 探测语言目录(dir, 语言);
        if (候选) {
            return 候选;
        }
        const parent = path.dirname(dir);
        if (parent === dir) {
            break;
        }
        dir = parent;
    }
    return undefined;
}