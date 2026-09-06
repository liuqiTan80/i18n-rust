# Chapter 6: Functions and Methods

## 6.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain what a function is using the **magic box** picture;
2. Write your own functions **with parameters and return values**;
3. Tell **statements** and **expressions** apart, and know how semicolons "swallow" return values;
4. Hand back a result early with the **`返回`** (return) keyword;
5. Write a **recursive** function (one that calls itself);
6. Attach **methods** to a type with an **`实现`** (impl) block, and recognize the three forms of **`自我`** (self);
7. Meet the **unit type** `()`.

---

## 6.1 What is a function: the magic box

💡 **Metaphor**: imagine a **magic box**:

- You push things in through the **top opening** (these are **parameters**);
- The inside of the box does its work (this is the **function body**);
- Then it spits the result out of the **bottom opening** (this is the **return value**).

Take a "juicer" box: push in apples (parameters), it squeezes inside (the body), and out comes apple juice (the return value).

📖 **Function**: a named, reusable set of instructions — written **`函数`** in the native dialect.

You've been using functions all along — `打印行!` is one (a special "macro" function; Chapter 2 explained the exclamation mark). What you'll learn now is **building boxes of your own**.

### Why functions exist

Suppose you need to print the same star pattern in three places. Without functions, you'd **copy the code three times**. Copying has two costs:

1. **Effort**: writing it three times wastes time;
2. **Fragility**: to change the pattern you must edit three places — miss one and the program breaks (jargon: a "bug").

With a function: put the pattern code in a box, give it a name. To use it, **call the name**; to change it, edit that one copy.

> 💡 **One line**: a function = a named code block, for **less copying and easier editing**.

---

## 6.2 The simplest function

Start with one that takes nothing and returns nothing:

```rust
函数 打招呼() {
    打印行!("你好！");
}

函数 主函数() {
    打招呼();
    打招呼();
}
```

What you should see:

```
你好！
你好！
```

Five parts, taken apart:

| Part | Example | Notes |
|---|---|---|
| The `函数` keyword | `函数` | Tells the computer "a box is being built here" |
| Function name | `打招呼` | The box's name — you choose it (verbs recommended) |
| Parentheses | `()` | Where parameters go; none for now, so empty |
| Curly braces | `{ }` | The box's "body" — the actual instructions |
| The call | `打招呼();` | Name + parentheses + semicolon: make the box work |

> ⚠️ **Careful**: **defining** a function (building the box) and **calling** it (making the box work) are two different things. Define without calling and the box never starts. A program's entrance is always `主函数`.

---

## 6.3 Parameters: pushing things into the box

📖 **Parameter**: the input a function receives — the name written when defining it.
📖 **Argument**: the actual value pushed in when calling it.

💡 **Metaphor**: the juicer's manual says "please insert **fruit**" — "fruit" is the **parameter** (a placeholder name); the actual apple you push in is the **argument**.

### One parameter

```rust
函数 问候(名字: 字符串引用) {
    打印行!("你好，{}！", 名字);
}

函数 主函数() {
    问候("小明");
    问候("小红");
}
```

> 📖 **字符串引用** (string reference): borrowed text, written as a `"..."` literal. Chapter 9 covers it — for now: text in double quotes belongs to this type.

Parameters are written **`name: type`** — Rust requires a **type for every parameter**, no exceptions.

### Multiple parameters

Separate them with **commas**:

```rust
函数 求和(甲: 整数, 乙: 整数) -> 整数 {
    甲 + 乙
}

函数 主函数() {
    让 和 = 求和(3, 7);
    打印行!("和：{}", 和);
}
// 预期输出: 和：10
```

Output: `和：10`

Note the new symbol **`->`** (minus + greater-than, meaning "points to"):

- `-> 整数` says "what this box spits out is an integer";
- No `->` means the box spits out nothing (strictly speaking, it spits out "empty" — section 6.8).

> ⚠️ **Careful**: the arguments at call time must **match the parameters in count and order**. `求和(3)` is one short; `求和("三", 7)` has the wrong type — both error out.

---

## 6.4 Return values: the box hands out a result

Three ways for a function to hand over a result.

