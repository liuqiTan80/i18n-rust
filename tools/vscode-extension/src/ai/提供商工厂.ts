/**
 * 提供商工厂
 *
 * 根据配置中的提供商标识创建对应的提供商实例，
 * 并在 baseUrl / model 未配置时自动填入该服务商的默认值。
 */

import { OpenAI兼容提供商 } from './OpenAI兼容提供商';
import { 提供商抽象 } from './提供商抽象';
import { AI配置, AI错误, 提供商定义, 提供商标识 } from './类型定义';

/**
 * 各服务商预设信息（默认地址 / 默认模型 / 是否需要密钥）
 * 所有服务商均使用 OpenAI 兼容协议，仅默认配置不同。
 */
const 提供商预设表: Record<提供商标识, 提供商定义> = {
    openai: {
        id: 'openai',
        名称: 'OpenAI',
        默认地址: 'https://api.openai.com/v1',
        默认模型: 'gpt-4o-mini',
        需要密钥: true
    },
    deepseek: {
        id: 'deepseek',
        名称: 'DeepSeek 深度求索',
        默认地址: 'https://api.deepseek.com/v1',
        默认模型: 'deepseek-chat',
        需要密钥: true
    },
    qwen: {
        id: 'qwen',
        名称: '通义千问（阿里云）',
        默认地址: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
        默认模型: 'qwen-plus',
        需要密钥: true
    },
    glm: {
        id: 'glm',
        名称: '智谱 GLM',
        默认地址: 'https://open.bigmodel.cn/api/paas/v4',
        默认模型: 'glm-4-flash',
        需要密钥: true
    },
    ollama: {
        id: 'ollama',
        名称: 'Ollama（本地模型）',
        默认地址: 'http://localhost:11434/v1',
        默认模型: 'qwen2.5',
        需要密钥: false
    },
    custom: {
        id: 'custom',
        名称: '自定义（OpenAI 兼容）',
        默认地址: '',
        默认模型: '',
        需要密钥: false
    }
};

/**
 * 返回全部支持的提供商预设列表（用于界面展示与选择）
 */
export function 提供商列表(): 提供商定义[] {
    return Object.values(提供商预设表);
}

/**
 * 查询单个提供商的预设信息
 */
export function 提供商预设(id: 提供商标识): 提供商定义 {
    return 提供商预设表[id];
}

/**
 * 根据配置创建提供商实例：
 * - baseUrl / model 为空时自动填入该服务商的默认值
 * - 自定义（custom）必须显式配置 baseUrl 与 model
 * - 当前所有服务商均采用 OpenAI 兼容协议
 *
 * 抛出的错误均为 AI错误（配置缺失 / 不支持）。
 */
export function 创建提供商(配置: AI配置): 提供商抽象 {
    const 预设 = 提供商预设表[配置.provider];
    if (!预设) {
        throw new AI错误('不支持', `不支持的提供商标识：${配置.provider}（可选值：${Object.keys(提供商预设表).join('、')}）`);
    }
    const 完整配置: AI配置 = {
        ...配置,
        baseUrl: 配置.baseUrl || 预设.默认地址,
        model: 配置.model || 预设.默认模型
    };
    if (!完整配置.baseUrl) {
        throw new AI错误(
            '配置缺失',
            `提供商「${预设.名称}」未配置 API 地址，请在设置中填写 i18n-rust.ai.baseUrl。`
        );
    }
    if (!完整配置.model) {
        throw new AI错误(
            '配置缺失',
            `提供商「${预设.名称}」未配置模型名称，请在设置中填写 i18n-rust.ai.model。`
        );
    }
    return new OpenAI兼容提供商(完整配置);
}
