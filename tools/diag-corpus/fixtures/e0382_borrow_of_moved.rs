pub fn consume(s: String) -> usize { s.len() }
pub fn f() {
    let a = String::from("x");
    let n = consume(a);
    let m = a.len();
    let _ = (n, m);
}
