# Chapter 8: References and Borrowing

## 8.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain the relationship between a **reference** and **borrowing** (one is a thing, the other an action);
2. Make read-only borrows with **`&`** and mutable borrows with **`可变引用`**;
3. State the borrowing iron rule: "**either one mutable, or many read-only**";
4. Read and fix **E0502** (borrowing conflict) and **E0596** (cannot borrow as mutable);
5. Use **`*`** to dereference — take the original value out of the borrow slip;
6. Know that "a borrow lives until its last use".

---

## 8.1 The problem left by last chapter

Chapter 7 said: a string passed into a function **moves**, and the original variable dies. But often we only want the function to **take a look** — not to give the thing away:

```rust
函数 展示(句子: 字符串) {   // pass by value: 句子 moves in, destroyed after use
    打印行!("{}", 句子);
}

函数 主函数() {
    让 问候 = 字符串::从("你好");
    展示(问候);
    // 问候 没了，想再用？没门
}
```

Confiscating the original after one look is far too bossy. The real-world solution is — **lend it**.

## 8.2 References: a borrow slip

💡 **Metaphor**: a classmate wants to read your novel. You don't **transfer ownership** of the book — you say "borrow it". What they hold is a **borrow slip** — it leads to the book, but the book is still yours.

📖 **Reference**: a "borrow slip" pointing at some value, written `&value`. `&` reads "take a reference". A reference is not the value itself; it only records "where the value lives".
📖 **Borrowing**: the **act** of using someone else's data temporarily through a reference. Borrowed things come back — a reference never changes ownership.

### The read-only borrow

```rust
// 参数类型 &字符串：借来看看
函数 展示(句子: &字符串) {
    打印行!("{}", 句子);
}

函数 主函数() {
    让 问候 = 字符串::从("你好");
    展示(&问候);     // hand over a slip
    展示(&问候);     // lend again — fine
    打印行!("还在：{}", 问候);   // the original is intact
}
```

Output:

```
你好
你好
还在：你好
```

`&问候` mints a slip and hands it to the function. The function reads, the slip expires, and `问候` is still the owner. Lend as many times as you like.

## 8.3 The mutable borrow: borrow to modify

Sometimes you don't just want to look — you want to **change** things. Like writing notes in a book's margins. That takes the **mutable borrow**, `可变引用`:

```rust
函数 加批注(书: 可变引用 字符串, 批注: 字符串引用) {
    书.推入字符串(批注);   // 推入字符串：往末尾加文字
}

函数 主函数() {
    让 可变 书 = 字符串::从("三国演义");
    加批注(可变引用 书, "（青少版）");
    打印行!("{}", 书);
}
// 预期输出: 三国演义（青少版）
```

Output: `三国演义（青少版）`

Two new requirements:

| Requirement | Spelling | Why |
|---|---|---|
| The borrowed value must be declared mutable | `让 可变 书 = ...` | An immutable thing can't be lent out for modification |
| Write `可变引用` when lending | `加批注(可变引用 书, ...)` | It declares "this is a mutable borrow" |

