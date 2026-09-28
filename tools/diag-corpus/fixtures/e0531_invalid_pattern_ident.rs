pub struct S { pub x: i32 }
pub fn f(s: Option<S>) {
    match s {
        Some(&x) => { let _ = x; }
        None => {}
    }
}
