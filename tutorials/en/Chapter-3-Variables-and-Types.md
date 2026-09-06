# Chapter 3: Variables and Types

Last chapter you taught the program to "speak" (print text). This chapter, we teach it to "remember" and "do math".

---

## 3.0 Learning goals

By the end of this chapter, you will be able to:

1. Create variables with `让` and explain what a variable is.
2. Explain why Rust variables can't be changed by default, and how `可变` unlocks them.
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
- the label → the variable's name (like `名字` "name" or `年龄` "age")
- what's inside → the variable's value (like `"小明"` or `12`)

> 💡 **Metaphor**: a running program uses lots of data. Data can't be strewn about — it goes into labeled boxes, and when you need it, you call the label.

### Creating variables with `让`

In native-language Rust, you create a variable with the keyword `让`.

> 📖 **`让`** (let): a keyword whose literal meaning is "let this name stand for this value". `让 年龄 = 12` reads as "let the name '年龄' (age) stand for 12".

```rust
函数 主函数() {
    让 年龄 = 12;
    打印行!("我今年{}岁", 年龄);
}
```

Line by line:

- Line 1: define the main function.
- Line 2: `让 年龄 = 12;` — take a box, label it "年龄" (age), put the number 12 inside. The semicolon ends the instruction.
- Line 3: fill the `{}` placeholder with what's in the "年龄" box (12) — the screen shows "我今年12岁".
- Line 4: the main function ends.

What you should see:

```
我今年12岁
```

> ⚠️ **Careful**: `让` means "create a NEW box". Later, when we meet `可变`, you'll see that "swapping what's inside an old box" is a different thing.

### The `=` sign is not "equals"

In math class, `=` means "both sides are equal". In programming, `=` is an **action**:

> 📖 **Assignment**: put the value on the right into the box on the left. Read it as "put … into …".

```rust
让 分数 = 100;
```

Reads as: "put 100 into the box called '分数' (score)."

It does not ask "is 分数 equal to 100?" — it's a **command**: "put it in!"

> ✨ **Tip**: asking "are they equal?" uses a different symbol, `==` (two equal signs) — you'll meet it in Chapter 5 on control flow.

### One program can have many boxes

```rust
函数 主函数() {
    让 名字 = "小华";
    让 年龄 = 12;
    让 班级 = "六（2）班";
    打印行!("大家好，我是{}，今年{}岁，来自{}", 名字, 年龄, 班级);
}
```

Line by line:

- Lines 2–4: create three boxes — holding text, a number, and text.
- Line 5: the three `{}` are filled by `名字`, `年龄` and `班级` in order.

What you should see:

```
大家好，我是小华，今年12岁，来自六（2）班
```

### The rules for variable names

Labeling boxes (naming things) has a few rules:

| Rule | Right | Wrong | Why |
|---|---|---|---|
| Don't use keywords as names | `让 分数 = 1;` | `让 让 = 1;` | `让` is a reserved keyword |
| Don't start with a digit | `让 二班 = 2;` | `让 2班 = 2;` | A leading digit would be read as a number |
| Letters, digits, underscores and native characters are fine | `让 学号_01 = 1;` | | All legal |

> 📖 **Underscore**: the `_` character on your keyboard (`Shift` + the minus key). It looks like a short dash and often joins words, as in `我的_分数`.

> ⚠️ **Careful**: don't name variables after mapping-table words like `结果`, `字符串` or `类型` — those words have other jobs and cause confusion. Specific names are safest: `总分`, `问候语`, `人数`.

---

## 3.2 Immutable and mutable

### Rust's special rule: boxes are locked by default

In Rust, once a variable is created it **cannot be changed by default**.

```rust
// 预期错误: E0384
函数 主函数() {
    让 分数 = 90;
    分数 = 100;   // ❌ error!
}
```

Running `rzc check` reports:

```
错误[E0384]: 不可变变量 `分数` 被重复赋值
💡 如果需要修改变量的值，请使用 `让 可变` 声明变量。
```

> 📖 **Immutable**: once set, it can't be changed. Rust does this on purpose — Chapter 8 explains why. Short version: data that can't change is safer and causes fewer surprises.

