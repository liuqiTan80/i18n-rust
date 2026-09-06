# Chapter 3: Variables and Types

Last chapter you taught the program to "speak" (print text). This chapter, we teach it to "remember" and "do math".

---

## 3.0 Learning goals

By the end of this chapter, you will be able to:

1. Create variables with `let` and explain what a variable is.
2. Explain why Rust variables can't be changed by default, and how `mut` unlocks them.
3. Name the four basic types — integers, floating-point numbers, booleans, characters — with an example of each.
4. Do addition, subtraction, multiplication and division in code.

---

## 3.1 What is a variable

### A story from life: labeled boxes

Imagine tidying your room. You take a box, stick a label on it that says "toys", and put the toys inside.

Later, to get the toys, you just say "hand me the 'toys' box" — and there they are.

You can also peel the label off and stick on a new one, or swap what's inside for something else.

**A variable is exactly that box**:

- the box → a small piece of storage in the computer
- the label → the variable's name (like `name` or `age`)
- what's inside → the variable's value (like `"Xiaoming"` or `12`)

> 💡 **Metaphor**: a running program uses lots of data. Data can't be strewn about — it goes into labeled boxes, and when you need it, you call the label.

### Creating variables with `let`

In Rust, you create a variable with the keyword `let`.

> 📖 **`let`**: a keyword whose literal meaning is "let this name stand for this value". `let age = 12;` reads as "let the name 'age' stand for 12".

```rust
fn main() {
    let age = 12;
    println!("I am {} years old", age);
}
```

Line by line:

- Line 1: define the main function.
- Line 2: `let age = 12;` — take a box, label it "age", put the number 12 inside. The semicolon ends the instruction.
- Line 3: fill the `{}` placeholder with what's in the "age" box (12) — the screen shows "I am 12 years old".
- Line 4: the main function ends.

What you should see:

```
I am 12 years old
```

> ⚠️ **Careful**: `let` means "create a NEW box". Later, when we meet `mut`, you'll see that "swapping what's inside an old box" is a different thing.

### The `=` sign is not "equals"

In math class, `=` means "both sides are equal". In programming, `=` is an **action**:

> 📖 **Assignment**: put the value on the right into the box on the left. Read it as "put … into …".

```rust
let score = 100;
```

Reads as: "put 100 into the box called 'score'."

It does not ask "is score equal to 100?" — it's a **command**: "put it in!"

> ✨ **Tip**: asking "are they equal?" uses a different symbol, `==` (two equal signs) — you'll meet it in Chapter 5 on control flow.

### One program can have many boxes

```rust
fn main() {
    let name = "Xiaohua";
    let age = 12;
    let class = "Class 6-2";
    println!("Hello everyone, I'm {}, {} years old, from {}", name, age, class);
}
```

Line by line:

- Lines 2–4: create three boxes — holding text, a number, and text.
- Line 5: the three `{}` are filled by `name`, `age` and `class` in order.

What you should see:

```
Hello everyone, I'm Xiaohua, 12 years old, from Class 6-2
```

### The rules for variable names

Labeling boxes (naming things) has a few rules:

| Rule | Right | Wrong | Why |
|---|---|---|---|
| Don't use keywords as names | `let score = 1;` | `let let = 1;` | `let` is a reserved keyword |
| Don't start with a digit | `let class_2 = 2;` | `let 2class = 2;` | A leading digit would be read as a number |
| Letters, digits and underscores are fine | `let student_no_01 = 1;` | | All legal |

> 📖 **Underscore**: the `_` character on your keyboard (`Shift` + the minus key). It looks like a short dash and often joins words, as in `my_score`.

> ⚠️ **Careful**: don't name variables after standard-library words like `String` or `Option` — those types have other jobs and cause confusion. Specific names are safest: `total_score`, `greeting`, `headcount`.

---

## 3.2 Immutable and mutable

### Rust's special rule: boxes are locked by default