### Way one: the trailing expression (no semicolon)

If the **last line** of the function body is an **expression** (something that evaluates to a value) and **carries no semicolon**, its value is the return value:

```rust
函数 平方(数: 整数) -> 整数 {
    数 * 数
}
```

`数 * 数` has no semicolon → its value is "spat out".

### Way two: the `返回` keyword

📖 **`返回`** (return): hand the value over immediately; the function ends right there.

```rust
函数 最大值(甲: 整数, 乙: 整数) -> 整数 {
    如果 甲 > 乙 {
        返回 甲;
    }
    乙
}
```

Read it as: "if 甲 is bigger, **hand back** 甲 immediately; otherwise walk to the last line and hand back 乙."

`返回` is common for **early exits**: condition met, hand in the answer, leave — no need to run anything below.

### Way three: the block expression

Chapter 5 said: a braced block is itself an expression, and its last line (no semicolon) is the block's value:

```rust
让 和值 = {
    让 甲 = 10;
    让 乙 = 20;
    甲 + 乙
};
打印行!("块的值：{}", 和值);
// 预期输出: 块的值：30
```

Output: `块的值：30`

### ⚠️ The semicolon trap: the most common mistake

What if you **add a semicolon** to `数 * 数`?

```rust
// 预期错误: E0308
函数 平方(数: 整数) -> 整数 {
    数 * 数;    // ❌ extra semicolon
}
```

The error: **E0308 type mismatch: expected `整数`, found `()`**.

💡 **Metaphor**: a semicolon is a "full stop" — it turns a sentence into a statement (done and gone, nothing left behind). The function promised to spit out an integer, but all it produced was a statement (the empty value `()`), so of course the compiler objects.

**The mantra**: **for a return value, the last line carries no semicolon.**

---

## 6.5 Functions can be defined anywhere

`主函数` can come first, with the functions it calls defined after:

```rust
函数 主函数() {
    打印行!("面积：{}", 长方形面积(3, 4));
}

函数 长方形面积(长: 整数, 宽: 整数) -> 整数 {
    长 * 宽
}
// 预期输出: 面积：12
```

Output: `面积：12`

Unlike Chapter 5's `如果` and `循环` — which must appear before use — **functions don't care about order**. The compiler "registers" all functions first; then call them however you like.

---

## 6.6 Recursion: a function calling itself

📖 **Recursion**: a function calling itself inside its own body.

💡 **Metaphor**: you ask Dad "what do I do about this?" Dad says "ask Grandpa", Grandpa says "ask Great-grandpa"… until the eldest, who answers directly — and the answer travels back down the chain.

The classic example: **factorial**. 5! = 5 × 4 × 3 × 2 × 1 = 120. The pattern: `阶乘(5) = 5 × 阶乘(4)`, on down to `阶乘(1) = 1`.

```rust
函数 阶乘(数: 整数) -> 整数 {
    如果 数 <= 1 {
        返回 1;          // reached the bottom — answer directly
    }
    数 * 阶乘(数 - 1)    // not at the bottom yet — ask "a smaller me"
}

函数 主函数() {
    打印行!("阶乘：{}", 阶乘(5));
}
// 预期输出: 阶乘：120
```

Output: `阶乘：120`

> ⚠️ **Careful**: recursion needs a **stopping condition** (here `数 <= 1`). Without one, the asking never ends and the program crashes — like a story where nobody ever gives an answer, so the question travels forever.

> ✨ It's fine if recursion doesn't click yet — loops (Chapter 5) solve most problems. Recursion is the "advanced style", and later chapters will meet it again.

---

## 6.7 Methods: functions attached to a type

The boxes so far are "universal" — anyone can use them. There's another kind of box that **belongs to one specific thing** — like a remote control belonging to one TV. A function attached to a type is called a **method**.

📖 **Method**: a function attached to a type. Like a toy's built-in button: the button is on the toy itself, and you need the toy before you can press it.

Chapter 10 properly teaches **structs** (custom types). Here, borrow the simplest possible struct to see what methods look like:

```rust
结构体 记分牌 {
    分数: 整数,
    次数: 整数,
}
```

(Read: the 记分牌 type holds two pieces of data — a score and a count.)

