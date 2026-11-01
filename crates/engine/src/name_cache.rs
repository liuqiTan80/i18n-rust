//! 名字级直映缓存：把逐标识符的映射表查找折算成一次槽位探测
//!
//! 转译管线的词法、别名、教学 lint 三个阶段都要对源码里的每个标识符查
//! 一两张 `HashMap`，而同一份源码里名字高度重复（局部变量、字段名、库 API
//! 被反复引用）。映射表查找实测约 22 ns/次且受随机访存限制（换更快哈希器
//! 只降到 17 ns），2000 行样例近 4000 个标识符合计就是上百微秒。这里按
//! 名字直映解析结果：重复名字只付一次「散列 + 字符串比较」的探测，首次
//! 出现才查表。
//!
//! 缓存是**开地址单槽**表：同一槽位的后来者直接覆盖，不做链式回溯。命中
//! 与否只影响速度、不影响正确性——槽内保存名字全文，取用时逐字节比较，
//! 散列碰撞不会把 A 的事实给到 B。

/// 槽位数取 2 的幂（256 槽）：整表随数组一次分配，探测只碰一次内存，
/// 冲突时覆盖即可（见模块文档）
const NAME_CACHE_BITS: usize = 8;

/// 槽位总数
pub(crate) const NAME_CACHE_SLOTS: usize = 1 << NAME_CACHE_BITS;

/// 名字直映槽位：按 8 字节一段的乘-异或散列（与 rustc 的 FxHash 同构）
///
/// 不引哈希器依赖也不调 `DefaultHasher`（后者是 SipHash，短名字上每字节约
/// 1 ns）；这里短名字一个循环就结束，且不碰任何内存。
#[inline]
pub(crate) fn name_cache_slot(name: &str) -> usize {
    const K: u64 = 0x517c_c1b7_2722_0a95;
    let bytes = name.as_bytes();
    let mut hash = bytes.len() as u64;
    let mut i = 0usize;
    // 逐段取 8 字节（末段不足补零）：全程用定长缓冲拷贝，不引入可 panic 分支
    // （生产路径的 panic 面由 tools/check-prod-panics.py 把守）
    while i < bytes.len() {
        let mut word = [0u8; 8];
        let end = (i + 8).min(bytes.len());
        word[..end - i].copy_from_slice(&bytes[i..end]);
        hash = (hash.rotate_left(5) ^ u64::from_le_bytes(word)).wrapping_mul(K);
        i += 8;
    }
    // 取高位：乘法低位扩散差（只差一个末字节的名字低位几乎相同），
    // std 的 HashMap 同理用高位定桶
    (hash >> (64 - NAME_CACHE_BITS)) as usize
}

/// 名字级直映缓存：槽内存「名字全文 + 事实」，不命中的事实自行解析
///
/// `T` 须为 `Copy`（通常是若干 `Option<&'a str>` 与 `bool` 组成的小结构），
/// 这样命中时零分配、零引用计数；`'a` 是名字与事实里借用的映射表值的
/// 共同生命周期（都来自同一次转译调用的输入）。
pub(crate) struct NameCache<'a, T: Copy> {
    slots: Vec<Option<(&'a str, T)>>,
}

impl<'a, T: Copy> NameCache<'a, T> {
    /// 新缓存：全部槽位置空（一次分配，256 项）
    pub(crate) fn new() -> Self {
        Self {
            slots: vec![None; NAME_CACHE_SLOTS],
        }
    }

    /// 取名字对应的事实：命中直接返回，未命中时调用 `resolve` 并回填
    #[inline]
    pub(crate) fn get_or<F: FnOnce() -> T>(&mut self, name: &'a str, resolve: F) -> T {
        let slot = name_cache_slot(name);
        match self.slots[slot] {
            Some((cached, facts)) if cached == name => facts,
            _ => {
                let facts = resolve();
                self.slots[slot] = Some((name, facts));
                facts
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_or_resolves_once_per_name() {
        // 同一名字第二次取值不再调用解析（用 Cell 计数验证）
        let mut calls = 0usize;
        let mut cache: NameCache<'_, u32> = NameCache::new();
        let a = cache.get_or("甲", || {
            calls += 1;
            1
        });
        let b = cache.get_or("甲", || {
            calls += 1;
            2
        });
        assert_eq!((a, b, calls), (1, 1, 1));
    }

    #[test]
    fn test_collision_keeps_both_names() {
        // 同槽位的不同名字各自解析，不会互相串味
        let want = name_cache_slot("甲");
        let other = (0..5000)
            .map(|i| format!("甲{i}"))
            .find(|name| name_cache_slot(name) == want)
            .expect("应能找到同槽位的另一个名字");
        let mut cache: NameCache<'_, &str> = NameCache::new();
        assert_eq!(cache.get_or("甲", || "one"), "one");
        assert_eq!(cache.get_or(&other, || "other"), "other");
        // 覆盖后回到「甲」会重新解析，但结果必须仍是「甲」的
        assert_eq!(cache.get_or("甲", || "one"), "one");
    }

    #[test]
    fn test_slot_in_range() {
        for name in ["", "a", "变量", "很长的标识符名字_1234567890"] {
            assert!(name_cache_slot(name) < NAME_CACHE_SLOTS);
        }
    }
}
