/**
 * AI 提供商抽象层 - 统一类型定义
 *
 * 定义所有 AI 提供商共用的数据类型与统一错误类型，
 * 不依赖具体服务商协议（当前统一为 OpenAI 兼容协议）。
 */

/**
 * 对话消息（与 OpenAI 兼容协议的消息格式一致）
 */
export interface 消息 {
    /** 角色：系统 / 用户 / 助手 */
    role: 'system' | 'user' | 'assistant';
    /** 消息内容 */
    content: string;
}

/**
 * 支持的提供商标识
 * - openai / deepseek / qwen / glm：云端服务，使用 OpenAI 兼容协议
 * - ollama：本地模型服务，同样兼容 OpenAI 协议（无需密钥）
 * - custom：用户自定义任意 baseUrl
 */
export type 提供商标识 = 'openai' | 'deepseek' | 'qwen' | 'glm' | 'ollama' | 'custom';

/**
 * AI 相关配置（对应 package.json 中 i18n-rust.ai.* 配置项）
 */
export interface AI配置 {
    /** 提供商标识 */
    provider: 提供商标识;
    /** API 密钥（Ollama 本地服务可留空） */
    apiKey: string;
    /** API 基础地址（留空时使用该服务商的默认地址） */
    baseUrl: string;
    /** 模型名称（留空时使用该服务商的默认模型） */
    model: string;
    /** 采样温度：越低越确定（0~2，默认 0.1，教学场景偏好稳定输出） */
    temperature: number;
    /** 最大生成 token 数（默认 2048） */
    maxTokens: number;
    /** 自定义系统提示词（可选，非空时覆盖按语言包自动生成的提示词） */
    systemPrompt: string;
    /** 请求超时时间（秒，默认 60） */
    timeout: number;
}

/**
 * 提供商预设信息（用于工厂默认值与界面展示）
 */
export interface 提供商定义 {
    /** 提供商标识 */
    id: 提供商标识;
    /** 中文显示名称 */
    名称: string;
    /** 默认 API 地址 */
    默认地址: string;
    /** 默认模型 */
    默认模型: string;
    /** 是否需要 API 密钥 */
    需要密钥: boolean;
}

/**
 * 统一错误类别
 */
export type 错误类别 =
    | '配置缺失'
    | '网络错误'
    | '超时'
    | 'API错误'
    | 'JSON解析错误'
    | '不支持';

/**
 * 统一 AI 错误：所有提供商与请求层抛出的错误均为该类型，
 * 携带类别（类别）与面向用户的中文描述（message）。
 */
export class AI错误 extends Error {
    constructor(
        /** 错误类别 */
        public 类别: 错误类别,
        message: string
    ) {
        super(message);
        this.name = 'AI错误';
    }
}

/**
 * 流式对话的回调类型：内容增量逐块回调
 */
export type 流式回调 = (chunk: string) => void;
