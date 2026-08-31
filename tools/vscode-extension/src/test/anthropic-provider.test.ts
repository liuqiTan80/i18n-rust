/**
 * 单元测试：AnthropicProvider（Claude Messages API HTTP 层）
 *
 * 通过替换全局 fetch 模拟：认证/版本头、system 顶层提升、content 块拼接、
 * SSE content_block_delta 流式、models 列表与回退。不发起真实网络请求。
 */

import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { AnthropicProvider } from '../ai/anthropic-provider';
import { AIError, ChatMessage } from '../ai/types';

/** 构造配置（默认超时 2s，保证超时测试不过长） */
function makeConfig(overrides: Record<string, unknown> = {}): any {
    return {
        provider: 'anthropic',
        apiKey: 'sk-ant-test',
        baseUrl: 'https://api.anthropic.com',
        model: 'claude-sonnet-4-5',
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

test('sendChat：成功返回助手回复（多 text 块拼接），携带 x-api-key 与 anthropic-version 头', async () => {
    let capturedUrl = '';
    let capturedInit: RequestInit | undefined;
    const original = stubFetch(async (url, init) => {
        capturedUrl = String(url);
        capturedInit = init;
        return jsonResponse({
            content: [
                { type: 'text', text: '你好，' },
                { type: 'text', text: '世界' }
            ]
        });
    });
    try {
        const provider = new AnthropicProvider(makeConfig());
        const result = await provider.sendChat(messages);
        assert.equal(result, '你好，世界');
        assert.equal(capturedUrl, 'https://api.anthropic.com/v1/messages');
        const headers = capturedInit?.headers as Record<string, string>;
        assert.equal(headers['x-api-key'], 'sk-ant-test');
        assert.equal(headers['anthropic-version'], '2023-06-01');
        assert.equal(headers['Authorization'], undefined);
        const body = JSON.parse(capturedInit?.body as string);
        assert.equal(body.model, 'claude-sonnet-4-5');
        assert.equal(body.max_tokens, 2048);
        assert.equal(body.stream, false);
        assert.equal(body.temperature, 0.1);
        assert.deepEqual(body.messages, messages);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('sendChat：system 消息提升为顶层 system 字段，不进 messages', async () => {
    let capturedInit: RequestInit | undefined;
    const original = stubFetch(async (_url, init) => {
        capturedInit = init;
        return jsonResponse({ content: [{ type: 'text', text: 'ok' }] });
    });
    try {
        const provider = new AnthropicProvider(makeConfig());
        await provider.sendChat([
            { role: 'system', content: '你是教学助手' },
            { role: 'user', content: '解释所有权' }
        ]);
        const body = JSON.parse(capturedInit?.body as string);
        assert.equal(body.system, '你是教学助手');
        assert.deepEqual(body.messages, [{ role: 'user', content: '解释所有权' }]);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('sendChat：baseUrl 尾部斜杠会被去除且 /v1 由 provider 补齐', async () => {
    let capturedUrl = '';
    const original = stubFetch(async (url) => {
        capturedUrl = String(url);
        return jsonResponse({ content: [{ type: 'text', text: 'ok' }] });
    });
    try {
        const provider = new AnthropicProvider(makeConfig({ baseUrl: 'https://proxy.example/' }));
        await provider.sendChat(messages);
        assert.equal(capturedUrl, 'https://proxy.example/v1/messages');
    } finally {
        (globalThis as any).fetch = original;
    }
});

// ============================================================
// sendChat：错误处理
// ============================================================

test('sendChat：HTTP 401 提取服务商错误详情', async () => {
    const original = stubFetch(async () =>
        jsonResponse({ type: 'error', error: { type: 'authentication_error', message: 'invalid x-api-key' } }, 401)
    );
    try {
        const provider = new AnthropicProvider(makeConfig());
        await assert.rejects(
            provider.sendChat(messages),
            (e: AIError) => e.category === 'API错误' && e.message.includes('invalid x-api-key')
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('sendChat：响应缺少 text 块时报 API 错误', async () => {
    const original = stubFetch(async () => jsonResponse({ content: [{ type: 'tool_use', id: 'x' }] }));
    try {
        const provider = new AnthropicProvider(makeConfig());
        await assert.rejects(
            provider.sendChat(messages),
            (e: AIError) => e.category === 'API错误' && e.message.includes('content')
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

// ============================================================
// streamChat：SSE 流式
// ============================================================

test('streamChat：逐 content_block_delta 回调内容，其他事件跳过', async () => {
    const original = stubFetch(async () =>
        sseResponse([
            'event: message_start\ndata: {"type":"message_start","message":{"id":"m1"}}\n\n',
            'event: content_block_delta\ndata: {"type":"content_block_delta","delta":{"type":"text_delta","text":"你"}}\n\n',
            'event: content_block_delta\ndata: {"type":"content_block_delta","delta":{"type":"text_delta","text":"好"}}\n\n',
            'event: message_stop\ndata: {"type":"message_stop"}\n\n'
        ])
    );
    try {
        const provider = new AnthropicProvider(makeConfig());
        const chunks: string[] = [];
        await provider.streamChat(messages, (c) => chunks.push(c));
        assert.deepEqual(chunks, ['你', '好']);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('streamChat：跨 chunk 的 SSE 行正确拼接（buffer 逻辑）', async () => {
    const original = stubFetch(async () =>
        sseResponse([
            'data: {"type":"content_block_delta","delta":{"type":"text_delta","te',
            'xt":"你好"}}\n\n'
        ])
    );
    try {
        const provider = new AnthropicProvider(makeConfig());
        const chunks: string[] = [];
        await provider.streamChat(messages, (c) => chunks.push(c));
        assert.deepEqual(chunks, ['你好']);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('streamChat：HTTP 错误时按 API 错误拒绝', async () => {
    const original = stubFetch(async () => jsonResponse({ error: { message: 'overloaded' } }, 529));
    try {
        const provider = new AnthropicProvider(makeConfig());
        await assert.rejects(
            provider.streamChat(messages, () => undefined),
            (e: AIError) => e.category === 'API错误' && e.message.includes('529')
        );
    } finally {
        (globalThis as any).fetch = original;
    }
});

// ============================================================
// listModels
// ============================================================

test('listModels：成功时返回 data[].id 列表', async () => {
    const original = stubFetch(async () =>
        jsonResponse({ data: [{ id: 'claude-sonnet-4-5' }, { id: 'claude-opus-4-1' }] })
    );
    try {
        const provider = new AnthropicProvider(makeConfig());
        const models = await provider.listModels();
        assert.deepEqual(models, ['claude-sonnet-4-5', 'claude-opus-4-1']);
    } finally {
        (globalThis as any).fetch = original;
    }
});

test('listModels：接口不可用时回退为配置的模型', async () => {
    const original = stubFetch(async () => {
        throw new Error('ECONNREFUSED');
    });
    try {
        const provider = new AnthropicProvider(makeConfig({ model: 'fallback' }));
        const models = await provider.listModels();
        assert.deepEqual(models, ['fallback']);
    } finally {
        (globalThis as any).fetch = original;
    }
});