### The `实现` block

Attaching methods to a type takes **`实现`** ("implement — fit something with features"):

```rust
实现 记分牌 {
    // 关联函数：没有 自我 参数，用来"造一个新的"
    函数 创建() -> 记分牌 {
        记分牌 { 分数: 0, 次数: 0 }
    }

    // 方法：&自我 = 只读借用自己
    函数 平均分(&自我) -> 整数 {
        如果 自我.次数 == 0 {
            返回 0;
        }
        自我.分数 / 自我.次数
    }

    // 方法：可变引用 自我 = 可以修改自己
    函数 记录一局(可变引用 自我, 得分: 整数) {
        自我.分数 = 自我.分数 + 得分;
        自我.次数 = 自我.次数 + 1;
    }
}
```

Two new words appeared:

📖 **`自我`** (self): the parameter inside a method that stands for "this very instance". `平均分` is a 记分牌 method, and inside it `自我` is "whichever scoreboard called it".

📖 **Associated function**: a function written inside an `实现` block but **without a `自我` parameter**. It doesn't target an existing instance; it usually **builds new ones** — hence also called a **constructor**.

### The three forms of `自我`

| Form | Meaning | Metaphor |
|---|---|---|
| `&自我` | Read-only borrow: look, don't touch | Borrow a classmate's book to flip through — no writing in it |
| `可变引用 自我` | Mutable borrow: may modify | Borrow a classmate's pen to write — return it after |
| `自我` | Take it outright (consume) | Eat the apple — the apple is gone |

Chapter 7 *Ownership* and Chapter 8 *References and Borrowing* explain "borrowing" in depth. For now: **methods that modify themselves take `可变引用 自我`; read-only ones take `&自我`**.

### Calling: the dot

```rust
让 可变 牌 = 记分牌::创建();   // 关联函数用 双冒号 调用
牌.记录一局(80);                 // 方法用 点号 调用
牌.记录一局(95);
牌.记录一局(76);
打印行!("平均分：{}", 牌.平均分());
```

Output: `平均分：83`

The two calling styles compared:

| Style | Example | Used for |
|---|---|---|
| `类型::名字()` | `记分牌::创建()` | Associated functions (no `自我`) |
| `实例.名字()` | `牌.平均分()` | Methods (first parameter is `自我`, passed in automatically) |

> ⚠️ **Naming warning**: when naming associated functions, **avoid words already taken by the standard library's mapping table** (like `新建` — it's a reserved mapping word; the definition and the call site may translate inconsistently, producing a "method not found" error). Pick your own fresh name (like `创建`) to be safe. Same for variable names: avoid `结果` (the mapped word for the `结果` type).

---

## 6.8 The unit type: spitting out nothing

A function without `->` actually spits out a special value: **empty**, written `()`.

📖 **Empty (the unit type)**: the type that holds nothing, written `()`. Its whole purpose is "so that every function has a return value" — you almost never touch it.

```rust
让 什么也没有: 空 = ();
打印行!("空：{:?}", 什么也没有);
// 预期输出: 空：()
```

Output: `空：()`

Remember the semicolon trap in 6.4? The error said "expected `整数`, found `()`" — that `()` is the empty type. Now you know who it is.

### Diverging functions and the "never" type (`!`)

Beyond "spits out nothing", there's an even stranger kind of function: the **diverging function** — it never even gets to "spit", because it "leaves forever" before reaching the end.

📖 **Diverging function**: a function that never returns normally; its return type is written `!`. It either loops forever, panics outright, or ends the program.

```rust
函数 永不停歇() -> ! {
    循环 {
        打印行!("我又来了！");
    }
}

函数 肯定完蛋() -> ! {
    恐慌!("这是一个必死函数");
}
```

The `!` type has a magic property: **it can stand in for any type**. For example, if one `匹配` arm returns `!`, the whole match's result type is decided by the other arms — the compiler allows it, because a `!` branch never actually produces a value:

```rust
函数 描述(可能有值: 选项<整数>) -> 字符串 {
    匹配 可能有值 {
        有值(值) => 值.转字符串(),
        无 => 恐慌!("这里不该有值"),   // 这个分支的类型是 !（永不返回），可以当任何类型用
    }
}
```

