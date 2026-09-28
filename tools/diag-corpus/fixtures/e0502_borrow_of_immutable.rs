pub fn f(v: &mut Vec<i32>) {
    let first = &v[0];
    v.push(1);
    let _ = first;
}
