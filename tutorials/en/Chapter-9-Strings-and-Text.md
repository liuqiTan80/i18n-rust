# Chapter 9: Strings and Text

## 9.0 Learning goals

By the end of this chapter, you will be able to:

1. Tell **string slices** (`&str`) and **`String`** apart;
2. Create, append to and join strings;
3. Query text with `.len()`, `.contains()`, `.starts_with()` and `.find()`;
4. Explain the difference between **bytes** and **characters** — and why slicing multilingual text can panic;
5. Process text with `.replace()`, `.trim()` and `.split()`;
6. Walk through text one character at a time with `.chars()`.

> ✨ Earlier chapters have owed "text" its own chapter; today the debt is paid. Strings are the most-used type in everyday programming — this one is worth reading slowly.

---

## 9.1 The two text types

Rust has two types for "a piece of text" — the spot where beginners get most confused.

💡 **Metaphor**:
- A **string slice** (`&str`) is like **words carved in stone** — fixed at birth, unchangeable by anyone;
- A **`String`** is like **words in a notebook** — add and edit freely (but the notebook must be declared "mutable").

| Type | Spelling | Character | Lives in |
|---|---|---|---|
| String slice | `&str` | Immutable, fixed text | The stone (built into the program) |
| String | `String` | Growable, modifiable | The warehouse (heap) |

```rust
let stone = "carved in stone";                  // type: &str
let mut notebook = String::from("written down"); // type: String
```

📖 **`String`**: a growable text type, stored on the heap.
📖 **`&str`** (string slice): borrowed, immutable text — a string literal (anything written directly in double quotes) is one.

### How to choose?

- Fixed, never-changing text (prompts, menu items) → a literal is enough (`&str`);
- Text that grows or gets assembled (user input, program-built messages) → a `String`.

---

## 9.2 The stone cannot be edited

Try to modify a literal:

```rust
// 预期错误: E0599
let mut stone = "carved in stone";
stone.push('!');   // ❌ compile error
```

The error says, roughly: you can't call `push` on a `&str`. Stone is stone — even `mut` can't save it. **The type decides the ability.** For editable text, use `String::from("...")` to **copy the inscription onto a notebook**.

---

## 9.3 Creating a string and growing it

```rust
// 从字面量造一个字符串
let mut diary = String::from("Today I learned about strings");

// push: append one character to the end (single quotes!)
diary.push(',');

// push_str: append a run of text
diary.push_str(" so much fun!");

println!("diary: {}", diary);
// 预期输出: diary: Today I learned about strings, so much fun!
```

Output: `diary: Today I learned about strings, so much fun!`

Three points:

| Spelling | Meaning |
|---|---|
| `','` single quotes | **one character** (the char type) |
| `"..."` double quotes | a run of text (a string slice) |
| `String::from("...")` | copy text into a modifiable string |

> ⚠️ **Careful**: single and double quotes must not be mixed. `push` takes a single character (single quotes); `push_str` takes a run of text (double quotes).

---

## 9.4 Joining: `format!` does it all

📖 **format!**: a macro that builds a new string from values and a template. The `{}`s in the template get replaced in order. It looks almost identical to `println!` — the difference: **`println!` shows on screen; `format!` returns a string**.

```rust
let name = "Xiaoming";
let greeting = format!("Hello, {}!", name);
println!("{}", greeting);
// 预期输出: Hello, Xiaoming!
```

Output: `Hello, Xiaoming!`

This one trick covers 90% of all text-joining needs.

---

## 9.5 Querying

```rust
let diary = String::from("Today I learned about strings, so much fun!");

println!("byte length: {}", diary.len());
println!("contains 'strings': {}", diary.contains("strings"));
println!("starts with 'Today': {}", diary.starts_with("Today"));
println!("ends with 'fun!': {}", diary.ends_with("fun!"));
```

Output:

```
byte length: 39
contains 'strings': true
starts with 'Today': true
ends with 'fun!': true
```

| Method | What it does | Returns |
|---|---|---|
| `.len()` | Byte count (careful — not the letter count!) | integer |
| `.contains(小段)` | Does it contain this piece? | boolean |
| `.starts_with(小段)` | Does it start with this? | boolean |
| `.ends_with(小段)` | Does it end with this? | boolean |
| `.find(小段)` | Position of the first occurrence (byte number) | option (Chapter 16) |

---

## 9.6 Bytes vs characters: the multilingual trap

Here's a counter-intuitive number: "Today I learned about strings, so much fun!" is visibly **39 characters** (count them!), and `.len()` happens to return **39** — because every letter and space takes exactly 1 byte in UTF-8. So far so good.

Now watch what happens with text beyond the English alphabet:

📖 **Byte**: the computer's smallest unit for storing text — one little box.
📖 **Character**: what a human eye sees as "one letter".

💡 **Metaphor**: the computer stores text in UTF-8 encoding — an English letter fits in 1 box, but characters from other alphabets need more: "é" takes 2 boxes, a Chinese character like "你" takes 3, an emoji like "🦀" takes 4. `.len()` counts **boxes (bytes)**, not letters.

```rust
fn main() {
    let text = "café ☕";
    println!("bytes: {}", text.len());       // 9
    let mut char_count = 0;
    for _ in text.chars() {
        char_count += 1;
    }
    println!("characters: {}", char_count);  // 6
}
```

Output:

```
bytes: 9
characters: 6
```

Count it: c, a, f, é (2 boxes!), space, ☕ (3 boxes!) — 5 + 1 + 3 = 9 boxes, but only 6 characters. **The same text, two different counts.**

### Why this is dangerous: slicing panics

Chapter 3 taught slices like `&array[0..3]`. Strings can be sliced too — but slicing counts **bytes**:

```rust
fn main() {
    let word = "café";
    let first_three = &word[0..3];    // first 3 bytes = "caf", ✅
    println!("first three: {}", first_three);
}
```

Output: `first three: caf`

But slice into the **middle of a multi-byte character**:

```rust
// 预期行为: 运行失败
fn main() {
    let word = "café";
    let broken = &word[0..4];   // ❌ cuts open "é" (bytes 3..5)
}
```

Runtime panic:

```
thread 'main' panicked at ...:
byte index 4 is not a char boundary; it is inside 'é' (bytes 3..5) of `café`
```

