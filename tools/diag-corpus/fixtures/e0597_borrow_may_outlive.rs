pub fn f() -> i32 {
    let x;
    {
        let y = 1;
        x = &y;
    }
    *x
}
