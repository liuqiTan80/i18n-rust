// 反例：unused_variables「赋值但从未使用」变体 + unused_assignments 的 help 子句
// 触发 rustc 两类只有真跑才会出现的动态消息：
//   main(unused_variables): variable `x` is assigned to, but never used
//   main(unused_assignments): value assigned to `x` is never read
//   help: maybe it is overwritten before being read?
#![allow(dead_code)]
pub fn overwritten() {
    let mut x = 5;
    x = 6;
    x = 7;
}