("Byte index 4 is not a character boundary; it lies inside 'é'" — half a box doesn't assemble into a letter, and Rust would rather panic than hand you garbage.)

> ⚠️ **Rule**: when subscript-slicing multilingual text, the boundaries must land in the **gaps between complete characters**. Unsure where the seams are? Use section 9.8's `.chars()`.

---

## 9.7 Processing: replace, trim, split

```rust
// replace: swap every match, returns a new string
let original = String::from("the cat is cute");
let improved = original.replace("cat", "panda");
println!("{}", improved);          // the panda is cute

// trim: strip leading and trailing whitespace
let dirty_data = "  apple,banana  ";
println!("[{}]", dirty_data.trim());   // [apple,banana]

// split: cut into pieces by a separator
for item in "apple,banana,orange".split(",") {
    println!("[{}]", item);
}
```

Output:

```
the panda is cute
[apple,banana]
[apple]
[banana]
[orange]
```

| Method | What it does |
|---|---|
| `.replace(旧, 新)` | Replaces every match, returns a new string |
| `.trim()` | Strips leading and trailing whitespace |
| `.split(分隔符)` | Cuts into pieces, pair with `for` to process one by one |

> ✨ **`.trim()` + `.split()`** is the golden pair for handling user input: wash off the surrounding whitespace first, then cut by commas/spaces.

---

## 9.8 Walking through text, character by character

To process one letter at a time, use `.chars()`:

```rust
for letter in "abc".chars() {
    println!("one letter: {}", letter);
}
```

Output:

```
one letter: a
one letter: b
one letter: c
```

Every character is treated equally — each round yields one complete character, **never splitting a multi-byte letter in half**. That's why it's safer than subscript slicing.

---

## 9.9 A complete example: the diary helper

```rust
fn main() {
    // —— Join text with format! ——
    let name = "Xiaoming";
    let greeting = format!("Hello, {}!", name);
    println!("{}", greeting);

    // —— A mutable string can grow ——
    let mut diary = String::from("Today I learned about strings");
    diary.push(',');
    diary.push_str(" so much fun!");
    println!("diary: {}", diary);

    // —— Queries ——
    println!("byte length: {}", diary.len());
    let mut char_count = 0;
    for _ in diary.chars() {
        char_count += 1;
    }
    println!("character count: {}", char_count);
    println!("contains 'strings': {}", diary.contains("strings"));

    // —— trim + split ——
    let dirty_data = "  apple,banana,orange  ";
    let clean_data = dirty_data.trim();
    for item in clean_data.split(",") {
        println!("[{}]", item);
    }

    // —— chars ——
    for letter in "abc".chars() {
        println!("one letter: {}", letter);
    }
}
```

## 9.10 Line by line

- **Lines 3–5**: `format!` fills `name` into the template and returns a **new string** stored in `greeting`.
- **Lines 8–10**: `String::from` builds a mutable string; `push` adds a single-quoted character, `push_str` adds double-quoted text.
- **Line 14**: `.len()` counts **bytes**.
- **Lines 15–18**: the way to count **characters** — iterate with `.chars()`, adding 1 to `char_count` per character. Compare with line 14 and you see bytes vs characters clearly.
- **Lines 22–26**: first `.trim()` washes off the surrounding spaces, then `.split(",")` cuts by commas, and `for` prints each piece.
- **Lines 29–31**: `.chars()` correctly yields every letter one at a time.

## 9.11 What you should see

```
Hello, Xiaoming!
diary: Today I learned about strings, so much fun!
byte length: 45
character count: 45
contains 'strings': true
[apple]
[banana]
[orange]
one letter: a
one letter: b
one letter: c
```

(For pure-English text, bytes and characters match — remember section 9.6, where multilingual text made them differ.)

---

## 9.12 Common mistakes and how to fix them

### Mistake one: literals can't be modified

Calling `push` and other modifying methods on `"..."` errors out. **Fix**: `String::from("...")` a mutable copy, then edit.

### Mistake two: E0308 type mismatch — expected `String`, found `&str`

```rust
// 预期错误: E0308
fn receive(phrase: String) { }
receive("Hello");   // ❌ a literal is a string slice
```

**Fix**: `receive(String::from("Hello"))`, or change the parameter to `&str` (recommended for read-only cases — simpler).

### Mistake three: slicing panic (byte position not on a character boundary)

A slice position landing inside a multi-byte character panics at runtime. **Fix**: use boundaries you've verified (pure-ASCII stretches), or switch to `.chars()`.

### Mistake four: single and double quotes mixed

`push("!")` errors (a character expected, a text given); `push_str('!')` likewise. **Fix**: single quotes for characters, double quotes for text.

### Mistake five: assuming `.len()` counts letters

For multilingual text, `.len()` counts bytes. Count characters with `.chars()`.

---

## 9.13 Chapter glossary

| Term | One-line meaning |
|---|---|
| String | The growable, modifiable text type |
| &str (string slice) | Borrowed, immutable text — string literals are these |
| Character | One letter as a human sees it, in single quotes |
| Byte | The smallest storage unit for text; letters take 1, "é" takes 2, "你" takes 3, "🦀" takes 4 |
| format! | The macro that fills values into a template, building a new string |
| push | Append one character to the end |
| push_str | Append a run of text to the end |
| replace | Replace every match with new content |
| trim | Strip leading and trailing whitespace |
| split | Cut into pieces by a separator |
| chars | Iterate one character at a time |
| Boundary | The start/end position of a complete character — slices must land on one |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 9.14 Exercises

> 💪 Try first, then peek.

### Exercise one: the roll caller

Use `format!` to build `"Student #N: name"` and print three students (N starting from 1).

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let a = "Xiaohua";
    let b = "Xiaogang";
    let c = "Xiaoli";
    println!("{}", format!("Student #{}: {}", 1, a));
    println!("{}", format!("Student #{}: {}", 2, b));
    println!("{}", format!("Student #{}: {}", 3, c));
}
```

</details>

### Exercise two: counting

Write a program that reports the byte length and the character count of `"café ☕"`.

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let text = "café ☕";
    println!("bytes: {}", text.len());
    let mut count = 0;
    for _ in text.chars() {
        count += 1;
    }
    println!("characters: {}", count);
}
// 预期输出:
// bytes: 9
// characters: 6
```
Output: `bytes: 9`, `characters: 6`

