pub struct Counter { pub n: i32 }
pub fn f() -> Counter { Counter { n: 1 } }
pub fn g() -> i32 { Counter().n }
