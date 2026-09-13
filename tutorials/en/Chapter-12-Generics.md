# Chapter 12: Generics

## 12.0 Learning Objectives

After reading this chapter, you will be able to:

1. Explain the problem generics solve: **one piece of code, many types**;
2. Write **generic functions** with `<T>`;
3. Read and understand **trait bounds** like `<T: Ord>`;
4. Write **generic structs** and **generic impl blocks**;
5. Explain **monomorphization**: why generics don't slow your program down;
6. Recognize old friends: `Option<T>`, `Result<T, E>` are all generic enums.

---

## 12.1 The Frustration of Copying Code

You want to find the larger of two numbers. For integers:

```rust
fn max_integer(a: i32, b: i32) -> i32 {
    if a > b { return a; }
    b
}
```

What about floating-point numbers? **Almost identical**, only the types changed:

```rust
fn max_float(a: f64, b: f64) -> f64 {
    if a > b { return a; }
    b
}
```

What about long integers? Characters? How long until this copying ends? Every time you change the logic, you have to edit N copies, and missing one is a bug.

💡 **Analogy**: You have one mold. Before, you needed a **dedicated machine for each type of cookie** (integer machine, float machine…). Now you want **one universal machine** — put in any dough, get out the corresponding shape of cookie.

📖 **Generics**: Write code using a "type placeholder", fill in the concrete type when you use it. The placeholder is conventionally written as `<T>` (T stands for "Type").

## 12.2 Generic Functions: The Universal Machine

```rust
// Expected error E0369 — no `>` operator for `T`
fn max<T>(a: T, b: T) -> T {
    if a > b {
        return a;
    }
    b
}
```

Reading: "`max` takes some type `T` (placeholder), two parameters of that type, returns the same type."

But this code won't compile! The error roughly says: no `>` operator found on type `T`. The reason is simple: **the universal machine can't do everything** — you dump in some "I don't know what" dough, how does the machine know if it can compare sizes? You must give it **requirements**.

## 12.3 Trait Bounds: Setting Requirements for Types

```rust
fn max<T: Ord>(a: T, b: T) -> T {
    if a > b {
        return a;
    }
    b
}

fn main() {
    println!("Larger integer: {}", max(3, 9));
    println!("Larger big integer: {}", max(3_i64, 9_i64));
}
```

Output:

```
Larger integer: 9
Larger big integer: 9
```

📖 **Trait bound**: The requirement in angle brackets, like `<T: Ord>` — "`T` must have the ability to compare."

📖 **Ord**: A standard trait meaning "can compare sizes." Integers and characters implement it — floats only have `PartialOrd`.

The same function works for many types — because at the call site, the compiler sees the concrete types and automatically "fills in" the placeholder. The full concept of traits is covered in the next chapter; for now, read bounds as "**capability requirements**."

## 12.4 Generic Structs: Universal Containers

Structs can also carry type placeholders:

```rust
struct Pair<T> {
    first: T,
    second: T,
}
```

`Pair` can hold anything, but **both fields must be the same type** (all the same `T`):

```rust
let num_pair = Pair { first: 1, second: 2 };          // T = i32
let str_pair = Pair { first: String::from("天"), second: String::from("地") };  // T = String
// Pair { first: 1, second: "二" }   ❌ types don't match — error
```

## 12.5 Generic Impl Blocks

To add methods to a generic struct, the `impl` block also needs the placeholder:

```rust
impl<T> Pair<T> {
    // Consumes self, returns a new instance with first and second swapped
    fn swap(self) -> Pair<T> {
        Pair { first: self.second, second: self.first }
    }
}
```

Three places line up: `impl<T>` (declare the placeholder) + `Pair<T>` (which struct) + the method body uses `T` as the return type.

> ⚠️ **Careful**: `swap`'s first parameter is `self` (consumed), not `&self` (borrowed). Because you're taking `first` and `second` out of the instance and swapping them — borrowing can't handle "moving stuff." Consume the old one, return a new one, clean and efficient.

