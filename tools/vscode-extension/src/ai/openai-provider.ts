/**
 * OpenAI-compatible provider
 *
 * Implements the OpenAI-compatible chat/completions protocol (including SSE streaming).
 * Different providers are selected via the baseUrl config:
 * - DeepSeek: https://api.deepseek.com/v1
 * - 通义千问: https://dashscope.aliyuncs.com/compatible-mode/v1
 * - 智谱 GLM: https://open.bigmodel.cn/api/paas/v4
 * - Ollama: http://localhost:11434/v1
 * - Custom: any user-provided URL
 *
 * All support sendChat / streamChat / listModels (GET /models).
 */

import { ProviderInterface } from './provider-interface';
import { AIConfig, AIError, ChatMessage, StreamCallback } from './types';

/**
 * OpenAI-compatible protocol provider
 */
export class OpenAICompatibleProvider extends ProviderInterface {
    /** Chat endpoint */
    private chatEndpoint(): string {
        return this.config.baseUrl.replace(/\/+$/, '') + '/chat/completions';
    }

    /** Models endpoint */
    private modelsEndpoint(): string {
        return this.config.baseUrl.replace(/\/+$/, '') + '/models';
    }

    /** Request headers: attach Bearer auth when an API key is configured */
    private buildHeaders(): Record<string, string> {
        const headers: Record<string, string> = { 'Content-Type': 'application/json' };
        if (this.config.apiKey) {
            headers['Authorization'] = `Bearer ${this.config.apiKey}`;
        }
        return headers;
    }

    /** Chat request body */
    private buildBody(messages: ChatMessage[], stream: boolean): unknown {
        return {
            model: this.config.model,
            messages,
            temperature: this.config.temperature,
            max_tokens: this.config.maxTokens,
            stream
        };
    }

    /** Send one full chat round, returning the assistant's full reply */
    async sendChat(messages: ChatMessage[]): Promise<string> {
        const response = await this.request(this.chatEndpoint(), {
            method: 'POST',
            headers: this.buildHeaders(),
            body: JSON.stringify(this.buildBody(messages, false))
        });
        const data = (await this.validateResponse(response, this.chatEndpoint())) as any;
        const content = data?.choices?.[0]?.message?.content;
        if (typeof content !== 'string') {
            throw new AIError(
                'API错误',
                `API 响应缺少 choices[0].message.content 字段：${JSON.stringify(data).slice(0, 300)}`
            );
        }
        return content;
    }

    /** Stream chat: parse SSE deltas and deliver content chunk by chunk */
    async streamChat(messages: ChatMessage[], onChunk: StreamCallback): Promise<void> {
        const response = await this.request(this.chatEndpoint(), {
            method: 'POST',
            headers: this.buildHeaders(),
            body: JSON.stringify(this.buildBody(messages, true))
        });
        if (!response.ok) {
            await this.validateResponse(response, this.chatEndpoint());
            return;
        }
        const reader = response.body?.getReader();
        if (!reader) {
            throw new AIError('网络错误', '流式响应无法读取（响应体为空）');
        }
        const decoder = new TextDecoder();
        let buffer = '';
        try {
            while (true) {
                const { done, value } = await reader.read();
                if (done) {
                    break;
                }
                // Split SSE data by lines (the last segment may be incomplete; keep it for the next round)
                buffer += decoder.decode(value, { stream: true });
                const lines = buffer.split('\n');
                buffer = lines.pop() ?? '';
                for (const line of lines) {
                    this.handleSSELine(line, onChunk);
                }
            }
            if (buffer.trim()) {
                this.handleSSELine(buffer, onChunk);
            }
        } catch (error) {
            if (error instanceof AIError) {
                throw error;
            }
            throw new AIError('网络错误', `流式读取中断：${(error as Error).message}`);
        } finally {
            try {
                reader.releaseLock();
            } catch {
                // Ignore lock release errors
            }
        }
    }

    /**
     * Handle a single SSE line (data: {json} or data: [DONE]).
     * Unparseable lines are skipped to tolerate provider differences.
     */
    private handleSSELine(line: string, onChunk: StreamCallback): void {
        const content = line.trim();
        if (!content.startsWith('data:')) {
            return;
        }
        const data = content.slice(5).trim();
        if (!data || data === '[DONE]') {
            return;
        }
        try {
            const parsed = JSON.parse(data) as any;
            const chunk = parsed?.choices?.[0]?.delta?.content;
            if (typeof chunk === 'string' && chunk.length > 0) {
                onChunk(chunk);
            }
        } catch {
            // Ignore unparseable SSE lines
        }
    }

    /** List models (GET /models); falls back to the configured model when unavailable */
    async listModels(): Promise<string[]> {
        try {
            const response = await this.request(this.modelsEndpoint(), {
                method: 'GET',
                headers: this.buildHeaders()
            });
            if (!response.ok) {
                return [this.config.model];
            }
            const data = (await this.validateResponse(response, this.modelsEndpoint())) as any;
            const models = Array.isArray(data?.data)
                ? data.data.map((item: any) => item?.id).filter((id: unknown): id is string => typeof id === 'string')
                : [];
            return models.length > 0 ? models : [this.config.model];
        } catch {
            // Fall back to the configured model when the endpoint is unavailable
            return [this.config.model];
        }
    }
}
