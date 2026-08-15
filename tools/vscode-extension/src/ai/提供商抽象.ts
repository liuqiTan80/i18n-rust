/**
 * 提供商抽象层 - 抽象基类
 *
 * 定义所有 AI 提供商的统一接口（发送对话 / 流式对话 / 获取模型列表），
 * 并提供带超时的网络请求与响应校验基础能力。
 * 具体服务商通过继承本类实现协议细节。
 */

import { AI配置, AI错误, 消息, 流式回调 } from './类型定义';

/**
 * 提供商抽象基类
 *
 * 子类必须实现：
 * - 发送对话：一次完整对话，返回助手回复全文
 * - 流式对话：内容增量通过回调逐块返回
 * - 获取模型列表：返回该服务商可用模型
 */
export abstract class 提供商抽象 {
    constructor(protected 配置: AI配置) {}

    /** 发送一次完整对话，返回助手回复全文 */
    abstract 发送对话(messages: 消息[]): Promise<string>;

    /** 流式对话：内容增量通过回调逐块返回 */
    abstract 流式对话(messages: 消息[], onChunk: 流式回调): Promise<void>;

    /** 获取该服务商可用的模型列表 */
    abstract 获取模型列表(): Promise<string[]>;

    /**
     * 带超时的 fetch 请求（Node.js 内置 fetch，VS Code 1.85+ 内置 Node 18+ 均支持）
     * 网络错误与超时统一转换为 AI错误。
     */
    protected async 请求(url: string, 选项: RequestInit): Promise<Response> {
        const 控制器 = new AbortController();
        const 定时器 = setTimeout(() => 控制器.abort(), this.配置.timeout * 1000);
        try {
            return await fetch(url, { ...选项, signal: 控制器.signal });
        } catch (错误) {
            if (错误 instanceof Error && 错误.name === 'AbortError') {
                throw new AI错误(
                    '超时',
                    `请求超时：已等待 ${this.配置.timeout} 秒无响应（${url}）。可增大 i18n-rust.ai.timeout 配置。`
                );
            }
            throw new AI错误(
                '网络错误',
                `网络错误：无法连接到 ${url}（${(错误 as Error).message}）。请检查网络与 baseUrl 配置。`
            );
        } finally {
            clearTimeout(定时器);
        }
    }

    /**
     * 校验响应并解析 JSON：
     * - 非 2xx 时提取服务商错误详情（兼容 OpenAI 的 {"error":{"message":...}} 格式）
     * - JSON 解析失败时给出明确提示
     */
    protected async 校验响应(响应: Response, url: string): Promise<unknown> {
        if (!响应.ok) {
            let 详情 = '';
            try {
                const 数据 = (await 响应.json()) as any;
                详情 = 数据?.error?.message ?? JSON.stringify(数据);
            } catch {
                详情 = await 响应.text().catch(() => '');
            }
            throw new AI错误(
                'API错误',
                `API 错误（HTTP ${响应.status} ${响应.statusText}）：${详情 || '无错误详情'}（${url}）`
            );
        }
        try {
            return await 响应.json();
        } catch {
            throw new AI错误(
                'JSON解析错误',
                `JSON 解析错误：服务商返回了无法解析的内容（${url}）。请确认 baseUrl 指向 OpenAI 兼容的服务。`
            );
        }
    }
}
