# Chapter 10: Structs

## 10.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain what a struct is with the **record card** metaphor;
2. Define a struct, build **instances**, read and write fields with a dot;
3. Fit a struct with methods using an **`实现`** block (Chapter 6's setup finally pays off);
4. Make a struct printable with `{:?}` using **`#[派生(调试)]`**;
5. Write **tuple structs**;
6. Build new instances quickly with the **`..old-instance`** update syntax;
7. Manage a batch of data with an array of structs.

---

## 10.1 Why structs exist: the pain of loose data

Say you want to record one student's information: name, age, score. With what you know so far, the only way is:

```rust
让 姓名甲 = "小华";
让 年龄甲 = 12;
让 分数甲 = 95;

让 姓名乙 = "小刚";
让 年龄乙 = 13;
让 分数乙 = 76;
```

The problem is obvious: three variables form a group only through a **naming convention** (all carrying "甲"). With more students, variables fly everywhere, and which belongs to whom depends on sharp eyes.

💡 **Metaphor**: it's like the whole class's records **scattered on the floor** — a name slip here, an age strip there, a score sheet over there. What we need is a **record card**: one card per student, three fields, everything in its place.

📖 **Struct**: a custom "record card" type that packs related data together. The keyword is `结构体`.

## 10.2 Defining a struct

```rust
结构体 学生 {
    姓名: 字符串,
    年龄: 整数,
    分数: 整数,
}
```

Read it as: "there is a new type called `学生` (student), with three columns: name (string), age (integer), score (integer)."

> ⚠️ **Careful**: defining a struct only **designs the card layout** — no real data exists yet. Like a print shop finishing the card template — no one's information is written on any card.

## 10.3 Building instances: filling in a card

📖 **Instance**: one concrete piece of data built to the struct's layout — a card with its contents filled in.

### Way one: fill the literal directly

```rust
让 小华 = 学生 {
    姓名: 字符串::从("小华"),
    年龄: 12,
    分数: 95,
};
```

The format: `TypeName { field: value, field: value, ... }`. **Every field must be filled** — miss one and it errors (E0063).

### Way two: an associated function (the constructor)

Seen in Chapter 6: a function without `自我` inside an `实现` block is an associated function, built for creating instances:

```rust
实现 学生 {
    函数 创建(姓名: 字符串, 分数: 整数) -> 学生 {
        学生 { 姓名: 姓名, 年龄: 12, 分数: 分数 }
    }
}

// 用起来：
让 小华 = 学生::创建(字符串::从("小华"), 95);
```

The benefit: the instance-building logic (say, a default age) lives in one place, and the main function stays a clean single line.

## 10.4 Reading and writing fields: the dot

```rust
// 读
打印行!("{}今年{}岁", 小华.姓名, 小华.年龄);

// 写（实例必须是 让 可变）
让 可变 小华 = 学生 { ... };
小华.分数 = 100;
```

> ⚠️ **Careful**: Rust has no "make just one field mutable" — to modify any field, the whole instance must be declared `让 可变`.

## 10.5 Methods: fitting the card with features

A function with a `自我` parameter inside an `实现` block is a **method**. Chapter 6's setup officially pays off now:

```rust
实现 学生 {
    // 只读方法
    函数 是否及格(&自我) -> 布尔 {
        自我.分数 >= 60
    }

    // 修改方法
    函数 加分(可变引用 自我, 加多少: 整数) {
        自我.分数 = 自我.分数 + 加多少;
    }
}

// 调用：
小华.加分(5);
打印行!("{}", 小华.是否及格());
```

What makes methods better than ordinary functions? **Data and behavior live together.** The "is passing" check lives on the student type itself, rather than scattered in some corner as `判断及格(某某学生)`.

## 10.6 Debug printing: `{:?}` and derive

Try printing a struct directly:

```rust
打印行!("{}", 小华);   // ❌ error: the 学生 type doesn't support {} display
```

`{}` requires the type to implement the "display" ability, which structs lack by default. Two remedies — beginners use the first:

**Stick on the Debug sticker, print with `{:?}`**:

```rust
#[派生(调试)]
结构体 学生 { ... }

打印行!("{:?}", 小华);
```

Output:

```
学生 { 姓名: "小华", 年龄: 12, 分数: 95 }
```

`{:?}` is the **debug format** — every field laid bare, a debugging superpower. Want a multi-line view for many fields? Use `{:#?}`:

```rust
打印行!("{:#?}", 天空蓝);
```

```
彩色 {
    红: 0,
    绿: 191,
    蓝: 255,
}
```

> 📖 **Derive**: having the compiler automatically generate certain abilities for a type, written `#[派生(...)]`. The trait names in the parentheses may be native (`调试`, `克隆`) or English originals (`Debug`, `Clone`).

## 10.7 Tuple structs: packing without field names

Sometimes data is too simple to name each column:

```rust
结构体 坐标(整数, 整数);

函数 主函数() {
    让 点 = 坐标(3, 5);
    打印行!("横：{}，纵：{}", 点.0, 点.1);
}
// 预期输出: 横：3，纵：5
```

Output: `横：3，纵：5`

Access by position with `.0`, `.1` (just like Chapter 4's tuples). Fits two fields or fewer whose meaning is self-evident (coordinates, colors…). Most of the time, use a regular struct with named fields — better readability.

## 10.8 The update syntax: change a few columns, copy the rest

Want a new card **mostly identical** to an old one:

```rust
让 小华二号 = 学生 { 分数: 88, ..小华 };
```

Read it as: "the new card's score is 88, **all other fields copied from 小华**."

> ⚠️ **Ownership warning**: `..小华` **moves** the old card's fields into the new one (remember Chapter 7). If a field is heap data like a `字符串`, `小华` dies from then on. Want both cards usable? `小华.克隆()` first.

## 10.9 A complete example: the class roster

Structs + methods + arrays + loops, managing a whole class:

```rust
#[派生(调试, 克隆)]
结构体 学生 {
    姓名: 字符串,
    分数: 整数,
}

实现 学生 {
    函数 创建(姓名: 字符串, 分数: 整数) -> 学生 {
        学生 { 姓名: 姓名, 分数: 分数 }
    }

    函数 是否及格(&自我) -> 布尔 {
        自我.分数 >= 60
    }
}

函数 主函数() {
    // 数组里装三个学生实例
    让 班级 = [
        学生::创建(字符串::从("小华"), 95),
        学生::创建(字符串::从("小刚"), 58),
        学生::创建(字符串::从("小丽"), 88),
    ];

    // 打印花名册
    对于 下标 在 0..3 {
        打印行!("{:?}，及格：{}", 班级[下标], 班级[下标].是否及格());
    }

    // 找最高分
    让 可变 最高 = 0;
    让 可变 冠军 = 学生::创建(字符串::从("无"), 0);
    对于 下标 在 0..3 {
        如果 班级[下标].分数 > 最高 {
            最高 = 班级[下标].分数;
            冠军 = 班级[下标].克隆();
        }
    }
    打印行!("冠军是{}，{}分", 冠军.姓名, 冠军.分数);
}
```

## 10.10 Line by line

- **Line 1**: derive two abilities at once — `Debug` (enables `{:?}` printing) and `Clone` (enables `.克隆()`), comma-separated.
- **Lines 2–5**: the `学生` struct, two fields.
- **Lines 8–10**: the associated function `创建`, building an instance in one line.
- **Lines 12–14**: the read-only method `是否及格`.
- **Lines 19–23**: an **array of structs** — each element is a student card. Array syntax is still Chapter 3's `[元素, 元素, ...]`.
- **Lines 26–28**: loop-print. `{:?}` spits out the whole card.
- **Lines 31–32**: `冠军` starts as a "none" placeholder, guarding against an empty run.
- **Lines 33–38**: the classic "king of the hill" max-finding. `班级[下标]` takes a card from the array; the array is read-only (not declared mutable), so cards can't be **taken away** — `.克隆()` duplicates one for `冠军`.
- **Line 40**: `冠军.姓名` reads a field with a dot.

## 10.11 What you should see

```
学生 { 姓名: "小华", 分数: 95 }，及格：真
学生 { 姓名: "小刚", 分数: 58 }，及格：假
学生 { 姓名: "小丽", 分数: 88 }，及格：真
冠军是小华，95分
```

---

## 10.12 Common mistakes and how to fix them

### Mistake one: E0063, a missing field

```
错误[E0063]: 结构体 `{结构名}` 缺少字段 `{字段名}`
💡 初始化结构体时必须为所有字段赋值。
```

**Fix**: fill in the field, or use `..旧实例` to copy the rest from an old one.

### Mistake two: printing a struct with `{}` errors

`打印行!("{}", 小华)` reports "学生 does not implement display". **Fix**: add `#[派生(调试)]` and use `{:?}`; or print specific fields like `小华.姓名`.

### Mistake three: assigning a field on an immutable instance

```rust
小华.分数 = 100;   // ❌ 小华 is not 让 可变
```

The error is E0594. **Fix**: add `可变` at the declaration: `让 可变 小华 = ...`.

### Mistake four: the old instance vanishes after the update syntax

`..小华` moved the fields away, so the `字符串` field inside `小华` was moved — using `小华` again errors with E0382. **Fix**: `小华.克隆()` before the update, or reorder the uses.

### Mistake five: a misspelled derived trait name

`#[派生(克降)]` reports "derive macro not found" — a look-alike typo in the trait name. **Fix**: check the spelling: `调试`, `克隆`, `复制`, `哈希`.

---

## 10.13 Chapter glossary

| Term | One-line meaning |
|---|---|
| Struct | A custom packing type |
| Field | Each column of data inside a struct |
| Instance | One concrete piece of data built to a struct's layout |
| Associated function | A function in an impl block without 自我 — usually builds instances |
| Constructor | See "associated function" |
| Method | A function with a 自我 parameter, attached to a type |
| Derive | Having the compiler generate trait implementations, written `#[派生(...)]` |
| Debug output | The `{:?}` format, paired with the debug derive for printing structs |
| Tuple struct | A struct without field names, accessed by position |
| Update syntax | `..旧实例` — copy the remaining fields to build a new instance quickly |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 10.14 Exercises

> 💪 Try first, then peek.

### Exercise one: a book card

Define a `图书` struct (书名: 字符串, 页数: 整数), write the associated function `创建`, build one book and print it with `{:?}`.

<details>
<summary>🔍 View answer</summary>

```rust
#[派生(调试)]
结构体 图书 {
    书名: 字符串,
    页数: 整数,
}

实现 图书 {
    函数 创建(书名: 字符串, 页数: 整数) -> 图书 {
        图书 { 书名: 书名, 页数: 页数 }
    }
}

函数 主函数() {
    让 书 = 图书::创建(字符串::从("小王子"), 97);
    打印行!("{:?}", 书);
}
// 预期输出: 图书 { 书名: "小王子", 页数: 97 }
```

Output: `图书 { 书名: "小王子", 页数: 97 }`

</details>

### Exercise two: a thick-book check

Add a method `是否厚书(&自我)` to `图书`: returns `真` when the page count exceeds 300.

<details>
<summary>🔍 View answer</summary>

```rust
函数 是否厚书(&自我) -> 布尔 {
    自我.页数 > 300
}
```

</details>

### Exercise three: tuple structs

Define a `温度(整数)` tuple struct, build two instances (北京 20, 哈尔滨 -5), and print their values.

<details>
<summary>🔍 View answer</summary>

```rust
结构体 温度(整数);

函数 主函数() {
    让 北京 = 温度(20);
    让 哈尔滨 = 温度(-5);
    打印行!("北京：{}度，哈尔滨：{}度", 北京.0, 哈尔滨.0);
}
```

</details>

### Exercise four: the update syntax

Based on a `图书` instance `原版`, use the update syntax to build a `修订版` — changing only the page count to 120, copying the rest (hint: clone first to prevent a move).

<details>
<summary>🔍 View answer</summary>

```rust
#[派生(调试, 克隆)]
结构体 图书 {
    书名: 字符串,
    页数: 整数,
}

函数 主函数() {
    让 原版 = 图书 { 书名: 字符串::从("小王子"), 页数: 97 };
    让 修订版 = 图书 { 页数: 120, ..原版.克隆() };
    打印行!("原版：{:?}", 原版);
    打印行!("修订版：{:?}", 修订版);
}
```

</details>

---

## 10.15 FAQ

**Q: Struct vs Chapter 4's tuple — what's the difference?**

Both pack. Tuple fields are **unnamed** (accessed `.0`, `.1`) — for temporary small groupings; struct fields are **named** — for real data models. Tuple structs sit between the two.

**Q: Can a struct hold a struct?**

Yes. For example, a `班级` struct holding an array of students. Data nests layer by layer, like record cards inside a filing cabinet. After Chapter 15's vectors (growable collections), it gets even more flexible.

**Q: What exactly is passed as `自我`?**

The instance that called the method. In `小华.是否及格()`, `自我` = `小华`. Write `&自我` to borrow, write `自我` to take it (consume). Chapter 8's knowledge is reused here in full.

**Q: Why is an associated function recommended over a literal for building instances?**

Two benefits — ① defaults (fix the age inside `创建`); ② validation (page counts can't be negative). As projects grow, keep construction logic in associated functions.

**Q: Does the `派生(调试)` sticker slow the program down?**

Practically no. It only generates printing code at compile time; if you never use `{:?}`, it never runs.

**Q: How do enums (Chapter 11) relate to structs?**

A struct is "**and**" — one card holds name AND age AND score. An enum is "**or**" — a value is one of several cases (passing / failing / absent). Next chapter settles it.

---

## What's next

Structs solved "packing several kinds together" (and); but many situations are "one of two, one of three" (or): a grade is either 优, 良 or 差; a coin is either heads or tails.

**Chapter 11, "Enums and Pattern Matching"**, teaches:

- Defining "one-of-many" types with `枚举`;
- Attaching data to each variant;
- `匹配` in its full glory: destructuring enums, extracting data;
- `选项` — Rust's standard answer to "maybe there's no value".

See you in the next chapter!
