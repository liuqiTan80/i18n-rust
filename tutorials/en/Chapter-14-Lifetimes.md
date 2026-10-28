# Chapter 14: Lifetimes

## 14.0 Learning Objectives

After reading this chapter, you will be able to:

1. Explain what a **lifetime** actually governs (how long a borrow slip stays valid);
2. Read and write the **`'a`** annotation syntax;
3. Know when a function **must** be annotated, and when the compiler works it out alone;
4. Annotate a **struct that holds a borrow**;
5. Read and fix **E0106** (missing annotation) and **E0597** (slip outlives the original);
6. Understand **`'static`** — why text carved in stone never expires.

> ✨ The punctuation in this chapter looks far scarier than it is. It does one job: **make sure every slip in your hand still points at something alive.** Read slowly, run every example.

---

## 14.1 Recap: slips expire

Chapter 8 said: a reference is a **borrow slip**. You write `&value` to lend something out, and the borrower hands it back.

💡 **Metaphor**: every slip carries an **expiry date** — from the moment it's written until the moment the original is dropped. Inside the window, use it freely; after the window, the slip is worthless.

```rust
{
    let temporary = String::from("short-lived");
    let slip = &temporary;
    println!("{}", slip);   // ✅ inside the block: the original is still alive
}
// the block ends, `temporary` is dropped — every slip pointing at it bounces
```

📖 **Lifetime**: the span of code over which a reference stays valid. Written `'name` — an apostrophe plus a name, like `'a`. It is a compile-time marker only; it costs nothing at runtime.

Somewhere inside the compiler sits a department called the **borrow checker**, and all day long it asks one question: *at the moment this slip is used, is the original still alive?*

## 14.2 Most of the time, the compiler does the math

You have written thirteen chapters of code without ever typing `'a`. That's because the borrow checker computes the window itself:

```rust
let greeting = String::from("hello");
let slip = &greeting;      // window: from this line until `greeting` is dropped
println!("{}", slip);      // ✅ inside the window
```

When a borrow comes from **one single source**, its expiry is obvious. Nothing for you to say.

## 14.3 When you must say it: several sources

Write a function that returns the longer of two pieces of text:

```rust
// 预期错误: E0106
fn longest(x: &str, y: &str) -> &str {
    x
}
```

The compiler answers with **E0106**:

```text
error[E0106]: missing lifetime specifier
   | fn longest(x: &str, y: &str) -> &str {
💡 The compiler needs lifetime annotations to understand how long references are valid. Add lifetime parameters like `&'a str`.
💡 Suggestion: this function's return type contains a borrowed value, but the signature does not say whether it is borrowed from `x` or `y`
💡 Suggestion: consider introducing a named lifetime parameter
```

Why can't it figure this one out? The return value is **either x or y** — and a signature states types, not body logic. Standing outside the function, the compiler genuinely cannot tell whose expiry the result inherits. **You have to say it out loud.**

## 14.4 `'a`: a joint guarantee

```rust
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() >= y.len() {
        return x;
    }
    y
}
```

Piece by piece:

| Fragment | How to read it |
|---|---|
| `longest<'a>` | "this function involves one expiry window, call it `'a`" (declaration) |
| `x: &'a str` | "x borrows some text, and its window is at least `'a`" |
| `y: &'a str` | "same for y" |
| `-> &'a str` | "the return value's window is `'a` as well" |

The core meaning: **the slip you hand back never outlives either slip that came in.** As long as the caller keeps both inputs alive, the result is safe to use.

> 💡 **Metaphor**: `'a` is a **joint guarantee**. Three parties sign the same clause — x, y, and the return value: "our validity is tied together; whoever expires first stops the whole deal."

> ✨ **Important clarification**: an `'a` annotation **does not change** how long anything actually lives. Lifetimes are decided by your code's structure — where values are declared, where blocks end. The annotation only **states the relationship** to the compiler, like writing a clause into the contract instead of inventing new physics.

## 14.5 Structs that hold a slip

Can a struct field be a reference? Yes — but then you must annotate it, which declares "**this struct may not outlive the data it borrowed**":

```rust
struct Excerpt<'a> {
    content: &'a str,
}

impl<'a> Excerpt<'a> {
    fn report(&self) {
        println!("excerpt: {}", self.content);
    }
}

// usage:
let source = String::from("I sleep and know not the dawn, everywhere I hear birds crying");
let piece = Excerpt { content: &source };
piece.report();
```

Reading: "`Excerpt` holds a borrowed piece of text; `'a` is that borrowing's window. `piece` may only be used while `source` is alive."

> ⚠️ **Remember Chapter 11?** It told you not to put `&str` in enum variant luggage — that warning was about *this*: put a slip in a type and the lifetime annotation comes with it. Put a `String` in, and you write nothing at all. **Beginner rule: when a struct or enum needs data, give it data it owns (`String`, `Vec`); keep the slips for function parameters.**

## 14.6 When you can skip it: the three elision rules

Three automatic rules. Know they exist; don't memorize the wording.

