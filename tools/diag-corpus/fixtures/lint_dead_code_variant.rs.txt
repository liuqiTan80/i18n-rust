pub enum E { Used(i32), NeverConstructed(String) }
pub fn f(e: E) -> i32 { match e { E::Used(n) => n } }
