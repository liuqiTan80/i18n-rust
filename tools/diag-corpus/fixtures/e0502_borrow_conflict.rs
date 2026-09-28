pub fn f(v: &mut Vec<i32>) {
    for x in v.iter() {
        v.push(*x);
    }
}
