# Chapter 9: Strings and Text

## 9.0 Learning goals

By the end of this chapter, you will be able to:

1. Tell **string references** (`&文本`) and **strings** apart;
2. Create, append to and join strings;
3. Query text with `长度()`, `包含()`, `以开头()` and `查找()`;
4. Explain the difference between **bytes** and **characters** — and why slicing Chinese text can panic;
5. Process text with `替换()`, `裁剪()` and `分割()`;
6. Walk through text one character at a time with `逐字符()`.

> ✨ Earlier chapters have owed "text" its own chapter; today the debt is paid. Strings are the most-used type in everyday programming — this one is worth reading slowly.

---

## 9.1 The two text types

Rust has two types for "a piece of text" — the spot where beginners get most confused.

💡 **Metaphor**:
- A **string reference** is like **words carved in stone** — fixed at birth, unchangeable by anyone;
- A **string** is like **words in a notebook** — add and edit freely (but the notebook must be declared "mutable").

| Type | Spelling | Character | Lives in |
|---|---|---|---|
| String reference | `字符串引用` (&文本) | Immutable, fixed text | The stone (built into the program) |
| String | `字符串` | Growable, modifiable | The warehouse (heap) |

```rust
让 石碑 = "刻死的字";                 // type: string reference
让 可变 笔记本 = 字符串::从("可写的字"); // type: string
```

📖 **字符串** (string): a growable text type, stored on the heap.
📖 **字符串引用** (string reference): borrowed, immutable text — a string literal (anything written directly in double quotes) is one.

### How to choose?

- Fixed, never-changing text (prompts, menu items) → a literal is enough (string reference);
- Text that grows or gets assembled (user input, program-built messages) → a `字符串`.

---

## 9.2 The stone cannot be edited

Try to modify a literal:

```rust
// 预期错误: E0599
让 可变 石碑 = "刻死的字";
石碑.推入('！');   // ❌ compile error
```

The error says, roughly: you can't call `推入` on a `字符串引用`. Stone is stone — even `可变` can't save it. **The type decides the ability.** For editable text, use `字符串::从("...")` to **copy the inscription onto a notebook**.

---

## 9.3 Creating a string and growing it

```rust
// 从字面量造一个字符串
让 可变 日记 = 字符串::从("今天我学了字符串");

// 推入：末尾加一个字符（单引号！）
日记.推入('，');

// 推入字符串：末尾加一段文字
日记.推入字符串("真好玩！");

打印行!("日记：{}", 日记);
// 预期输出: 日记：今天我学了字符串，真好玩！
```

Output: `日记：今天我学了字符串，真好玩！`

Three points:

| Spelling | Meaning |
|---|---|
| `'，'` single quotes | **one character** (the character type) |
| `"..."` double quotes | a run of text (a string reference) |
| `字符串::从("...")` | copy text into a modifiable string |

> ⚠️ **Careful**: single and double quotes must not be mixed. `推入` takes a single character (single quotes); `推入字符串` takes a run of text (double quotes).

---

## 9.4 Joining: `格式化!` does it all

📖 **格式化!** (format): a macro that builds a new string from values and a template. The `{}`s in the template get replaced in order. It looks almost identical to `打印行!` — the difference: **`打印行!` shows on screen; `格式化!` returns a string**.

```rust
让 名字 = "小明";
让 问候 = 格式化!("你好，{}！", 名字);
打印行!("{}", 问候);
// 预期输出: 你好，小明！
```

Output: `你好，小明！`

This one trick covers 90% of all text-joining needs.

---

## 9.5 Querying

```rust
让 日记 = 字符串::从("今天我学了字符串，真好玩！");

打印行!("字节长度：{}", 日记.长度());
打印行!("包含'字符串'吗：{}", 日记.包含("字符串"));
打印行!("以'今天'开头吗：{}", 日记.以开头("今天"));
打印行!("以'！'结尾吗：{}", 日记.以结尾("！"));
```

Output:

