#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Id(u32);
#[derive(Copy)]
pub struct Wrap { pub id: Id, pub s: String }