> 📖 **推入字符串** (push_str): a string method appending text to the end. Its sibling `推入` (push) appends a single character.

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
让 可变 口号 = 字符串::从("加油");
让 看一眼 = &口号;        // read-only borrow active
让 改一下 = 可变引用 口号;    // ❌ a mutable borrow at the same time
打印行!("{}", 看一眼);     // 看一眼 is still used after this — the borrow must live to here
```

The real error:

```
错误[E0502]: 无法借用 `口号` 为可变，因为它同时也被不可变借用
📌 变量 `口号` 在第 4 行被借用，第 6 行仍在被使用。
💡 Rust 不允许同时存在可变借用和不可变借用。尝试缩小不可变借用的作用范围。
```

### Why so strict?

Suppose "reading while writing" were allowed: function A holds a slip and reads the string, while function B suddenly appends text — the string may need to **move to a bigger room** in the warehouse (reallocating memory) — and A's slip instantly points at an **already-vacated** room. Crash. The iron rule kills this accident class at compile time.

### Borrowing one after another is fine

The rule governs "**at the same time**". Once the previous borrow is finished, the next one comes — perfectly legal:

```rust
让 可变 书 = 字符串::从("三国演义");
展示(&书);                    // first read-only borrow, expired after use
加批注(可变引用 书, "（青少版）"); // mutable borrow, expired after use
展示(&书);                    // another read-only borrow, no problem
```

### A borrow's "lifetime"

A borrow's lifetime = **from birth to its last use**. Once used up, the iron rule no longer applies to it:

```rust
让 可变 口号 = 字符串::从("加油");
让 看一眼 = &口号;
打印行!("{}", 看一眼);    // 看一眼 last used here
// 看一眼's lifetime ends here
让 改一下 = 可变引用 口号;    // ✅ fine: the read-only borrow has expired
改一下.推入('!');
```

> ✨ This is the compiler being clever: it doesn't care how long you "kept" the slip — only where you **actually used it**.

## 8.5 No modifying through a read-only borrow: E0596

The parameter is declared `&字符串` (read-only), yet you try to modify it inside the function — that's **E0596**:

```rust
// 预期错误: E0596
函数 乱改(书: &字符串) {
    书.推入('!');    // ❌
}
```

The real error:

```
错误[E0596]: 无法借用为可变
💡 只有可变变量才能被可变借用（`可变引用`）。
```

**Fix**: to modify, change the parameter to `可变引用 字符串`, and make sure the caller's variable is `让 可变`.

## 8.6 Dereferencing: taking the original out of the slip

📖 **Dereference**: take the value a reference points to — the symbol `*`.

```rust
让 分数 = 95;
让 借据 = &分数;
让 抄一份 = *借据;      // * follows the slip to the original, copies it out
打印行!("抄出来的值：{}", 抄一份);
// 预期输出: 抄出来的值：95
```

Output: `抄出来的值：95`

`&` "writes a slip"; `*` "takes goods by slip" — opposite directions.

> ✨ Everyday code rarely dereferences by hand — when you call methods, the compiler follows the chain automatically (`(&书).长度()` is simply written `书.长度()`). Knowing `*` exists is enough to read error messages.

## 8.7 Can you borrow an integer?

Yes. References and copying don't conflict:

```rust
让 分数 = 95;
让 借据 = &分数;
打印行!("借据指向：{}", 借据);
打印行!("原值还在：{}", 分数);   // integers copy anyway — all is well
```

But integers are small and copy anyway — pass them by value. References truly shine with the "big items": **strings, vectors, structs**.

---

## 8.8 A complete example: the library lending system

Read-only borrows, mutable borrows, many-at-once, one-after-another — all in one story:

```rust
// 只读借用：看看就还
函数 展示(书: &字符串) {
    打印行!("书的内容：{}", 书);
}

// 可变借用：修改后归还
函数 加批注(书: 可变引用 字符串, 批注: 字符串引用) {
    书.推入字符串(批注);
}

// 两个只读借用同时存在：没问题
函数 比较长短(甲: &字符串, 乙: &字符串) {
    如果 甲.长度() > 乙.长度() {
        打印行!("甲更长");
    } 否则 {
        打印行!("乙不短");
    }
}

函数 主函数() {
    让 可变 书 = 字符串::从("三国演义");
    展示(&书);                       // 只读借用
    加批注(可变引用 书, "（青少版）");    // 可变借用
    展示(&书);                       // 只读借用

    让 另一本 = 字符串::从("西游记");
    比较长短(&书, &另一本);           // 两个只读借用并行

    打印行!("原书还在：{}", 书);
}
```

## 8.9 Line by line

- **Lines 2–4**: `展示` takes `&字符串` — borrow, don't take. The original is untouched after the call.
- **Lines 7–9**: `加批注` takes `可变引用 字符串`; `推入字符串` appends text at the end.
- **Lines 12–18**: `比较长短` holds two read-only borrows at once — the iron rule allows "many readers in parallel". `长度()` returns the **byte count** (one Chinese character takes 3 bytes; "三国演义" is 12).
- **Line 21**: `让 可变` — a mutable borrow comes later, so mutability must be declared.
- **Lines 22–24**: three borrows happen **one after another**; each expires after use — no conflicts.
- **Line 26**: two "books" borrowed read-only at the same time.
- **Line 28**: every borrow returned; the original book is intact and still printable.

## 8.10 What you should see

```
书的内容：三国演义
书的内容：三国演义（青少版）
甲更长
原书还在：三国演义（青少版）
```

"三国演义（青少版）" has more bytes than "西游记", hence "甲更长".

---

## 8.11 Common mistakes and how to fix them

### Mistake one: E0502 borrow conflict (mutable + read-only at once)

```
错误[E0502]: 无法借用 `口号` 为可变，因为它同时也被不可变借用
```

**Fix**: stagger the two borrows — finish the read-only business first (use it up), then start the mutable borrow; or simply clone a copy and edit that.

### Mistake two: E0596 modifying through a read-only borrow

```
错误[E0596]: 无法借用为可变
```

**Fix**: check three places at once — ① is the original variable `让 可变`; ② does the call site write `可变引用`; ③ is the function's parameter type `可变引用 类型`? All three must hold.

### Mistake three: forgot `让 可变`

Writing `可变引用 甲` on an immutable variable also errors with E0596. **Fix**: add `可变` at the declaration.

### Mistake four: a reference outliving its owner

```rust
// 预期错误: E0597
让 借据;
{
    让 临时 = 字符串::从("短命");
    借据 = &临时;
}
打印行!("{}", 借据);   // ❌ 临时 is gone — the slip is now a bounced check
```

The error complains the "temporary value was released too early". **Fix**: a slip's lifetime must not exceed the original's. Let the original outlive the slip.

> 📖 **Dangling reference**: a reference pointing at vanished data. Rust forbids it at compile time — the core guarantee of reference safety.

---

## 8.12 Chapter glossary

| Term | One-line meaning |
|---|---|
| Reference | A "borrow slip" pointing at a value, written `&值` |
| Borrowing | Using someone else's data temporarily through a reference, ownership unchanged |
| Read-only borrow | `&值` — many may exist at once |
| Mutable borrow | `可变引用 值` — only one at any moment |
| Borrowing iron rule | Either many read-only, or one mutable — never both |
| Dereference | Taking the original value out of a reference with `*` |
| E0502 | The error code for a mutable/read-only borrow conflict |
| E0596 | The error code for failing to borrow as mutable |
| Dangling reference | A reference to vanished data (forbidden at compile time in Rust) |
| 推入字符串 | The method appending text to a string's end |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 8.13 Exercises

> 💪 Try first, then peek.

### Exercise one: the read-only borrow

Write a function `报长度(话: &字符串)` that prints "这句话N个字节". Call it twice on the same string in the main function, proving the original wasn't taken away.

<details>
<summary>🔍 View answer</summary>

```rust
函数 报长度(话: &字符串) {
    打印行!("这句话{}个字节", 话.长度());
}

