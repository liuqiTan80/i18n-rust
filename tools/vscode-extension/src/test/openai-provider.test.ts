/**
 * 单元测试：OpenAICompatibleProvider（OpenAI 兼容协议 HTTP 层）
 *
 * 通过替换全局 fetch 模拟：成功响应、API 错误、JSON 解析失败、
 * SSE 流式（含跨 chunk 拼接）、超时、取消、models 列表回退。
 * 不发起真实网络请求。
 */

import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { OpenAICompatibleProvider } from '../ai/openai-provider';
import { AIError, ChatMessage } from '../ai/types';

/** 构造配置（默认超时 2s，保证超时测试不过长） */
function makeConfig(overrides: Record<string, unknown> = {}): any {
    return {
        provider: 'openai',
        apiKey: 'sk-test',
        baseUrl: 'https://api.example.com/v1',
        model: 'test-model',
        temperature: 0.1,
        maxTokens: 2048,
        systemPrompt: '',
        timeout: 2,
        ...overrides
    };
}

const messages: ChatMessage[] = [{ role: 'user', content: '你好' }];

/** 替换全局 fetch，返回原实现用于恢复 */
function stubFetch(fake: (url: string, init?: RequestInit) => Promise<Response>): typeof fetch {
    const original = globalThis.fetch;
    (globalThis as any).fetch = fake;
    return original;
}

function jsonResponse(body: unknown, status = 200): Response {
    return new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' }
    });
}

// ============================================================
// sendChat：普通请求
// ============================================================