In Rust, once a variable is created it **cannot be changed by default**.

```rust
// 预期错误: E0384
fn main() {
    let score = 90;
    score = 100;   // ❌ error!
}
```

Running `cargo check` reports:

```
error[E0384]: cannot assign twice to immutable variable `score`
```

> 📖 **Immutable**: once set, it can't be changed. Rust does this on purpose — Chapter 8 explains why. Short version: data that can't change is safer and causes fewer surprises.

### Unlocking with `mut`

If you truly need to swap what's in the box, add the keyword `mut` when creating it:

> 📖 **`mut`** (mutable): a keyword meaning "changeable". It goes after `let` and declares that what's in this box may be swapped later.

```rust
fn main() {
    let mut score = 90;
    println!("First exam: {}", score);
    score = 100;               // ✅ with mut, swapping is allowed
    println!("Second exam: {}", score);
}
```

Line by line:

- Line 2: create the "score" box holding 90, and declare it changeable.
- Line 3: print 90.
- Line 4: swap the 90 for 100. Note: **no `let` when swapping** — `let` only creates new boxes.
- Line 5: print 100.

What you should see:

```
First exam: 90
Second exam: 100
```

> 💡 **Metaphor**: `let` buys a new box and locks it; `mut` fits the box with a latch so you can open it later; using `let` again buys yet another new box.

> ✨ **Tip**: when should you use `mut`? For data that changes while the program runs (counters, running scores). For data that never changes (a name, pi), don't.

---

## 3.3 Constants: the box that can never change

Some values must never change, ever — like "a year has 12 months". Those are **constants**:

> 📖 **`const`**: a keyword meaning "a value that stays constant forever". Once defined, a constant can never change — and its type must be written out.

```rust
fn main() {
    const MAX_CAPACITY: i32 = 45;
    println!("The class holds at most {} students", MAX_CAPACITY);
}
```

Line by line:

- Line 2: `const MAX_CAPACITY: i32 = 45;` — define a constant that equals 45 forever. The `: i32` between `MAX_CAPACITY` and `=` is a **type annotation** — next section. (Constants are conventionally named in ALL_CAPS.)
- Line 3: print it.

> ⚠️ **Careful**: constants differ from immutable variables in three ways:
> 1. Constants can't take `mut` — they never change.
> 2. Constants must have a type annotation (`: i32`); variables don't need one.
> 3. Constants conventionally live outside functions, where the whole program can use them — you'll see one right in the next section.

---

## 3.4 Types: the "kinds" of data

### Why kinds matter

What a box can hold depends on the kind marked on it. A box for numbers can't hold text — that's a **type**.

> 📖 **Type**: the "kind" of data. Like supermarket shelves labeled "food", "stationery", "toys", data comes in kinds — numbers, text, true/false. The type decides what operations the data can take: numbers can be added, text can't "plus one".

> 📖 **Type annotation**: writing `: TypeName` after a name to tell the compiler exactly what kind of data the box holds — like labeling the box "apples" up front.

Rust's types have English names already. Let's meet them one by one.

### Type one: integers — numbers without a decimal point

> 📖 **Integer**: a number with no decimal part, like 3, -8, 0. Rust's default integer type is `i32`.

```rust
let count = 45;        // an i32 by default
let temperature = -8;  // negatives work too
```

Rust actually has a whole family of integers, differing in "how big" and "negative allowed or not":

| Name | Range | Notes |
|---|---|---|
| `i8` | -128 to 127 | the smallest |
| `i16` | about ±33,000 | |
| `i32` | about ±2.1 billion | **the default, most used** |
| `i64` | astronomically large | holds huge numbers |
| `u32` | 0 to about 4.2 billion | no negatives allowed |

> 📖 **Unsigned**: the "sign" is the plus/minus. Unsigned means "no sign" — only 0 and positives. The upside: the same space holds bigger positive numbers.

