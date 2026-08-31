/**
 * Anthropic provider（Claude Messages API）
 *
 * Anthropic 不使用 OpenAI 兼容协议，协议差异在本类中适配：
 * - 认证头 x-api-key（而非 Authorization: Bearer）+ 必填 anthropic-version 头
 * - system 消息提升为顶层字段，messages 仅含 user / assistant
 * - max_tokens 必填；响应为 content 块数组（type: text 拼接）
 * - 流式事件为 content_block_delta（delta.text），与 OpenAI 的 choices[].delta 不同
 * - 模型列表：GET /v1/models，字段 data[].id
 */

import { ProviderInterface } from './provider-interface';
import { AIError, ChatMessage, StreamCallback } from './types';

/** Anthropic API 版本头（Messages API 要求显式指定） */
const ANTHROPIC_VERSION = '2023-06-01';

/**
 * Anthropic Messages API provider
 */
export class AnthropicProvider extends ProviderInterface {
    /** Messages 端点：baseUrl（如 https://api.anthropic.com）不带 /v1，此处补齐 */
    private messagesEndpoint(): string {
        return this.config.baseUrl.replace(/\/+$/, '') + '/v1/messages';
    }

    /** 模型列表端点 */
    private modelsEndpoint(): string {
        return this.config.baseUrl.replace(/\/+$/, '') + '/v1/models';
    }

    /** 认证与版本头：x-api-key + anthropic-version（content-type 由基类约定） */
    private buildHeaders(): Record<string, string> {
        const headers: Record<string, string> = {
            'Content-Type': 'application/json',
            'anthropic-version': ANTHROPIC_VERSION
        };
        if (this.config.apiKey) {
            headers['x-api-key'] = this.config.apiKey;
        }
        return headers;
    }

    /**
     * 请求体：system 消息提升为顶层字段（Anthropic 不支持 system 角色），
     * messages 仅保留 user / assistant。
     */
    private buildBody(messages: ChatMessage[], stream: boolean): unknown {
        const system = messages
            .filter(m => m.role === 'system')
            .map(m => m.content)
            .join('\n');
        const chatMessages = messages
            .filter(m => m.role !== 'system')
            .map(m => ({ role: m.role, content: m.content }));
        const body: Record<string, unknown> = {
            model: this.config.model,
            max_tokens: this.config.maxTokens,
            messages: chatMessages,
            stream
        };
        if (this.config.temperature !== undefined) {
            body.temperature = this.config.temperature;
        }
        if (system) {
            body.system = system;
        }
        return body;
    }

    /** 发送一轮完整对话，返回助手完整回复（拼接全部 text 块） */
    async sendChat(messages: ChatMessage[], signal?: AbortSignal): Promise<string> {
        const response = await this.request(this.messagesEndpoint(), {
            method: 'POST',
            headers: this.buildHeaders(),
            body: JSON.stringify(this.buildBody(messages, false))
        }, signal);
        const data = (await this.validateResponse(response, this.messagesEndpoint())) as any;
        const parts: string[] = Array.isArray(data?.content)
            ? data.content
                .filter((block: any) => block?.type === 'text' && typeof block.text === 'string')
                .map((block: any) => block.text)
            : [];
        if (parts.length === 0) {
            throw new AIError(
                'API错误',
                `API 响应缺少 content 文本块：${JSON.stringify(data).slice(0, 300)}`
            );
        }
        return parts.join('');
    }

    /** 流式对话：SSE 事件 content_block_delta（delta.text）逐块输出 */
    async streamChat(messages: ChatMessage[], onChunk: StreamCallback, signal?: AbortSignal): Promise<void> {
        const response = await this.request(this.messagesEndpoint(), {
            method: 'POST',
            headers: this.buildHeaders(),
            body: JSON.stringify(this.buildBody(messages, true))
        }, signal);
        if (!response.ok) {
            await this.validateResponse(response, this.messagesEndpoint());
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
                if (signal?.aborted) {
                    await reader.cancel().catch(() => undefined);
                    throw new AIError('已取消', '请求已取消');
                }
                const { done, value } = await reader.read();
                if (done) {
                    break;
                }
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
            if (signal?.aborted) {
                throw new AIError('已取消', '请求已取消');
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
     * 处理单条 SSE 行（data: {json}）。
     * Anthropic 流式事件：message_start / content_block_start / content_block_delta /
     * content_block_stop / message_delta / message_stop / ping。
     * 只取 content_block_delta 中的 delta.text；无法解析的行跳过以容忍差异。
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
            if (parsed?.type === 'content_block_delta') {
                const chunk = parsed?.delta?.text;
                if (typeof chunk === 'string' && chunk.length > 0) {
                    onChunk(chunk);
                }
            }
        } catch {
            // Ignore unparseable SSE lines
        }
    }

    /** 列出模型（GET /v1/models，data[].id）；端点不可用时回退配置模型 */
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
            return [this.config.model];
        }
    }
}
