use std::fmt;
pub struct Foo;
pub fn f<T: fmt::Display>() {}
pub fn g() { f::<Foo>(); }
