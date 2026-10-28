# Chapter 16: Error Handling

## 16.0 Learning Objectives

After reading this chapter, you will be able to:

1. Tell the two kinds of failure apart: **recoverable** (use `Result`) and **unrecoverable** (use `panic!`);
2. Let a function report success or failure gracefully with **`Result<T, E>`**;
3. Handle a result in two postures: **`match`** and **`?`**;
4. Judge both the convenience and the danger of **`unwrap()` / `expect()`**;
5. Read a runtime **panic** message;
6. Use the "should I panic or return a `Result`?" decision table.

---

## 16.1 Two kinds of failure, two kinds of tool

Programs fail. The user types letters into a number field, the file isn't there, the network drops. Rust sorts all of it into two buckets:

| | Recoverable error | Unrecoverable error |
|---|---|---|
| Examples | file not found, bad number format | the program's own logic contradicts itself, out of bounds |
| Tool | the `Result` enum | `panic!` |
| Attitude | report upward, work out a plan | stop right now, do not continue |

💡 **Metaphor**: a recoverable error is like **the delivery arriving with the wrong flavour** — send it back, order again. An unrecoverable error is like **the kitchen being on fire** — forget the deliveries, call the fire brigade.

## 16.2 `Result<T, E>`: a box holding the outcome

📖 **`Result`**: a standard-library enum with two variants:

```
Result<T, E>
├── Ok(value)     // it worked — the result is in here
└── Err(reason)   // it didn't — the cause is in here
```

Write a function that parses an age. Success hands back a number; failure hands back a reason:

```rust
fn parse_age(text: &str) -> Result<i32, String> {
    match text.parse::<i32>() {
        Ok(n) => {
            if n < 0 {
                return Err(String::from("age cannot be negative"));
            }
            Ok(n)
        }
        Err(_) => Err(format!("cannot parse: {}", text)),
    }
}
```

A few details:

- **`text.parse::<i32>()`** turns text into a number. `parse` *itself* returns a `Result` — `"twenty"` doesn't parse.
- **`parse::<i32>()`**: the type you want goes inside the angle brackets. This syntax is nicknamed **turbofish** (`::<>`) — you met the same brackets when generic types were introduced in Chapter 12.
- We **checked two kinds of bad news** — wrong format, and a negative number — and gave each its own reason.

## 16.3 Handling a result: open the box

The caller receives a `Result` and **must open it** before using the value inside. That is the whole design of Rust: it makes you face the possibility of failure.

```rust
fn parse_age(text: &str) -> Result<i32, String> {
    match text.parse::<i32>() {
        Ok(n) => {
            if n < 0 {
                return Err(String::from("age cannot be negative"));
            }
            Ok(n)
        }
        Err(_) => Err(format!("cannot parse: {}", text)),
    }
}

fn main() {
    match parse_age("25") {
        Ok(age) => println!("age is: {}", age),
        Err(reason) => println!("error: {}", reason),
    }
    match parse_age("twenty") {
        Ok(age) => println!("age is: {}", age),
        Err(reason) => println!("error: {}", reason),
    }
}
// 预期输出:
// age is: 25
// error: cannot parse: twenty
```

`if let` works too, when you only care about one side:

```rust
fn parse_age(text: &str) -> Result<i32, String> {
    match text.parse::<i32>() {
        Ok(n) => Ok(n),
        Err(_) => Err(format!("cannot parse: {}", text)),
    }
}

fn main() {
    if let Ok(age) = parse_age("25") {
        println!("got a number: {}", age);
    }
}
// 预期输出: got a number: 25
```

## 16.4 `?`: hand the baton when you drop it

What if a function has five steps and any of them can fail? Writing a `match` around each one is exhausting. The **`?` operator** is the shortcut:

```rust
fn parse_age(text: &str) -> Result<i32, String> {
    match text.parse::<i32>() {
        Ok(n) => {
            if n < 0 {
                return Err(String::from("age cannot be negative"));
            }
            Ok(n)
        }
        Err(_) => Err(format!("cannot parse: {}", text)),
    }
}

fn make_report(text: &str) -> Result<String, String> {
    let age = parse_age(text)?;
    Ok(format!("profile built, age {}", age))
}

fn main() {
    println!("{}", make_report("25").unwrap());
}
// 预期输出: profile built, age 25
```

What `parse_age(text)?` means:

- it got `Ok(value)` → take the value out and keep running;
- it got `Err(reason)` → **return that error from the current function immediately**; nothing below this line executes.

💡 **Metaphor**: `?` is the **baton rule in a relay race** — the runner who's fine keeps running; the moment somebody drops the baton, the whole team stops and reports. Nobody pretends to keep running.

⚠️ **Precondition**: `?` only works inside a function **whose own return type is a `Result` (or an `Option`)**. A plain `fn main() { ... }` returns `()` and cannot use it:

```rust
fn parse_age(text: &str) -> Result<i32, String> {
    match text.parse::<i32>() {
        Ok(n) => Ok(n),
        Err(_) => Err(format!("cannot parse: {}", text)),
    }
}

fn main() {
    let n = parse_age("25")?;
    println!("{}", n);
}
// 预期错误: E0277
```

The compiler's complaint, in words: **the `?` operator can only be used in a function that returns `Result` or `Option`**. The fix is one of two things: change the function's return type to `Result<..., ...>`, or open the value with `match` here instead. (`fn main()` itself can be upgraded to return a `Result` — see 16.8.)

> ⚠️ Run this block through `rzc check` and the error line arrives as `error[E0277]` with the generic trait-bound wording rather than the sentence quoted above — same code, same remedy. The `?`-specific wording is on the en pack's to-do list.

## 16.5 Panic: pull the fire alarm

📖 **Panic**: the program has hit a situation it cannot continue from — it prints a message, cleans up, and stops.

### Passive panic: the program triggers it itself

Going out of bounds on an array panics. When the index only becomes known at runtime, the compiler has no way to stop you:

```rust
fn main() {
    let scores = [1, 2, 3];
    let index: usize = "9".parse().unwrap();   // the index arrives at runtime
    println!("{}", scores[index]);
}
// 预期行为: 运行失败
```

The crash line is one sentence long, and it says everything:

```text
thread 'main' panicked at src/main.rs:4:20:
index out of bounds: the len is 3 but the index is 9
```

The program exits with code 101. Rust panics rather than hand back whatever sits at that address: **a loud stop beats a quietly wrong value**.

> ✨ Here's the interesting part: write the index as a **literal**, `scores[9]`, and the compiler can prove the panic before the program ever runs — the full compile (what `rzc run` does) answers `error[unconditional_panic]: this operation will panic at runtime`. Rust tries hard to move even this kind of error into compile time. (`rzc check` skips the analysis that finds it, so run the program to see the message.)

### Active panic: pull the alarm yourself

```rust
fn main() {
    panic!("this should not happen: {}", 42);
}
// 预期行为: 运行失败
```

The output looks like this:

```text
thread 'main' panicked at src/main.rs:2:5:
this should not happen: 42
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

Read it in three pieces: **which thread** panicked, **where** (file, line, column), and **what message** you asked for.

When should you panic on purpose? **When an invariant of your own program has been broken** — "this can never be empty; if it is, something upstream is badly wrong". Better to sound the alarm on the spot than to keep running on a broken assumption.

## 16.6 `unwrap` and `expect`: the fast-forward keys

When you are certain the outcome is `Ok`, there are shortcuts that take the value directly:

```rust
fn main() {
    let n = "42".parse::<i32>().unwrap();
    let m = "7".parse::<i32>().expect("should be a number");
    println!("{} {}", n, m);
}
// 预期输出: 42 7
```

- **`unwrap()`**: take the `Ok` value out, no ceremony.
- **`expect("...")`**: the same, except your sentence is attached to the crash message.

⚠️ **Danger warning**: both **panic outright** when they meet `Err`. This is what one actually looks like:

```text
thread 'main' panicked at src/main.rs:2:37:
should be a number: ParseIntError { kind: InvalidDigit }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

Fun while learning; in real code, do the honest thing and use `match` or `?`. If you must use a shortcut, prefer `expect` — at least the crash line says what you believed at the time.

## 16.7 Decision table: which one do I use?

| Situation | Choice |
|---|---|
| Something outside the program may fail (user input, files, network) | `Result` |
| The caller **can do something about it** (retry, switch plans) | `Result` |
| Internal logic should never fail; if it does, it's a bug | `panic!` |
| Writing an example or a prototype, wanting it short | `unwrap()` / `expect()` (with a comment) |

---