> ✨ **Tip**: beginners only need to remember `i32`. The "i" means integer; the number is how many bits (little binary cells) store it. More cells, bigger numbers.

### Type two: floating-point — numbers with a decimal point

> 📖 **Floating-point**: a number with a decimal point, like 3.14 or -0.5. The default type is `f64` ("f" is for float — the decimal point "floats").

```rust
let pi = 3.14;
let body_temperature = 36.5;
```

> ⚠️ **Careful**: integer divided by integer is still an integer — the decimal part gets **chopped off**:

```rust
let quotient = 7 / 2;
println!("{}", quotient);   // shows 3, not 3.5!
```

For a decimal result, at least one side must be a floating-point number:

```rust
let quotient = 7.0 / 2.0;
println!("{}", quotient);   // shows 3.5
```

> 📖 **Remainder**: the `%` symbol computes "the remainder of a division". 7 % 2 = 1 (7 ÷ 2 is 3 remainder 1). Great for odd/even checks: remainder 0 means even.

### Type three: booleans — only true and false

> 📖 **Boolean**: a type with exactly two possible values — `true` or `false` — named after the mathematician Boole. It answers yes/no questions.

```rust
let homework_done = true;
let raining_today = false;
```

> 📖 **`true`** and **`false`**: keywords — the only two values a boolean has. Not one character can be off.

Booleans shine in Chapter 5's `if` decisions: "**if** raining_today **is** `true`, take an umbrella."

### Type four: characters — a single letter

> 📖 **Character**: a single text symbol, wrapped in **single quotes** `'` (not double quotes).

```rust
let grade = 'A';
let symbol = '★';
```

> ⚠️ **Careful**: character vs string — `'A'` is one character (one letter); `"excellent"` is a string (a run of letters). Single quotes hold one; double quotes hold a run. Strings get their own chapter — Chapter 9.

### The whole type family, at a glance

| Name | Example | Remember it by |
|---|---|---|
| `i32` | `45` | no decimal point |
| `i64` | `9999999999` | big numbers |
| `f64` | `3.14` | has a decimal point |
| `bool` | `true` / `false` | yes or no |
| `char` | `'A'` | single quotes, one letter |
| `String` | `"hello"` | double quotes, a run of letters (Chapter 9) |

### The compiler guesses types for you

When you write `let age = 12;` you never wrote `: i32` — how does the compiler know it's an integer?

Because the compiler **infers** it: seeing 12 with no decimal point, it guesses `i32`; seeing 3.14, it guesses `f64`; seeing quotes, it guesses a string.

> 📖 **Type inference**: the compiler's ability to judge a type from the shape of the value. Less typing for you, still reliable. But you can always write the annotation — it makes things clearer.

---

## 3.5 Making the program do math: operators

> 📖 **Operator**: a symbol that performs an operation. `+`, `-`, `*`, `/` are all operators.

### The five basic arithmetic operators

```rust
fn main() {
    let a = 10;
    let b = 3;
    println!("sum: {}", a + b);         // 13
    println!("difference: {}", a - b);  // 7
    println!("product: {}", a * b);     // 30
    println!("quotient: {}", a / b);    // 3 (integer division, decimals chopped)
    println!("remainder: {}", a % b);   // 1 (the remainder)
}
```

| Symbol | Name | Example | Result |
|---|---|---|---|
| `+` | add | `10 + 3` | 13 |
| `-` | subtract | `10 - 3` | 7 |
| `*` | multiply | `10 * 3` | 30 |
| `/` | divide | `10 / 3` | 3 |
| `%` | remainder | `10 % 3` | 1 |

> ⚠️ **Careful**: both numbers in an operation must be **the same type**. `i32 + f64` errors out. To mix, convert with `as` (coming right up).

### Compound assignment: shortcuts for changing yourself

```rust
let mut score = 90;
score += 5;    // same as score = score + 5 — now 95
score -= 10;   // same as score = score - 10 — now 85
score *= 2;    // 85 × 2 = 170
```

