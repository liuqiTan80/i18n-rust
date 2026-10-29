// `?` 的两条 E0277 消息（实机采集：en 第十六章写 `?` 时撞到）。
//
// 它们的反引号内容（`?`、`Result`、目标错误类型）会被 [E0277] 桩按出现顺序
// 当成「类型/特征」填进模板，渲染成 "the trait bound `?`: `Result` is not
// satisfied" 这类答非所问的句子。语言包为这两句收录整句键后由整句译文接管，
// 本反例让语料门禁守住「不能退回桩的猜法」。
pub struct IoError;

/// 本地错误类型：用 `String` 作目标错误会让 rustc 追加一条「`String`
/// implements `From<T>`」的长 help（需为它再维护一条译文），这里用自定义
/// 类型只留两条 main 消息。
pub struct SignupError;

pub fn parse() -> Result<u32, IoError> {
    Ok(1)
}

/// 在返回 `u32` 的函数里用 `?` → "the `?` operator can only be used in a
/// function that returns `Result` or `Option` (or another type that
/// implements `FromResidual`)"
pub fn without_result() -> u32 {
    let x = parse()?;
    x
}

/// 错误类型无法转换 → "`?` couldn't convert the error to `SignupError`"
pub fn wrong_error() -> Result<u32, SignupError> {
    let x = parse()?;
    Ok(x)
}