1. **Every reference parameter gets its own window** — in `fn foo(a: &str, b: &str)`, `a` and `b` are treated as two independent lifetimes;
2. **If there is exactly one reference parameter**, its window is handed to the return value — that's why `fn first_word(text: &str) -> &str` needs no annotation;
3. **If `&self` appears in a method**, `self` owns the "protagonist" window, which is automatically passed to inputs and outputs — which is why none of Chapter 6's methods ever mentioned a lifetime.

Not sure? **Write nothing.** When the rules don't cover your case, E0106 tells you exactly where to insert `'a` — and even shows the corrected signature, ready to copy.

## 14.7 `'static`: carved in stone

One kind of reference has the longest window of all — as long as the program itself: string **literals**.

```rust
let slogan = "study hard, improve every day";   // type: &'static str
println!("{}", slogan);
```

That text is **carved in stone** (Chapter 9): baked into the program at compile time, present from start to finish. Its lifetime is called **`'static`**.

You'll almost never write it by hand — literals are recognized automatically. Where most readers first meet the word is inside a compiler message, where a type is written `&'static str`. Don't be alarmed; it only says "this text lives forever, so it certainly outlives your borrow."

## 14.8 Complete Example: the longer text, and an excerpt card

The chapter's two protagonists, together:

```rust
// Explicit lifetime: the return value's window is tied to both parameters
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() >= y.len() {
        return x;
    }
    y
}

// A struct that holds a borrow: must be annotated
struct Excerpt<'a> {
    content: &'a str,
}

impl<'a> Excerpt<'a> {
    fn report(&self) {
        println!("excerpt: {}", self.content);
    }
}

fn main() {
    // —— longest: two slips in, one slip out ——
    let short = "hi";
    let long = String::from("hello, world!");
    let winner = longest(short, &long);
    println!("the longer one: {}", winner);

    // —— excerpt: a struct holding a slip ——
    let source = String::from("I sleep and know not the dawn, everywhere I hear birds crying");
    let piece = Excerpt { content: &source };
    piece.report();
    println!("the original is still here: {}", source);
}
```

## 14.9 Line-by-Line Walkthrough

- **Lines 1–7**: `longest` — all three references (two parameters, one return value) carry `'a`. The joint guarantee is now written into the signature.
- **Lines 9–12**: `Excerpt<'a>` — the struct's *name* takes the `<'a>` parameter too, precisely because one of its fields is a slip.
- **Lines 14–18**: an impl block over a lifetime-bearing type must say `impl<'a> Excerpt<'a>` — the window parameter is handed into the impl, and it has to appear in **both** places.
- **Lines 22–25**: `short` is a literal (`'static`, so it satisfies any `'a` asked of it); `long` is a `String`. Both are lent to `longest`; the returned `winner` shares their window. Both originals outlive every use of `winner`, so this is safe.
- **Lines 28–30**: `piece` borrows `source`'s text. The borrow checker keeps watch: no use of `piece` may cross `source`'s drop point.
- **Line 31**: after `piece` is done with, `source` still prints fine — **borrowing doesn't damage the owner.**

## 14.10 Output

```
the longer one: hello, world!
excerpt: I sleep and know not the dawn, everywhere I hear birds crying
the original is still here: I sleep and know not the dawn, everywhere I hear birds crying
```

`.len()` counts **bytes**: `"hello, world!"` is 13 bytes and wins over `"hi"` at 2. (Chapter 9's warning still stands: `.len()` is not a character count — one Chinese character takes 3 bytes.)

---

## 14.11 Common Errors and Fixes

### Error One: E0106 — missing lifetime specifier

Raised by a function with several reference parameters that returns a reference. **Fix**: do what the diagnostic says — put `'a` on the parameters *and* on the return type. A one-source function doesn't need it (elision rule 2).

### Error Two: E0597 — borrowed value does not live long enough

```rust
// 预期错误: E0597
fn main() {
    let slip;
    {
        let temporary = String::from("short-lived");
        slip = &temporary;
    }
    println!("{}", slip);
}
```

```text
error[E0597]: `temporary` does not live long enough
   |         slip = &temporary;
💡 The reference outlives the value it points at. Move the borrowed value into a wider scope, or clone it so each side owns its own data.
```

The classic shape: you mint a slip inside a block and want to cash it outside. **Three fixes**: ① declare the original in the wider scope so it outlives the slip; ② stop borrowing — build your own data with `String::from(...)`; ③ `.clone()` it.

### Error Three: a struct field is a reference, with no annotation

```rust
// 预期错误: E0106
struct Excerpt {
    content: &str,
}

fn main() {
    let piece = Excerpt { content: "hi" };
    println!("{}", piece.content);
}
```