> 📖 **Compound assignment**: `+=`, `-=`, `*=`, `/=` mean "operate on myself, then put the result back". Read `score += 5` as "add 5 to score and store it back".

### A little trick: there is no `++`

Rust has **no** `++` operator. To add 1, write:

```rust
counter += 1;
```

### Type conversion: `as`

> 📖 **`as`**: a keyword that temporarily treats a value "as" another type.

```rust
fn main() {
    let count = 7;
    let count_as_float = count as f64;   // first turn 7 into 7.0
    let average = count_as_float / 2.0;  // then divide by 2.0 → 3.5
    println!("average: {}", average);    // 3.5
}
```

Line by line:

- Line 2: `count` is the integer 7.
- Line 3: `count as f64` temporarily treats 7 as 7.0 and stores it in a new box. The original 7 is untouched.
- Line 4: 7.0 divided by 2.0 is 3.5.

> ⚠️ **Careful**: when a float takes part in an operation, the other side must be written as a float too (like `2.0`). `count_as_float / 2` errors out — integers and decimals can't be divided directly.

> ⚠️ **Careful**: don't squeeze `as` into a longer line, like `count as f64 / 2` — the computer may read the order differently than you expect. Two lines is safest.

> 💡 **Metaphor**: `as` is like wearing a mask — the integer puts on a "floating-point mask" to attend the floats' party; take the mask off and it's still the same integer.

---

## 3.6 A complete example: the student info card

### The complete code

```rust
// 学生信息卡程序
const FULL_SCORE: i32 = 100;

fn main() {
    // 基本信息（不可变，创建后不改）
    let name = "Xiaohua";
    let age = 12;

    // 成绩（数学之后要更新，所以只有数学加"可变"）
    let chinese = 88;
    let mut math = 95;

    // 先记录一下原来的数学成绩
    println!("math originally: {}", math);

    // 数学考了满分，更新一下
    math = FULL_SCORE;

    // 计算总分和平均分
    let total = chinese + math;
    let total_as_float = total as f64;
    let average = total_as_float / 2.0;

    // 打印信息卡
    println!("====== Student Info Card ======");
    println!("name: {}", name);
    println!("age: {}", age);
    println!("chinese: {} points", chinese);
    println!("math: {} points", math);
    println!("total: {} points", total);
    println!("average: {}", average);
    println!("excellent: {}", average >= 90.0);
}
```

### Line by line

- Line 2: define the constant `FULL_SCORE`, annotated `i32`, value 100. It sits at the top level, outside the functions, where the whole program can use it.
- Lines 6–7: two immutable boxes for name and age — this information won't change.
- Lines 10–11: two boxes for scores. `chinese` won't change, no `mut`; `math` will be updated, so `mut`.
- Line 14: print the original math score, 95. This "uses up" the 95 so the next line can safely swap it.
- Line 17: replace `math` with `FULL_SCORE` (100). Note: no `let` — this is swapping, not creating a new box.
- Line 20: `chinese + math` = 88 + 100 = 188, into the new box `total`.
- Line 21: convert 188 into 188.0 and store it in the new box `total_as_float`.
- Line 22: 188.0 ÷ 2.0 = 94.0. Two lines, to dodge the "one long line, misread order" trap.
- Lines 25–32: print the info card line by line. Each `{}` is filled by the matching box.
- Line 32: `average >= 90.0` is a comparison producing a boolean — `true` or `false`. `>=` means "greater than or equal to" — Chapter 5 covers it. When comparing against a float, write the 90 as 90.0.
- Line 33: the main function ends.

### What you should see

```
math originally: 95
====== Student Info Card ======
name: Xiaohua
age: 12
chinese: 88
math: 100
total: 188
average: 94
excellent: true
```

> ⚠️ **Careful**: the last line shows `true` — that's the default way booleans are printed. It doesn't affect the program; after Chapter 11 you'll learn to display it any way you like.

