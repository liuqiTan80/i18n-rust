# Chapter 4: Compound Types

In the last two chapters, every box held exactly one thing. This chapter we learn to **pack** — putting several things into one package.

---

## 4.0 Learning goals

By the end of this chapter, you will be able to:

1. Pack several *different* things together with a **tuple**, and unpack them again.
2. Store a row of same-kind data in an **array**, and fetch any item by number.
3. **Borrow** just a slice of an array without copying the whole row.
4. Explain why "counting starts at 0" — and never trip over it again.

---

## 4.1 Why packing exists

### A story from life: the transfer-student folder

Imagine transferring to a new school. The school doesn't hand you one box — it hands you a **folder**: name, age, grades… many different sheets, all in one bag, carried everywhere.

Programming is the same. One student's information has several parts:

- name (text)
- age (a number)
- lives in the dorm (true or false)

A separate box for each gets messy fast. The better way is to **pack**.

Three packing methods in this chapter:

| Method | Metaphor | Best for |
|---|---|---|
| Tuple | A gift box | A few **different** things bundled together |
| Array | A row of lockers | A row of **same-kind** things, fixed count |
| Slice | A window | Not moving the whole locker row — just viewing a stretch of it |

---

## 4.2 Tuples: one gift box, several different things

### Creating a tuple

Wrap several things in parentheses, separated by commas — that's a tuple:

```rust
let student = ("Xiaohua", 12, true);
```

Piece by piece:

- `("Xiaohua", 12, true)`: a gift box holding **three things** — a piece of text, a number, a boolean. Different kinds mixed together is perfectly fine; that's what tuples are for.
- The three things are the tuple's **elements** (its "members"). This tuple has 3 elements.

> 📖 **Tuple**: a compound type that packs several values into one group (pronounced like "toople"). Elements may be of different kinds.

### Getting things out: counting starts at 0

To take one item out, write `.number` after the tuple's name:

```rust
fn main() {
    let student = ("Xiaohua", 12, true);
    println!("Item one: {}", student.0);
    println!("Item two: {}", student.1);
    println!("Item three: {}", student.2);
}
```

What you should see:

```
Item one: Xiaohua
Item two: 12
Item three: true
```

> ⚠️ **Careful**: **counting starts at 0, not 1!** The first element's number is 0, the second is 1, the third is 2.

This is a rule across the entire programming world. Why? We'll explain with arrays in the next section — for now, remember: **when counting positions, the first number is "0"**.

> 📖 **Index**: the formal name for this "number" — the address you use to look something up. The book uses both words interchangeably.

### Unpacking the whole tuple at once

Want all three? Unpack in one go:

```rust
fn main() {
    let student = ("Xiaohua", 12, true);
    let (name, age, lives_in_dorm) = student;
    println!("{} is {} years old", name, age);
    println!("In the dorm? {}", lives_in_dorm);
}
```

Line by line:

- Line 3: `let (name, age, lives_in_dorm) = student;` — the left side of the `=` also has a bracket shape, meaning "split the package into three". The computer matches by position: item 1 goes to `name`, item 2 to `age`, item 3 to `lives_in_dorm`.
- After unpacking, the three names are independent boxes of their own.

> 📖 **Destructuring**: writing that splits packed data apart by shape and takes each piece out. Like untying a knot and laying the parts out.

> 💡 **Metaphor**: destructuring is like opening a delivery — three things in the parcel, three baskets ready, everything sorted in one pass.

What you should see:

```
Xiaohua is 12 years old
In the dorm? true
```

### Printing the whole tuple: `{:?}`

To print the entire tuple at once, the placeholder is `{:?}` (a colon and a question mark inside the braces):

```rust
fn main() {
    let student = ("Xiaohua", 12, true);
    println!("the whole tuple: {:?}", student);
}
```

What you should see:

```
the whole tuple: ("Xiaohua", 12, true)
```

> 📖 **Debug printing**: `{:?}` is the "for debugging" print — it shows the thing's internal structure exactly (quotes, brackets and all). The plain `{}` prints the clean, human-facing version. The full story of debug printing is in Chapter 10.

