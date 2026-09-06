# Chapter 8: References and Borrowing

## 8.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain the relationship between a **reference** and **borrowing** (one is a thing, the other an action);
2. Make read-only borrows with **`&`** and mutable borrows with **`&mut`**;
3. State the borrowing iron rule: "**either one mutable, or many read-only**";
4. Read and fix **E0502** (borrowing conflict) and **E0596** (cannot borrow as mutable);
5. Use **`*`** to dereference — take the original value out of the borrow slip;
6. Know that "a borrow lives until its last use".

---

## 8.1 The problem left by last chapter

Chapter 7 said: a string passed into a function **moves**, and the original variable dies. But often we only want the function to **take a look** — not to give the thing away:

```rust
fn show(sentence: String) {   // pass by value: sentence moves in, destroyed after use
    println!("{}", sentence);
}

fn main() {
    let greeting = String::from("Hello");
    show(greeting);
    // greeting is gone — want to use it again? No way
}
```

Confiscating the original after one look is far too bossy. The real-world solution is — **lend it**.

## 8.2 References: a borrow slip

💡 **Metaphor**: a classmate wants to read your novel. You don't **transfer ownership** of the book — you say "borrow it". What they hold is a **borrow slip** — it leads to the book, but the book is still yours.

📖 **Reference**: a "borrow slip" pointing at some value, written `&value`. `&` reads "take a reference". A reference is not the value itself; it only records "where the value lives".
📖 **Borrowing**: the **act** of using someone else's data temporarily through a reference. Borrowed things come back — a reference never changes ownership.

### The read-only borrow

```rust
// The parameter type &String: borrow to look
fn show(sentence: &String) {
    println!("{}", sentence);
}

fn main() {
    let greeting = String::from("Hello");
    show(&greeting);     // hand over a slip
    show(&greeting);     // lend again — fine
    println!("still here: {}", greeting);   // the original is intact
}
```

Output:

```
Hello
Hello
still here: Hello
```

`&greeting` mints a slip and hands it to the function. The function reads, the slip expires, and `greeting` is still the owner. Lend as many times as you like.

## 8.3 The mutable borrow: borrow to modify

Sometimes you don't just want to look — you want to **change** things. Like writing notes in a book's margins. That takes the **mutable borrow**, `&mut`:

```rust
fn annotate(book: &mut String, note: &str) {
    book.push_str(note);   // push_str: append text to the end
}

fn main() {
    let mut book = String::from("Romance of the Three Kingdoms");
    annotate(&mut book, " (youth edition)");
    println!("{}", book);
}
```

Output: `Romance of the Three Kingdoms (youth edition)`

Two new requirements:

| Requirement | Spelling | Why |
|---|---|---|
| The borrowed value must be declared mutable | `let mut book = ...` | An immutable thing can't be lent out for modification |
| Write `&mut` when lending | `annotate(&mut book, ...)` | It declares "this is a mutable borrow" |

> 📖 **push_str**: a string method appending text to the end. Its sibling `push` appends a single character.

## 8.4 The borrowing iron rule: pick one

Rust has one iron rule for borrowing:

> **At any moment: either any number of read-only borrows, or exactly one mutable borrow. Never both.**

💡 **Metaphor**: the library's rules —
- **Read-only borrow** = several people **reading in the reading room** at once; many readers, no problem;
- **Mutable borrow** = someone taking the book away to **rebind it** — that requires exclusivity; nobody may look.
- The two **cannot happen at once**: while others are reading, you yank the book away to rebind it, and everyone's page numbers turn to nonsense.

Breaking the iron rule reports **E0502**:

```rust
// 预期错误: E0502
fn main() {
    let mut slogan = String::from("go team");
    let peek = &slogan;        // read-only borrow active
    let edit = &mut slogan;    // ❌ a mutable borrow at the same time
    println!("{}", peek);     // peek is still used after this — the borrow must live to here
}
```

The real error:

```
error[E0502]: cannot borrow `slogan` as mutable because it is also borrowed as immutable
 --> src/main.rs:4:16
 |
3 |     let peek = &slogan;        // immutable borrow occurs here
4 |     let edit = &mut slogan;    // ❌ mutable borrow occurs here
  |                ^^^^^^^^^^ mutable borrow occurs here
5 |     println!("{}", peek);     // immutable borrow later used here
```

### Why so strict?

Suppose "reading while writing" were allowed: function A holds a slip and reads the string, while function B suddenly appends text — the string may need to **move to a bigger room** in the warehouse (reallocating memory) — and A's slip instantly points at an **already-vacated** room. Crash. The iron rule kills this accident class at compile time.

### Borrowing one after another is fine

The rule governs "**at the same time**". Once the previous borrow is finished, the next one comes — perfectly legal:

```rust
let mut book = String::from("Romance of the Three Kingdoms");
show(&book);                       // first read-only borrow, expired after use
annotate(&mut book, " (youth edition)"); // mutable borrow, expired after use
show(&book);                       // another read-only borrow, no problem
```

### A borrow's "lifetime"

A borrow's lifetime = **from birth to its last use**. Once used up, the iron rule no longer applies to it:

```rust
let mut slogan = String::from("go team");
let peek = &slogan;
println!("{}", peek);    // peek last used here
// peek's lifetime ends here
let edit = &mut slogan;    // ✅ fine: the read-only borrow has expired
edit.push('!');
```

> ✨ This is the compiler being clever: it doesn't care how long you "kept" the slip — only where you **actually used it**.

## 8.5 No modifying through a read-only borrow: E0596

The parameter is declared `&String` (read-only), yet you try to modify it inside the function — that's **E0596**:

```rust
// 预期错误: E0596
fn mess_with(book: &String) {
    book.push('!');    // ❌
}
```

The real error:

```
error[E0596]: cannot borrow `book` as mutable, as it is not declared as mutable
```

**Fix**: to modify, change the parameter to `&mut String`, and make sure the caller's variable is `let mut`.

## 8.6 Dereferencing: taking the original out of the slip

📖 **Dereference**: take the value a reference points to — the symbol `*`.

```rust
fn main() {
    let score = 95;
    let slip = &score;
    let copied = *slip;      // * follows the slip to the original, copies it out
    println!("the copied value: {}", copied);
}
```

Output: `the copied value: 95`

`&` "writes a slip"; `*` "takes goods by slip" — opposite directions.

> ✨ Everyday code rarely dereferences by hand — when you call methods, the compiler follows the chain automatically (`(&book).len()` is simply written `book.len()`). Knowing `*` exists is enough to read error messages.

## 8.7 Can you borrow an integer?

Yes. References and copying don't conflict:

```rust
fn main() {
    let score = 95;
    let slip = &score;
    println!("the slip points at: {}", slip);
    println!("the original is intact: {}", score);   // integers copy anyway — all is well
}
```

But integers are small and copy anyway — pass them by value. References truly shine with the "big items": **strings, vectors, structs**.

---

## 8.8 A complete example: the library lending system

Read-only borrows, mutable borrows, many-at-once, one-after-another — all in one story:

```rust
// Read-only borrow: look and return
fn show(book: &String) {
    println!("book contents: {}", book);
}

// Mutable borrow: modify and return
fn annotate(book: &mut String, note: &str) {
    book.push_str(note);
}

// Two read-only borrows at once: no problem
fn compare_lengths(a: &String, b: &String) {
    if a.len() > b.len() {
        println!("a is longer");
    } else {
        println!("b is no shorter");
    }
}

fn main() {
    let mut book = String::from("Romance of the Three Kingdoms");
    show(&book);                       // read-only borrow
    annotate(&mut book, " (youth edition)"); // mutable borrow
    show(&book);                       // read-only borrow

    let another = String::from("Journey to the West");
    compare_lengths(&book, &another);  // two read-only borrows in parallel

    println!("the original book survives: {}", book);
}
```

## 8.9 Line by line

- **Lines 2–4**: `show` takes `&String` — borrow, don't take. The original is untouched after the call.
- **Lines 7–9**: `annotate` takes `&mut String`; `push_str` appends text at the end.
- **Lines 12–18**: `compare_lengths` holds two read-only borrows at once — the iron rule allows "many readers in parallel". `.len()` returns the **byte count** (one Chinese character takes 3 bytes; "三国演义" is 12).
- **Line 21**: `let mut` — a mutable borrow comes later, so mutability must be declared.
- **Lines 22–24**: three borrows happen **one after another**; each expires after use — no conflicts.
- **Line 26**: two "books" borrowed read-only at the same time.
- **Line 28**: every borrow returned; the original book is intact and still printable.

## 8.10 What you should see

```
book contents: Romance of the Three Kingdoms
book contents: Romance of the Three Kingdoms (youth edition)
a is longer
the original book survives: Romance of the Three Kingdoms (youth edition)
```

"Romance of the Three Kingdoms (youth edition)" has more bytes than "Journey to the West", hence "a is longer".

---

## 8.11 Common mistakes and how to fix them

### Mistake one: E0502 borrow conflict (mutable + read-only at once)

```
error[E0502]: cannot borrow `slogan` as mutable because it is also borrowed as immutable
```

**Fix**: stagger the two borrows — finish the read-only business first (use it up), then start the mutable borrow; or simply clone a copy and edit that.

### Mistake two: E0596 modifying through a read-only borrow

```
error[E0596]: cannot borrow as mutable
```

**Fix**: check three places at once — ① is the original variable `let mut`; ② does the call site write `&mut`; ③ is the function's parameter type `&mut Type`? All three must hold.

### Mistake three: forgot `let mut`

Writing `&mut a` on an immutable variable also errors with E0596. **Fix**: add `mut` at the declaration.