---

## 3.7 Shadowing: a new box with the same name covers the old one

Rust has a curious quirk: you can use `let` again with the same name, and the new box "covers" the old one:

```rust
fn main() {
    let number = 5;
    let number = number + 1;    // new box covers old, holds 6
    let number = number * 2;    // yet another new box, holds 12
    println!("{}", number);     // 12
}
```

> 📖 **Shadowing**: creating a new variable with `let` under an existing name — the new variable shadows the old one, and calling the name now finds the new box. The old box still exists, just hidden "behind the shadow", out of reach.

> 💡 **Metaphor**: it's like stacking boxes — the later same-named box stands on the old one's shoulders, and when you call the name, the topmost box answers.

Shadowing vs `mut`:

| | Shadowing | `mut` |
|---|---|---|
| How it's written | `let` every time | `let` once, then plain assignment |
| The box | Each time creates a NEW box | Always the same box |
| The type | Can switch to a completely different type | Only values of the same type |

---

## 3.8 Common mistakes and how to fix them

### Mistake 1: swapping an immutable variable

```rust
// 预期错误: E0384
fn main() {
    let score = 90;
    score = 100;   // ❌
}
```

The report: `error[E0384]: cannot assign twice to immutable variable `score``.

**Fix**: if it really must change, add `mut` at creation.

### Mistake 2: using a box you never created

```rust
fn main() {
    println!("{}", height);   // ❌ there was never a "let height = ..."
}
```

The report says, roughly, that nothing called `height` can be found.

**Fix**: check for typos; make sure creation comes before use.

### Mistake 3: mixing types in an operation

```rust
// 预期错误: E0277
fn main() {
    let a = 10;          // integer
    let b = 3.5;         // float
    let total = a + b;   // ❌ integers and floats don't add directly
}
```

**Fix**: convert one side with `as`, and **write it in two lines**:

```rust
let a_as_float = a as f64;
let total = a_as_float + b;
```

### Mistake 4: Chinese punctuation sneaking into code

```rust
let name = "Xiaohua"；   // ❌ that's a Chinese semicolon at the end
```

The report usually points at the line, complaining about an unknown symbol.

**Fix**: switch to English input mode and change `；` to `;`. Remember the mantra: **the code skeleton is English; only inside quotes is native**.

---

## 3.9 Chapter glossary

| Term | Meaning |
|---|---|
| Assignment | Put the right-hand value into the left-hand box — the `=` symbol |
| Boolean | The type with only `true`/`false` values |
| Immutable | Can't change after creation — Rust variables' default nature |
| Constant | A value that can never change — the `const` keyword |
| Floating-point | A number with a decimal point; default type `f64` |
| Compound assignment | `+=`, `-=` etc. — "operate, then store back" shorthands |
| Mutable | The `mut` keyword that lets a box swap contents |
| Type | The kind of data |
| Type annotation | Writing `: TypeName` after a name |
| Type inference | The compiler judging the type from the value |
| Remainder | The `%` operation — the rest left over from division |
| Unsigned | Cannot represent negatives, like `u32` |
| Underscore | The `_` symbol, usable in variable names |
| Operator | A symbol that performs an operation, like `+` and `-` |
| Integer | A number with no decimal point; default type `i32` |
| Character | A single text symbol, in single quotes |
| Shadowing | A new same-named variable covering the old one |
| true/false | The two boolean values |
| as | The keyword for temporary type conversion |

---

## 3.10 Exercises

> 💪 Try first, then peek.

### Exercise 1: self-introduction card