`恐慌!`, `待完成!` and `返回` — these "leave early" expressions all have the type `!`, which is why they can appear anywhere any type is expected.

💡 **Metaphor**: `()` is "handing the recipient an empty box"; `!` is "the courier vanished halfway" — the box never arrives, so nobody needs to specify what it should contain.

---

## 6.9 A complete example: the three-subject score analyzer

Everything from this chapter at once: ordinary functions (parameters, return values, early returns) + struct methods (associated function, `&自我`, `可变引用 自我`):

```rust
// ============ 普通函数 ============

// 求和：两个参数，一个返回值
函数 求和(甲: 整数, 乙: 整数) -> 整数 {
    甲 + 乙
}

// 求较大值：用到提前 返回
函数 最大值(甲: 整数, 乙: 整数) -> 整数 {
    如果 甲 > 乙 {
        返回 甲;
    }
    乙
}

// 打招呼：没有返回值，用到第五章的循环
函数 打招呼(次数: 整数) {
    对于 _ 在 0..次数 {
        打印行!("你好！");
    }
}

// ============ 结构体 + 方法 ============

结构体 记分牌 {
    分数: 整数,
    次数: 整数,
}

实现 记分牌 {
    // 关联函数：造一个全新的记分牌
    函数 创建() -> 记分牌 {
        记分牌 { 分数: 0, 次数: 0 }
    }

    // 方法：记录一局成绩（要改自己，所以用 可变引用 自我）
    函数 记录一局(可变引用 自我, 得分: 整数) {
        自我.分数 = 自我.分数 + 得分;
        自我.次数 = 自我.次数 + 1;
    }

    // 方法：算平均分（只读自己，用 &自我）
    函数 平均分(&自我) -> 整数 {
        如果 自我.次数 == 0 {
            返回 0;    // 一局没玩，直接返回 0 防止除以 0
        }
        自我.分数 / 自我.次数
    }

    // 方法：汇报战况（方法里还能调用别的方法）
    函数 汇报(&自我) {
        打印行!("共{}局，总分{}，平均分{}", 自我.次数, 自我.分数, 自我.平均分());
    }
}

// ============ 主函数 ============

函数 主函数() {
    打印行!("和：{}", 求和(3, 7));
    打印行!("较大的是：{}", 最大值(5, 9));
    打招呼(2);

    让 可变 牌 = 记分牌::创建();
    牌.记录一局(80);
    牌.记录一局(95);
    牌.记录一局(76);
    牌.汇报();
    打印行!("平均分是：{}", 牌.平均分());

    让 什么也没有: 空 = ();
    打印行!("空：{:?}", 什么也没有);
}
```

---

## 6.10 Line by line

- **Lines 4–6**: `求和` takes two integers; the last line `甲 + 乙` carries no semicolon, so its value is the return value.
- **Lines 9–14**: in `最大值`, if `甲 > 乙` holds, `返回 甲;` ends early; otherwise fall through and hand back `乙`.
- **Lines 17–21**: `打招呼` has no `->`, returning empty. `对于 _ 在 0..次数`: the `_` (underscore) means "the loop variable is unused — no name needed".
- **Lines 25–28**: define the struct `记分牌`, two integer fields.
- **Lines 32–34**: the associated function `创建` has no `自我` parameter; it returns a brand-new "0 points, 0 games" scoreboard.
- **Lines 37–40**: `记录一局` takes `可变引用 自我`, because it modifies both of its own fields.
- **Lines 43–48**: `平均分` guards against division by zero (returns 0 early when `次数 == 0`), then does integer division.
- **Lines 51–53**: `汇报` shows **a method calling another method** with `自我.平均分()`.
- **Lines 58–60**: calls to the three ordinary functions.
- **Line 62**: `让 可变` — the following calls to `可变引用 自我` methods modify it, so it must be mutable.
- **Lines 63–65**: dot-call the methods; the scoreboard's internal data updates.
- **Line 68**: the unit type `()` demo, printed with the `{:?}` debug format.

---

## 6.11 What you should see

