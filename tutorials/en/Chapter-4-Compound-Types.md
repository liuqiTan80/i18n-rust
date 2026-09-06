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
让 学生 = ("小华", 12, 真);
```

Piece by piece:

- `("小华", 12, 真)`: a gift box holding **three things** — a piece of text, a number, a boolean. Different kinds mixed together is perfectly fine; that's what tuples are for.
- The three things are the tuple's **elements** (its "members"). This tuple has 3 elements.

> 📖 **Tuple**: a compound type that packs several values into one group (pronounced like "toople"). Elements may be of different kinds.

### Getting things out: counting starts at 0

To take one item out, write `.number` after the tuple's name:

```rust
函数 主函数() {
    让 学生 = ("小华", 12, 真);
    打印行!("第一项：{}", 学生.0);
    打印行!("第二项：{}", 学生.1);
    打印行!("第三项：{}", 学生.2);
}
```

What you should see:

```
第一项：小华
第二项：12
第三项：真
```

> ⚠️ **Careful**: **counting starts at 0, not 1!** The first element's number is 0, the second is 1, the third is 2.

This is a rule across the entire programming world. Why? We'll explain with arrays in the next section — for now, remember: **when counting positions, the first number is "0"**.

> 📖 **Index**: the formal name for this "number" — the address you use to look something up. The book uses both words interchangeably.

### Unpacking the whole tuple at once

Want all three? Unpack in one go:

```rust
函数 主函数() {
    让 学生 = ("小华", 12, 真);
    让 (姓名, 岁数, 是否住宿) = 学生;
    打印行!("{}今年{}岁", 姓名, 岁数);
    打印行!("住宿吗？{}", 是否住宿);
}
```

Line by line:

- Line 3: `让 (姓名, 岁数, 是否住宿) = 学生;` — the left side of the `=` also has a bracket shape, meaning "split the package into three". The computer matches by position: item 1 goes to `姓名`, item 2 to `岁数`, item 3 to `是否住宿`.
- After unpacking, the three names are independent boxes of their own.

> 📖 **Destructuring**: writing that splits packed data apart by shape and takes each piece. "解" is untie, "构" is structure.

> 💡 **Metaphor**: destructuring is like opening a delivery — three things in the parcel, three baskets ready, everything sorted in one pass.

What you should see:

```
小华今年12岁
住宿吗？真
```

### Printing the whole tuple: `{:?}`

To print the entire tuple at once, the placeholder is `{:?}` (a colon and a question mark inside the braces):

```rust
函数 主函数() {
    让 学生 = ("小华", 12, 真);
    打印行!("整个元组：{:?}", 学生);
}
```

What you should see:

```
整个元组：("小华", 12, 真)
```

> 📖 **Debug printing**: `{:?}` is the "for debugging" print — it shows the thing's internal structure exactly (quotes, brackets and all). The plain `{}` prints the clean, human-facing version. The full story of debug printing is in Chapter 10.

> ⚠️ **Careful**: printing a tuple with plain `{}` is an error. Tuples must use `{:?}`.

---

## 4.3 Arrays: a row of numbered lockers

### Creating an array

Picture a corridor of school lockers — each locker the same size, all for the same kind of thing (say, scores). That's an **array**.

Create arrays with **square brackets**, elements separated by commas:

```rust
让 分数 = [88, 95, 76, 90, 82];
```

This array holds a school week's scores — 5 elements in total.

> 📖 **Array**: a row of **fixed-length**, **same-kind** data. Square brackets `[ ]` are the array's signature.

> ⚠️ **Careful**: don't mix up square brackets `[ ]` and parentheses `( )` — square is an array (a row of same-kind things), parentheses are a tuple (several different things).

### A shortcut for repeated values

If every locker holds the same thing, there's a shorthand:

```rust
让 五个零 = [0; 5];
```

> 📖 Read this as "0, repeated 5 times": before the semicolon is the content, after it the count. Same as `[0, 0, 0, 0, 0]`.

```rust
打印行!("{:?}", 五个零);   // [0, 0, 0, 0, 0]
```

### Fetching lockers by index: counting starts at 0

```rust
函数 主函数() {
    让 分数 = [88, 95, 76, 90, 82];
    打印行!("第一天：{}", 分数[0]);
    打印行!("第三天：{}", 分数[2]);
    打印行!("最后一天：{}", 分数[4]);
}
```

What you should see:

```
第一天：88
第三天：76
最后一天：82
```

A picture makes it clearest:

```
柜子:  [ 88 ]  [ 95 ]  [ 76 ]  [ 90 ]  [ 82 ]
索引:     0       1       2       3       4
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

