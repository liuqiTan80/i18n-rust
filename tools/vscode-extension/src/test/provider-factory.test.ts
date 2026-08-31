/**
 * 单元测试：provider-factory（提供商预设与 provider 实例创建）
 *
 * 覆盖：预设列表完整性、默认地址/模型填充、custom 必须显式配置、
 * 不支持的提供商报错。全部为纯逻辑，无需 mock VS Code。
 */

import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { createProvider, listProviders, getProviderPreset } from '../ai/provider-factory';
import { AnthropicProvider } from '../ai/anthropic-provider';
import { GeminiProvider } from '../ai/gemini-provider';
import { OpenAICompatibleProvider } from '../ai/openai-provider';
import { AIError, ProviderId } from '../ai/types';

/** 构造一份合法的最小配置（各用例按需覆盖字段） */
function baseConfig(overrides: Record<string, unknown> = {}): any {
    return {
        provider: 'openai',
        apiKey: '',
        baseUrl: '',
        model: '',
        temperature: 0.1,
        maxTokens: 2048,
        systemPrompt: '',
        timeout: 60,
        ...overrides
    };
}

// ============================================================
// 预设列表
// ============================================================

test('listProviders：返回全部 8 个提供商预设', () => {
    const presets = listProviders();
    assert.equal(presets.length, 8);
    const ids = presets.map(p => p.id).sort();
    assert.deepEqual(ids, ['anthropic', 'custom', 'deepseek', 'gemini', 'glm', 'ollama', 'openai', 'qwen']);
});

test('listProviders：每项预设字段完整（地址/模型/密钥要求）', () => {
    for (const preset of listProviders()) {
        assert.ok(preset.displayName.length > 0, `${preset.id} 缺显示名`);
        assert.equal(typeof preset.defaultBaseUrl, 'string');
        assert.equal(typeof preset.defaultModel, 'string');
        assert.equal(typeof preset.requiresApiKey, 'boolean');
    }
});

test('getProviderPreset：ollama 本地模型不需要 API 密钥', () => {
    const preset = getProviderPreset('ollama');
    assert.equal(preset.requiresApiKey, false);
    assert.equal(preset.defaultBaseUrl, 'http://localhost:11434/v1');
});

test('getProviderPreset：云端提供商均要求 API 密钥', () => {
    const cloudIds: ProviderId[] = ['openai', 'deepseek', 'qwen', 'glm', 'anthropic', 'gemini'];
    for (const id of cloudIds) {
        assert.equal(getProviderPreset(id).requiresApiKey, true, `${id} 应要求密钥`);
    }
});

test('getProviderPreset：Anthropic 默认地址与模型', () => {
    const preset = getProviderPreset('anthropic');
    assert.equal(preset.defaultBaseUrl, 'https://api.anthropic.com');
    assert.equal(preset.defaultModel, 'claude-sonnet-4-5');
});

test('getProviderPreset：Gemini 默认地址与模型', () => {
    const preset = getProviderPreset('gemini');
    assert.equal(preset.defaultBaseUrl, 'https://generativelanguage.googleapis.com');
    assert.equal(preset.defaultModel, 'gemini-2.5-pro');
});

// ============================================================
// createProvider：默认值填充
// ============================================================

test('createProvider：deepseek 未配置地址/模型时填充默认值', async () => {
    const calls: string[] = [];
    const originalFetch = globalThis.fetch;
    (globalThis as any).fetch = async (url: string) => {
        calls.push(String(url));
        return new Response(JSON.stringify({ data: [{ id: 'deepseek-chat' }] }), { status: 200 });
    };
    try {
        const provider = createProvider(baseConfig({ provider: 'deepseek', apiKey: 'sk-x' }));
        assert.ok(provider instanceof OpenAICompatibleProvider);
        await provider.listModels();
        assert.ok(
            calls[0].startsWith('https://api.deepseek.com/v1/models'),
            `应使用 deepseek 默认地址，实际请求：${calls[0]}`
        );
    } finally {
        (globalThis as any).fetch = originalFetch;
    }
});