> ⚠️ **Careful**: printing a tuple with plain `{}` is an error. Tuples must use `{:?}`.

---

## 4.3 Arrays: a row of numbered lockers

### Creating an array

Picture a corridor of school lockers — each locker the same size, all for the same kind of thing (say, scores). That's an **array**.

Create arrays with **square brackets**, elements separated by commas:

```rust
let scores = [88, 95, 76, 90, 82];
```

This array holds a school week's scores — 5 elements in total.

> 📖 **Array**: a row of **fixed-length**, **same-kind** data. Square brackets `[ ]` are the array's signature.

> ⚠️ **Careful**: don't mix up square brackets `[ ]` and parentheses `( )` — square is an array (a row of same-kind things), parentheses are a tuple (several different things).

### A shortcut for repeated values

If every locker holds the same thing, there's a shorthand:

```rust
let five_zeros = [0; 5];
```

> 📖 Read this as "0, repeated 5 times": before the semicolon is the content, after it the count. Same as `[0, 0, 0, 0, 0]`.

```rust
println!("{:?}", five_zeros);   // [0, 0, 0, 0, 0]
```

### Fetching lockers by index: counting starts at 0

```rust
fn main() {
    let scores = [88, 95, 76, 90, 82];
    println!("Day one: {}", scores[0]);
    println!("Day three: {}", scores[2]);
    println!("The last day: {}", scores[4]);
}
```

What you should see:

```
Day one: 88
Day three: 76
The last day: 82
```

A picture makes it clearest:

```
locker:  [ 88 ]  [ 95 ]  [ 76 ]  [ 90 ]  [ 82 ]
index:      0       1       2       3       4
```

5 lockers, but the indices run 0 to 4. For the Nth one, the index is N-1.

### Why start at 0?

Here's a reason you can sell to yourself:

an index means "**how many steps from the start**".

- Locker 1: **0** steps from the start → index 0.
- Locker 2: **1** step → index 1.
- Locker 5: **4** steps → index 4.

> 💡 **Metaphor**: like measuring height — the ruler's "0 mark" sits on the floor. The first centimeter's mark is 0, not 1. An index is "distance from the starting point".

### Two everyday array skills

**Skill one: `.len()` — count the lockers**

```rust
println!("{} days in total", scores.len());   // 5 days in total
```

> 📖 **`.len()`**: a method that returns the element count. The dot is followed by a method; the parentheses stay empty. What's a "method"? Chapter 6 explains — for now, treat `.len()` as a fixed incantation.

**Skill two: `.contains()` — is this value in there?**

```rust
println!("Is there a 95? {}", scores.contains(&95));   // Is there a 95? true
```

> 📖 **`.contains()`**: a method that checks whether the array contains a value, returning a boolean. Copy the `&95` for now — `&` is the "borrow" mark, explained fully in Chapter 8. For now: "contains needs a &".

### Out of bounds: reaching for a locker that isn't there ends badly

The array has 5 lockers (indices 0–4). Insist on number 6:

```rust
// 预期行为: 运行失败
fn main() {
    let scores = [88, 95, 76, 90, 82];
    println!("{}", scores[5]);   // ❌ out of bounds!
}
```

Rust immediately **panics** — the program stops on the spot.

> 📖 **Panic**: when a program hits an unrecoverable error, it shouts "something's wrong!" and stops immediately, never running while broken. Like a fire alarm: everyone stops what they're doing and evacuates. Chapter 16 covers this.

The panic message looks like this:

```
thread 'main' panicked at ...:
index out of bounds: the len is 5 but the index is 5
```

Translated: "index out of bounds: the length is 5, but you asked for number 5." (The biggest legal number is 4!)

> ✨ **Tip**: if you write a **constant** out-of-bounds number (like `scores[5]` directly), the compiler stops you at compile time with "this operation will panic at runtime" — the program never even runs. Only when the number comes from outside, unknowable at compile time, does the panic truly happen at runtime. Either way, Rust never lets you quietly read wrong data.
>
> ⚠️ **Careful**: `rzc check` only translates and checks — it can't catch this. To see it, actually run the program.