**Skill one: `长度()` — count the lockers**

```rust
打印行!("共{}天", 分数.长度());   // 共5天
```

> 📖 **`长度()`**: a method that returns the element count. The dot is followed by a method; the parentheses stay empty. What's a "method"? Chapter 6 explains — for now, treat `.长度()` as a fixed incantation.

**Skill two: `包含()` — is this value in there?**

```rust
打印行!("有95分吗？{}", 分数.包含(&95));   // 有95分吗？真
```

> 📖 **`包含()`**: a method that checks whether the array contains a value, returning a boolean. Copy the `&95` for now — `&` is the "borrow" mark, explained fully in Chapter 8. For now: "contains needs a &".

### Out of bounds: reaching for a locker that isn't there ends badly

The array has 5 lockers (indices 0–4). Insist on number 6:

```rust
// 预期行为: 运行失败
函数 主函数() {
    让 分数 = [88, 95, 76, 90, 82];
    打印行!("{}", 分数[5]);   // ❌ out of bounds!
}
```

Rust immediately **panics** — the program stops on the spot.

> 📖 **Panic**: when a program hits an unrecoverable error, it shouts "something's wrong!" and stops immediately, never running while broken. Like a fire alarm: everyone stops what they're doing and evacuates. Chapter 16 covers this.

The panic message looks like this:

```
线程 '主函数' 恐慌于 ...:
index out of bounds: the len is 5 but the index is 5
```

Translated: "index out of bounds: the length is 5, but you asked for number 5." (The biggest legal number is 4!)

> ✨ **Tip**: if you write a **constant** out-of-bounds number (like `分数[5]` directly), the compiler stops you at compile time with "this operation will panic at runtime" — the program never even runs. Only when the number comes from outside, unknowable at compile time, does the panic truly happen at runtime. Either way, Rust never lets you quietly read wrong data.
>
> ⚠️ **Careful**: `rzc check` only translates and checks — it can't catch this. To see it, actually run `rzc run`.

> 📖 **Index out of bounds**: accessing a number beyond the range. The original message is "index out of bounds" — literally "beyond the boundary".

---

## 4.4 Slices: a window onto the array

### Don't want to move the whole locker row — just look at a few?

Say you only want the first three days' scores. Copying them into a new array is wasteful. A **slice** is smarter: no copying, just "pointing" at a stretch of the original.

> 📖 **Slice**: a reference to **a run of consecutive elements** in an array — literally a "slice": cut a piece of sausage, but the sausage itself stays.

Write `[start..end]` after the array, with a `&` in front:

```rust
函数 主函数() {
    让 分数 = [88, 95, 76, 90, 82];
    让 前三天 = &分数[0..3];
    打印行!("前三天：{:?}", 前三天);
    打印行!("切片长度：{}", 前三天.长度());
}
```

What you should see:

```
前三天：[88, 95, 76]
切片长度：3
```

`&分数[0..3]` piece by piece:

