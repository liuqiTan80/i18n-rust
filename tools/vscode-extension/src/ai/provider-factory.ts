/**
 * Provider factory
 *
 * Creates the provider instance matching the configured provider id,
 * filling in the provider's defaults when baseUrl / model are unset.
 */

import { OpenAICompatibleProvider } from './openai-provider';
import { ProviderInterface } from './provider-interface';
import { AIConfig, AIError, ProviderPreset, ProviderId } from './types';

/**
 * Presets for each provider (default address / default model / key requirement).
 * All providers use the OpenAI-compatible protocol; only defaults differ.
 */
const PRESETS: Record<ProviderId, ProviderPreset> = {
    openai: {
        id: 'openai',
        displayName: 'OpenAI',
        defaultBaseUrl: 'https://api.openai.com/v1',
        defaultModel: 'gpt-4o-mini',
        requiresApiKey: true
    },
    deepseek: {
        id: 'deepseek',
        displayName: 'DeepSeek 深度求索',
        defaultBaseUrl: 'https://api.deepseek.com/v1',
        defaultModel: 'deepseek-chat',
        requiresApiKey: true
    },
    qwen: {
        id: 'qwen',
        displayName: '通义千问（阿里云）',
        defaultBaseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
        defaultModel: 'qwen-plus',
        requiresApiKey: true
    },
    glm: {
        id: 'glm',
        displayName: '智谱 GLM',
        defaultBaseUrl: 'https://open.bigmodel.cn/api/paas/v4',
        defaultModel: 'glm-4-flash',
        requiresApiKey: true
    },
    ollama: {
        id: 'ollama',
        displayName: 'Ollama（本地模型）',
        defaultBaseUrl: 'http://localhost:11434/v1',
        defaultModel: 'qwen2.5',
        requiresApiKey: false
    },
    custom: {
        id: 'custom',
        displayName: '自定义（OpenAI 兼容）',
        defaultBaseUrl: '',
        defaultModel: '',
        requiresApiKey: false
    }
};

/**
 * Return all supported provider presets (for UI display and selection)
 */
export function listProviders(): ProviderPreset[] {
    return Object.values(PRESETS);
}

/**
 * Query a single provider preset
 */
export function getProviderPreset(id: ProviderId): ProviderPreset {
    return PRESETS[id];
}

/**
 * Create a provider instance from config:
 * - Fills in the provider's defaults when baseUrl / model are empty
 * - custom must explicitly configure baseUrl and model
 * - All providers use the OpenAI-compatible protocol
 *
 * Throws AIError (配置缺失 / 不支持).
 */
export function createProvider(config: AIConfig): ProviderInterface {
    const preset = PRESETS[config.provider];
    if (!preset) {
        throw new AIError('不支持', `不支持的提供商标识：${config.provider}（可选值：${Object.keys(PRESETS).join('、')}）`);
    }
    const fullConfig: AIConfig = {
        ...config,
        baseUrl: config.baseUrl || preset.defaultBaseUrl,
        model: config.model || preset.defaultModel
    };
    if (!fullConfig.baseUrl) {
        throw new AIError(
            '配置缺失',
            `提供商「${preset.displayName}」未配置 API 地址，请在设置中填写 i18n-rust.ai.baseUrl。`
        );
    }
    if (!fullConfig.model) {
        throw new AIError(
            '配置缺失',
            `提供商「${preset.displayName}」未配置模型名称，请在设置中填写 i18n-rust.ai.model。`
        );
    }
    return new OpenAICompatibleProvider(fullConfig);
}