> 📖 **Index out of bounds**: accessing a number beyond the range. The original message is "index out of bounds" — literally "beyond the boundary".

---

## 4.4 Slices: a window onto the array

### Don't want to move the whole locker row — just look at a few?

Say you only want the first three days' scores. Copying them into a new array is wasteful. A **slice** is smarter: no copying, just "pointing" at a stretch of the original.

> 📖 **Slice**: a reference to **a run of consecutive elements** in an array — literally a "slice": cut a piece of sausage, but the sausage itself stays.

Write `[start..end]` after the array, with a `&` in front:

```rust
fn main() {
    let scores = [88, 95, 76, 90, 82];
    let first_three = &scores[0..3];
    println!("First three days: {:?}", first_three);
    println!("Slice length: {}", first_three.len());
}
```

What you should see:

```
First three days: [88, 95, 76]
Slice length: 3
```

`&scores[0..3]` piece by piece:

| Part | Meaning |
|---|---|
| `&` | Borrow (don't take the original — Chapter 8) |
| `scores` | The array being sliced |
| `[0..3]` | From index 0 up to **before** index 3 — i.e. 0, 1 and 2 |

> ⚠️ **Careful**: `0..3` is "**include the head, exclude the tail**" — includes 0, excludes 3. Count them: exactly 3 elements. This rule holds across the whole book.

> 📖 **Range**: the `start..end` form is called a range — "from here to there".

### Shorter forms

```rust
let from_start_to_third = &scores[..3];    // omitted start = from 0
let from_third_to_end = &scores[2..];      // omitted end = to the last one
let everything = &scores[..];              // both omitted = the whole stretch
```

```rust
println!("{:?} {:?} {:?}", from_start_to_third, from_third_to_end, everything);
// [88, 95, 76] [76, 90, 82] [88, 95, 76, 90, 82]
```

### Text can be sliced too

Strings can be sliced as well (details in Chapter 9 — just a first meeting here):

```rust
let greeting = &"你好世界"[0..6];
println!("{}", greeting);   // 你好
```

> ⚠️ **Careful**: it's 0..6, not 0..2 — each Chinese character occupies 3 "bytes" (little storage cells) in the computer, so "你好" is 6 cells. What's a byte? Chapter 9 explains. For now, copy the 0..6.

---

## 4.5 A complete example: the week's score manager

### The complete code

```rust
fn main() {
    // The week's scores (Monday to Friday)
    let week_scores = [88, 95, 76, 90, 82];

    // Cut the week into two halves
    let first_two = &week_scores[0..2];
    let last_three = &week_scores[2..5];

    println!("The week's scores: {:?}", week_scores);
    println!("{} days in total", week_scores.len());
    println!("First two days: {:?}", first_two);
    println!("Last three days: {:?}", last_three);

    // Add the total by hand
    let total = week_scores[0] + week_scores[1] + week_scores[2] + week_scores[3] + week_scores[4];
    let total_as_float = total as f64;
    let average = total_as_float / 5.0;
    println!("total: {}, average: {}", total, average);

    // Student information packed in a tuple
    let student = ("Xiaohua", 12, true);
    let (name, age, lives_in_dorm) = student;
    println!("{} is {} years old, dorm: {}", name, age, lives_in_dorm);
    println!("The tuple's second item: {}", student.1);
}
```

### Line by line

- Line 3: create the array of five days' scores. Square brackets = array.
- Line 6: slice out indices 0 and 1 (head in, tail out).
- Line 7: slice indices 2, 3, 4 — exactly the remaining three days.
- Line 9: `{:?}` debug-prints the whole array, brackets included.
- Line 10: `.len()` returns 5.
- Lines 11–12: print the two slices.
- Line 15: add all five elements by hand = 431. (After Chapter 5's loops, this becomes one line — no more writing five.)
- Line 16: convert to a float (two lines, dodging the `as` precedence trap from Chapter 3).
- Line 17: 431.0 ÷ 5.0 = 86.2.
- Line 20: create a three-item tuple: text, number, boolean, mixed.
- Line 21: destructure — three names, one item each, by position.
- Line 22: print using the three unpacked boxes.
- Line 23: take the tuple's second item directly with `.1` (counting from 0, `.1` is item two: 12).

### What you should see

```
The week's scores: [88, 95, 76, 90, 82]
5 days in total
First two days: [88, 95]
Last three days: [76, 90, 82]
total: 431 average: 86.2
Xiaohua is 12 years old, dorm: true
The tuple's second item: 12
```

---

## 4.6 Common mistakes and how to fix them

### Mistake 1: index out of bounds

5 elements, biggest index 4. Writing `scores[5]` is out of bounds.

**Fix**: the biggest legal index is always `.len() - 1`. Unsure? Print `.len()` first.

### Mistake 2: counting from 1

Wanting the first but writing `scores[1]` gets you the second.

**Fix**: chant the mantra — "**the first one is 0**".

### Mistake 3: square brackets on a tuple

```rust
// 预期错误: E0608
let student = ("Xiaohua", 12, true);
println!("{}", student[0]);   // ❌ tuples don't use square brackets
```

**Fix**: tuples use a **dot**: `student.0`. Square brackets are for arrays. Mantra: **tuple dot, array square**.

### Mistake 4: `.contains()` without the `&`

```rust
scores.contains(95)    // ❌ error
scores.contains(&95)   // ✅ correct
```

**Fix**: copy the `&` — Chapter 8 explains why.

### Mistake 5: mixed kinds in an array

```rust
// 预期错误: E0308
let mixed = [1, "two", 3];   // ❌ arrays must be one kind
```

**Fix**: for mixing, use a tuple `(1, "two", 3)`; arrays hold one kind only.

### A bonus experiment: watch a real panic with your own eyes (optional)

This example uses "environment variables" and other later concepts — every line is commented, so you can follow it and see a real panic message:

```rust
use std::env;

fn main() {
    let scores = [88, 95, 76];
    // Read a value called INDEX from the computer's environment variables
    // (a message passed in from outside the program)
    let text = env::var("INDEX").expect("please set the INDEX variable");
    // Trim the whitespace off both ends
    let clean = text.trim();
    // Turn the text into a number. usize means "an integer for counting"
    let index: usize = clean.parse().expect("a number is needed");
    println!("the score is: {}", scores[index]);
}
```

On Linux / macOS (set INDEX to 5 first, on purpose out of bounds):

```bash
INDEX=5 cargo run
```

You'll see a real panic message:

```
thread 'main' panicked at ...:
index out of bounds: the len is 3 but the index is 5
```

Now a legal number:

```bash
INDEX=1 cargo run
```

Output: `the score is: 95`.

> ✨ **Tip**: in Windows PowerShell, set the variable as `$env:INDEX="5"` before running. The `env::var`, `.trim()`, `.parse()` and `.expect()` used here all get proper introductions in later chapters — this experiment is just an early look at what a panic looks like.

---

## 4.7 Chapter glossary

| Term | Meaning |
|---|---|
| contains | The `.contains()` method — does the array contain a value |
| len | The `.len()` method — returns the element count |
| Debug printing | Showing data's internal structure with `{:?}` |
| Range | The `start..end` form — head in, tail out |
| Panic | The program stopping immediately on an unrecoverable error |
| Destructuring | Unpacking packed data by shape |
| Slice | A reference to a run of consecutive array elements |
| Index | An element's number in a collection, starting at 0 |
| Index out of bounds | Accessing a number beyond the range |
| Tuple | Several different-kind values packed as a group |
| Element | Each member of a collection |
| Array | A fixed-length row of same-kind data |
| Byte | A computer storage cell — one Chinese character takes 3 |
| Compound type | The family of types that hold multiple values |

---

## 4.8 Exercises

> 💪 Try first, then peek.

### Exercise 1: meet the tuple

Create a tuple `city` with three elements: name "Beijing", population 2189, capital `true`. Print each with `.0`, `.1`, `.2`.

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let city = ("Beijing", 2189, true);
    println!("{}", city.0);
    println!("{}", city.1);
    println!("{}", city.2);
}
```

</details>

### Exercise 2: unpack

Destructure Exercise 1's tuple into three variables, then print one sentence: `Beijing has a population of 21.89 million; capital: true`.

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let city = ("Beijing", 2189, true);
    let (name, population, is_capital) = city;
    println!("{} has {}0,000 people; capital: {}", name, population, is_capital);
}
```

</details>

### Exercise 3: fetch from an array

Create an array `temperatures` with seven days: `[22, 25, 27, 24, 23, 26, 28]`. Print day 1, day 4 and the last day, plus the array's length.

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let temperatures = [22, 25, 27, 24, 23, 26, 28];
    println!("Day one: {}", temperatures[0]);
    println!("Day four: {}", temperatures[3]);
    println!("The last day: {}", temperatures[6]);
    println!("{} days in total", temperatures.len());
}
```

Note the last day's index is 6, not 7 — 7 elements means indices 0 through 6.

</details>

### Exercise 4: slices

Using Exercise 3's array, slice out the "weekend" (the last two days) and the "weekdays" (the first five), and print both.

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let temperatures = [22, 25, 27, 24, 23, 26, 28];
    let weekdays = &temperatures[0..5];
    let weekend = &temperatures[5..7];
    println!("weekdays: {:?}", weekdays);
    println!("weekend: {:?}", weekend);
}
```