### Mistake four: a reference outliving its owner

```rust
// 预期错误: E0597
let slip;
{
    let temporary = String::from("short-lived");
    slip = &temporary;
}
println!("{}", slip);   // ❌ temporary is gone — the slip is now a bounced check
```

The error complains the "temporary value was released too early" (`temporary` does not live long enough). **Fix**: a slip's lifetime must not exceed the original's. Let the original outlive the slip.

> 📖 **Dangling reference**: a reference pointing at vanished data. Rust forbids it at compile time — the core guarantee of reference safety.

---

## 8.12 Chapter glossary

| Term | One-line meaning |
|---|---|
| Reference | A "borrow slip" pointing at a value, written `&value` |
| Borrowing | Using someone else's data temporarily through a reference, ownership unchanged |
| Read-only borrow | `&value` — many may exist at once |
| Mutable borrow | `&mut value` — only one at any moment |
| Borrowing iron rule | Either many read-only, or one mutable — never both |
| Dereference | Taking the original value out of a reference with `*` |
| E0502 | The error code for a mutable/read-only borrow conflict |
| E0596 | The error code for failing to borrow as mutable |
| Dangling reference | A reference to vanished data (forbidden at compile time in Rust) |
| push_str | The method appending text to a string's end |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 8.13 Exercises

> 💪 Try first, then peek.

### Exercise one: the read-only borrow

Write a function `report_length(phrase: &String)` that prints "this sentence is N bytes". Call it twice on the same string in the main function, proving the original wasn't taken away.

<details>
<summary>🔍 View answer</summary>

```rust
fn report_length(phrase: &String) {
    println!("this sentence is {} bytes", phrase.len());
}

fn main() {
    let phrase = String::from("study hard");
    report_length(&phrase);
    report_length(&phrase);
    println!("original: {}", phrase);
}
```

Output (the phrase occupies 12 bytes in UTF-8):

```
this sentence is 12 bytes
this sentence is 12 bytes
original: study hard
```

</details>

### Exercise two: the mutable borrow

Write a function `double_phrase(phrase: &mut String)` that appends the string to its own end (hint: clone first, then push — otherwise you'll trip over borrowing from yourself).

<details>
<summary>🔍 View answer</summary>

```rust
fn double_phrase(phrase: &mut String) {
    let copy = phrase.clone();
    phrase.push_str(&copy);
}

fn main() {
    let mut slogan = String::from("go team");
    double_phrase(&mut slogan);
    println!("{}", slogan);
}
```

Output: `go teamgo team`

(Writing `phrase.push_str(phrase)` directly is a borrow conflict: `phrase` is mutably borrowed on one side and read on the other. Cloning first dodges it.)

</details>

### Exercise three: true or false

Which of these lines compile?

```rust
let mut notes = String::from("math");
let a = &notes;
let b = &notes;
let c = &mut notes;
```

<details>
<summary>🔍 View answer</summary>

The `a` and `b` lines are fine (many read-only borrows in parallel); the `c` line reports E0502 — because if `a` and `b` are used again later, they conflict with the mutable borrow. If everything before `c` is "used up and expired" (they never appear again), the compiler lets it through.

</details>

---

## 8.14 FAQ

**Q: Is `&String` the same as the string slice `&str`?**

Siblings, not twins. `&String` is "a reference to a `String`"; `&str` is "a borrow of the text itself". A string literal `"Hello"` is an `&str`. Both "read text read-only", and most methods work on either. Chapter 9 draws the line and shows the conversions.

**Q: Why can't there be two mutable borrows?**

Both claim "I will modify" — different orders of modification produce different results, and the data turns to chaos. Keeping exactly one mutable borrow makes "who changed what" perfectly clear.

**Q: Does borrow checking cost runtime speed?**

No. It all happens at compile time; runtime carries zero overhead. Rust's "zero-cost safety".

**Q: I keep mixing up the borrowing rules. A mnemonic?**

One sentence: **"reads may crowd, writes must be alone, and readers and writers never share a room."**

**Q: Should function parameters take values or borrows?**

Rule of thumb — **look: `&`; modify: `&mut`; truly take: pass by value**. For small things like integers, passing by value is fine too (copying is cheap).

**Q: Are a method's `&self` / `&mut self` the same as this chapter's `&` / `&mut`?**

Exactly the same. A method's `&self` means "this method borrows the instance read-only"; `&mut self` means "borrows it mutably". You should now understand Chapter 6 completely.

---

## What's next

We still owe "text" its own chapter: why can't string literals be modified? What's the real relationship between `String` and `&str`? How do you join, cut and search text?

**Chapter 9, "Strings and Text"**, covers it all:

- A thorough comparison of the two text types;
- Joining, appending, slicing, searching;
- The trap of Chinese characters vs byte lengths;
- The right way to walk through text.

See you in the next chapter!
