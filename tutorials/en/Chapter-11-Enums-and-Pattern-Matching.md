# Chapter 11: Enums and Pattern Matching

## 11.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain the difference between enums and structs ("or" vs "and");
2. Define **simple enums** and **enums with data**;
3. Destructure enums with `匹配` and extract their data;
4. Read and fix **E0004** (non-exhaustive match);
5. Use the **`选项`** (Option) type for "there may be a value, or not";
6. Explain why Rust has no "null pointer" trap.

---

## 11.1 Enums: a multiple-choice question

Chapter 10's struct was "**and**": one student card **simultaneously** has a name, an age and a score.

But life is mostly "**or**":

- A traffic light is **either** red, **either** yellow, **either** green;
- A coin is **either** heads, **either** tails;
- An answer is **either** correct, **either** wrong, **either** unattempted.

📖 **Enum**: a type where a value can only be **one of the listed cases**. The keyword is `枚举`.

💡 **Metaphor**: an enum is a **multiple-choice answer sheet** — you can only pick one of the given options. No "other", and definitely not two at once.

## 11.2 Simple enums: the traffic light

```rust
枚举 红绿灯 {
    红,
    黄,
    绿,
}
```

> 📖 **Variant**: each possible value of an enum. `红绿灯` has three variants.

Use them as `枚举名::变体名`:

```rust
函数 红绿灯行动(灯: 红绿灯) {
    匹配 灯 {
        红绿灯::红 => 打印行!("停"),
        红绿灯::黄 => 打印行!("准备"),
        红绿灯::绿 => 打印行!("走"),
    }
}

函数 主函数() {
    红绿灯行动(红绿灯::红);
    红绿灯行动(红绿灯::黄);
    红绿灯行动(红绿灯::绿);
}
```

Output:

```
停
准备
走
```

Why is this better than 1, 2, 3 for the lights? **The names carry meaning**, and there's no way to pass a "light #4" — the compiler simply refuses.

## 11.3 Enums with data

Variants can also **carry luggage**:

```rust
枚举 成绩 {
    分数(整数),        // 带一个整数
    缺考,              // 不带
    免修(字符串),      // 带一段文字（写明免修原因）
}
```

Read it as: "a 成绩 (grade) has three possible cases: a concrete score, absent, or excused (with a reason)."

Building values:

```rust
让 甲 = 成绩::分数(88);
让 乙 = 成绩::缺考;
让 丙 = 成绩::免修(字符串::从("已学过"));
```