test('createProvider：自定义 baseUrl 时不再覆盖用户配置', async () => {
    const calls: string[] = [];
    const originalFetch = globalThis.fetch;
    (globalThis as any).fetch = async (url: string) => {
        calls.push(String(url));
        return new Response(JSON.stringify({ data: [] }), { status: 200 });
    };
    try {
        const provider = createProvider(
            baseConfig({ provider: 'openai', baseUrl: 'https://my-proxy.example/v1' })
        );
        await provider.listModels();
        assert.ok(calls[0].startsWith('https://my-proxy.example/v1/models'));
    } finally {
        (globalThis as any).fetch = originalFetch;
    }
});

test('createProvider：custom 未配置地址时报配置缺失', () => {
    assert.throws(
        () => createProvider(baseConfig({ provider: 'custom' })),
        (e: AIError) => e.category === '配置缺失'
    );
});

test('createProvider：custom 仅配置地址未配置模型时报配置缺失', () => {
    assert.throws(
        () => createProvider(baseConfig({ provider: 'custom', baseUrl: 'http://x/v1' })),
        (e: AIError) => e.category === '配置缺失'
    );
});

test('createProvider：custom 完整配置可创建 provider', () => {
    const provider = createProvider(
        baseConfig({ provider: 'custom', baseUrl: 'http://x/v1', model: 'm' })
    );
    assert.ok(provider instanceof OpenAICompatibleProvider);
});

test('createProvider：不支持的提供商抛「不支持」错误', () => {
    assert.throws(
        () => createProvider(baseConfig({ provider: 'unknown-vendor' })),
        (e: AIError) => e.category === '不支持'
    );
});

test('createProvider：Anthropic 未配置地址/模型时填充默认值并分派专有实现', async () => {
    const calls: string[] = [];
    const originalFetch = globalThis.fetch;
    (globalThis as any).fetch = async (url: string) => {
        calls.push(String(url));
        return new Response(JSON.stringify({ data: [{ id: 'claude-sonnet-4-5' }] }), { status: 200 });
    };
    try {
        const provider = createProvider(baseConfig({ provider: 'anthropic', apiKey: 'sk-ant-x' }));
        assert.ok(provider instanceof AnthropicProvider);
        await provider.listModels();
        assert.ok(
            calls[0].startsWith('https://api.anthropic.com/v1/models'),
            `应使用 anthropic 默认地址，实际请求：${calls[0]}`
        );
    } finally {
        (globalThis as any).fetch = originalFetch;
    }
});

test('createProvider：Gemini 未配置地址/模型时填充默认值并分派专有实现', async () => {
    const calls: string[] = [];
    const originalFetch = globalThis.fetch;
    (globalThis as any).fetch = async (url: string) => {
        calls.push(String(url));
        return new Response(JSON.stringify({ models: [{ name: 'models/gemini-2.5-pro' }] }), { status: 200 });
    };
    try {
        const provider = createProvider(baseConfig({ provider: 'gemini', apiKey: 'AIza-x' }));
        assert.ok(provider instanceof GeminiProvider);
        await provider.listModels();
        assert.ok(
            calls[0].startsWith('https://generativelanguage.googleapis.com/v1beta/models'),
            `应使用 gemini 默认地址，实际请求：${calls[0]}`
        );
    } finally {
        (globalThis as any).fetch = originalFetch;
    }
});

test('createProvider：Anthropic/Gemini 未配置地址时也填充默认值', () => {
    assert.ok(createProvider(baseConfig({ provider: 'anthropic', apiKey: 'k' })) instanceof AnthropicProvider);
    assert.ok(createProvider(baseConfig({ provider: 'gemini', apiKey: 'k' })) instanceof GeminiProvider);
});

test('createProvider：云端提供商未配置地址时也会填充默认值并走 OpenAI 兼容实现（openai）', () => {
    const provider = createProvider(baseConfig({ provider: 'openai', apiKey: 'k' }));
    assert.ok(provider instanceof OpenAICompatibleProvider);
});
