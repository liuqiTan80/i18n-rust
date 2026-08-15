/**
 * OpenAI 兼容提供商
 *
 * 实现 OpenAI 兼容的 chat/completions 协议（含 SSE 流式），
 * 通过 baseUrl 配置切换不同服务商：
 * - DeepSeek：https://api.deepseek.com/v1
 * - 通义千问：https://dashscope.aliyuncs.com/compatible-mode/v1
 * - 智谱 GLM：https://open.bigmodel.cn/api/paas/v4
 * - Ollama：http://localhost:11434/v1
 * - 自定义：用户任意填写
 *
 * 均支持 发送对话 / 流式对话 / 获取模型列表（GET /models）。
 */

import { 提供商抽象 } from './提供商抽象';
import { AI配置, AI错误, 消息, 流式回调 } from './类型定义';

/**
 * OpenAI 兼容协议提供商
 */
export class OpenAI兼容提供商 extends 提供商抽象 {
    /** 对话端点 */
    private 对话端点(): string {
        return this.配置.baseUrl.replace(/\/+$/, '') + '/chat/completions';
    }

    /** 模型列表端点 */
    private 模型端点(): string {
        return this.配置.baseUrl.replace(/\/+$/, '') + '/models';
    }

    /** 请求头：配置了密钥时附加 Bearer 认证（Ollama 本地服务可无密钥） */
    private 请求头(): Record<string, string> {
        const 头: Record<string, string> = { 'Content-Type': 'application/json' };
        if (this.配置.apiKey) {
            头['Authorization'] = `Bearer ${this.配置.apiKey}`;
        }
        return 头;
    }

    /** 对话请求体 */
    private 请求体(messages: 消息[], 流式: boolean): unknown {
        return {
            model: this.配置.model,
            messages,
            temperature: this.配置.temperature,
            max_tokens: this.配置.maxTokens,
            stream: 流式
        };
    }

    /** 发送一次完整对话，返回助手回复全文 */
    async 发送对话(messages: 消息[]): Promise<string> {
        const 响应 = await this.请求(this.对话端点(), {
            method: 'POST',
            headers: this.请求头(),
            body: JSON.stringify(this.请求体(messages, false))
        });
        const 数据 = (await this.校验响应(响应, this.对话端点())) as any;
        const 内容 = 数据?.choices?.[0]?.message?.content;
        if (typeof 内容 !== 'string') {
            throw new AI错误(
                'API错误',
                `API 响应缺少 choices[0].message.content 字段：${JSON.stringify(数据).slice(0, 300)}`
            );
        }
        return 内容;
    }

    /** 流式对话：按 SSE 增量解析，逐块回调内容 */
    async 流式对话(messages: 消息[], onChunk: 流式回调): Promise<void> {
        const 响应 = await this.请求(this.对话端点(), {
            method: 'POST',
            headers: this.请求头(),
            body: JSON.stringify(this.请求体(messages, true))
        });
        if (!响应.ok) {
            await this.校验响应(响应, this.对话端点());
            return;
        }
        const 读取器 = 响应.body?.getReader();
        if (!读取器) {
            throw new AI错误('网络错误', '流式响应无法读取（响应体为空）');
        }
        const 解码器 = new TextDecoder();
        let 缓冲 = '';
        try {
            while (true) {
                const { done, value } = await 读取器.read();
                if (done) {
                    break;
                }
                // 按行切分 SSE 数据（最后一段可能不完整，留到下一轮）
                缓冲 += 解码器.decode(value, { stream: true });
                const 行们 = 缓冲.split('\n');
                缓冲 = 行们.pop() ?? '';
                for (const 行 of 行们) {
                    this.处理SSE行(行, onChunk);
                }
            }
            if (缓冲.trim()) {
                this.处理SSE行(缓冲, onChunk);
            }
        } catch (错误) {
            if (错误 instanceof AI错误) {
                throw 错误;
            }
            throw new AI错误('网络错误', `流式读取中断：${(错误 as Error).message}`);
        } finally {
            try {
                读取器.releaseLock();
            } catch {
                // 忽略释放锁时的异常
            }
        }
    }

    /**
     * 处理单行 SSE 数据（data: {json} 或 data: [DONE]）
     * 无法解析的行直接跳过，兼容服务商差异。
     */
    private 处理SSE行(行: string, onChunk: 流式回调): void {
        const 内容 = 行.trim();
        if (!内容.startsWith('data:')) {
            return;
        }
        const 数据 = 内容.slice(5).trim();
        if (!数据 || 数据 === '[DONE]') {
            return;
        }
        try {
            const 解析 = JSON.parse(数据) as any;
            const 片段 = 解析?.choices?.[0]?.delta?.content;
            if (typeof 片段 === 'string' && 片段.length > 0) {
                onChunk(片段);
            }
        } catch {
            // 忽略无法解析的 SSE 行
        }
    }

    /** 获取模型列表（GET /models）；端点不可用时回退为当前配置的模型，不中断流程 */
    async 获取模型列表(): Promise<string[]> {
        try {
            const 响应 = await this.请求(this.模型端点(), {
                method: 'GET',
                headers: this.请求头()
            });
            if (!响应.ok) {
                return [this.配置.model];
            }
            const 数据 = (await this.校验响应(响应, this.模型端点())) as any;
            const 模型列表 = Array.isArray(数据?.data)
                ? 数据.data.map((项: any) => 项?.id).filter((id: unknown): id is string => typeof id === 'string')
                : [];
            return 模型列表.length > 0 ? 模型列表 : [this.配置.model];
        } catch {
            // 模型端点不可用（如某些兼容网关）时回退为当前配置的模型
            return [this.配置.model];
        }
    }
}
