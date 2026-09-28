mod inner {
    struct Secret { pub v: i32 }
    pub fn make() -> Secret { Secret { v: 1 } }
}
pub fn f() -> i32 { inner::make().v }