```text
error[E0106]: missing lifetime specifier
   |     content: &str,
💡 The compiler needs lifetime annotations to understand how long references are valid. Add lifetime parameters like `&'a str`.
```

**Fix**: `struct Excerpt<'a> { content: &'a str }` — or, usually better, drop the borrow altogether and store `content: String`.

### Error Four: annotation in the wrong place

`'a` sticks to the `&`, not to the type: write `&'a str`. Putting it after the type is a syntax error:

```rust
// 预期错误: any
fn main() {
    let banner = "no sleep till Shibuya";
    let _p: & str 'a = banner;
}
```

```text
error: expected one of `!`, `(`, `::`, `;`, `<`, or `=`, found `'a`
   |     let _p: & str 'a = banner;
```

Note there's no `E` code here — the parser gives up before name resolution. Read the token it choked on and move the `'a` back next to the `&`.

---

## 14.12 Chapter Glossary

| Term | One-sentence explanation |
|---|---|
| Lifetime | The span over which a reference stays valid, written `'name` |
| Borrow checker | The compiler department auditing every slip's expiry |
| Lifetime annotation | Stating the relationship between references to the compiler, e.g. `<'a>` |
| Elision rules | The three patterns where the compiler infers lifetimes for you |
| `'static` | A window as long as the program — every string literal has one |
| E0106 | Missing lifetime specifier |
| E0597 | Borrowed value does not live long enough |

> 📖 **Reminder**: for any term that feels rusty, look it up in the Master Glossary at the front of this book.

---

## 14.13 Exercises

> 💪 Try first, then check the answers.

### Exercise One: the shorter one

Write `shortest<'a>`: returns the shorter of two pieces of text (the lifetime annotation is up to you).

<details>
<summary>🔍 See Answer</summary>

```rust
fn shortest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() <= y.len() {
        return x;
    }
    y
}

fn main() {
    println!("{}", shortest("cat", "catalog"));
}
// 预期输出: cat
```

Output: `cat`

</details>

### Exercise Two: a bookmark

Define `Bookmark<'a> { position: &'a str }`, make one that points into a `String`, and print the position.

<details>
<summary>🔍 See Answer</summary>

```rust
struct Bookmark<'a> {
    position: &'a str,
}

fn main() {
    let book = String::from("Chapter 3, Section 5");
    let mark = Bookmark { position: &book };
    println!("reading: {}", mark.position);
}
```

The `impl` block here is optional — a bare struct with one field is enough to see how `<'a>` rides on the type name.

</details>

### Exercise Three: fix it

The program in **Error Two** above reports E0597. Fix it — either of the two approaches counts.

<details>
<summary>🔍 See Answer</summary>

Approach one: move the original into the outer scope, so it outlives the slip.

```rust
fn main() {
    let temporary = String::from("long-lived");
    let slip = &temporary;
    println!("{}", slip);
}
```

Approach two: stop borrowing — take a copy that belongs to you.

```rust
fn main() {
    let owned;
    {
        let temporary = String::from("short-lived");
        owned = temporary.clone();   // `owned` now carries its own data
    }
    println!("{}", owned);
}
```

</details>

---

## 14.14 Frequently Asked Questions

**Q: Do lifetimes make my program slower?**

A: Not one instruction. Every check happens at compile time; nothing is emitted for runtime. `'a` is a note addressed to the compiler, and it's discarded the moment compilation finishes.

**Q: Why don't other languages have this?**

A: Other languages pick one of two bills: either they allow dangling references (and pay for it in crashes), or they run a garbage collector (and pay for it at runtime, always). Rust settles the question at compile time — you type a few `'a` markers and get memory safety with no collector and no cost.

**Q: Must I annotate every reference I write?**

A: No. In everyday code, roughly ninety percent of references need nothing (the elision rules catch them). Only two shapes force your hand: ① a function with several reference parameters that returns a reference; ② a struct or enum that holds a reference. When you miss one, the error points at the exact column.

**Q: Can I use `'a` and `'b` together?**

A: Yes — different names mean different windows, not tied to each other. Only shared `'a` creates the joint guarantee. For example `fn pick<'a, 'b>(one: &'a str, two: &'b str) -> &'a str` says "the return value is only guaranteed by `one`'s window," which is exactly right for a function that may return `one` and never `two`.

**Q: Earlier chapters said "avoid lifetimes for now" — is that still true?**

A: You can relax: you can now read `&'a str` and know what it promises. But the engineering advice stands — a type that owns its data is easier to move around, store, and return. Borrowing is a tool for hot paths and read-only views, not a default.

**Q: How do lifetimes relate to ownership?**

A: Ownership answers "**who** holds the value, and when is it freed". Lifetime answers "**how far** does this slip reach into that owner's life". One is title, the other is a lease term; together they are Rust's memory-safety scheme.

---

## Next Chapter Preview

Everything so far has stored multiple items in arrays — but an array's **length is fixed at birth**. Want to add one more element? No way.

Chapter 15, **Collection Types**, brings the grow-up versions:

- **`Vec<T>`**: an array that can stretch whenever you like;
- **`HashMap<K, V>`**: a phone book you query by name;
- iterating, inserting, removing, looking up — the full set of moves;
- and the subtle way collections and ownership get tangled (storing a slip in a `Vec` is Chapter 14 all over again).

See you in the next chapter!
