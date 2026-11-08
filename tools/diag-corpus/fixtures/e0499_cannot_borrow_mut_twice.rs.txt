pub fn f(v: &mut Vec<i32>) {
    let a = &mut v[0];
    let b = &mut v[1];
    *a += 1;
    *b += 1;
}