</details>

### Exercise three: data washing

Given `"   chinese,math,english   "`, trim it, split by commas, and print each subject wrapped in 《》.

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let dirty_data = "   chinese,math,english   ";
    for item in dirty_data.trim().split(",") {
        println!("《{}》", item);
    }
}
// 预期输出:
// 《chinese》
// 《math》
// 《english》
```

Output: three lines. (`dirty_data.trim().split(",")` demonstrates **method chaining**: the previous method's result feeds straight into the next.)

</details>

### Exercise four: the word filter

Use `.replace()` to swap `"rubbish"` for `"fantastic"` in `"this game is rubbish"`, printing before and after.

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let original = String::from("this game is rubbish");
    let cleaned = original.replace("rubbish", "fantastic");
    println!("before: {}", original);
    println!("after: {}", cleaned);
}
```

</details>

---

## 9.15 FAQ

**Q: Why does Rust insist on two text types — wouldn't one do?**

Division of labor saves resources. The stone (literals) is carved into the program at compile time — no warehouse rent; the notebook (String) is flexible but costs upkeep. Read-only uses take slices at zero cost; you build a String only when processing is needed.

**Q: How does `&str` relate to Chapter 8's "references"?**

`&str` is one kind of **reference** — a read-only borrow of a run of text. `&String` is "a reference to a String"; `&str` is "a reference to the text itself". They cooperate: a `&String` passed where an `&str` is expected works too (automatic coercion).

**Q: How do I turn a number into text?**

`format!("{}", number)` is the universal answer. Chapter 15 (collections) and Chapter 16 (error handling) will also introduce `.parse()` (text → number) in more depth.

**Q: Slicing is that dangerous — should I still learn it?**

Yes. For pure-ASCII text and protocol data, slicing is fast and everywhere. One rule only: **boundaries must land on character seams**. For multilingual text, when in doubt, use `.chars()`.

**Q: How many bytes does a char like `'你'` occupy?**

The char type itself is fixed at 4 bytes (room for any character in the world); but when stored as UTF-8 text, "你" takes 3 bytes. Beginners needn't fuss over this detail.

**Q: Can the pieces from `.split()` be stored for later?**

They are all **borrows** of the original string (string slices) — usable only while the original lives. For long-term storage, convert them into independent Strings (a trick from upcoming chapters). For now, learn "process while splitting".

---

## What's next

So far, data has been "loose": a name here, an age there, a score somewhere else… In the real world they all belong to **one record card**.

**Chapter 10, "Structs"**, teaches you to pack related data together:

- Define your own type: `struct Student { ... }`;
- Build instances, read fields, modify fields;
- Fit it with methods using an `impl` block (Chapter 6's setup finally pays off);
- `#[derive(Debug)]` makes a struct directly printable.

See you in the next chapter!
