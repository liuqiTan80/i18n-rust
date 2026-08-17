/**
 * Provider abstraction layer - abstract base class
 *
 * Defines the unified interface for all AI providers
 * (send chat / stream chat / list models) and provides
 * base capabilities for network requests with timeout and response validation.
 * Concrete providers extend this class to implement protocol details.
 */

import { AIConfig, AIError, ChatMessage, StreamCallback } from './types';

/**
 * Abstract base class for providers
 *
 * Subclasses must implement:
 * - sendChat: one full chat round, returning the assistant's full reply
 * - streamChat: content deltas delivered chunk by chunk via callback
 * - listModels: the models available from this provider
 */
export abstract class ProviderInterface {
    constructor(protected config: AIConfig) {}

    /** Send one full chat round, returning the assistant's full reply */
    abstract sendChat(messages: ChatMessage[], signal?: AbortSignal): Promise<string>;

    /** Stream chat: content deltas delivered chunk by chunk via callback */
    abstract streamChat(messages: ChatMessage[], onChunk: StreamCallback, signal?: AbortSignal): Promise<void>;

    /** List the models available from this provider */
    abstract listModels(): Promise<string[]>;

    /**
     * Fetch with timeout (built-in Node.js fetch, supported by VS Code 1.85+ / Node 18+)
     * 可传入外部 AbortSignal 支持用户取消。
     * Network errors and timeouts are converted into AIError.
     */
    protected async request(url: string, options: RequestInit, signal?: AbortSignal): Promise<Response> {
        if (signal?.aborted) {
            throw new AIError('已取消', '请求已取消');
        }
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), this.config.timeout * 1000);
        const onExternalAbort = () => controller.abort();
        signal?.addEventListener('abort', onExternalAbort);
        try {
            return await fetch(url, { ...options, signal: controller.signal });
        } catch (error) {
            if (signal?.aborted) {
                throw new AIError('已取消', '请求已取消');
            }
            if (error instanceof Error && error.name === 'AbortError') {
                throw new AIError(
                    '超时',
                    `请求超时：已等待 ${this.config.timeout} 秒无响应（${url}）。可增大 i18n-rust.ai.timeout 配置。`
                );
            }
            throw new AIError(
                '网络错误',
                `网络错误：无法连接到 ${url}（${(error as Error).message}）。请检查网络与 baseUrl 配置。`
            );
        } finally {
            clearTimeout(timer);
            signal?.removeEventListener('abort', onExternalAbort);
        }
    }

    /**
     * Validate the response and parse JSON:
     * - For non-2xx responses, extract provider error details
     *   (compatible with OpenAI's {"error":{"message":...}} format)
     * - Give a clear hint when JSON parsing fails
     */
    protected async validateResponse(response: Response, url: string): Promise<unknown> {
        if (!response.ok) {
            let detail = '';
            try {
                const data = (await response.json()) as any;
                detail = data?.error?.message ?? JSON.stringify(data);
            } catch {
                detail = await response.text().catch(() => '');
            }
            throw new AIError(
                'API错误',
                `API 错误（HTTP ${response.status} ${response.statusText}）：${detail || '无错误详情'}（${url}）`
            );
        }
        try {
            return await response.json();
        } catch {
            throw new AIError(
                'JSON解析错误',
                `JSON 解析错误：服务商返回了无法解析的内容（${url}）。请确认 baseUrl 指向 OpenAI 兼容的服务。`
            );
        }
    }
}