Store the code in `src/主函数.zh` and run (`rzc run src/主函数.zh` in the terminal, or the top-right ▶):

```
和：10
较大的是：9
你好！
你好！
共3局，总分251，平均分83
平均分是：83
空：()
```

Checking against the answers:

- `和：10`: 3 + 7;
- `较大的是：9`: of 5 and 9, the bigger is 9;
- Two lines of `你好！`: `打招呼(2)` loops twice;
- `共3局，总分251，平均分83`: 80 + 95 + 76 = 251, and 251 ÷ 3 = 83 (integer division drops the decimal — 251 ÷ 3 is 83.66…, taking 83);
- `空：()`: the unit type prints as a pair of parentheses.

---

## 6.12 Common mistakes and how to fix them

### Mistake one: E0308 return type mismatch

```rust
// 预期错误: E0308
函数 翻倍(数: 整数) -> 整数 {
    "变大了"
}
```

The error:

```
错误[E0308]: 类型不匹配：期望 `整数`，实际得到 `字符串引用`
```

**Cause**: promised an integer, handed over text. **Fix**: check the return value against the type after `->`. Usually one of two things: the type is wrong, or a semicolon is extra/missing (see next).

### Mistake two: forgot to drop the semicolon (E0308 + `()`)

```rust
// 预期错误: E0308
函数 平方(数: 整数) -> 整数 {
    数 * 数;
}
```

The error: `类型不匹配：期望 整数，实际得到 ()`.

**Fix**: delete the semicolon from the return-value line. Whenever the error contains `()`, suspect the semicolon first.

### Mistake three: a parameter without a type

```rust
函数 平方(数) -> 整数 {   // ❌ 数 is missing ": 整数"
```

The error complains about a missing type annotation. **Fix**: in Rust, **every function parameter must have a type** — no exceptions: `(数: 整数)`.

### Mistake four: arguments don't match at call time

`求和(3)` is one short, `求和(3, 7, 9)` is one too many — both error with "this function takes N arguments". **Fix**: count the parameters at the definition and pass exactly that many.

### Mistake five: division by zero doesn't error?

Without guarding `次数 == 0` in `平均分`, the first run **panics** (a runtime crash — Chapter 3 mentioned it). Integer division by zero in Rust is a runtime error; the compiler doesn't intercept it early. **Fix**: always check the divisor before dividing.

### Mistake six: method not found (a naming collision)

Name your own function `新建` and you may see "the definition didn't get translated but the call site did", producing E0599 "method not found on this type". **Fix**: keep your own function/variable names away from standard-library mapping words (`新建`, `结果`, etc.) — pick fresh ones.

---

## 6.13 Chapter glossary

| Term | One-line meaning |
|---|---|
| Function | A named, reusable instruction block |
| Parameter | The input name written at definition — type required |
| Argument | The actual value passed at call time |
| Return value | The result a function hands back after running |
| 返回 | Hand back a result early and end the function |
| Expression | A piece of writing that evaluates to a value (last line without semicolon can be the return value) |
| Statement | An instruction that does one thing, ends with a semicolon, produces no value |
| Recursion | A function calling itself — must have a stopping condition |
| Method | A function attached to a type |
| 实现 | The block that fits a type with methods |
| 自我 | The parameter inside a method standing for "this instance itself" |
| Associated function | A function in an impl block without 自我 — usually builds new instances |
| Constructor | See "associated function" |
| Empty (unit type) | The type holding nothing, written () |
| Panic | The program crashing out on an error (at runtime) |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 6.14 Exercises

> 💪 Try first, then peek.

### Exercise one: a multiplication function

Write a function `相乘` taking two integers and returning their product. Print `相乘(6, 7)` in the main function.

<details>
<summary>🔍 View answer</summary>

```rust
函数 相乘(甲: 整数, 乙: 整数) -> 整数 {
    甲 * 乙
}

函数 主函数() {
    打印行!("{}", 相乘(6, 7));
}
// 预期输出: 42
```

Output: `42`

</details>

### Exercise two: even or odd

Write a function `是否偶数` taking one integer and returning a boolean. Hint: `数 % 2` is the remainder after dividing by 2 — remainder 0 means even.