| Part | Meaning |
|---|---|
| `&` | Borrow (don't take the original — Chapter 8) |
| `分数` | The array being sliced |
| `[0..3]` | From index 0 up to **before** index 3 — i.e. 0, 1 and 2 |

> ⚠️ **Careful**: `0..3` is "**include the head, exclude the tail**" — includes 0, excludes 3. Count them: exactly 3 elements. This rule holds across the whole book.

> 📖 **Range**: the `start..end` form is called a range — "from here to there".

### Shorter forms

```rust
让 从头到第三 = &分数[..3];    // omitted start = from 0
让 从第三到底 = &分数[2..];    // omitted end = to the last one
让 全部 = &分数[..];           // both omitted = the whole stretch
```

```rust
打印行!("{:?} {:?} {:?}", 从头到第三, 从第三到底, 全部);
// [88, 95, 76] [76, 90, 82] [88, 95, 76, 90, 82]
```

### Text can be sliced too

Strings can be sliced as well (details in Chapter 9 — just a first meeting here):

```rust
让 问候 = &"你好世界"[0..6];
打印行!("{}", 问候);   // 你好
```

> ⚠️ **Careful**: it's 0..6, not 0..2 — each Chinese character occupies 3 "bytes" (little storage cells) in the computer, so "你好" is 6 cells. What's a byte? Chapter 9 explains. For now, copy the 0..6.

---

## 4.5 A complete example: the week's score manager

### The complete code

```rust
函数 主函数() {
    // 一周的分数（周一到周五）
    让 一周分数 = [88, 95, 76, 90, 82];

    // 把一周切成两半看
    让 前两天 = &一周分数[0..2];
    让 后三天 = &一周分数[2..5];

    打印行!("一周分数：{:?}", 一周分数);
    打印行!("共{}天", 一周分数.长度());
    打印行!("前两天：{:?}", 前两天);
    打印行!("后三天：{:?}", 后三天);

    // 手动求总分和平均分
    让 总分 = 一周分数[0] + 一周分数[1] + 一周分数[2] + 一周分数[3] + 一周分数[4];
    让 总分为小数 = 总分 作为 浮点数;
    让 平均分 = 总分为小数 / 5.0;
    打印行!("总分：{} 平均分：{}", 总分, 平均分);

    // 学生信息用元组打包
    让 学生 = ("小华", 12, 真);
    让 (姓名, 岁数, 是否住宿) = 学生;
    打印行!("{}今年{}岁，住宿：{}", 姓名, 岁数, 是否住宿);
    打印行!("元组第二项：{}", 学生.1);
}
```

### Line by line

- Line 3: create the array of five days' scores. Square brackets = array.
- Line 6: slice out indices 0 and 1 (head in, tail out).
- Line 7: slice indices 2, 3, 4 — exactly the remaining three days.
- Line 9: `{:?}` debug-prints the whole array, brackets included.
- Line 10: `.长度()` returns 5.
- Lines 11–12: print the two slices.
- Line 15: add all five elements by hand = 431. (After Chapter 5's loops, this becomes one line — no more writing five.)
- Line 16: convert to a float (two lines, dodging the `作为` precedence trap from Chapter 3).
- Line 17: 431.0 ÷ 5.0 = 86.2.
- Line 20: create a three-item tuple: text, number, boolean, mixed.
- Line 21: destructure — three names, one item each, by position.
- Line 22: print using the three unpacked boxes.
- Line 23: take the tuple's second item directly with `.1` (counting from 0, `.1` is item two: 12).

### What you should see

```
一周分数：[88, 95, 76, 90, 82]
共5天
前两天：[88, 95]
后三天：[76, 90, 82]
总分：431 平均分：86.2
小华今年12岁，住宿：真
元组第二项：12
```

---

## 4.6 Common mistakes and how to fix them

### Mistake 1: index out of bounds

5 elements, biggest index 4. Writing `分数[5]` is out of bounds.

**The fix**: the biggest legal index is always `长度() - 1`. Unsure? Print `.长度()` first.

### Mistake 2: counting from 1

Wanting the first but writing `分数[1]` gets you the second.

**The fix**: chant the mantra — "**the first one is 0**".

### Mistake 3: square brackets on a tuple

```rust
// 预期错误: E0608
让 学生 = ("小华", 12, 真);
打印行!("{}", 学生[0]);   // ❌ tuples don't use square brackets
```

**The fix**: tuples use a **dot**: `学生.0`. Square brackets are for arrays. Mantra: **tuple dot, array square**.

### Mistake 4: `包含` without the `&`

```rust
分数.包含(95)    // ❌ error
分数.包含(&95)   // ✅ correct
```

**The fix**: copy the `&` — Chapter 8 explains why.

### Mistake 5: mixed kinds in an array

```rust
// 预期错误: E0308
让 混合 = [1, "二", 3];   // ❌ arrays must be one kind
```

**The fix**: for mixing, use a tuple `(1, "二", 3)`; arrays hold one kind only.

### A bonus experiment: watch a real panic with your own eyes (optional)

This example uses "environment variables" and other later concepts — every line is commented, so you can follow it and see a real panic message:

```rust
使用 标准库::环境::环境变量;

函数 主函数() {
    让 分数 = [88, 95, 76];
    // 从电脑的环境变量里读一个叫 序号 的值（相当于程序外的"传话"）
    让 文字 = 环境变量("序号").期望("请设置环境变量");
    // 裁剪掉首尾的空白
    让 干净文字 = 文字.裁剪();
    // 把文字变成数字。: 无符号机器整数 表示"专门用来数数的整数"
    让 下标: 无符号机器整数 = 干净文字.解析().期望("要数字");
    打印行!("分数是：{}", 分数[下标]);
}
```

On Linux / macOS (type 5 first, on purpose out of bounds):

```bash
序号=5 rzc run src/主函数.zh
```

You'll see a real panic message:

```
thread '主函数' panicked at ...:
下标越界：长度是 3，但下标是 5
```

Now a legal number:

```bash
序号=1 rzc run src/主函数.zh
```

Output: `分数是：95`.

> ✨ **Tip**: in Windows PowerShell, set the variable as `$env:序号="5"` before running. The `环境变量`, `裁剪`, `解析` and `期望` used here all get proper introductions in later chapters — this experiment is just an early look at what a panic looks like.

---

## 4.7 Chapter glossary

| Term | Meaning |
|---|---|
| 包含 | The `包含()` method — does the array contain a value |
| 长度 | The `长度()` method — returns the element count |
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

### Exercise 1: meet the tuple

Create a tuple `城市` (city) with three elements: name "北京", population 2189, capital `真`. Print each with `.0`, `.1`, `.2`.

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 城市 = ("北京", 2189, 真);
    打印行!("{}", 城市.0);
    打印行!("{}", 城市.1);
    打印行!("{}", 城市.2);
}
```

</details>

### Exercise 2: unpack

Destructure Exercise 1's tuple into three variables, then print one sentence: `北京有2189万人，是首都`.

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 城市 = ("北京", 2189, 真);
    让 (名字, 人口, 是首都) = 城市;
    打印行!("{}有{}万人，是首都：{}", 名字, 人口, 是首都);
}
```

</details>

### Exercise 3: fetch from an array

Create an array `气温` (temperatures) with seven days: `[22, 25, 27, 24, 23, 26, 28]`. Print day 1, day 4 and the last day, plus the array's length.

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 气温 = [22, 25, 27, 24, 23, 26, 28];
    打印行!("第一天：{}", 气温[0]);
    打印行!("第四天：{}", 气温[3]);
    打印行!("最后一天：{}", 气温[6]);
    打印行!("共{}天", 气温.长度());
}
```

Note the last day's index is 6, not 7 — 7 elements means indices 0 through 6.

</details>

### Exercise 4: slices

Using Exercise 3's array, slice out the "weekend" (the last two days) and the "weekdays" (the first five), and print both.

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 气温 = [22, 25, 27, 24, 23, 26, 28];
    让 工作日 = &气温[0..5];
    让 周末 = &气温[5..7];
    打印行!("工作日：{:?}", 工作日);
    打印行!("周末：{:?}", 周末);
}
```

The weekend can also be written `&气温[5..]` (omitted end = to the last).

</details>

### Exercise 5 (challenge): spot the bug

What happens when this code runs? How do you fix it?

```rust
函数 主函数() {
    让 三个数 = [1, 2, 3];
    打印行!("{}", 三个数[3]);
}
```

<details>
<summary>Reference answer</summary>

The array has 3 elements; the biggest index is 2. `三个数[3]` is out of bounds. Because it's a fixed number, the compiler intercepts it at build time with "this operation will panic at runtime". Change it to `三个数[2]` to get the 3.

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
4. `.长度()` counts, `{:?}` debug-prints, out-of-bounds always ends badly — and Rust always catches it.

> 📖 If anything in this chapter confused you, check the chapter glossary above or the master glossary at the front of the book.

## What's next

In **Chapter 5, "Control Flow"**, the program learns to "make choices" and "do repetitive work":

- **`如果 / 否则`**: umbrella if it rains, hat if it doesn't.
- **`匹配`**: like a vending machine — press a button, get that snack.
- **`循环`, `当`, `对于`**: hand the repetition to the computer.
- And you'll discover that a code block `{ }` is itself a "value" you can use.