## 12.6 Monomorphization: Why Generics Aren't Slow

A "universal machine" sounds like it might guess types at runtime and be slow. **It's not.**

📖 **Monomorphization**: At compile time, the compiler generates **one dedicated copy of the code for each concrete type** you use.

You wrote one `max<T: Ord>`, called it with `i32` and `i64` — the compiled output actually contains `max_i32_version` and `max_i64_version`, two separate pieces of machine code.

💡 **Analogy**: You wrote a "universal machine blueprint," and the factory (compiler) built two dedicated machines for the assembly line based on orders. At runtime, the dedicated machines run — the speed is the same as if you'd handwritten both copies. **Use generics for convenience when coding, get dedicated-code speed when running** — this is Rust's "zero-cost abstraction."

## 12.7 Old Friends Are All Generics

Look back at `Option` from Chapter 11:

```rust
enum Option<T> {
    Some(T),
    None,
}
```

`Option<i32>` holds integers, `Option<String>` holds strings — **one enum definition, infinitely many concrete types**. Chapter 16's `Result<T, E>` is the same (T holds the success value, E holds the error). Generics aren't new knowledge — you've been using them all along.

---

## 12.8 Where Clauses and Constant Generics

### Moving Bounds Out: the Where Clause

Bounds always sit inside angle brackets: `<T: Ord>`. When there are many requirements, the signature gets long and cramped:

```
fn max<T: Ord + Display>(a: T, b: T) -> T { ... }
```

Hard to read. Rust lets you **move the bounds out of the angle brackets**, writing them separately before the function body, starting with `where`:

```rust
use std::fmt::Display;

// Capability requirements moved from angle brackets into a where clause
fn report_larger<T>(a: T, b: T) -> T
where
    T: Ord + Display,
{
    if a > b { a } else { b }
}

fn main() {
    println!("Larger: {}", report_larger(7, 3));
}
```

Output:

```
Larger: 7
```

📖 **Where clause**: Moves trait bounds from angle brackets to before the function body. Two rules: `where` takes its own line, constraints for different type parameters are separated by **commas**, and multiple constraints for the same parameter are joined with `+`.

> ⚠️ The angle-bracket style and where-clause style are **completely equivalent** — the compiler treats them identically. It's purely about "where you place them." Few constraints? Angle brackets are convenient. Many constraints, long signature? Move into a where clause so the signature stays readable and you can read the requirements line by line.

### Constant Generics: Lengths as Parameters

Generic parameters can be **constants** — a number known at compile time. The most common use: array lengths.

```rust
struct FixedBox<T, const N: usize> {
    data: [T; N],
}
```

Reading: "`FixedBox` has two parameters: `T` is a **type** (what to hold), `const N` is a **number** (how many to hold)."

💡 **Analogy**: Type generics are a "universal mold" — put in any dough, get any cookie; constant generics are an "adjustable-size mold" — turn the knob (N), and the output size changes.

A runnable example — the same struct, holding 3 integers, 2 strings:

```rust
struct FixedBox<T, const N: usize> {
    data: [T; N],
}

fn main() {
    let small = FixedBox::<i32, 3> { data: [1, 2, 3] };
    let large = FixedBox::<String, 2> { data: [String::from("天"), String::from("地")] };
    println!("Small box holds {} items: {}", small.data.len(), small.data[0]);
    println!("Large box holds {} items: {}", large.data.len(), large.data[1]);
}
```

Output:

```
Small box holds 3 items: 1
Large box holds 2 items: 地
```

Note the `FixedBox::<i32, 3>` syntax: inside angle brackets, **fill in the type first, then the number**. `N` must be a compile-time constant — using a variable for the length will cause an error (see Common Errors below).

> ⚠️ `usize` is the unsigned integer type for array lengths (the "machine word" size). `const N` just counts — it works whether you hold integers, strings, or anything else in the same `FixedBox`.