<details>
<summary>🔍 View answer</summary>

```rust
函数 是否偶数(数: 整数) -> 布尔 {
    数 % 2 == 0
}

函数 主函数() {
    打印行!("8是偶数吗：{}", 是否偶数(8));
    打印行!("7是偶数吗：{}", 是否偶数(7));
}
```

Output:

```
8是偶数吗：真
7是偶数吗：假
```

</details>

### Exercise three: max of three

Using this chapter's `最大值` function, find the biggest of three numbers. Hint: take the max of the first two, then compare with the third.

<details>
<summary>🔍 View answer</summary>

```rust
函数 最大值(甲: 整数, 乙: 整数) -> 整数 {
    如果 甲 > 乙 {
        返回 甲;
    }
    乙
}

函数 三数最大(甲: 整数, 乙: 整数, 丙: 整数) -> 整数 {
    最大值(最大值(甲, 乙), 丙)
}

函数 主函数() {
    打印行!("{}", 三数最大(3, 9, 5));
}
// 预期输出: 9
```

Output: `9`

</details>

### Exercise four: a counter method

For a struct `计数器` (one field `值: 整数`), write: the associated function `创建` (starting from 0), the method `加一` (`可变引用 自我`), and the method `报数` (prints the current value). In the main function, add three times and print.

<details>
<summary>🔍 View answer</summary>

```rust
结构体 计数器 {
    值: 整数,
}

实现 计数器 {
    函数 创建() -> 计数器 {
        计数器 { 值: 0 }
    }
    函数 加一(可变引用 自我) {
        自我.值 = 自我.值 + 1;
    }
    函数 报数(&自我) {
        打印行!("现在是：{}", 自我.值);
    }
}

函数 主函数() {
    让 可变 计数 = 计数器::创建();
    计数.加一();
    计数.加一();
    计数.加一();
    计数.报数();
}
// 预期输出: 现在是：3
```

Output: `现在是：3`

</details>

---

## 6.15 FAQ

**Q: Can function names be in my native language? Any limits?**

Yes — this book uses native function names throughout. Start with a **verb** (`求和`, `打印`, `检查`) so the box's purpose is obvious at a glance. Don't reuse words already in the mapping table (like `新建` or `打印行`) as your own function names — they collide.

**Q: How do functions relate to Chapter 5's `匹配` and `循环`?**

`如果` and `循环` are **control flow** (deciding execution order); functions are **code organization** (packing instructions under a name). Function bodies can use control flow freely, and control flow can call functions. They cooperate.

**Q: Can a function return multiple values?**

Strictly, one value only. But pack several into a **tuple** (Chapter 4) and return that, e.g. `-> (整数, 整数)`. After Chapter 10's structs, you can also return a struct.

**Q: `返回` or the trailing expression — which is better?**

Simple computations suit the trailing expression (more concise); use `返回` when you need an **early exit** (condition met, stop calculating). Both are everyday tools.

**Q: What does the `&` in `&自我` mean?**

It means "borrow" — the method merely **borrows** the instance to look at or modify it, then gives it back; it doesn't consume it. Chapter 7 *Ownership* and Chapter 8 *References and Borrowing* spend two full chapters on this. For now, treat it as fixed spelling.

**Q: Which is faster, recursion or a loop?**

Most of the time, loops are faster and lighter on memory. Recursion wins on elegance and clarity (especially for "layers within layers" problems). At the beginner stage, learn both; don't fuss over performance.

**Q: Why does `打招呼` use `_` as the loop variable?**

`_` means "this value goes unused". The loop repeats `次数` times, but we don't care which round it is — `_` skips the naming. It also avoids the "defined but never used" warning.

---

## What's next

Your boxes are multiplying — but the data inside them has a big question: **who owns it? And who cleans up when it's done?**

In **Chapter 7, "Ownership"**, you'll learn Rust's most distinctive rules:

- Every value has exactly one **owner**;
- When the owner leaves, the value is **cleaned up** automatically (released);
- Hand a value to someone else and **you can no longer use it** (moving);
- `克隆` copies out an independent duplicate.

This is what makes Rust unlike any other language — and it's the secret weapon of its memory safety. See you in the next chapter!
