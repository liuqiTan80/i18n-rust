# Chapter 7: Ownership

## 7.0 Learning goals

By the end of this chapter, you will be able to:

1. State **the three rules of ownership**;
2. Explain what happens to a value when its **scope** ends;
3. Tell **copying** (small data like integers) from **moving** (heap data like strings);
4. Read and fix an **E0382** ("value used after move") error;
5. Use **`克隆()`** (clone) to make an independent duplicate;
6. Say where ownership goes when a value **enters a function** or **returns from one**.

> ✨ This is Rust's most distinctive chapter — most other languages have nothing like it. Not understanding it on the first read is normal. Read it twice, run every example, and it will click.

---

## 7.1 Why ownership exists: who does the cleaning

💡 **Story**: the school organizes a big cleanup. Without **assigned duties**, two disasters happen:

1. **Nobody cleans**: some corner belongs to no one, and the trash piles up — in programs this is a **memory leak** (memory requested but never returned; memory usage grows until everything grinds to a halt);
2. **Two people clean the same spot**: one just wiped it, the other "fixes" it their own way — chaos. In programs this is a **data race** (covered properly in the multithreading chapters).

Rust's solution is blunt and simple: **everything must have exactly one owner. The owner is responsible; when the owner leaves, the thing is cleaned up automatically.**

📖 **Ownership**: Rust's core rule — every value has exactly one "owner" (a variable), and when the owner leaves the scene, the value is released automatically.

This is why Rust guarantees memory safety without a "garbage collector" (the automatic cleaner other languages hire): **everyone is responsible, and responsibility lands on exactly one person**.

---

## 7.2 The three rules of ownership

Memorize these three and half the chapter is won:

| # | Rule | Plain words |
|---|---|---|
| Rule 1 | Every value has an **owner** (a variable) | Everything has an owner |
| Rule 2 | At any moment there is **only one** owner | One thing can't have two owners |
| Rule 3 | When the owner leaves the **scope**, the value is **released automatically** | When the owner leaves, the thing gets cleaned up |

📖 **Scope**: the stretch of code where a name is "valid" — usually one pair of curly braces `{ }`. Like the playground: it's where you're allowed to act — step off it, and playground business is no longer yours.

---

## 7.3 Scope: the life of a value

Look at this code:

```rust
函数 主函数() {
    {
        让 临时 = 字符串::从("只活在块里");
        打印行!("块内：{}", 临时);
    }
    // 临时 在这里已经不存在了
    打印行!("块外继续");
}
```

Output:

```
块内：只活在块里
块外继续
```

`临时` is born inside the inner braces — and the moment those braces end, it **ceases to exist**. Its memory is returned automatically. Use `临时` outside the block and the compiler reports "can't find this variable".

> 💡 **Metaphor**: a value's life is a hotel stay: **check in** (declaration) → use → **check out** (leaving the scope). At check-out the room is cleaned automatically (memory released) — nothing to worry about, and nobody can overstay.

> 📖 **Release**: the cleanup executed automatically when a value leaves its scope — the memory goes back to the system.

---

## 7.4 Two storage places: the pocket and the warehouse

Why does some data "copy" while other data "moves"? First, meet the two places data lives:

📖 **The stack**: a fast, small memory region. Like a **pocket** — easy to reach into, but nothing bulky fits.
📖 **The heap**: a slower, large memory region. Like a **warehouse** — it holds big things, but fetching requires a trip and some paperwork.

| Place | Holds | Metaphor |
|---|---|---|
| Stack (pocket) | Integers, floats, booleans, characters — small fixed-size things | Keys, an eraser |
| Heap (warehouse) | Strings, vectors — big things of varying size | A whole crate of books |

A `字符串` (string)'s contents live on the **heap**: the row of letters sits in the warehouse, and the variable itself (in your pocket) holds only a **claim ticket** (recording the warehouse location and length).

With that picture, copy and move below make perfect sense.

---

## 7.5 Copy: pocket things duplicate for free

```rust
让 甲 = 5;
让 乙 = 甲;
打印行!("甲：{}，乙：{}", 甲, 乙);
// 预期输出: 甲：5，乙：5
```

Output: `甲：5，乙：5`

Integers live in the **pocket**; duplicating one costs nothing. So `让 乙 = 甲` is a **copy**: 甲 still exists, and 乙 is an independent duplicate. Both variables are free to use.

📖 **Copy**: the property of being automatically duplicated bit by bit when a value leaves its scope. Integers, floats, booleans and characters all have it.