### Common Error: Using a Runtime Variable for Array Length

```rust
// Expected error E0435
fn main() {
    let count = 3;
    let data: [i32; count] = [1, 2, 3];
    println!("{}", data[0]);
}
```

```
error[E0435]: constant value used in a constant
 --> line 3: at the array length
```

**Fix**: Array lengths and constant generics must be **numbers known at compile time** — a literal `3` works, a `const N: usize` parameter works, a plain `let count = 3` does not. If the length comes from a runtime variable? Use a `Vec` (Chapter 15) or design the length as a constant generic parameter.

---

## 12.9 Complete Example: Universal Toolbox

Generic functions + generic structs + generic methods, all in one:

```rust
// Generic function: works for any type that can be compared
fn max<T: Ord>(a: T, b: T) -> T {
    if a > b {
        return a;
    }
    b
}

// Generic struct
struct Pair<T> {
    first: T,
    second: T,
}

// Generic impl block
impl<T> Pair<T> {
    fn swap(self) -> Pair<T> {
        Pair { first: self.second, second: self.first }
    }
}

fn main() {
    // One function, two types
    println!("Larger integer: {}", max(3, 9));
    println!("Larger big integer: {}", max(3_i64, 9_i64));

    // One struct, two uses
    let pair_nums = Pair { first: 1, second: 2 };
    let swapped = pair_nums.swap();
    println!("After swap: first={}, second={}", swapped.first, swapped.second);

    let pair_strs = Pair { first: String::from("天"), second: String::from("地") };
    println!("First: {}", pair_strs.first);
}
```

## 12.10 Line-by-Line Walkthrough

- **Lines 2–7**: `max` with the `Ord` bound — only types that can be compared can call it.
- **Lines 10–13**: `Pair<T>` has two fields of the same type.
- **Lines 16–20**: Generic impl block. `swap` consumes self, creates a new instance.
- **Lines 24–25**: One function, first call fills `T = i32`, second call fills `T = i64`.
- **Lines 28–30**: `pair_nums` has `T = i32`. After calling `swap`, `pair_nums` is consumed (the ownership rules from Chapter 7 apply to generics just the same), and `swapped` catches the new instance.
- **Lines 32–33**: `pair_strs` has `T = String` — the same struct definition, holding completely different things.

## 12.11 Output

```
Larger integer: 9
Larger big integer: 9
After swap: first=2, second=1
First: 天
```

---

## 12.12 Common Errors and Fixes

### Error One: E0369 Operator Not Found on Type T

Using `>`, `+` directly on `T` in a generic function causes errors. **Fix**: Add trait bounds — use `Ord` for comparison, `PartialEq` for equality checks, `Display` for printing. Chapter 13 covers this systematically.

### Error Two: Generic Struct with Mismatched Field Types

`Pair { first: 1, second: "二" }` causes errors. **Fix**: Same placeholder = same type. Want two different types? Use two placeholders: `struct Pair<A, B> { first: A, second: B }`.

### Error Three: Forgot the Placeholder in the Impl Block

`impl Pair<T>` without the preceding `impl<T>` reports "type parameter `T` not found." **Fix**: `impl<T> Pair<T>` — the angle brackets appear twice, each with its own job.

### Error Four: E0507 Trying to Move From a Borrow

Returning `self.field` (where the field is a moveable type like String) from a method taking `&self` reports "cannot move out of borrowed content." **Fix**: Change the method to consume `self`, or return `.clone()` of the field.

---

## 12.13 Chapter Glossary

| Term | One-sentence explanation |
|---|---|
| Generics | Type placeholders, one codebase for many types, written as `<T>` |
| Type parameter | Placeholder in angle brackets, like `<T>` |
| Trait bound | Capability requirement on a type parameter, like `<T: Ord>` |
| Ord | Standard trait for "can compare sizes" |
| PartialEq | Standard trait for "can check equality" |
| Monomorphization | Compiler generates dedicated code for each concrete type at compile time |
| Zero-cost abstraction | Design goal: convenient to use, free to run |
| Where clause | Move trait bounds out of angle brackets into a separate clause |
| Constant generic | Generic parameter that is a compile-time constant value, like `const N: usize` |

