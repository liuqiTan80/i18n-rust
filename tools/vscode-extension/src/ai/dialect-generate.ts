/**
 * 方言代码「生成 → 校验 → 回喂修正」闭环的核心逻辑（纯函数层）
 *
 * 与 VS Code 解耦：provider 调用与 rzc 校验都以依赖注入传入，
 * 因此可以在 node --test 下用假 provider / 假校验器完整跑通闭环。
 *
 * 设计约束（对应方案 Q3）：
 * - 地面真值来自注入的 validate（生产实现走 `rzc check`），本模块不碰 shell/fs；
 * - 校验失败才回喂，最多 maxFixRounds 轮，绝不无限循环；
 * - 尊重 AbortSignal，取消时尽快收敛返回。
 */

import { ChatMessage } from './types';

/** 一次方言校验的结果：是否通过 + 供回喂的报错文本 */
export interface DialectValidation {
    ok: boolean;
    report: string;
}

/** 闭环运行所需依赖（全部注入，便于测试与替换） */
export interface GenerateDeps {
    /** 调用模型：整段回复（生产为 provider.sendChat） */
    chat: (messages: ChatMessage[], signal?: AbortSignal) => Promise<string>;
    /** 校验一段方言代码（生产为写临时文件 + rzc check） */
    validate: (code: string) => Promise<DialectValidation>;
    /** 系统提示词（已含方言词表与硬约束） */
    systemPrompt: string;
    /** 用户的生成需求 */
    userRequest: string;
    /** 首次生成之后允许的修正轮数上限 */
    maxFixRounds: number;
    /** 取消信号 */
    signal?: AbortSignal;
    /** 进度日志（可选） */
    log?: (message: string) => void;
}

/** 闭环最终产物 */
export interface GenerateResult {
    /** 最后一轮生成的方言代码 */
    code: string;
    /** 最后一次校验是否通过 */
    ok: boolean;
    /** 实际执行的修正轮数（首轮生成不计） */
    rounds: number;
}

/**
 * 从模型回复中抽取代码块内容。
 *
 * 优先取 ``` 围栏内的代码（围栏语言可为 rust / rust-zh 等带连字符标识）；
 * 多个代码块以空行拼接。若无任何围栏，回退为整段文本去首尾空白
 * （交由校验判定是否为有效方言，避免把散文当代码丢弃后无所适从）。
 */
export function extractDialectCode(text: string): string {
    const re = /```[^\n`]*\n([\s\S]*?)```/g;
    const blocks: string[] = [];
    let m: RegExpExecArray | null;
    while ((m = re.exec(text)) !== null) {
        const body = m[1].replace(/\s+$/, '');
        if (body.trim().length > 0) {
            blocks.push(body);
        }
    }
    if (blocks.length > 0) {
        return blocks.join('\n\n') + '\n';
    }
    const trimmed = text.trim();
    return trimmed.length > 0 ? trimmed + '\n' : '';
}

/** 构造回喂给模型的修正指令：附上校验报错 + 明确的「只输出方言代码」要求 */
export function buildFixFeedback(report: string): string {
    return [
        '上面的代码经 rzc check 校验未通过，报错如下：',
        report && report.trim() ? report.trim() : '（无详细输出）',
        '',
        '请只输出修正后的完整方言代码（用 ```rust 代码块包裹），不要输出解释文字。',
        '映射表中已有对应方言词的关键字/类型/宏/标准库路径必须使用方言词，禁止英文原形；',
        '确实没有对应方言译法的术语保留英文原名，切勿臆造。'
    ].join('\n');
}

/**
 * 运行「生成 → 校验 → 回喂修正」闭环。
 *
 * 流程：首轮生成 → 校验；未通过且未达轮数上限则回喂报错、再生成再校验，
 * 直到通过 / 达到上限 / 被取消。返回最后一版代码及其校验状态。
 */
export async function runDialectGenerate(deps: GenerateDeps): Promise<GenerateResult> {
    const messages: ChatMessage[] = [
        { role: 'system', content: deps.systemPrompt },
        { role: 'user', content: deps.userRequest }
    ];

    const 已取消 = () => deps.signal?.aborted === true;

    let 回复 = await deps.chat(messages, deps.signal);
    messages.push({ role: 'assistant', content: 回复 });
    let code = extractDialectCode(回复);
    let validation = await deps.validate(code);

    let rounds = 0;
    while (!validation.ok && rounds < deps.maxFixRounds && !已取消()) {
        rounds++;
        deps.log?.(`第 ${rounds} 轮校验未通过，回喂诊断给模型修正…`);
        messages.push({ role: 'user', content: buildFixFeedback(validation.report) });
        回复 = await deps.chat(messages, deps.signal);
        messages.push({ role: 'assistant', content: 回复 });
        code = extractDialectCode(回复);
        validation = await deps.validate(code);
    }

    if (!validation.ok && !已取消()) {
        deps.log?.(`已达修正上限（${deps.maxFixRounds} 轮），rzc check 仍未通过。`);
    }

    return { code, ok: validation.ok, rounds };
}