### Unlocking with `可变`

If you truly need to swap what's in the box, add the keyword `可变` when creating it:

> 📖 **`可变`** (mutable): a keyword meaning "changeable". It goes after `让` and declares that what's in this box may be swapped later.

```rust
函数 主函数() {
    让 可变 分数 = 90;
    打印行!("第一次考试：{}分", 分数);
    分数 = 100;               // ✅ with 可变, swapping is allowed
    打印行!("第二次考试：{}分", 分数);
}
```

Line by line:

- Line 2: create the "分数" box holding 90, and declare it changeable.
- Line 3: print 90.
- Line 4: swap the 90 for 100. Note: **no `让` when swapping** — `让` only creates new boxes.
- Line 5: print 100.

What you should see:

```
第一次考试：90分
第二次考试：100分
```

> 💡 **Metaphor**: `让` buys a new box and locks it; `可变` fits the box with a latch so you can open it later; using `让` again buys yet another new box.

> ✨ **Tip**: when should you use `可变`? For data that changes while the program runs (counters, running scores). For data that never changes (a name, pi), don't.

---

## 3.3 Constants: the box that can never change

Some values must never change, ever — like "a year has 12 months". Those are **constants**:

> 📖 **`常量`** (const): a keyword meaning "a value that stays constant forever". Once defined, a constant can never change — and its type must be written out.

```rust
函数 主函数() {
    常量 最大人数: 整数 = 45;
    打印行!("班级最多{}人", 最大人数);
}
```

Line by line:

- Line 2: `常量 最大人数: 整数 = 45;` — define a constant that equals 45 forever. The `: 整数` between `最大人数` and `=` is a **type annotation** — next section.
- Line 3: print it.

> ⚠️ **Careful**: constants differ from immutable variables in three ways:
> 1. Constants can't take `可变` — they never change.
> 2. Constants must have a type annotation (`: 整数`); variables don't need one.
> 3. Constants conventionally live outside functions, where the whole program can use them — you'll see one right in the next section.

---

## 3.4 Types: the "kinds" of data

### Why kinds matter

What a box can hold depends on the kind marked on it. A box for numbers can't hold text — that's a **type**.

> 📖 **Type**: the "kind" of data. Like supermarket shelves labeled "food", "stationery", "toys", data comes in kinds — numbers, text, true/false. The type decides what operations the data can take: numbers can be added, text can't "plus one".

> 📖 **Type annotation**: writing `: TypeName` after a name to tell the compiler exactly what kind of data the box holds — like labeling the box "apples" up front.

Native-language Rust translates all of Rust's types into native words. Let's meet them one by one.

### Type one: integers — numbers without a decimal point

> 📖 **Integer**: a number with no decimal part, like 3, -8, 0. The default integer type in native-language Rust is simply called `整数`.

```rust
让 人数 = 45;        // an "整数" by default
让 温度 = -8;        // negatives work too
```

Rust actually has a whole family of integers, differing in "how big" and "negative allowed or not":

| Native name | English | Range | Notes |
|---|---|---|---|
| `微整数` | i8 | -128 to 127 | the smallest |
| `短整数` | i16 | about ±33,000 | |
| `整数` | i32 | about ±2.1 billion | **the default, most used** |
| `长整数` | i64 | astronomically large | holds huge numbers |
| `无符号整数` | u32 | 0 to about 4.2 billion | no negatives allowed |

> 📖 **Unsigned**: the "sign" is the plus/minus. Unsigned means "no sign" — only 0 and positives. The upside: the same space holds bigger positive numbers.

> ✨ **Tip**: beginners only need to remember `整数`. The "i" in the English names means integer; the number is how many bits (little binary cells) store it. More cells, bigger numbers.

### Type two: floating-point — numbers with a decimal point

> 📖 **Floating-point**: a number with a decimal point, like 3.14 or -0.5. The default type is called `浮点数` ("f" is for float — the decimal point "floats").

```rust
让 圆周率 = 3.14;
让 体温 = 36.5;
```

