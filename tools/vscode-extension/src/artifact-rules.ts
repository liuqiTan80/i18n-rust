/**
 * 转译产物隐藏规则（纯逻辑，不依赖 VS Code API）
 *
 * 方言项目以 .zh 等为源码、.rs 为转译产物，产物文件（及检查/运行
 * 产生的 .rs.bak 备份）在资源管理器中大量出现会干扰视线。开关开启时
 * 把两个产物匹配模式（见「产物匹配模式们」）写入工作区 files.exclude
 * （隐藏），关闭时移除条目（恢复显示）。本文件只负责排除项对象的
 * 计算与比对，VS Code 读写胶水见 artifact-hide.ts。
 */

/** 产物匹配模式（files.exclude 键） */
export const 产物匹配模式们 = ['**/*.rs', '**/*.rs.bak'];

/**
 * 计算应用开关后的 files.exclude 对象
 *
 * 保留现有条目（含用户自定义排除项），仅增删这两个产物模式；
 * 不修改传入对象（返回新对象）。
 */
export function 计算排除项(
    当前: Readonly<Record<string, boolean>> | undefined,
    开启: boolean
): Record<string, boolean> {
    const 结果: Record<string, boolean> = { ...当前 };
    for (const 模式 of 产物匹配模式们) {
        if (开启) {
            结果[模式] = true;
        } else {
            delete 结果[模式];
        }
    }
    return 结果;
}

/**
 * 判断现有条目是否已与开关一致
 *
 * 一致时跳过写入：避免每次激活/配置事件都重写工作区设置文件。
 * 开启：两个模式均为 true；关闭：两个模式均不存在。
 */
export function 排除项已一致(
    当前: Readonly<Record<string, boolean>> | undefined,
    开启: boolean
): boolean {
    const 值 = 当前 ?? {};
    return 开启
        ? 产物匹配模式们.every(模式 => 值[模式] === true)
        : 产物匹配模式们.every(模式 => !(模式 in 值));
}