## 16.8 Custom errors and automatic conversion

Everything above used `String` as the error type — lazy, and crude: you can't enumerate the kinds of failure, and the caller can't branch on them. Real projects **define their own error type**.

### What an error type looks like

A presentable custom error is a **three-piece kit**:

1. an **enum**: list every kind of failure (the variants *are* the categories);
2. **`impl Display`**: so `{}` can print a human sentence;
3. **`impl std::error::Error`**: the ticket that admits you to the standard error ecosystem (the automatic `?` conversion below depends on having it).

```rust
use std::fmt::Display;
use std::fmt::Formatter;

#[derive(Debug)]
enum SignupError {
    NameTooShort,
    BadAge,
}

impl Display for SignupError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        let text = match self {
            SignupError::NameTooShort => "name must be at least two characters",
            SignupError::BadAge => "age must be between 0 and 150",
        };
        write!(f, "{}", text)
    }
}

impl std::error::Error for SignupError {}

fn main() {
    println!("{}", SignupError::NameTooShort);
    println!("{}", SignupError::BadAge);
}
// 预期输出:
// name must be at least two characters
// age must be between 0 and 150
```

The points that matter:

- `#[derive(Debug)]`: the `Error` trait requires the type to be debug-printable, so stick the sticker on (Chapter 10's sticker earns its keep again). Leave it off and the compiler complains (E0277) about the missing `Debug` implementation.
- The **signature of `fmt` is boilerplate**: take `&self` and a `&mut Formatter`, return `Result<(), std::fmt::Error>`. Copy it verbatim; don't agonise over why.
- `write!(f, "{}", text)` is the macro for "write into the formatter" — the same family as `println!`, only aimed at a buffer instead of the screen.
- `impl std::error::Error for SignupError {}` is an **empty** implementation. The trait has no methods you must supply; the line exists as a certificate for the compiler to accept.

📖 **The `Error` trait**: the standard library's membership card for "things that are errors". Implement it and your error is treated the same way as the standard ones (file, network, parse…).

### `From`: the interpreter behind `?`

16.4 said: on failure, `?` **returns the error out of the current function**. There's a hidden mechanism: if the error type your function promises and the error type actually produced are **different**, `?` tries to translate it with **`From`**.

The situation: `text.parse::<i32>()` fails with the standard library's `ParseIntError`, while our function returns `SignupError` — the types don't match. Solution: implement one `From`:

```rust
use std::num::ParseIntError;

#[derive(Debug)]
enum SignupError {
    NameTooShort,
    BadFormat,
}

// tells the compiler: a failed integer parse is one of our "format" errors
impl From<ParseIntError> for SignupError {
    fn from(_input: ParseIntError) -> SignupError {
        SignupError::BadFormat
    }
}

fn main() {
    let boom = "twenty".parse::<i32>().unwrap_err();
    println!("{:?}", SignupError::from(boom));
}
// 预期输出: BadFormat
```

With that in place, when `text.parse::<i32>()?` hits a bad parse, the `?` **calls this `from` automatically**, turns the `ParseIntError` into `SignupError::BadFormat`, and returns it up the stack. The `main` above calls the same conversion by hand, so you can see what `?` is doing behind your back.

Skip the implementation and `?` refuses to compile with E0277: the conversion it is missing is `From<ParseIntError>` for `SignupError` (the wording caveat from 16.4 applies to this one too).

(`From` / `Into` are a pair: implement `From` and you get `Into` for free — `value.into()` works immediately.)

### Full example: the signup desk

```rust
use std::fmt::Display;
use std::fmt::Formatter;
use std::num::ParseIntError;

#[derive(Debug)]
enum SignupError {
    NameTooShort,
    BadAge,
    BadFormat,
}

impl Display for SignupError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        let text = match self {
            SignupError::NameTooShort => "name must be at least two characters",
            SignupError::BadAge => "age must be between 0 and 150",
            SignupError::BadFormat => "input format is wrong",
        };
        write!(f, "{}", text)
    }
}

impl std::error::Error for SignupError {}

// the key line: a failed integer parse becomes our BadFormat automatically
impl From<ParseIntError> for SignupError {
    fn from(_input: ParseIntError) -> SignupError {
        SignupError::BadFormat
    }
}

fn check_name(name: &str) -> Result<(), SignupError> {
    if name.chars().count() < 2 {
        return Err(SignupError::NameTooShort);
    }
    Ok(())
}

fn check_age(text: &str) -> Result<i32, SignupError> {
    let age = text.parse::<i32>()?;   // on failure, ? calls the from() above
    if age < 0 || age > 150 {
        return Err(SignupError::BadAge);
    }
    Ok(age)
}

fn signup(name: &str, age_text: &str) -> Result<(), SignupError> {
    check_name(name)?;
    check_age(age_text)?;
    Ok(())
}

fn main() {
    match signup("A", "abc") {
        Ok(_) => println!("signup ok"),
        Err(reason) => println!("failed: {}", reason),
    }
    match signup("Jo", "200") {
        Ok(_) => println!("signup ok"),
        Err(reason) => println!("failed: {}", reason),
    }
    match signup("Jo", "abc") {
        Ok(_) => println!("signup ok"),
        Err(reason) => println!("failed: {}", reason),
    }
    match signup("Jo", "25") {
        Ok(_) => println!("signup ok"),
        Err(reason) => println!("failed: {}", reason),
    }
}
// 预期输出:
// failed: name must be at least two characters
// failed: age must be between 0 and 150
// failed: input format is wrong
// signup ok
```

Four failures, four human sentences: `"A"` is one character short (`chars().count()` counts **characters**; Chapter 9's `.len()` counts **bytes**, which is a different number for Chinese); `"200"` is over age; `"abc"` cannot be parsed at all — and that third one is the `From` implementation doing its job, invisibly.

> 💡 **Why design it this way?** The three functions (`check_name` / `check_age` / `signup`) all return the *same* `SignupError`, and that's what lets `?` relay from one to the next with `from` as the automatic translator. If each function returned a different error type, the baton couldn't be passed — `From` is what keeps one error vocabulary.

### Making `main` return a `Result`

`main` can return a `Result` too! Then `?` works all the way down to the top of the program:

```rust
use std::fmt::Display;
use std::fmt::Formatter;

#[derive(Debug)]
enum SignupError {
    NameTooShort,
}

impl Display for SignupError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "name must be at least two characters")
    }
}

impl std::error::Error for SignupError {}

fn check_name(name: &str) -> Result<(), SignupError> {
    if name.chars().count() < 2 {
        return Err(SignupError::NameTooShort);
    }
    Ok(())
}

fn main() -> Result<(), SignupError> {
    check_name("Jo")?;
    println!("signup ok");
    Ok(())
}
// 预期输出: signup ok
```

Make the check fail instead (pass `"A"`), and `main` returns `Err`. The runtime then prints the error itself — on **standard error**, with exit code 1:

```text
Error: NameTooShort
```

The `Error: ` prefix is added by the runtime, and it uses the `{:?}` debug format — which is exactly why the error type has to derive `Debug`.

> ⚠️ **Note**: a `Result`-returning `main` turns an error into "the program exited with a failure code" — perfect as the **outermost** safety net. Business logic still handles its errors with `match` and `?` where it can do something about them.

### Two mistakes people actually make

**Mistake one: `?` says the error type can't be converted.** `?` has met a type it wasn't promised and there is no `From` implementation, so the compile fails. **Fix**: implement `From<Source>` for your target error type (or `From<String>` as a catch-all).

**Mistake two: the error type has no `Debug`.** Returning a `Result` from `main`, or implementing the `Error` trait, both require debug printing. **Fix**: stick `#[derive(Debug)]` on it (Chapter 10's sticker has earned another day's pay).

---

## 16.9 Full example: the profile desk

Everything from this chapter in one program: parsing + business rules + `?` relay + the caller branching on the outcome:

```rust
// parse the age: Ok hands back a number, Err hands back the reason
fn parse_age(text: &str) -> Result<i32, String> {
    match text.parse::<i32>() {
        Ok(n) => {
            if n < 0 {
                return Err(String::from("age cannot be negative"));
            }
            Ok(n)
        }
        Err(_) => Err(format!("cannot parse: {}", text)),
    }
}

// the ? relay: whichever step fails, its error is handed straight up
fn make_report(text: &str) -> Result<String, String> {
    let age = parse_age(text)?;
    Ok(format!("profile built, age {}", age))
}

fn main() {
    for input in ["25", "-3", "twenty"] {
        match make_report(input) {
            Ok(report) => println!("{} -> {}", input, report),
            Err(reason) => println!("{} -> failed: {}", input, reason),
        }
    }
}
```

## 16.10 Line-by-line explanation

- **Lines 2–12**: `parse_age` returns `Result<i32, String>`. Three endings: parses and non-negative → `Ok(n)`; negative → `Err("age cannot be negative")`; doesn't parse → `Err("cannot parse: ...")`.
- **Lines 15–18**: `make_report` uses `?` — if parsing fails the error is handed upward automatically; if it succeeds, the number gets wrapped into a brand-new `Ok` and the work continues.
- **Lines 20–27**: `main` runs three inputs down the pipeline: `"25"` succeeds; `"-3"` dies at the business rule; `"twenty"` dies at the parse. The caller opens the result with `match` and takes the two branches separately.

## 16.11 What it prints

```text
25 -> profile built, age 25
-3 -> failed: age cannot be negative
twenty -> failed: cannot parse: twenty
```

Three reasons, three different sentences — that is what a good error message looks like: **it says what went wrong, and why**.

---

## 16.12 Common Errors and Fixes

### Error one: using a `Result` as if it were the value inside

```rust
fn main() {
    let n: i32 = "25".parse();
    println!("{}", n);
}
// 预期错误: E0308
```

The compiler says `expected i32, found Result<_, _>` — `parse` hands you a **box**, not a number. **Fix**: open it first, with `match`, `if let`, `?`, or (if you truly mean it) `.unwrap()`.

### Error two: `?` inside a function that returns nothing

Using `?` in a `fn` that returns `()` is rejected (E0277): the `?` operator can only be used in a function that returns `Result` or `Option`. **Fix**: give the function a `Result` return type, or handle this one value with `match` where it appears. See 16.4.

### Error three: spelling the failure variant wrong

```rust
fn parse_age(text: &str) -> Result<i32, String> {
    match text.parse::<i32>() {
        Ok(n) => Ok(n),
        Fail(_) => Err(String::from("bad")),
    }
}

fn main() {
    println!("{:?}", parse_age("25"));
}
// 预期错误: E0531
```

The `Result` failure variant is called **`Err`**. Write `Fail(...)` and the compiler answers `unknown tuple struct or tuple variant`. **Fix**: `Err(...)`, and remember that variants are reached through their type — `Result::Err`.

### Error four: `unwrap` blew up

```rust
fn main() {
    let n = "twenty".parse::<i32>().unwrap();
    println!("{}", n);
}
// 预期行为: 运行失败
```

The crash line reads ``called `Result::unwrap()` on an `Err` value: ParseIntError { kind: InvalidDigit }``. **Fix**: don't `unwrap()` a result you haven't earned — use `match`, or `expect("why you believe this can't fail")`, so the crash is at least legible.

### Error five: the custom error won't accept `impl Error`

Implementing `std::error::Error` requires the type to be both `Display` and `Debug`. Miss `#[derive(Debug)]` and you get another E0277 about the missing `Debug`. **Fix**: add the sticker; it costs one line.

---

## 16.13 Chapter Glossary

| Term | One-line explanation |
|---|---|
| recoverable error | a failure you can do something about — expressed with `Result` |
| unrecoverable error | a failure you must stop for — expressed with `panic!` |
| `Result<T, E>` | an enum holding either success or failure |
| `Ok` / `Err` | the two variants of `Result` |
| `?` operator | on `Ok` take the value and continue; on `Err` return it immediately |
| panic | print a message, clean up, stop the program |
| `panic!` | the macro that pulls the alarm yourself |
| `unwrap()` | take the `Ok` value directly; panics on `Err` |
| `expect()` | `unwrap()` plus a note of your own in the crash message |
| `parse()` | convert text into another type (a number, for instance) — returns a `Result` |
| `From` | the conversion trait `?` uses to translate one error type into another |

> 📖 **Reminder**: any term you don't recognise is in the master glossary at the front of the book.

---

## 16.14 Exercises

> 💪 Try it first, then look at the answer.

### Exercise 1: safe division

Write `divide(a: i32, b: i32) -> Result<i32, String>`: when `b` is 0 return `Err("cannot divide by zero")`, otherwise return the quotient. Call it from `main` with `10 / 2` and `10 / 0`.

<details>
<summary>🔍 Show the answer</summary>

```rust
fn divide(a: i32, b: i32) -> Result<i32, String> {
    if b == 0 {
        return Err(String::from("cannot divide by zero"));
    }
    Ok(a / b)
}

fn main() {
    match divide(10, 2) {
        Ok(q) => println!("quotient: {}", q),
        Err(reason) => println!("failed: {}", reason),
    }
    match divide(10, 0) {
        Ok(q) => println!("quotient: {}", q),
        Err(reason) => println!("failed: {}", reason),
    }
}
// 预期输出:
// quotient: 5
// failed: cannot divide by zero
```

</details>

### Exercise 2: a two-stage `?` relay

Write `half_the_age(text: &str) -> Result<i32, String>`: first `parse_age`, then `divide` by 2, both with `?`. Test one input that fails at the parse stage.

<details>
<summary>🔍 Show the answer</summary>

```rust
fn parse_age(text: &str) -> Result<i32, String> {
    match text.parse::<i32>() {
        Ok(n) => Ok(n),
        Err(_) => Err(format!("cannot parse: {}", text)),
    }
}

fn divide(a: i32, b: i32) -> Result<i32, String> {
    if b == 0 {
        return Err(String::from("cannot divide by zero"));
    }
    Ok(a / b)
}

fn half_the_age(text: &str) -> Result<i32, String> {
    let age = parse_age(text)?;
    let half = divide(age, 2)?;
    Ok(half)
}

fn main() {
    match half_the_age("25") {
        Ok(half) => println!("half: {}", half),
        Err(reason) => println!("failed: {}", reason),
    }
    match half_the_age("twenty") {
        Ok(half) => println!("half: {}", half),
        Err(reason) => println!("failed: {}", reason),
    }
}
// 预期输出:
// half: 12
// failed: cannot parse: twenty
```

Whichever stage fails, its error travels up through both `?` marks on its own.

</details>

### Exercise 3: a panic experiment

Write a function that panics with `panic!("argument cannot be negative: {}", input)` when its argument is negative, and call it with `-5`. Read the output.

<details>
<summary>🔍 Show the answer</summary>

```rust
fn strict_check(input: i32) {
    if input < 0 {
        panic!("argument cannot be negative: {}", input);
    }
    println!("check passed: {}", input);
}

fn main() {
    strict_check(-5);
}
// 预期行为: 运行失败
```

The output starts with `thread 'main' panicked at ...` — the program stops on the spot, and the `println!` inside the function never runs.

</details>

---

## 16.15 Frequently Asked Questions

**Q: Why not just throw exceptions like other languages?**

A: Exceptions get missed — nobody is forced to catch them. Rust **writes the failure into the return type**: `Result<T, E>` is visible at a glance, and the compiler keeps you to the bargain of handling it. Errors are values, not accidents.

**Q: How do I choose between `Option` and `Result`?**

A: `Option` means "there may be nothing here, and that isn't an error" (a name not in the phone book — normal). `Result` means "it failed, and there's a reason to report" (the file's format is broken). The first one is `Some`/`None`; the second is `Ok`/`Err`.

**Q: Can `?` convert error types by itself?**

A: It can, through `From`. 16.8 showed the mechanism: implement `From<Source>` and `?` translates one error into the other automatically. Chapter 25 puts it to work on a real-sized project.

**Q: Is data lost when a panic happens?**

A: The default behaviour is to unwind — destructors run as the stack unwinds — and then the process exits. Either way: **a panic is the end of the program's life**. Don't count on finishing the work afterwards.

**Q: When is panicking genuinely the right call?**

A: Three rules of thumb: ① in tests (a failed assertion should panic); ② when an internal agreement of your own program has been broken (a bug-level contradiction); ③ in example code, where brevity matters. For anything a user can hit: `Result`.

**Q: Can `main` return a `Result`?**

A: Yes — 16.8 has the full form: `fn main() -> Result<(), MyError>`. When it ends in `Err`, the program prints `Error: ...` on standard error and exits with code 1.

---

## Next chapter preview

Every program so far lives in a single file. Real projects run to hundreds of lines and have to be sorted.

Chapter 17, **Modules**, teaches you to:

- put code into separate drawers with **`mod`**;
- find what's inside a drawer with **`use`** and paths;
- draw the line between **public** and private;
- organise a multi-file project.

See you in the next chapter!