```
字节长度：39
包含'字符串'吗：真
以'今天'开头吗：真
以'！'结尾吗：真
```

| Method | What it does | Returns |
|---|---|---|
| `长度()` | Byte count (careful — not the letter count!) | integer |
| `包含(小段)` | Does it contain this piece? | boolean |
| `以开头(小段)` | Does it start with this? | boolean |
| `以结尾(小段)` | Does it end with this? | boolean |
| `查找(小段)` | Position of the first occurrence (byte number) | option (Chapter 16) |

---

## 9.6 Bytes vs characters: the Chinese trap

Here's a counter-intuitive number: "今天我学了字符串，真好玩！" is visibly **13 characters**, yet `长度()` returns **39**.

📖 **Byte**: the computer's smallest unit for storing text — one little box.
📖 **Character**: what a human eye sees as "one letter".

💡 **Metaphor**: the computer stores text in UTF-8 encoding — an English letter fits in 1 box, but **a Chinese character needs 3 boxes**. `长度()` counts **boxes (bytes)**, not letters. 13 characters × 3 boxes = 39.

### Why this is dangerous: slicing panics

Chapter 3 taught slices like `&数组[0..3]`. Strings can be sliced too — but slicing counts **bytes**:

```rust
让 句子 = "你好世界";
让 前半 = &句子[0..6];    // first 6 bytes = the first two characters, ✅
打印行!("前半：{}", 前半);
// 预期输出: 前半：你好
```

Output: `前半：你好`

But slice into the **middle of a character**:

```rust
// 预期行为: 运行失败
让 句子 = "你好";
让 半个字 = &句子[0..1];   // ❌ cuts open box 1 of "你"
```

Runtime panic:

```
线程 '主函数' 恐慌：
字节索引 1 不是字符边界；它位于 '你'（字节 0..3）内部
```