> ⚠️ **Careful**: integer divided by integer is still an integer — the decimal part gets **chopped off**:

```rust
让 结果商 = 7 / 2;
打印行!("{}", 结果商);   // shows 3, not 3.5!
```

For a decimal result, at least one side must be a floating-point number:

```rust
让 结果商 = 7.0 / 2.0;
打印行!("{}", 结果商);   // shows 3.5
```

> 📖 **Remainder**: the `%` symbol computes "the remainder of a division". 7 % 2 = 1 (7 ÷ 2 is 3 remainder 1). Great for odd/even checks: remainder 0 means even.

### Type three: booleans — only true and false

> 📖 **Boolean**: a type with exactly two possible values — `真` (true) or `假` (false) — named after the mathematician Boole. It answers yes/no questions.

```rust
让 已完成作业 = 真;
让 今天下雨 = 假;
```

> 📖 **`真`** and **`假`**: keywords — the only two values a boolean has. Not one character can be off.

Booleans shine in Chapter 5's `如果` decisions: "**if** it's raining **is** `真`, take an umbrella."

### Type four: characters — a single letter

> 📖 **Character**: a single text symbol, wrapped in **single quotes** `'` (not double quotes).

```rust
让 等级 = '优';
让 符号 = '★';
```

> ⚠️ **Careful**: character vs string — `'优'` is one character (one letter); `"优秀"` is a string (a run of letters). Single quotes hold one; double quotes hold a run. Strings get their own chapter — Chapter 9.

### The whole type family, at a glance

| Native name | English | Example | Remember it by |
|---|---|---|---|
| `整数` | i32 | `45` | no decimal point |
| `长整数` | i64 | `9999999999` | big numbers |
| `浮点数` | f64 | `3.14` | has a decimal point |
| `布尔` | bool | `真` / `假` | yes or no |
| `字符` | char | `'优'` | single quotes, one letter |
| `字符串` | String | `"你好"` | double quotes, a run of letters (Chapter 9) |

### The compiler guesses types for you

When you write `让 年龄 = 12;` you never wrote `: 整数` — how does the compiler know it's an integer?

Because the compiler **infers** it: seeing 12 with no decimal point, it guesses `整数`; seeing 3.14, it guesses `浮点数`; seeing quotes, it guesses a string.

> 📖 **Type inference**: the compiler's ability to judge a type from the shape of the value. Less typing for you, still reliable. But you can always write the annotation — it makes things clearer.

---

## 3.5 Making the program do math: operators

> 📖 **Operator**: a symbol that performs an operation. `+`, `-`, `*`, `/` are all operators.

### The five basic arithmetic operators

```rust
函数 主函数() {
    让 甲 = 10;
    让 乙 = 3;
    打印行!("加：{}", 甲 + 乙);     // 13
    打印行!("减：{}", 甲 - 乙);     // 7
    打印行!("乘：{}", 甲 * 乙);     // 30
    打印行!("除：{}", 甲 / 乙);     // 3 (integer division, decimals chopped)
    打印行!("余：{}", 甲 % 乙);     // 1 (the remainder)
}
```

| Symbol | Name | Example | Result |
|---|---|---|---|
| `+` | add | `10 + 3` | 13 |
| `-` | subtract | `10 - 3` | 7 |
| `*` | multiply | `10 * 3` | 30 |
| `/` | divide | `10 / 3` | 3 |
| `%` | remainder | `10 % 3` | 1 |

> ⚠️ **Careful**: both numbers in an operation must be **the same type**. `整数 + 浮点数` errors out. To mix, convert with `作为` (coming right up).

### Compound assignment: shortcuts for changing yourself

```rust
让 可变 分数 = 90;
分数 += 5;    // same as 分数 = 分数 + 5 — now 95
分数 -= 10;   // same as 分数 = 分数 - 10 — now 85
分数 *= 2;    // 85 × 2 = 170
```

> 📖 **Compound assignment**: `+=`, `-=`, `*=`, `/=` mean "operate on myself, then put the result back". Read `分数 += 5` as "add 5 to 分数 and store it back".

### A little trick: there is no `++`

