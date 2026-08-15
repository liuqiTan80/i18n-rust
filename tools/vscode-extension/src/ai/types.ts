/**
 * AI provider abstraction layer - shared type definitions
 *
 * Defines the common data types and unified error type for all AI providers.
 * Currently all providers use the OpenAI-compatible protocol.
 */

/**
 * Chat message (same format as the OpenAI-compatible protocol)
 */
export interface ChatMessage {
    /** Role: system / user / assistant */
    role: 'system' | 'user' | 'assistant';
    /** Message content */
    content: string;
}

/**
 * Supported provider identifiers
 * - openai / deepseek / qwen / glm: cloud services using the OpenAI-compatible protocol
 * - ollama: local model service (also OpenAI-compatible, no API key required)
 * - custom: user-defined arbitrary baseUrl
 */
export type ProviderId = 'openai' | 'deepseek' | 'qwen' | 'glm' | 'ollama' | 'custom';

/**
 * AI-related configuration (corresponds to i18n-rust.ai.* settings)
 */
export interface AIConfig {
    /** Provider identifier */
    provider: ProviderId;
    /** API key (may be empty for local services like Ollama) */
    apiKey: string;
    /** API base URL (empty means use the provider default) */
    baseUrl: string;
    /** Model name (empty means use the provider default) */
    model: string;
    /** Sampling temperature: lower is more deterministic (0~2, default 0.1 for teaching) */
    temperature: number;
    /** Maximum generated tokens (default 2048) */
    maxTokens: number;
    /** Custom system prompt (optional; overrides the auto-generated one when set) */
    systemPrompt: string;
    /** Request timeout in seconds (default 60) */
    timeout: number;
}

/**
 * Provider preset info (used by the factory for defaults and UI display)
 */
export interface ProviderPreset {
    /** Provider identifier */
    id: ProviderId;
    /** Display name */
    displayName: string;
    /** Default API base URL */
    defaultBaseUrl: string;
    /** Default model name */
    defaultModel: string;
    /** Whether an API key is required */
    requiresApiKey: boolean;
}

/**
 * Unified error categories
 */
export type ErrorCategory =
    | '配置缺失'
    | '网络错误'
    | '超时'
    | 'API错误'
    | 'JSON解析错误'
    | '不支持';

/**
 * Unified AI error: thrown by all providers and the request layer,
 * carrying a category and a user-facing message.
 */
export class AIError extends Error {
    constructor(
        /** Error category */
        public category: ErrorCategory,
        message: string
    ) {
        super(message);
        this.name = 'AIError';
    }
}

/**
 * Callback type for streaming chat: content deltas are delivered chunk by chunk
 */
export type StreamCallback = (chunk: string) => void;