("Byte index 1 is not a character boundary; it lies inside '你' (bytes 0..3)" — half a box doesn't assemble into a letter, and Rust would rather panic than hand you garbage.)

> ⚠️ **Rule**: when subscript-slicing a string containing Chinese characters, the boundaries must land in the **gaps between complete characters** (each takes 3 bytes, so 0, 3, 6, 9… are safe seams). Unsure? Use section 9.8's `逐字符()`.

---

## 9.7 Processing: replace, trim, split

```rust
// 替换：把甲段换成乙段，返回新字符串
让 原句 = 字符串::从("猫很可爱");
让 新句 = 原句.替换("猫", "熊猫");
打印行!("{}", 新句);          // 熊猫很可爱

// 裁剪：去掉首尾的空白
让 脏数据 = "  苹果,香蕉  ";
打印行!("[{}]", 脏数据.裁剪());   // [苹果,香蕉]

// 分割：按分隔符切成一段段
对于 项 在 "苹果,香蕉,橘子".分割(",") {
    打印行!("[{}]", 项);
}
```

Output:

```
熊猫很可爱
[苹果,香蕉]
[苹果]
[香蕉]
[橘子]
```

| Method | What it does |
|---|---|
| `替换(旧, 新)` | Replaces every match, returns a new string |
| `裁剪()` | Strips leading and trailing whitespace |
| `分割(分隔符)` | Cuts into pieces, pair with `对于` to process one by one |

> ✨ **裁剪 + 分割** is the golden pair for handling user input: wash off the surrounding whitespace first, then cut by commas/spaces.

---

## 9.8 Walking through text, character by character

To process one letter at a time, use `逐字符()`:

```rust
对于 字 在 "你好呀".逐字符() {
    打印行!("一个字：{}", 字);
}

```
Output:

```
一个字：你
一个字：好
一个字：呀
```

Every character is treated equally — each round yields one complete character, **never splitting a Chinese letter in half**. That's why it's safer than subscript slicing.

---

## 9.9 A complete example: the diary helper

```rust
函数 主函数() {
    // —— 用 格式化! 拼文字 ——
    让 名字 = "小明";
    让 问候 = 格式化!("你好，{}！", 名字);
    打印行!("{}", 问候);

    // —— 可变字符串能长大 ——
    让 可变 日记 = 字符串::从("今天我学了字符串");
    日记.推入('，');
    日记.推入字符串("真好玩！");
    打印行!("日记：{}", 日记);

    // —— 查询 ——
    打印行!("字节长度：{}", 日记.长度());
    让 可变 字数 = 0;
    对于 _ 在 日记.逐字符() {
        字数 = 字数 + 1;
    }
    打印行!("字符个数：{}", 字数);
    打印行!("包含'字符串'吗：{}", 日记.包含("字符串"));

    // —— 裁剪 + 分割 ——
    让 脏数据 = "  苹果,香蕉,橘子  ";
    让 干净数据 = 脏数据.裁剪();
    对于 项 在 干净数据.分割(",") {
        打印行!("[{}]", 项);
    }

    // —— 逐字符 ——
    对于 字 在 "你好呀".逐字符() {
        打印行!("一个字：{}", 字);
    }
}
```

## 9.10 Line by line

- **Lines 3–5**: `格式化!` fills `名字` into the template and returns a **new string** stored in `问候`.
- **Lines 8–10**: `字符串::从` builds a mutable string; `推入` adds a single-quoted character, `推入字符串` adds double-quoted text.
- **Line 14**: `长度()` counts **bytes**: 13 characters × 3 = 39.
- **Lines 15–18**: the way to count **characters** — iterate with `逐字符()`, adding 1 to `字数` per character. Compare with line 14 and you see bytes vs characters clearly.
- **Lines 22–26**: first `裁剪()` washes off the surrounding spaces, then `分割(",")` cuts by commas, and `对于` prints each piece.
- **Lines 29–31**: `逐字符()` correctly yields every character one at a time.

## 9.11 What you should see

```
你好，小明！
日记：今天我学了字符串，真好玩！
字节长度：39
字符个数：13
包含'字符串'吗：真
[苹果]
[香蕉]
[橘子]
一个字：你
一个字：好
一个字：呀
```

The key comparison: **39 bytes versus 13 characters** — the same string, two ways of counting.

---

## 9.12 Common mistakes and how to fix them

### Mistake one: literals can't be modified

Calling `推入` and other modifying methods on `"..."` errors out. **Fix**: `字符串::从("...")` a mutable copy, then edit.

### Mistake two: E0308 type mismatch — expected `字符串`, found `字符串引用`

```rust
// 预期错误: E0308
函数 收下(话: 字符串) { }
收下("你好");   // ❌ a literal is a string reference
```

**Fix**: `收下(字符串::从("你好"))`, or change the parameter to `字符串引用` (recommended for read-only cases — simpler).

### Mistake three: slicing panic (byte position not on a character boundary)

A slice position landing inside a character panics at runtime. **Fix**: use boundaries at multiples of 3 (for Chinese), or switch to `逐字符()`.

### Mistake four: mixing single and double quotes

`推入("！")` errors (a character expected, a text given); `推入字符串('！')` likewise. **Fix**: single quotes for characters, double quotes for text.

### Mistake five: assuming `长度()` counts letters

For text containing Chinese characters, `长度()` counts bytes. Count characters with `逐字符()`.

---

## 9.13 Chapter glossary

| Term | One-line meaning |
|---|---|
| 字符串 | The growable, modifiable text type |
| 字符串引用 | Borrowed, immutable text — string literals are these |
| Character | One letter as a human sees it, in single quotes |
| Byte | The smallest storage unit for text; a Chinese character takes 3 |
| 格式化! | The macro that fills values into a template, building a new string |
| 推入 | Append one character to the end |
| 推入字符串 | Append a run of text to the end |
| 替换 | Replace every match with new content |
| 裁剪 | Strip leading and trailing whitespace |
| 分割 | Cut into pieces by a separator |
| 逐字符 | Iterate one character at a time |
| Boundary | The start/end position of a complete character — slices must land on one |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 9.14 Exercises

> 💪 Try first, then peek.

### Exercise one: the roll caller

Use `格式化!` to build `"第N位同学：名字"` and print three students (N starting from 1).

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 甲 = "小华";
    让 乙 = "小刚";
    让 丙 = "小丽";
    打印行!("{}", 格式化!("第{}位同学：{}", 1, 甲));
    打印行!("{}", 格式化!("第{}位同学：{}", 2, 乙));
    打印行!("{}", 格式化!("第{}位同学：{}", 3, 丙));
}
```

</details>

### Exercise two: counting

Write a program that reports the byte length and the character count of `"你好呀"`.

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 话 = "你好呀";
    打印行!("字节：{}", 话.长度());
    让 可变 个数 = 0;
    对于 _ 在 话.逐字符() {
        个数 = 个数 + 1;
    }
    打印行!("字符：{}", 个数);
}
// 预期输出:
// 字节：9
// 字符：3
```
Output: `字节：9` (3 characters × 3), `字符：3`

