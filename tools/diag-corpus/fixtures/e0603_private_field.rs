pub mod m {
    pub struct P { v: i32 }
    pub fn make() -> P { P { v: 1 } }
}
pub fn f() -> i32 { m::make().v }