Create variables for your name, age and school, and print them on three lines.

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let name = "Xiaohua";
    let age = 12;
    let school = "Sunshine Primary School";
    println!("name: {}", name);
    println!("age: {}", age);
    println!("school: {}", school);
}
```

</details>

### Exercise 2: a calculator

Create two integer variables `a` = 17 and `b` = 5, and print their sum, difference, product, quotient and remainder.

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let a = 17;
    let b = 5;
    println!("sum: {}", a + b);
    println!("difference: {}", a - b);
    println!("product: {}", a * b);
    println!("quotient: {}", a / b);
    println!("remainder: {}", a % b);
}
```

The results are 22, 12, 85, 3 and 2.

</details>

### Exercise 3: a temperature log

Create a mutable variable `temperature` = 38.5, simulate it dropping to 36.8 after medicine, and print both.

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let mut temperature = 38.5;
    println!("before medicine: {}", temperature);
    temperature = 36.8;
    println!("after medicine: {}", temperature);
}
```

</details>

### Exercise 4: spot the mistake

This code has one mistake. Find it and fix it:

```rust
fn main() {
    let mut counter = 0;
    let counter = counter + 1;
    println!("{}", counter);
}
```

<details>
<summary>Reference answer</summary>

Line 3 has an extra `let`. To swap the value in an existing box, don't write `let` again (writing it creates a NEW box — that's shadowing). Change it to:

```rust
counter = counter + 1;
// or more concisely: counter += 1;
```

(Small note: the original code actually runs — but line 3's `let` creates a new box covering the old one, making the `mut` pointless. The point of the exercise was spotting "swapping doesn't take `let`".)

</details>

### Exercise 5 (challenge): the average score

Three scores — 92, 85 and 78. Compute and print the average (with decimals).

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let chinese = 92;
    let math = 85;
    let english = 78;
    let total = chinese + math + english;
    let total_as_float = total as f64;
    let average = total_as_float / 3.0;
    println!("average: {}", average);
}
```

The total is 255, and 255.0 / 3.0 = 85. This happens to divide evenly; if the total were 256, skipping the conversion would chop off the decimal — which is exactly why `as f64` exists.

</details>

---

## 3.11 FAQ

**Q1: Why doesn't Rust let variables change by default? Every other language does.**

It's a safety design. Most data never needs to change; locking it by default prevents "accidentally changed it" bugs. When you truly need to change something, add `mut` — which spells out "I intend to change this" in black and white.

**Q2: `const` or an ordinary variable without `mut` — when do I use which?**

Values that are absolutely fixed for the whole program (a full score, pi) are constants. Values used only inside one function, that never change anyway, can be ordinary variables.

**Q3: What if my number is too big?**

Problems. `i32` maxes out around 2.1 billion — beyond that it errors or overflows. Use `i64`: `let big_number: i64 = 9999999999;`

**Q4: Are `true` and `"true"` the same thing?**

No. `true` is a boolean (no quotes); `"true"` is a string (quotes — five letters). Like the number 5 versus the word "five" — not the same thing.

**Q5: If type inference is so good, do I never write annotations?**

Most of the time, correct. But constants require them; and writing them makes code clearer — like adding an extra explanatory sentence in an essay. Never hurts.

**Q6: Why does a printed boolean show `true`/`false`?**

That's how Rust's built-in printing renders booleans. If you want localized output, later chapters show formatting techniques. The program's logic is unaffected.

---

## Chapter summary

1. A **variable** is a labeled box, created with `let`.
2. Rust variables are **immutable** by default; add `mut` to change that.
3. **Constants** never change and must carry a type annotation.
4. Four basic types: **integers** (45), **floats** (3.14), **booleans** (true/false), **characters** ('A').
5. Operations: `+ - * / %`; change yourself with `+=`; convert with `as`.
6. Integer division chops decimals; use floats when you want them.

> 📖 If anything in this chapter confused you, check the chapter glossary above or the master glossary at the front of the book.

## What's next

In **Chapter 4, "Compound Types"**, you'll learn to pack several pieces of data together:

- **Tuples**: one gift box holding several different things.
- **Arrays**: a row of numbered lockers.
- **Slices**: looking at part of an array through a window.