**Which types copy?** Remember the mantra: **"numbers, booleans, single characters"** — integers (`整数`, `长整数`…), floats (`浮点数`), booleans (`真`/`假`), characters. All pocket-dwellers; assignment duplicates them.

---

## 7.6 Move: there's only one claim ticket

A string lives in the **warehouse**; the variable holds only the claim ticket. Now:

```rust
让 句子甲 = 字符串::从("你好");
让 句子乙 = 句子甲;
打印行!("句子乙：{}", 句子乙);
```

`让 句子乙 = 句子甲` is not a copy — it's a **move**: the claim ticket passed from 句子甲's hand to 句子乙's. There's only one ticket — the new owner is 句子乙.

💡 **Metaphor**: a movie ticket handed to a friend leaves your hand empty. Want in? Either your friend gives it back, or you buy another (the clone in 7.7).

If you try the old variable name after the move:

```rust
让 句子甲 = 字符串::从("你好");
让 句子乙 = 句子甲;
打印行!("句子甲：{}", 句子甲);   // ❌
```

The compiler reports, mercilessly:

```
错误[E0382]: 值在移动后被使用：`句子甲`
📌 变量 `句子甲` 在第 3 行被移动，第 4 行尝试再次使用。
💡 Rust 中值被移动后不能再使用。考虑使用 `引用`（&）或 `克隆()` 来避免移动。
```

📖 **Move**: handing a heap value's ownership from one variable to another; the original variable dies instantly.

> 📖 **E0382**: the error code for "value used after move". Seeing it means: some value changed owners, and you're still asking the old owner for it.

**Why is Rust so stingy?** Copying a whole crate of books (possibly hundreds of megabytes) slows the program; two people sharing one claim ticket means double-cleaning the same hotel room at check-out — an instant crash. Rust picks the cleanest scheme: **one thing, one owner; give it away and it's gone**.

---

## 7.7 Clone: buy another ticket

Want two independent copies? Use **`克隆()`** ("clone, duplicate"):

```rust
让 原件 = 字符串::从("藏宝图");
让 复制件 = 原件.克隆();
打印行!("原件：{}，复制件：{}", 原件, 复制件);
// 预期输出: 原件：藏宝图，复制件：藏宝图
```

Output: `原件：藏宝图，复制件：藏宝图`

Clone **truly duplicates what's in the warehouse** — two claim tickets, each pointing at its own warehouse. Change one, the other is untouched.

> ⚠️ **Careful**: cloning takes time (photocopying the whole crate). If a move solves it, don't clone. At the beginner stage, clone freely — performance tuning comes later.

**Comparison table to remember**:

| Operation | Pocket data (integers etc.) | Warehouse data (strings etc.) |
|---|---|---|
| `让 乙 = 甲` | Copy — both usable | Move — 甲 dies |
| `让 乙 = 甲.克隆()` | Also clones (unnecessary) | Copy — both usable |

---

## 7.8 Ownership and functions: transfers at both doors

Values **entering a function** and **returning from one** transfer ownership the same way.

### Passing in: the value moves into the function

```rust
函数 收下(句子: 字符串) {
    打印行!("函数里收到：{}", 句子);
}

函数 主函数() {
    让 祝福 = 字符串::从("生日快乐");
    收下(祝福);
    // 祝福 已经搬进函数，这里不能再用
}
```

`收下(祝福)` transfers the whole blessing to the function's parameter `句子`. When the call ends, `句子` leaves its scope and the value is released. `祝福` in the main function is dead from then on.

### Returning: the value moves back to the caller

```rust
函数 制作问候(名字: 字符串引用) -> 字符串 {
    字符串::从(名字)
}

函数 主函数() {
    让 问候 = 制作问候("小明");
    打印行!("拿到：{}", 问候);
}
// 预期输出: 拿到：小明
```

Output: `拿到：小明`

The string built inside the function is transferred via `返回` to `问候` — its new owner.