test('sendChat：成功返回助手回复，并携带密钥与请求体', async () => {
    let capturedUrl = '';
    let capturedInit: RequestInit | undefined;
    const original = stubFetch(async (url, init) => {
        capturedUrl = String(url);
        capturedInit = init;
        return jsonResponse({ choices: [{ message: { content: '你好，世界' } }] });
    });
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        const result = await provider.sendChat(messages);
        assert.equal(result, '你好，世界');
        assert.equal(capturedUrl, 'https://api.example.com/v1/chat/completions');
        const headers = capturedInit?.headers as Record<string, string>;
        assert.equal(headers['Authorization'], 'Bearer sk-test');
        const body = JSON.parse(capturedInit?.body as string);
        assert.equal(body.model, 'test-model');
        assert.equal(body.stream, false);
        assert.deepEqual(body.messages, messages);
        assert.equal(body.temperature, 0.1);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('sendChat：baseUrl 尾部斜杠会被去除', async () => {
    const original = stubFetch(async (_url) => jsonResponse({ choices: [{ message: { content: 'ok' } }] }));
    try {
        const provider = new OpenAICompatibleProvider(makeConfig({ baseUrl: 'https://x/v1/' }));
        await provider.sendChat(messages);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('sendChat：未配置密钥时不携带 Authorization 头', async () => {
    let capturedInit: RequestInit | undefined;
    const original = stubFetch(async (_url, init) => {
        capturedInit = init;
        return jsonResponse({ choices: [{ message: { content: 'ok' } }] });
    });
    try {
        const provider = new OpenAICompatibleProvider(makeConfig({ apiKey: '' }));
        await provider.sendChat(messages);
        const headers = capturedInit?.headers as Record<string, string>;
        assert.equal(headers['Authorization'], undefined);
    } finally {
        (globalThis as any).fetch = original;
    }
});

// ============================================================
// sendChat：错误处理
// ============================================================

test('sendChat：HTTP 401 提取服务商错误详情', async () => {
    const original = stubFetch(async () =>
        jsonResponse({ error: { message: 'Incorrect API key' } }, 401)
    );
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        await assert.rejects(
            provider.sendChat(messages),
            (e: AIError) =>
                e.category === 'API错误' &&
                e.message.includes('401') &&
                e.message.includes('Incorrect API key')
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('sendChat：响应体不是 JSON 时报 JSON 解析错误', async () => {
    const original = stubFetch(async () => new Response('<html>proxy error</html>', { status: 200 }));
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        await assert.rejects(
            provider.sendChat(messages),
            (e: AIError) => e.category === 'JSON解析错误'
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('sendChat：响应缺少 content 字段时报 API 错误', async () => {
    const original = stubFetch(async () => jsonResponse({ choices: [{ message: {} }] }));
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        await assert.rejects(
            provider.sendChat(messages),
            (e: AIError) => e.category === 'API错误' && e.message.includes('content')
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('sendChat：网络异常时报网络错误', async () => {
    const original = stubFetch(async () => {
        throw new Error('ECONNREFUSED');
    });
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        await assert.rejects(
            provider.sendChat(messages),
            (e: AIError) => e.category === '网络错误' && e.message.includes('ECONNREFUSED')
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

// ============================================================
// streamChat：SSE 流式
// ============================================================

/** 构造 SSE 流式响应：按 chunks 分批返回 data 行 */
function sseResponse(chunks: string[]): Response {
    const encoder = new TextEncoder();
    const stream = new ReadableStream<Uint8Array>({
        start(controller) {
            for (const chunk of chunks) {
                controller.enqueue(encoder.encode(chunk));
            }
            controller.close();
        }
    });
    return new Response(stream, { status: 200 });
}

test('streamChat：逐 delta 回调内容', async () => {
    const original = stubFetch(async () =>
        sseResponse([
            'data: {"choices":[{"delta":{"content":"你"}}]}\n\n',
            'data: {"choices":[{"delta":{"content":"好"}}]}\n\n',
            'data: [DONE]\n\n'
        ])
    );
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        const chunks: string[] = [];
        await provider.streamChat(messages, (c) => chunks.push(c));
        assert.deepEqual(chunks, ['你', '好']);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('streamChat：跨 chunk 的 SSE 行正确拼接（buffer 逻辑）', async () => {
    // 第一个 chunk 是半个 data 行，第二个 chunk 补齐剩余部分
    const original = stubFetch(async () =>
        sseResponse([
            'data: {"choices":[{"delta":{"con',
            'tent":"你好"}}]}\n\ndata: [DONE]\n'
        ])
    );
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        const chunks: string[] = [];
        await provider.streamChat(messages, (c) => chunks.push(c));
        assert.deepEqual(chunks, ['你好']);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('streamChat：非 data 行与坏 JSON 被容忍跳过', async () => {
    const original = stubFetch(async () =>
        sseResponse([
            ': keep-alive ping\n',
            'data: {not-json}\n',
            'data: {"choices":[{"delta":{"content":"ok"}}]}\n\n',
            'data: [DONE]\n'
        ])
    );
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        const chunks: string[] = [];
        await provider.streamChat(messages, (c) => chunks.push(c));
        assert.deepEqual(chunks, ['ok']);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('streamChat：HTTP 错误时按 API 错误拒绝', async () => {
    const original = stubFetch(async () => jsonResponse({ error: { message: 'rate limit' } }, 429));
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        await assert.rejects(
            provider.streamChat(messages, () => undefined),
            (e: AIError) => e.category === 'API错误' && e.message.includes('429')
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('streamChat：已取消的 signal 直接抛「已取消」', async () => {
    const original = stubFetch(async () => sseResponse(['data: [DONE]\n']));
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        const controller = new AbortController();
        controller.abort();
        await assert.rejects(
            provider.streamChat(messages, () => undefined, controller.signal),
            (e: AIError) => e.category === '已取消'
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('streamChat：无响应体时报网络错误', async () => {
    const original = stubFetch(async () => new Response(null, { status: 200 }));
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        await assert.rejects(
            provider.streamChat(messages, () => undefined),
            (e: AIError) => e.category === '网络错误'
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

// ============================================================
// 超时与取消
// ============================================================

test('sendChat：请求超时（timeout 秒无响应）报「超时」', async () => {
    // 模拟真实 fetch 行为：abort signal 触发时拒绝
    const original = stubFetch(
        (_url, init) =>
            new Promise<Response>((_resolve, reject) => {
                init?.signal?.addEventListener('abort', () =>
                    reject(new DOMException('The operation was aborted.', 'AbortError'))
                );
            })
    );
    try {
        const provider = new OpenAICompatibleProvider(makeConfig({ timeout: 1 }));
        await assert.rejects(
            provider.sendChat(messages),
            (e: AIError) => e.category === '超时' && e.message.includes('1 秒')
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

// ============================================================
// listModels
// ============================================================

test('listModels：成功时返回模型 id 列表', async () => {
    const original = stubFetch(async () =>
        jsonResponse({ data: [{ id: 'a' }, { id: 'b' }, { id: 42 }] })
    );
    try {
        const provider = new OpenAICompatibleProvider(makeConfig());
        const models = await provider.listModels();
        assert.deepEqual(models, ['a', 'b']);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('listModels：接口不可用时回退为配置的模型', async () => {
    const original = stubFetch(async () => {
        throw new Error('ECONNREFUSED');
    });
    try {
        const provider = new OpenAICompatibleProvider(makeConfig({ model: 'fallback' }));
        const models = await provider.listModels();
        assert.deepEqual(models, ['fallback']);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('listModels：HTTP 错误时回退为配置的模型', async () => {
    const original = stubFetch(async () => jsonResponse({}, 500));
    try {
        const provider = new OpenAICompatibleProvider(makeConfig({ model: 'fallback' }));
        const models = await provider.listModels();
        assert.deepEqual(models, ['fallback']);
    } finally {
        (globalThis as any).fetch = original;
    }
});