The weekend can also be written `&temperatures[5..]` (omitted end = to the last).

</details>

### Exercise 5 (challenge): spot the bug

What happens when this code runs? How do you fix it?

```rust
fn main() {
    let three_numbers = [1, 2, 3];
    println!("{}", three_numbers[3]);
}
```

<details>
<summary>Reference answer</summary>

The array has 3 elements; the biggest index is 2. `three_numbers[3]` is out of bounds. Because it's a fixed number, the compiler intercepts it at build time with "this operation will panic at runtime". Change it to `three_numbers[2]` to get the 3.

</details>

---

## 4.9 FAQ

**Q1: Tuple or array — when do I use which?**

Look at what's inside. Several **different kinds** of information bundled together (name + age + a flag) → tuple. A row of **same-kind** data (five days of scores) → array.

**Q2: Why is counting from 0 so unnatural?**

Because an index literally means "how many steps from the start". The first element takes zero steps, so it's 0. Every mainstream language (Rust, C, Java, Python…) does this — and once you're used to it, the head-in/tail-out range notation feels natural too.

**Q3: Can an array's length change after it's set?**

No. Array length is fixed. For a "box that grows", that's the **vector** — the star of Chapter 15.

**Q4: What does `:?` in `{:?}` mean?**

The `?` marks "debug format". `{}` is the pretty, human-facing version; `{:?}` is the full structure programmers inspect. Chapter 10 covers the mechanism behind them.

**Q5: Is the `&` in a slice really required?**

Yes. A slice "borrows" from the original array, and `&` is the borrow mark. Omit it and it errors. Chapter 8, in full.

**Q6: Does `[0..3]` include 3?**

No. Head in, tail out: it includes 0, 1 and 2. Want 3 included? Write `[0..4]`. Mantra: **the endpoint is always one short**.

---

## Chapter summary

1. **Tuples**: parentheses, kinds may differ, fetched with `.0`/`.1`, unpacked by destructuring.
2. **Arrays**: square brackets, one kind, fixed length, indices from 0.
3. **Slices**: `&array[start..end]`, head in tail out, borrow without moving.
4. `.len()` counts, `{:?}` debug-prints, out-of-bounds always ends badly — and Rust always catches it.

> 📖 If anything in this chapter confused you, check the chapter glossary above or the master glossary at the front of the book.

## What's next

In **Chapter 5, "Control Flow"**, the program learns to "make choices" and "do repetitive work":

- **`if / else`**: umbrella if it rains, hat if it doesn't.
- **`match`**: like a vending machine — press a button, get that snack.
- **`loop`, `while`, `for`**: hand the repetition to the computer.
- And you'll discover that a code block `{ }` is itself a "value" you can use.