> 💡 **Metaphor**: a function is a parcel station. Passing in = you **mail the parcel in** (it leaves your hands); returning = the station **mails a new parcel out to you** (it's yours from then on).

> ✨ **You may find this annoying right now: can't I just lend things?** You can! Chapter 8, *References and Borrowing*, is entirely about "how to lend" — lend, return, no ownership transfer.

---

## 7.9 A complete example: the parcel station

All the ownership knowledge, strung into a "mailing a parcel" story:

```rust
// 派生(克隆)：让结构体自动获得 克隆 能力
// 派生括号里的特征名可以用中文（克隆）
#[派生(克隆)]
结构体 包裹 {
    内容: 字符串,
    重量: 整数,
}

实现 包裹 {
    函数 创建(内容: 字符串, 重量: 整数) -> 包裹 {
        包裹 { 内容: 内容, 重量: 重量 }
    }

    函数 介绍(&自我) {
        打印行!("包裹：{}，{}克", 自我.内容, 自我.重量);
    }
}

// 签收 = 把包裹的所有权收进来，签收完包裹就"用掉"了
函数 签收(件: 包裹) {
    件.介绍();
    打印行!("已签收！");
}

函数 主函数() {
    // 造一个包裹
    让 甲 = 包裹::创建(字符串::从("玩具"), 500);
    甲.介绍();

    // 克隆一份寄出，原件自己留着
    让 副本 = 甲.克隆();
    签收(副本);

    // 原件还在，继续用
    甲.介绍();

    // 最后把原件也寄出去
    签收(甲);
    // 从此 甲 失效
}
```

---

## 7.10 Line by line

- **Line 3**: `#[派生(克隆)]` is a "sticker" (formally an **attribute**) telling the compiler to generate a `克隆()` method for `包裹` automatically. The parentheses accept native trait names (`克隆`, `调试`) or the English originals.
- **Lines 4–7**: the struct `包裹` has two fields. `内容` is a `字符串` (lives in the warehouse); `重量` is an integer (lives in the pocket).
- **Lines 10–13**: the associated function `创建`, returning a new parcel (ownership handed to the caller).
- **Lines 15–17**: `介绍` takes `&自我` — **only borrows**, a look but no take, so the parcel survives the call (borrowing details in Chapter 8).
- **Lines 21–24**: `签收`'s parameter type is `包裹` (no `&`) — the incoming parcel's ownership belongs to `件`, and when the function ends, `件` is released. The sender has lost this parcel.
- **Line 28**: `字符串::从("玩具")` builds a string, owned by `甲` along with the parcel.
- **Line 32**: `甲.克隆()` duplicates a fully independent `副本` (the inner string included).
- **Line 33**: `签收(副本)` — the copy is collected and released.
- **Line 36**: `甲` is intact and can still introduce itself.
- **Line 39**: `签收(甲)` — the original is collected too. Writing `甲.介绍()` after this line would be an E0382.

---

## 7.11 What you should see

```
包裹：玩具，500克
包裹：玩具，500克
已签收！
包裹：玩具，500克
包裹：玩具，500克
已签收！
```

Mapping it: line 1 is the self-introduction after creation; lines 2–3 are the cloned copy being signed for; lines 4–6 are the original introducing itself and then being signed for too. The two parcels hold identical contents — but they are **two independent values**.

---

## 7.12 Common mistakes and how to fix them

### Mistake one: E0382, used after move

```
错误[E0382]: 值在移动后被使用：`句子甲`
💡 Rust 中值被移动后不能再使用。考虑使用 `引用`（&）或 `克隆()` 来避免移动。
```

**Three repair routes**:

1. **Just want to look** → pass a reference, `&句子甲` (Chapter 8);
2. **Both sides need a copy** → `句子甲.克隆()`;
3. **Truly giving it to one person** → reorder the code so the last use comes before the move.

### Mistake two: applying integer behavior to strings

"Why can't I use `甲` anymore? `乙 = 甲` worked fine with integers!" — because integers **copy**, strings **move**. Revisit 7.4's mantra: only "numbers, booleans, single characters" copy.

### Mistake three: cloned, but "method not found"

Calling `.克隆()` on your own struct errors because structs **don't** clone by default. Add the sticker `#[派生(克隆)]` before the struct definition.

### Mistake four: a misspelled derived trait

`#[派生(克降)]` reports "derive macro not found" — careful not to typo `克隆` into look-alike characters. Supported native trait names: `克隆`, `调试`, `复制`, `哈希` and more.

---

## 7.13 Chapter glossary

| Term | One-line meaning |
|---|---|
| Ownership | Every value has exactly one owner; when the owner leaves, the value is released |
| Scope | The stretch of code where a name is valid — usually one pair of braces |
| Release | The automatic memory cleanup when a value leaves its scope |
| Stack | The fast, small memory region for fixed-size data — the pocket |
| Heap | The large, slower memory region for variable-size data — the warehouse |
| Copy | How stack data assigns — both sides keep an independent copy |
| Move | How heap data assigns — the claim ticket handed over, the original variable dies |
| Clone | Manually duplicating the full heap copy |
| E0382 | The error code for "value used after move" |
| Attribute | A "sticker" on code, written `#[...]`, giving the compiler extra instructions |
| Derive | Having the compiler generate trait implementations automatically, like `#[派生(克隆)]` |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 7.14 Exercises

> 💪 Try first, then peek.

### Exercise one: who can still be used

Decide which variables are still usable after each line (write your answers on paper first):

```rust
让 甲 = 10;
让 乙 = 甲;
让 丙 = 字符串::从("你好");
让 丁 = 丙;
让 戊 = 丁.克隆();
```

<details>
<summary>🔍 View answer</summary>

- `甲`: usable (integers copy);
- `乙`: usable;
- `丙`: **not usable** (moved to 丁);
- `丁`: usable;
- `戊`: usable (an independent clone).

</details>

### Exercise two: fix the E0382

This code errors — fix it **two different ways** (hint: one uses clone, one reorders the uses):

```rust
// 预期错误: E0382
函数 主函数() {
    让 祝福 = 字符串::从("新年好");
    让 备份 = 祝福;
    打印行!("{}", 祝福);
}
```

<details>
<summary>🔍 View answer</summary>

Way one: clone.

```rust
让 备份 = 祝福.克隆();
```

Way two: print before moving.

```rust
让 祝福 = 字符串::从("新年好");
打印行!("{}", 祝福);
让 备份 = 祝福;
```

</details>

### Exercise three: a parcel between functions

Write a function `打包` (takes an integer weight, returns a parcel) and a function `拆开` (takes a parcel, prints its contents). In the main function, pack a "积木" (building block), unpack it, then try to use the original variable — and watch the compiler remind you.

<details>
<summary>🔍 View answer</summary>

```rust
结构体 包裹 {
    内容: 字符串,
    重量: 整数,
}

函数 打包(重量: 整数) -> 包裹 {
    包裹 { 内容: 字符串::从("积木"), 重量: 重量 }
}

函数 拆开(件: 包裹) {
    打印行!("拆出了：{}", 件.内容);
}

函数 主函数() {
    让 礼物 = 打包(800);
    拆开(礼物);
    // 下一行会报 E0382：礼物已经过户给 拆开 的参数了
    // 打印行!("{}", 礼物.重量);
}
// 预期输出: 拆出了：积木
```

Output: `拆出了：积木`

</details>

---

## 7.15 FAQ

**Q: Is the ownership check at runtime or compile time?**

**Compile time**. The compiler traces every ownership transfer before you run; non-compliant programs don't even compile. So Rust programs carry no ownership-checking cost at runtime — the "zero cost" philosophy in action.

**Q: Do integers really never move?**

Correct. Integers, floats, booleans and characters — "pocket data" — always copy on assignment. Later you'll learn the precise rule: types implementing the `复制` (Copy) trait all copy.

**Q: Is the string literal `"你好"` a move too?**

A literal is a **string reference** (`&文本`) — a "borrow". Borrowed things only copy the borrow slip on assignment; both sides stay usable. What truly moves is the `字符串` type, like what `字符串::从("你好")` builds. Chapter 9 draws the line precisely.

**Q: What's the real difference between clone and copy?**

Copy is **automatic and cheap** (the compiler photocopies a few pocket bytes); clone is **manual and pricier** (a full warehouse photocopy). Types that copy can also clone — but needn't; types that move must clone if you want two.

**Q: Won't thinking about ownership for every variable be exhausting?**

At first, yes — it's the biggest gate in learning Rust. The good news: after two or three chapters of projects, "who owns whom" becomes intuition. And the compiler is a free partner — the moment you forget, it reminds you, with a fix suggestion attached.

**Q: Why don't other languages (Python, JavaScript) have any of this?**

Most of them use a "garbage collector": a background cleaner hired while the program runs, patrolling and sweeping up unused values. Convenient — but you pay runtime performance costs and unpredictability. Rust solves it at compile time, so at runtime it's fast and steady.

---

## What's next

Ownership is safe, but "give it away and it's gone" is inconvenient — must you transfer ownership just to let someone *look* at your data?

**Chapter 8, "References and Borrowing"**, teaches Rust's lending rules:

- A **reference** `&`: borrow to look; the owner is untouched;
- A **mutable reference** `可变引用`: borrow to modify — with strict limits;
- **One iron rule**: at any moment, either one mutable borrow or any number of read-only borrows;
- Why that rule prevents data from fighting.

See you in the next chapter!
