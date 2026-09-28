pub fn apply(f: fn(i32) -> i32) -> i32 { f(1) }
pub fn f() -> i32 { apply(|x| x.to_string()) }
