pub fn longest(a: &str, b: &str) -> &str {
    if a.len() > b.len() { a } else { b }
}
pub fn use_it(s: &str) -> &str { longest(s, "x") }