Rust has **no** `++` operator. To add 1, write:

```rust
计数 += 1;
```

### Type conversion: `作为`

> 📖 **`作为`** (as): a keyword that temporarily treats a value "as" another type.

```rust
函数 主函数() {
    让 人数 = 7;
    让 人数为小数 = 人数 作为 浮点数;   // first turn 7 into 7.0
    让 平均 = 人数为小数 / 2.0;          // then divide by 2.0 → 3.5
    打印行!("平均：{}", 平均);           // 3.5
}
```

Line by line:

- Line 2: `人数` is the integer 7.
- Line 3: `人数 作为 浮点数` temporarily treats 7 as 7.0 and stores it in a new box. The original 7 is untouched.
- Line 4: 7.0 divided by 2.0 is 3.5.

> ⚠️ **Careful**: when a float takes part in an operation, the other side must be written as a float too (like `2.0`). `人数为小数 / 2` errors out — integers and decimals can't be divided directly.

> ⚠️ **Careful**: don't squeeze `作为` into a longer line, like `人数 作为 浮点数 / 2` — the computer may read the order differently than you expect. Two lines is safest.

> 💡 **Metaphor**: `作为` is like wearing a mask — the integer puts on a "floating-point mask" to attend the floats' party; take the mask off and it's still the same integer.

---

## 3.6 A complete example: the student info card

### The complete code

```rust
// 学生信息卡程序
常量 满分: 整数 = 100;

函数 主函数() {
    // 基本信息（不可变，创建后不改）
    让 名字 = "小华";
    让 年龄 = 12;

    // 成绩（数学之后要更新，所以只有数学加"可变"）
    让 语文 = 88;
    让 可变 数学 = 95;

    // 先记录一下原来的数学成绩
    打印行!("数学原来：{}分", 数学);

    // 数学考了满分，更新一下
    数学 = 满分;

    // 计算总分和平均分
    让 总分 = 语文 + 数学;
    让 总分为小数 = 总分 作为 浮点数;
    让 平均分 = 总分为小数 / 2.0;

    // 打印信息卡
    打印行!("====== 学生信息卡 ======");
    打印行!("姓名：{}", 名字);
    打印行!("年龄：{}", 年龄);
    打印行!("语文：{}分", 语文);
    打印行!("数学：{}分", 数学);
    打印行!("总分：{}分", 总分);
    打印行!("平均分：{}", 平均分);
    打印行!("是否优秀：{}", 平均分 >= 90.0);
}
```

### Line by line

- Line 1: a comment saying what this program does.
- Line 2: define the constant `满分` (full score), annotated `整数`, value 100. It sits at the top level, outside the functions, where the whole program can use it.
- Lines 6–7: two immutable boxes for name and age — this information won't change.
- Lines 10–11: two boxes for scores. 语文 (Chinese) won't change, no `可变`; 数学 (math) will be updated, so `可变`.
- Line 14: print the original math score, 95. This "uses up" the 95 so the next line can safely swap it.
- Line 17: replace `数学` with `满分` (100). Note: no `让` — this is swapping, not creating a new box.
- Line 20: `语文 + 数学` = 88 + 100 = 188, into the new box `总分`.
- Line 21: convert 188 into 188.0 and store it in the new box `总分为小数`.
- Line 22: 188.0 ÷ 2.0 = 94.0. Two lines, to dodge the "one long line, misread order" trap.
- Lines 25–32: print the info card line by line. Each `{}` is filled by the matching box.
- Line 32: `平均分 >= 90.0` is a comparison producing a boolean — `真` or `假`. `>=` means "greater than or equal to" — Chapter 5 covers it. When comparing against a float, write the 90 as 90.0.
- Line 33: the main function ends.

### What you should see

```
数学原来：95分
====== 学生信息卡 ======
姓名：小华
年龄：12
语文：88分
数学：100分
总分：188分
平均分：94
是否优秀：真
```

> ⚠️ **Careful**: the last line shows `真` — that's the default way booleans are printed. It doesn't affect the program; after Chapter 11 you'll learn to display it any way you like.

---

