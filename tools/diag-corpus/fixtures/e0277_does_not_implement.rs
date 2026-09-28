pub trait Speak { fn speak(&self); }
pub struct Dog;
pub fn whoami<T: Speak>(t: T) { t.speak(); }
pub fn g() { whoami(Dog); }
