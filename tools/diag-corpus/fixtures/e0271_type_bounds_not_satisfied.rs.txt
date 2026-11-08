pub fn take<T: std::fmt::Debug>(_t: T) {}
pub struct Raw;
pub fn f() { take(Raw); }