## 3.7 Shadowing: a new box with the same name covers the old one

Rust has a curious quirk: you can use `让` again with the same name, and the new box "covers" the old one:

```rust
函数 主函数() {
    让 数字 = 5;
    让 数字 = 数字 + 1;    // new box covers old, holds 6
    让 数字 = 数字 * 2;    // yet another new box, holds 12
    打印行!("{}", 数字);   // 12
}
```

> 📖 **Shadowing**: creating a new variable with `让` under an existing name — the new variable shadows the old one, and calling the name now finds the new box. The old box still exists, just hidden "behind the shadow", out of reach.

> 💡 **Metaphor**: it's like stacking boxes — the later same-named box stands on the old one's shoulders, and when you call the name, the topmost box answers.

Shadowing vs `可变`:

| | Shadowing | `可变` |
|---|---|---|
| How it's written | `让` every time | `让` once, then plain assignment |
| The box | Each time creates a NEW box | Always the same box |
| The type | Can switch to a completely different type | Only values of the same type |

---

## 3.8 Common mistakes and how to fix them

### Mistake 1: swapping an immutable variable

```rust
// 预期错误: E0384
让 分数 = 90;
分数 = 100;   // ❌
```

The report: `错误[E0384]: 不可变变量 `分数` 被重复赋值`, with the suggestion "use `让 可变`".

**The fix**: if it really must change, add `可变` at creation.

### Mistake 2: using a box you never created

```rust
打印行!("{}", 身高);   // ❌ there was never a "让 身高 = ..."
```

The report says, roughly, that nothing called `身高` can be found.

**The fix**: check for typos; make sure creation comes before use.

### Mistake 3: mixing types in an operation

```rust
// 预期错误: E0277
让 甲 = 10;          // integer
让 乙 = 3.5;         // float
让 总和 = 甲 + 乙;   // ❌ integers and floats don't add directly
```

**The fix**: convert one side with `作为`, and **write it in two lines**:

```rust
让 甲为小数 = 甲 作为 浮点数;
让 总和 = 甲为小数 + 乙;
```

### Mistake 4: Chinese punctuation sneaking into code

```rust
让 名字 = "小华"；   // ❌ that's a Chinese semicolon at the end
```

The report usually points at the line, complaining about an unknown symbol.

**The fix**: switch to English input mode and change `；` to `;`. Remember the mantra: **the code skeleton is English; only inside quotes is native**.

---

## 3.9 Chapter glossary

| Term | Meaning |
|---|---|
| Assignment | Put the right-hand value into the left-hand box — the `=` symbol |
| Boolean | The type with only `真`/`假` values |
| Immutable | Can't change after creation — Rust variables' default nature |
| Constant | A value that can never change — the `常量` keyword |
| Floating-point | A number with a decimal point; default type `浮点数` |
| Compound assignment | `+=`, `-=` etc. — "operate, then store back" shorthands |
| Mutable | The keyword that lets a box swap contents |
| Type | The kind of data |
| Type annotation | Writing `: TypeName` after a name |
| Type inference | The compiler judging the type from the value |
| Remainder | The `%` operation — the rest left over from division |
| Unsigned | Cannot represent negatives, like `无符号整数` |
| Underscore | The `_` symbol, usable in variable names |
| Operator | A symbol that performs an operation, like `+` and `-` |
| Integer | A number with no decimal point; default type `整数` |
| Character | A single text symbol, in single quotes |
| Shadowing | A new same-named variable covering the old one |
| 真/假 | The two boolean values |
| 作为 | The keyword for temporary type conversion |

---

## 3.10 Exercises

### Exercise 1: self-introduction card

Create variables for your name, age and school, and print them on three lines.

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 姓名 = "小华";
    让 年龄 = 12;
    让 学校 = "阳光小学";
    打印行!("姓名：{}", 姓名);
    打印行!("年龄：{}", 年龄);
    打印行!("学校：{}", 学校);
}
```

</details>

### Exercise 2: a calculator

Create two integer variables `甲数` = 17 and `乙数` = 5, and print their sum, difference, product, quotient and remainder.

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 甲数 = 17;
    让 乙数 = 5;
    打印行!("和：{}", 甲数 + 乙数);
    打印行!("差：{}", 甲数 - 乙数);
    打印行!("积：{}", 甲数 * 乙数);
    打印行!("商：{}", 甲数 / 乙数);
    打印行!("余：{}", 甲数 % 乙数);
}
```