函数 主函数() {
    让 话 = 字符串::从("好好学习");
    报长度(&话);
    报长度(&话);
    打印行!("原值：{}", 话);
}
```

Output ("好好学习" is 12 bytes in total):

```
这句话12个字节
这句话12个字节
原值：好好学习
```

</details>

### Exercise two: the mutable borrow

Write a function `翻倍话(话: 可变引用 字符串)` that appends the string to its own end (hint: clone first, then push — otherwise you'll trip over borrowing from yourself).

<details>
<summary>🔍 View answer</summary>

```rust
函数 翻倍话(话: 可变引用 字符串) {
    让 副本 = 话.克隆();
    话.推入字符串(&副本);
}

函数 主函数() {
    让 可变 口号 = 字符串::从("加油");
    翻倍话(可变引用 口号);
    打印行!("{}", 口号);
}
// 预期输出: 加油加油
```

Output: `加油加油`

(Writing `话.推入字符串(话)` directly is a borrow conflict: `话` is mutably borrowed on one side and read on the other. Cloning first dodges it.)

</details>

### Exercise three: true or false

Which of these lines compile?

```rust
让 可变 笔记 = 字符串::从("数学");
让 甲 = &笔记;
让 乙 = &笔记;
让 丙 = 可变引用 笔记;
```

<details>
<summary>🔍 View answer</summary>

The `甲` and `乙` lines are fine (many read-only borrows in parallel); the `丙` line reports E0502 — because if `甲` and `乙` are used again later, they conflict with the mutable borrow. If everything before `丙` is "used up and expired" (they never appear again), the compiler lets it through.

</details>

---

## 8.14 FAQ

**Q: Is `&字符串` the same as Chapter 7's `字符串引用` (`&文本`)?**

Siblings, not twins. `&字符串` is "a reference to a `字符串`"; `字符串引用` is "a borrow of the text itself". A string literal `"你好"` is a `字符串引用`. Both "read text read-only", and most methods work on either. Chapter 9 draws the line and shows the conversions.

**Q: Why can't there be two mutable borrows?**

Both claim "I will modify" — different orders of modification produce different results, and the data turns to chaos. Keeping exactly one mutable borrow makes "who changed what" perfectly clear.

**Q: Does borrow checking cost runtime speed?**

No. It all happens at compile time; runtime carries zero overhead. Rust's "zero-cost safety".

**Q: I keep mixing up the borrowing rules. A mnemonic?**

One sentence: **"reads may crowd, writes must be alone, and readers and writers never share a room."**

**Q: Should function parameters take values or borrows?**

Rule of thumb — **look: `&`; modify: `可变引用`; truly take: pass by value**. For small things like integers, passing by value is fine too (copying is cheap).

**Q: Are a method's `&自我` / `可变引用 自我` the same as this chapter's `&` / `可变引用`?**

Exactly the same. A method's `&自我` means "this method borrows the instance read-only"; `可变引用 自我` means "borrows it mutably". You should now understand Chapter 6 completely.

---

## What's next

We still owe "text" its own chapter: why can't string literals be modified? What's the real relationship between `字符串` and `字符串引用`? How do you join, cut and search text?

**Chapter 9, "Strings and Text"**, covers it all:

- A thorough comparison of the two text types;
- Joining, appending, slicing, searching;
- The trap of Chinese characters vs byte lengths;
- The right way to walk through text.

See you in the next chapter!
