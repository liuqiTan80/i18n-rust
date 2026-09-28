use std::collections::HashMap;
pub fn f() {
    let mut m = HashMap::new();
    m.insert(1, "a");
    m.for_each(|_k, _v| {});
}