> ⚠️ **Careful**: prefer `字符串` over `字符串引用` in variant luggage — the latter drags in lifetime annotations (Chapter 14's headache). Beginners should steer clear.

### Enums vs structs

| | Struct | Enum |
|---|---|---|
| Meaning | These fields hold **at the same time** | These cases are **one of** |
| Example | Student = name + age + score | Grade = score / absent / excused |

The two can also **nest** (you'll see it in Chapter 13's traits and Chapter 15's collections) — powerful combinations.

## 11.4 匹配: unpacking the luggage

Chapter 5 taught `匹配` basics (matching integers, ranges). For enums, `匹配` can also **open a variant's luggage and take the data out**:

```rust
函数 成绩报告(单: 成绩) {
    匹配 单 {
        成绩::分数(值) => 打印行!("得分：{}", 值),
        成绩::缺考 => 打印行!("缺考，无成绩"),
        成绩::免修(原因) => 打印行!("免修：{}", 原因),
    }
}
```

`成绩::分数(值)` reads: "if this is a score-carrying grade, **take the integer out and call it `值`**." The parentheses are the unpacking action.

Output:

```
得分：88
缺考，无成绩
免修：已学过
```

## 11.5 Exhaustiveness: not one case missed

Rust's iron law: **the match must cover every variant**. Miss one and you get **E0004**:

```rust
枚举 灯 { 红, 绿 }

匹配 当前 {
    灯::红 => 打印行!("停"),
    // 漏了 绿 ❌
}
```

The real error:

```
错误[E0004]: 匹配表达式不完备：存在没有匹配分支的可能取值
💡 匹配必须覆盖所有可能的情况。请补齐遗漏的分支，或添加 `_` 通配分支兜底。
```

**Two fixes**: ① list every variant; ② catch-all with `_` (Chapter 5):

```rust
匹配 当前 {
    灯::红 => 打印行!("停"),
    _ => 打印行!("其他情况都走"),
}
```

> 💡 **Why so strict?** Imagine adding a "blinking" variant to `红绿灯` later — every match point that ignores it instantly errors with E0004. The compiler **marches you** through updating every corner; none escapes. That's the biggest source of an enum's sense of safety.

## 11.6 Bonus: the full life of patterns

So far, patterns only appeared inside `匹配`. Actually patterns show up in a **bunch of other places**, and come in many flavors. This section completes the craft — from now on, wherever you see "unpacking a value" in code, you'll recognize a pattern at work.

### 如果让: caring about one case only

`匹配` demands exhaustiveness (just covered in 11.5). But if you only care about **one case** and lump the rest together, `匹配` is wordy:

```rust
让 分数: 选项<整数> = 有值(95);

如果让 有值(值) = 分数 {
    打印行!("有分：{}", 值);
} 否则 {
    打印行!("没分");
}
```

📖 **如果让** (if-let): `如果让 模式 = 值 { ... } 否则 { ... }` — try `值` against `模式`: **on a match** unpack and run the first block; **on failure** walk the `否则`.

💡 **Metaphor**: `匹配` is a vending machine where you press **every** button; `如果让` is pressing **one** button — it dispenses on a hit; miss, and you move to the next machine.

`如果让 有值(值) = 分数` reads: "if `分数` is a `有值(...)`, take the inner value and call it `值`." Identical to `匹配 分数 { 有值(值) => ..., 无 => ... }` — just shorter.

### 当让: the loop version of 如果让

```rust
让 可变 剩余: 选项<整数> = 有值(3);
当让 有值(值) = 剩余 {
    打印行!("剩余 {}", 值);
    剩余 = 无;
}
```

📖 **当让** (while-let): `当让 模式 = 值 { ... }` — as long as `值` matches `模式`, unpack, run the body once, then **try again**; on failure the loop ends.

(The example above deliberately sets `剩余` to `无`, so the loop runs once; real scenarios usually repeatedly draw from a "changing value", like pulling from an iterator.)

### 让 否则: if it won't unpack, leave

Sometimes "it won't unpack" means **there's no point running the rest**. Rust 2021 added a syntax for this:

```rust
让 输入: 选项<整数> = 有值(7);
让 有值(数) = 输入 否则 {
    返回;
};
打印行!("让否则 拿到 {}", 数);
```

📖 **让 否则** (let-else): `让 模式 = 值 否则 { exit code };` — the pattern **must** match; on a match, continue; on failure, run the `否则` block (usually `返回` or `中断`), and **the code after never executes**.

> ⚠️ **Note**: the `否则` block **must end in a non-returning way** (`返回`, `中断`, `继续`, `恐慌!` etc.) — the compiler must guarantee "if it didn't unpack, we can never reach the code below".

### Match guards: a threshold on a match arm

Patterns handle "is it this shape", but sometimes you also need a **condition**. A guard is an `if` placed before the `=>`:

```rust
让 数 = 7;
匹配 数 {
    值 如果 值 > 5 => 打印行!("大于5：{}", 值),
    _ => 打印行!("不大于5"),
}
```

Read: the first arm's full threshold is "matched a `值` **and** `值 > 5`". If the condition fails, fall to the next arm.

> ⚠️ **Note**: guard expressions may be written natively (`值 如果 值 > 5`) — the `如果` translates to a standard `if`. Note that `如果` means something else inside patterns (`如果让`), but as a guard it's just an `if` after translation.

### @ bindings: match and keep the name

Want "the range matched successfully, AND I keep the matched value"? `@` exists for this:

```rust
让 数 = 7;
匹配 数 {
    值 @ 1..=5 => 打印行!("1到5：{}", 值),
    _ => 打印行!("范围外"),
}
```

📖 **@ binding**: `名字 @ 模式` — store the matched value **simultaneously** into `名字`. `值 @ 1..=5` reads "if the number is between 1 and 5, call the number itself `值`".

Without `@`, `1..=5` can only judge "in range or not" — the concrete number is lost. `@` fills that gap.

### Or-patterns: one arm, several values

Several **same-type** values sharing one path? Use `|` (vertical bar) to merge them into one arm:

```rust
让 数 = 7;
匹配 数 {
    1 | 2 => 打印行!("1或2"),
    _ => 打印行!("其他"),
}
```

Enums work the same: `成绩::缺考 | 成绩::免修(_) => 打印行!("没有具体分数")` — both "no score" cases share one arm.

### Structs and tuples: unpacking packed data

Patterns unpack structs and tuples too, not just enums:

```rust
结构体 点 {
    横: 整数,
    纵: 整数,
}

让 点甲 = 点 { 横: 10, 纵: 20 };
匹配 点甲 {
    点 { 横, 纵 } => 打印行!("横={} 纵={}", 横, 纵),
}

让 (甲, 乙) = (1, 2);
打印行!("{} {}", 甲, 乙);
```

- The struct pattern `点 { 横, 纵 }`: unpacks **by field name** — order doesn't matter;
- The tuple pattern `(甲, 乙)`: unpacks **by position**, one-to-one.

💡 **Metaphor**: enum patterns are like opening delivery boxes (which courier is it? what's inside?); struct patterns pick items **by label**; tuple patterns call names **by seat number**.

### Complete example one: 如果让, 当让 and 让 否则

```rust
函数 主函数() {
    // 如果让
    让 分数: 选项<整数> = 有值(95);
    如果让 有值(值) = 分数 {
        打印行!("有分：{}", 值);
    } 否则 {
        打印行!("没分");
    }

    // 当让
    让 可变 剩余: 选项<整数> = 有值(3);
    当让 有值(值) = 剩余 {
        打印行!("剩余 {}", 值);
        剩余 = 无;
    }

    // 让否则
    让 输入: 选项<整数> = 有值(7);
    让 有值(数) = 输入 否则 {
        返回;
    };
    打印行!("让否则 拿到 {}", 数);
}
```

Output:

```
有分：95
剩余 3
让否则 拿到 7
```

### Complete example two: guards, @ and destructuring

Guards, `@`, `|`, struct destructuring and tuple destructuring in one program:

```rust
结构体 点 {
    横: 整数,
    纵: 整数,
}

函数 主函数() {
    // 匹配守卫
    让 数 = 7;
    匹配 数 {
        值 如果 值 > 5 => 打印行!("大于5：{}", 值),
        _ => 打印行!("不大于5"),
    }

    // @ 绑定
    匹配 数 {
        值 @ 1..=5 => 打印行!("1到5：{}", 值),
        _ => 打印行!("范围外"),
    }

    // 或模式
    匹配 数 {
        1 | 2 => 打印行!("1或2"),
        _ => 打印行!("其他"),
    }

    // 结构体解构
    让 点甲 = 点 { 横: 10, 纵: 20 };
    匹配 点甲 {
        点 { 横, 纵 } => 打印行!("横={} 纵={}", 横, 纵),
    }

    // 元组解构
    让 (甲, 乙) = (1, 2);
    打印行!("{} {}", 甲, 乙);
}
```

Output:

```
大于5：7
范围外
其他
横=10 纵=20
1 2
```

The first three are `7` passing through the guard, the range and `|` in turn; the last two show how a struct and a tuple unpack all their data in one breath.

### Refutable vs irrefutable: the two faces of patterns

One last concept. Patterns come in two kinds:

📖 **Refutable**: might fail. Like `有值(值)` — if the value is `无`, no match.
📖 **Irrefutable**: **always** matches. Like `x`, `(甲, 乙)` — any value unpacks.

**Position matters**:

- `让` bindings, function parameters, `对于` loops: require **irrefutable** patterns (the compiler checks);
- `匹配` arms, `如果让`, `当让`: **either kind** (a miss walks another path or ends).

The reverse also fails — an irrefutable pattern inside `如果让` makes the compiler grumble that it's pointless (hinting "this pattern always succeeds; no need for 如果让").

**The most common trap**: writing `让 有值(值) = 分数;` — the compiler reports **E0005** directly:

```
错误[E0005]: 该模式不能保证匹配成功，但此处要求必然匹配成功的模式
💡 `让` 绑定与函数参数等位置要求必然成功的模式。可能失败的模式请改用 `匹配` 或 `如果 让` 处理。
```

**Fix**: leave the "might not unpack" shell-splitting to `匹配` / `如果让` / `让 否则`; use plain `让` only for things that are "definitely there".

---

## 11.7 选项: handling "maybe there's nothing"

A common situation: **the value may not exist**. Like "the class's highest score" — what if nobody's in the class?


Many languages use "null pointers" for "nothing", birthing the billion-dollar mistake: forget to check for null and the program crashes on the spot. Rust's answer is a built-in enum:

```rust
枚举 选项<物> {
    有值(物),    // 有值，值装在里面
    无,          // 没有值
}
```

(`<物>` is a generic — Chapter 12 explains. For now, read it as "an option of what type the value is".)

📖 **选项** (Option): the type for "there may be a value, or not". `有值` = there is; `无` = there isn't.

```rust
让 有分: 选项<整数> = 有值(95);
让 没分: 选项<整数> = 无;

匹配 有分 {
    有值(值) => 打印行!("有分数：{}", 值),
    无 => 打印行!("没有分数"),
}
匹配 没分 {
    有值(值) => 打印行!("有分数：{}", 值),
    无 => 打印行!("没有分数"),
}
```

Output:

```
有分数：95
没有分数
```

The key idea: **"no value" is itself a value that must be handled explicitly**. You can't pretend `无` doesn't exist — to take a value out of an option, you must use `匹配` and care for both cases (or Chapter 16's convenience methods). "Forgot to check for null" becomes **impossible** in Rust.

---

## 11.8 A complete example: grading the answer sheet

An enum array + match + loops, grading a 10-question answer sheet:

```rust
// 每道题的作答情况（派生 Copy：无数据的枚举可以随手复制）
#[派生(Copy, Clone)]
枚举 答案 {
    对,
    错,
    未答,
}

// 每题得分规则
函数 得分(题: 答案) -> 整数 {
    匹配 题 {
        答案::对 => 5,
        答案::错 => 0,
        答案::未答 => 0,
    }
}

// 总评等级
枚举 评级 { 优, 良, 中, 差 }

函数 评级根据(总分: 整数) -> 评级 {
    如果 总分 >= 45 { 返回 评级::优; }
    如果 总分 >= 35 { 返回 评级::良; }
    如果 总分 >= 20 { 返回 评级::中; }
    评级::差
}

函数 主函数() {
    // 一张 10 题的答题卡
    让 答题卡 = [
        答案::对, 答案::对, 答案::错, 答案::对, 答案::未答,
        答案::对, 答案::对, 答案::错, 答案::对, 答案::对,
    ];

    让 可变 总分 = 0;
    让 可变 对题数 = 0;
    对于 下标 在 0..10 {
        总分 = 总分 + 得分(答题卡[下标]);
        匹配 答题卡[下标] {
            答案::对 => 对题数 = 对题数 + 1,
            答案::错 => {}
            答案::未答 => {}
        }
    }
    打印行!("对{}题，总分{}", 对题数, 总分);

    匹配 评级根据(总分) {
        评级::优 => 打印行!("总评：优，真棒！"),
        评级::良 => 打印行!("总评：良，不错！"),
        评级::中 => 打印行!("总评：中，继续加油"),
        评级::差 => 打印行!("总评：差，要努力了"),
    }
}
```

## 11.9 Line by line

- **Line 2**: `#[派生(Copy, Clone)]` — `答案` carries no data, so it can be **casually copied** like an integer. This matters: array elements can't be taken out by default (E0508), but with `Copy`, `答题卡[下标]` automatically duplicates one for handing to functions.
- **Lines 10–16**: `得分` maps the three cases to numbers via match. The arms' right sides are bare expressions `5` and `0`.
- **Line 19**: `评级` is a pure label enum.
- **Lines 21–26**: `评级根据` converts a total score to a grade via Chapter 5's `如果` chain, with `评级::差` as the floor.
- **Lines 30–33**: the enum array — 10 elements.
- **Lines 37–43**: one loop does two jobs — accumulate the total, count correct answers. `答案::错 => {}` and `答案::未答 => {}` are empty braces: match arms **must** list all three cases, so even these "do nothing" arms must be written out.
- **Lines 46–51**: match the grade enum into well-wishes — all four variants, none missed.

## 11.10 What you should see

```
对7题，总分35
总评：良，不错！
```

7 correct × 5 points = 35, landing in the 35–44 range → 良.

---

## 11.11 Common mistakes and how to fix them

### Mistake one: E0004 non-exhaustive match

A variant is missing. **Fix**: list it, or catch-all with `_`. The error lists the "uncovered patterns" — copy them in.

### Mistake two: E0508 can't move out of an array

`函数(数组[下标])` tries to **take** the element, but the array refuses. **Fix**: derive `Copy` for the enum (when it has no data), or pass a reference `&数组[下标]` (adjust the parameter), or `.克隆()`.

### Mistake three: a variant holding `字符串引用` errors with E0106

Writing `免修(字符串引用)` in a variant demands lifetime annotations. **Fix**: use `字符串`: `免修(字符串)`, building with `字符串::从("...")`.

### Mistake four: printing an enum directly

`打印行!("{}", 答案::对)` errors. **Fix**: `#[派生(调试)]` + `{:?}` (Chapter 10's old trick).

### Mistake five: assuming the option's value

`有值(95)` can't be used directly as `95` — it's still wrapped in a shell. **Fix**: unpack with `匹配`, or Chapter 16's `拆包期望` and friends.

---

## 11.12 Chapter glossary

| Term | One-line meaning |
|---|---|
| Enum | The "one of many" type |
| Variant | Each possible value of an enum |
| Carried data | Data attached to a variant, unpacked at match time |
| Exhaustive | A match must cover every possibility, none missed |
| E0004 | The error code for a non-exhaustive match |
| E0508 | The error code for moving an element out of an array |
| 选项 | The type for "there is or there isn't" |
| 有值 | The option variant for "there is a value" |
| 无 | The option variant for "no value" |
| Null pointer | Other languages' dangerous way of expressing "nothing" — replaced by 选项 in Rust |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 11.13 Exercises

> 💪 Try first, then peek.

### Exercise one: the coin

Define an enum `硬币 { 正面, 反面 }`, and write a function `报(币: 硬币)` printing the result via match.

<details>
<summary>🔍 View answer</summary>

```rust
枚举 硬币 { 正面, 反面 }

函数 报(币: 硬币) {
    匹配 币 {
        硬币::正面 => 打印行!("正面！"),
        硬币::反面 => 打印行!("反面！"),
    }
}

函数 主函数() {
    报(硬币::正面);
    报(硬币::反面);
}
```

</details>

### Exercise two: shapes with data

Define an enum `形状 { 圆(整数), 方(整数, 整数) }` (circle carries a radius; rectangle carries length and width), and a function `面积`: circles use `3 * 半径 * 半径` as an approximation, rectangles length × width.

<details>
<summary>🔍 View answer</summary>

```rust
枚举 形状 {
    圆(整数),
    方(整数, 整数),
}

函数 面积(形: 形状) -> 整数 {
    匹配 形 {
        形状::圆(半径) => 3 * 半径 * 半径,
        形状::方(长, 宽) => 长 * 宽,
    }
}

函数 主函数() {
    打印行!("{}", 面积(形状::圆(10)));     // 300
    打印行!("{}", 面积(形状::方(3, 4)));   // 12
}
```

</details>

### Exercise three: unpacking the option

Write a function `报分数(分: 选项<整数>)`: with a value, print "得了N分"; without, print "没有记录". Call it with `有值(72)` and `无`.

<details>
<summary>🔍 View answer</summary>

```rust
函数 报分数(分: 选项<整数>) {
    匹配 分 {
        有值(值) => 打印行!("得了{}分", 值),
        无 => 打印行!("没有记录"),
    }
}

函数 主函数() {
    报分数(有值(72));
    报分数(无);
}
```

</details>

---

## 11.14 FAQ

**Q: What's the advantage of enums over strings?**

Strings accept anything — `"优"`, `"优 "`, `"優"` are all different values, and typos compile happily. Enums have a fixed set of values; a typo won't compile, and the exhaustive check guarantees every case is handled. **Far safer.**

**Q: Why must data-free enums derive `Copy` to be taken from an array?**

Array elements are "look, yes; take, no" by default. `Copy` tells the compiler "this type is dirt cheap to duplicate (it's just a label)", so taking automatically duplicates and the array stays put. Enums with data might be costly to copy, so you must state your intent (`克隆` or derive `Clone`).

**Q: 选项 is so cumbersome — why not just allow "no value"?**

Because "forgot to check for nothing" is history's most expensive defect source. Rust makes "nothing" a **type that must be handled explicitly** — the hassle moves to writing time, and crashes never happen in front of users. Once fluent, the unpacking is two lines.

**Q: 匹配 or an 如果 chain?**

Discrete cases (enums, a few fixed values) suit `匹配`; continuous ranges (score bands) flow better as an `如果` chain. Both are legal — whichever reads clearer.

**Q: Can an enum hold a struct?**

Yes! A variant's luggage can be any type — structs, even other enums. The capstone projects later lean heavily on this combination play.

**Q: Is the `结果` type also an enum?**

Yes! `结果` = `成功(值)` / `错误(原因)` — same pattern as `选项`, specialized for "operations that might fail". Chapter 16, *Error Handling*, is its home turf.

---

## What's next

You can build all kinds of types now. But one chore repeats: the `最大值` function must be written once for integers, again for floats? Can "can be compared" be abstracted into one piece of code that serves every type?

**Chapter 12, "Generics"**, answers:

- The full syntax of `<T>` type parameters;
- Generic functions, structs and enums;
- Monomorphization: why generics are **not slower**;
- Laying groundwork for next chapter's "trait bounds".

See you in the next chapter!