> 📖 **Reminder**: For unfamiliar terms, refer back to the "Master Glossary" at the front of this book.

---

## 12.14 Exercises

> 💪 Try first, then check the answers.

### Exercise One: Generic Minimum

Write `min<T: Ord>` following the pattern of `max`, test it with both `i32` and `i64`.

<details>
<summary>🔍 See Answer</summary>

```rust
fn min<T: Ord>(a: T, b: T) -> T {
    if a < b {
        return a;
    }
    b
}

fn main() {
    println!("{}", min(3, 9));
    println!("{}", min(3_i64, 2_i64));
}
// Expected output:
// 3
// 2
```

Output: `3`, `2`

</details>

### Exercise Two: Two-Type Struct

Define `Tuple<A, B> { left: A, right: B }`, create one with `Tuple { left: 100, right: String::from("分") }`, and print both fields.

<details>
<summary>🔍 See Answer</summary>

```rust
struct Tuple<A, B> {
    left: A,
    right: B,
}

fn main() {
    let score = Tuple { left: 100, right: String::from("分") };
    println!("{}{}", score.left, score.right);
}
// Expected output: 100分
```

Output: `100分`

</details>

### Exercise Three: Generic Method

Add a method `get_left(&self) -> &A` to `Tuple<A, B>` that returns a **reference** to the left field. Hint: returning a reference doesn't move anything, borrowing is enough.

<details>
<summary>🔍 See Answer</summary>

```rust
impl<A, B> Tuple<A, B> {
    fn get_left(&self) -> &A {
        &self.left
    }
}
```

(The full signature involves lifetime elision rules, which the compiler can infer here automatically. Chapter 14 covers the principles.)

</details>

---

## 12.15 Frequently Asked Questions

**Q: Does `T` in `<T>` have to be called `T`?**

A: No, any name works. This book uses Chinese placeholders (`数`, `物`) for clarity, but in real projects single letters `T`, `U` are most common.

**Q: Where do I put bounds?**

A: At the declaration: `fn name<T: Ord>(...)`. Multiple bounds with `+`: `<T: Ord + Display>` (can both compare and print). Multiple type parameters separated by commas: `<A, B>`.

**Q: Are generics the same as "any type" interfaces in other languages?**

A: No. Generics determine types at **compile time** and generate dedicated code for each — fast and type-safe. "Any type" interfaces check types at runtime, which is slower and error-prone.

**Q: When should I use generics?**

A: When you find yourself **copying almost identical code, only the types differ**. After two or three repetitions, it's worth abstracting. But don't use generics for the sake of it — if there's only one type in play, writing the concrete type is clearer.

**Q: Are `Option<i32>` and `Option<String>` the same type?**

A: No. They're two distinct types produced by filling different parameters into the same template. They can't be assigned to each other. Like a red bin and a blue bin — same design, but two different boxes.

**Q: What's the relationship between "traits" in the next chapter and "bounds" in this chapter?**

A: A trait = **the definition of a capability** (what "comparable" means); a bound = **the requirement that a type has a capability** when using generics. The next chapter teaches you to define your own traits, which fully unlocks the power of generics.

---

## Next Chapter Preview

Generics abstract "types," but "capabilities" aren't abstracted yet: `Ord`, `Display` — these standard capabilities are built-in. How can **my own type** gain "can print", "can compare" abilities? How do I define entirely new capabilities?

Chapter 13 **Traits** reveals:

- Define traits: `trait Intro { ... }`;
- Implement traits for types (`impl TraitName for TypeName`);
- Default methods, full syntax for trait bounds;
- The truth behind the `#[derive(...)]` sticker.

See you in the next chapter!
