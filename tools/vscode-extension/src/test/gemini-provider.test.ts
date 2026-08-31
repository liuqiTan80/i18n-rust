/**
 * 单元测试：GeminiProvider（Google Gemini generateContent API HTTP 层）
 *
 * 通过替换全局 fetch 模拟：x-goog-api-key 头、角色映射与 systemInstruction、
 * generationConfig、candidates 提取、SSE 流式、models 列表与回退。
 * 不发起真实网络请求。
 */

import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { GeminiProvider } from '../ai/gemini-provider';
import { AIError, ChatMessage } from '../ai/types';

/** 构造配置（默认超时 2s，保证超时测试不过长） */
function makeConfig(overrides: Record<string, unknown> = {}): any {
    return {
        provider: 'gemini',
        apiKey: 'AIza-test',
        baseUrl: 'https://generativelanguage.googleapis.com',
        model: 'gemini-2.5-pro',
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

// ============================================================
// sendChat：普通请求
// ============================================================

test('sendChat：成功返回助手回复（多 parts 拼接），携带 x-goog-api-key 头', async () => {
    let capturedUrl = '';
    let capturedInit: RequestInit | undefined;
    const original = stubFetch(async (url, init) => {
        capturedUrl = String(url);
        capturedInit = init;
        return jsonResponse({
            candidates: [{
                content: {
                    role: 'model',
                    parts: [{ text: '你好，' }, { text: '世界' }]
                }
            }]
        });
    });
    try {
        const provider = new GeminiProvider(makeConfig());
        const result = await provider.sendChat(messages);
        assert.equal(result, '你好，世界');
        assert.equal(capturedUrl, 'https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent');
        const headers = capturedInit?.headers as Record<string, string>;
        assert.equal(headers['x-goog-api-key'], 'AIza-test');
        assert.equal(headers['Authorization'], undefined);
        const body = JSON.parse(capturedInit?.body as string);
        assert.deepEqual(body.contents, [{ role: 'user', parts: [{ text: '你好' }] }]);
        assert.deepEqual(body.generationConfig, { temperature: 0.1, maxOutputTokens: 2048 });
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('sendChat：system 映射为顶层 systemInstruction，assistant 映射为 model 角色', async () => {
    let capturedInit: RequestInit | undefined;
    const original = stubFetch(async (_url, init) => {
        capturedInit = init;
        return jsonResponse({ candidates: [{ content: { parts: [{ text: 'ok' }] } }] });
    });
    try {
        const provider = new GeminiProvider(makeConfig());
        await provider.sendChat([
            { role: 'system', content: '你是教学助手' },
            { role: 'user', content: '解释所有权' },
            { role: 'assistant', content: '好的' },
            { role: 'user', content: '继续' }
        ]);
        const body = JSON.parse(capturedInit?.body as string);
        assert.deepEqual(body.systemInstruction, { parts: [{ text: '你是教学助手' }] });
        assert.deepEqual(body.contents, [
            { role: 'user', parts: [{ text: '解释所有权' }] },
            { role: 'model', parts: [{ text: '好的' }] },
            { role: 'user', parts: [{ text: '继续' }] }
        ]);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('sendChat：baseUrl 尾部斜杠会被去除', async () => {
    let capturedUrl = '';
    const original = stubFetch(async (url) => {
        capturedUrl = String(url);
        return jsonResponse({ candidates: [{ content: { parts: [{ text: 'ok' }] } }] });
    });
    try {
        const provider = new GeminiProvider(makeConfig({ baseUrl: 'https://proxy.example/' }));
        await provider.sendChat(messages);
        assert.equal(capturedUrl, 'https://proxy.example/v1beta/models/gemini-2.5-pro:generateContent');
    } finally {
        (globalThis as any).fetch = original;
    }
});

// ============================================================
// sendChat：错误处理
// ============================================================

test('sendChat：HTTP 401 提取服务商错误详情', async () => {
    const original = stubFetch(async () =>
        jsonResponse({ error: { code: 401, message: 'API key not valid' } }, 401)
    );
    try {
        const provider = new GeminiProvider(makeConfig());
        await assert.rejects(
            provider.sendChat(messages),
            (e: AIError) => e.category === 'API错误' && e.message.includes('API key not valid')
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('sendChat：响应缺少 candidates 时报 API 错误', async () => {
    const original = stubFetch(async () => jsonResponse({ promptFeedback: { blockReason: 'SAFETY' } }));
    try {
        const provider = new GeminiProvider(makeConfig());
        await assert.rejects(
            provider.sendChat(messages),
            (e: AIError) => e.category === 'API错误' && e.message.includes('candidates')
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

// ============================================================
// streamChat：SSE 流式
// ============================================================

test('streamChat：请求端点含 alt=sse，逐 candidates 文本回调，无 candidates 事件跳过', async () => {
    let capturedUrl = '';
    const original = stubFetch(async (url) => {
        capturedUrl = String(url);
        return sseResponse([
            'data: {"candidates":[{"content":{"parts":[{"text":"你"}]}}]}\n\n',
            'data: {"usageMetadata":{"totalTokenCount":10}}\n\n',
            'data: {"candidates":[{"content":{"parts":[{"text":"好"}]}}]}\n\n'
        ]);
    });
    try {
        const provider = new GeminiProvider(makeConfig());
        const chunks: string[] = [];
        await provider.streamChat(messages, (c) => chunks.push(c));
        assert.deepEqual(chunks, ['你', '好']);
        assert.ok(capturedUrl.endsWith(':streamGenerateContent?alt=sse'), `应含 alt=sse，实际：${capturedUrl}`);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('streamChat：跨 chunk 的 SSE 行正确拼接（buffer 逻辑）', async () => {
    const original = stubFetch(async () =>
        sseResponse([
            'data: {"candidates":[{"content":{"parts":[{"te',
            'xt":"你好"}]}}]}\n\n'
        ])
    );
    try {
        const provider = new GeminiProvider(makeConfig());
        const chunks: string[] = [];
        await provider.streamChat(messages, (c) => chunks.push(c));
        assert.deepEqual(chunks, ['你好']);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('streamChat：HTTP 错误时按 API 错误拒绝', async () => {
    const original = stubFetch(async () => jsonResponse({ error: { message: 'quota exceeded' } }, 429));
    try {
        const provider = new GeminiProvider(makeConfig());
        await assert.rejects(
            provider.streamChat(messages, () => undefined),
            (e: AIError) => e.category === 'API错误' && e.message.includes('429')
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

// ============================================================
// listModels
// ============================================================

test('listModels：成功时返回剥离 models/ 前缀的名称列表', async () => {
    const original = stubFetch(async () =>
        jsonResponse({ models: [{ name: 'models/gemini-2.5-pro' }, { name: 'models/gemini-2.5-flash' }] })
    );
    try {
        const provider = new GeminiProvider(makeConfig());
        const models = await provider.listModels();
        assert.deepEqual(models, ['gemini-2.5-pro', 'gemini-2.5-flash']);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('listModels：接口不可用时回退为配置的模型', async () => {
    const original = stubFetch(async () => {
        throw new Error('ECONNREFUSED');
    });
    try {
        const provider = new GeminiProvider(makeConfig({ model: 'fallback' }));
        const models = await provider.listModels();
        assert.deepEqual(models, ['fallback']);
    } finally {
        (globalThis as any).fetch = original;
    }
});