The results are 22, 12, 85, 3 and 2.

</details>

### Exercise 3: a temperature log

Create a mutable variable `体温` = 38.5, simulate it dropping to 36.8 after medicine, and print both.

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 可变 体温 = 38.5;
    打印行!("吃药前：{}", 体温);
    体温 = 36.8;
    打印行!("吃药后：{}", 体温);
}
```

</details>

### Exercise 4: spot the mistake

This code has one mistake. Find it and fix it:

```rust
函数 主函数() {
    让 可变 计数器 = 0;
    让 计数器 = 计数器 + 1;
    打印行!("{}", 计数器);
}
```

<details>
<summary>Reference answer</summary>

Line 3 has an extra `让`. To swap the value in an existing box, don't write `让` again (writing it creates a NEW box — that's shadowing). Change it to:

```rust
计数器 = 计数器 + 1;
// or more concisely: 计数器 += 1;
```

(Small note: the original code actually runs — but line 3's `让` creates a new box covering the old one, making the `可变` pointless. The point of the exercise was spotting "swapping doesn't take `让`".)

</details>

### Exercise 5 (challenge): the average score

Three scores — 92, 85 and 78. Compute and print the average (with decimals).

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 语文 = 92;
    让 数学 = 85;
    让 英语 = 78;
    让 总分 = 语文 + 数学 + 英语;
    让 总分为小数 = 总分 作为 浮点数;
    让 平均分 = 总分为小数 / 3.0;
    打印行!("平均分：{}", 平均分);
}
```

The total is 255, and 255.0 / 3.0 = 85. This happens to divide evenly; if the total were 256, skipping the conversion would chop off the decimal — which is exactly why `作为 浮点数` exists.

</details>

---

## 3.11 FAQ

**Q1: Why doesn't Rust let variables change by default? Every other language does.**

It's a safety design. Most data never needs to change; locking it by default prevents "accidentally changed it" bugs. When you truly need to change something, add `可变` — which spells out "I intend to change this" in black and white.

**Q2: `常量` or an ordinary variable without `可变` — when do I use which?**

Values that are absolutely fixed for the whole program (a full score, pi) are constants. Values used only inside one function, that never change anyway, can be ordinary variables.

**Q3: What if my number is too big?**

Problems. `整数` maxes out around 2.1 billion — beyond that it errors or overflows. Use `长整数`: `让 大数: 长整数 = 9999999999;`

**Q4: Are `真` and `"真"` the same thing?**

No. `真` is a boolean (no quotes); `"真"` is a string (quotes — it's one character). Like the number 5 versus the word "five" — not the same thing.

**Q5: If type inference is so good, do I never write annotations?**

Most of the time, correct. But constants require them; and writing them makes code clearer — like adding an extra explanatory sentence in an essay. Never hurts.

**Q6: Why does a printed boolean show `真`/`假`?**

That's how Rust's built-in printing renders booleans. Full localization of output comes in a later chapter's formatting techniques. The program's logic is unaffected.

---

## Chapter summary

1. A **variable** is a labeled box, created with `让`.
2. Rust variables are **immutable** by default; add `可变` to change that.
3. **Constants** never change and must carry a type annotation.
4. Four basic types: **integers** (45), **floats** (3.14), **booleans** (真/假), **characters** ('优').
5. Operations: `+ - * / %`; change yourself with `+=`; convert with `作为`.
6. Integer division chops decimals; use floats when you want them.

> 📖 If anything in this chapter confused you, check the chapter glossary above or the master glossary at the front of the book.

## What's next

In **Chapter 4, "Compound Types"**, you'll learn to pack several pieces of data together:

- **Tuples**: one gift box holding several different things.
- **Arrays**: a row of numbered lockers.
- **Slices**: looking at part of an array through a window.