</details>

### Exercise three: data washing

Given `"   语文,数学,英语   "`, trim it, split by commas, and print each subject wrapped in 《》.

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 脏数据 = "   语文,数学,英语   ";
    对于 项 在 脏数据.裁剪().分割(",") {
        打印行!("《{}》", 项);
    }
}
// 预期输出:
// 《语文》
// 《数学》
// 《英语》
```

Output: three lines — `《语文》`, `《数学》`, `《英语》`. (`脏数据.裁剪().分割(",")` demonstrates **method chaining**: the previous method's result feeds straight into the next.)

</details>

### Exercise four: the word filter

Use `替换()` to swap `"垃圾"` for `"有趣"` in `"这个游戏真垃圾"`, printing before and after.

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 原话 = 字符串::从("这个游戏真垃圾");
    让 净化 = 原话.替换("垃圾", "有趣");
    打印行!("原来：{}", 原话);
    打印行!("净化：{}", 净化);
}
```

</details>

---

## 9.15 FAQ

**Q: Why does Rust insist on two text types — wouldn't one do?**

Division of labor saves resources. The stone (literals) is carved into the program at compile time — no warehouse rent; the notebook (strings) is flexible but costs upkeep. Read-only uses take references at zero cost; you build a string only when processing is needed.

**Q: How does `字符串引用` relate to Chapter 8's "references"?**

`字符串引用` is one kind of **reference** — a read-only borrow of a run of text. Chapter 8's `&字符串` is "a reference to a string"; `字符串引用` is "a reference to the text itself". They cooperate: a `&字符串` passed where a `字符串引用` is expected usually works too (automatic conversion).

**Q: How do I turn a number into text?**

`格式化!("{}", 数字)` is the universal answer. Chapter 15 (collections) and Chapter 16 (error handling) will also introduce `解析()` (text → number) in more depth.

**Q: Slicing is that dangerous — should I still learn it?**

Yes. For pure-ASCII text and protocol data, slicing is fast and everywhere. One rule only: **boundaries must land on character seams**. For Chinese text, when in doubt, use `逐字符()`.

**Q: How many bytes does a character like `'你'` occupy?**

The character type itself is fixed at 4 bytes (room for any character in the world); but when stored as UTF-8 text, "你" takes 3 bytes. Beginners needn't fuss over this detail.

**Q: Can the pieces from `分割` be stored for later?**

They are all **borrows** of the original string (string references) — usable only while the original lives. For long-term storage, convert them into independent strings (a trick from upcoming chapters). For now, learn "process while splitting".

---

## What's next

So far, data has been "loose": a name here, an age there, a score somewhere else… In the real world they all belong to **one record card**.

**Chapter 10, "Structs"**, teaches you to pack related data together:

- Define your own type: `结构体 学生 { ... }`;
- Build instances, read fields, modify fields;
- Fit it with methods using an `实现` block (Chapter 6's setup finally pays off);
- `#[派生(调试)]` makes a struct directly printable.

See you in the next chapter!
