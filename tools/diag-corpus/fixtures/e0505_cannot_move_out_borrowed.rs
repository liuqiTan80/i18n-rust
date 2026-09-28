pub fn f(mut v: Vec<i32>) -> i32 {
    let r = &v;
    v.push(1);
    r.len() as i32
}
