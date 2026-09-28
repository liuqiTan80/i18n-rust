pub fn f<'a>(x: &'a u8) -> &'static u8 {
    let y: &'x u8 = x;
    y
}
