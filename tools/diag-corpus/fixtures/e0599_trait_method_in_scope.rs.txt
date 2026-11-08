mod m {
    pub trait Speak { fn speak(&self) -> String; }
    pub struct Cat;
    impl Speak for Cat { fn speak(&self) -> String { String::new() } }
}
pub fn f(c: m::Cat) -> String { c.speak() }
