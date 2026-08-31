/**
 * Gemini provider（Google Gemini generateContent API）
 *
 * Gemini 不使用 OpenAI 兼容协议，协议差异在本类中适配：
 * - 认证头 x-goog-api-key（而非 Authorization: Bearer）
 * - 端点按模型动态拼接：/v1beta/models/{model}:generateContent 与 :streamGenerateContent
 * - system 消息映射为顶层 systemInstruction；contents 角色为 user / model
 * - maxOutputTokens 在 generationConfig 内（而非顶层 max_tokens）
 * - 响应为 candidates[].content.parts[].text；流式 SSE 的 data 行同样是该结构
 * - 模型列表：GET /v1beta/models，字段 models[].name（带 models/ 前缀需剥离）
 */

import { ProviderInterface } from './provider-interface';
import { AIError, ChatMessage, StreamCallback } from './types';

/**
 * Gemini provider
 */
export class GeminiProvider extends ProviderInterface {
    /** 生成端点：按模型拼接，流式时加 alt=sse */
    private generateEndpoint(stream: boolean): string {
        const base = this.config.baseUrl.replace(/\/+$/, '');
        const action = stream ? ':streamGenerateContent' : ':generateContent';
        const query = stream ? '?alt=sse' : '';
        return `${base}/v1beta/models/${encodeURIComponent(this.config.model)}${action}${query}`;
    }

    /** 模型列表端点（密钥走请求头，query 不留密钥避免泄漏到日志/代理） */
    private modelsEndpoint(): string {
        return this.config.baseUrl.replace(/\/+$/, '') + '/v1beta/models';
    }

    /** 请求头：x-goog-api-key（官方支持的密钥头，避免 query 泄漏） */
    private buildHeaders(): Record<string, string> {
        const headers: Record<string, string> = { 'Content-Type': 'application/json' };
        if (this.config.apiKey) {
            headers['x-goog-api-key'] = this.config.apiKey;
        }
        return headers;
    }

    /**
     * 请求体：system 消息映射为顶层 systemInstruction（取全部 system 消息拼接，
     * Gemini 不支持 system 角色）；contents 角色 user / assistant → user / model。
     */
    private buildBody(messages: ChatMessage[]): unknown {
        const system = messages
            .filter(m => m.role === 'system')
            .map(m => m.content)
            .join('\n');
        const contents = messages
            .filter(m => m.role !== 'system')
            .map(m => ({
                role: m.role === 'assistant' ? 'model' : 'user',
                parts: [{ text: m.content }]
            }));
        const body: Record<string, unknown> = {
            contents,
            generationConfig: {
                temperature: this.config.temperature,
                maxOutputTokens: this.config.maxTokens
            }
        };
        if (system) {
            body.systemInstruction = { parts: [{ text: system }] };
        }
        return body;
    }

    /** 从响应中提取全部文本：candidates[].content.parts[].text 拼接 */
    private extractText(data: any): string | undefined {
        const parts: string[] = [];
        for (const candidate of data?.candidates ?? []) {
            for (const part of candidate?.content?.parts ?? []) {
                if (typeof part?.text === 'string') {
                    parts.push(part.text);
                }
            }
        }
        return parts.length > 0 ? parts.join('') : undefined;
    }

    /** 发送一轮完整对话，返回助手完整回复 */
    async sendChat(messages: ChatMessage[], signal?: AbortSignal): Promise<string> {
        const endpoint = this.generateEndpoint(false);
        const response = await this.request(endpoint, {
            method: 'POST',
            headers: this.buildHeaders(),
            body: JSON.stringify(this.buildBody(messages))
        }, signal);
        const data = (await this.validateResponse(response, endpoint)) as any;
        const text = this.extractText(data);
        if (text === undefined) {
            throw new AIError(
                'API错误',
                `API 响应缺少 candidates[].content.parts[].text 字段：${JSON.stringify(data).slice(0, 300)}`
            );
        }
        return text;
    }

    /** 流式对话：SSE 事件 data 行同为 candidates 结构，逐块输出 text */
    async streamChat(messages: ChatMessage[], onChunk: StreamCallback, signal?: AbortSignal): Promise<void> {
        const endpoint = this.generateEndpoint(true);
        const response = await this.request(endpoint, {
            method: 'POST',
            headers: this.buildHeaders(),
            body: JSON.stringify(this.buildBody(messages))
        }, signal);
        if (!response.ok) {
            await this.validateResponse(response, endpoint);
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
     * Gemini 流式 data 行与普通响应同构（candidates[].content.parts[]），
     * 无 candidates 的事件（如 usageMetadata / promptFeedback）直接跳过。
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
            const text = this.extractText(JSON.parse(data) as any);
            if (typeof text === 'string' && text.length > 0) {
                onChunk(text);
            }
        } catch {
            // Ignore unparseable SSE lines
        }
    }

    /** 列出模型（GET /v1beta/models，models[].name 剥离 models/ 前缀）；端点不可用时回退配置模型 */
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
            const models = Array.isArray(data?.models)
                ? data.models
                    .map((item: any) => typeof item?.name === 'string' ? item.name.replace(/^models\//, '') : undefined)
                    .filter((name: unknown): name is string => typeof name === 'string' && name.length > 0)
                : [];
            return models.length > 0 ? models : [this.config.model];
        } catch {
            return [this.config.model];
        }
    }
}
