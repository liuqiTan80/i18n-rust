# Chapter 5: Control Flow

Programs so far ran "in a straight line" — one line after another, top to bottom. This chapter, the program learns three new skills: **making choices**, **doing repetitive work**, and **dispatching by case**.

---

## 5.0 Learning goals

By the end of this chapter, you will be able to:

1. Use `如果 / 否则` (if / else) to send the program down different paths based on a condition.
2. Combine comparison and logic symbols into complex conditions.
3. Use `匹配` (match) to list every possible case of a value in one place.
4. Repeat work with `循环` (loop), `当` (while) and `对于…在` (for…in), controlling the rhythm with `中断` (break) and `继续` (continue).

---

## 5.1 The program's "footprints"

Programs so far executed like this — line after line, top to bottom:

```
Line 1 → Line 2 → Line 3 → end
```

That's **sequential execution**. But real life isn't a straight line:

- **If** it rains, take an umbrella; **otherwise**, wear a hat. → a **choice**.
- Run 5 laps around the track. → **repetition**.
- Red light stop, green light go, yellow light slow. → **dispatching by case**.

> 📖 **Control flow**: the family of statements that steer the program's "footsteps". `如果`, `匹配`, `循环`, `当` and `对于` in this chapter are all control-flow statements.

---

## 5.2 如果 / 否则: the fork in the road

### The simplest choice: 如果

```rust
函数 主函数() {
    让 温度 = 32;
    如果 温度 > 30 {
        打印行!("好热，吃冰棍！");
    }
    打印行!("程序结束");
}
```

Line by line:

- Line 3: `如果 温度 > 30 {` — checks whether the statement "temperature is greater than 30" is true or false. True: run what's inside the braces. False: skip the whole block.
- The condition is true (32 really is greater than 30), so "好热，吃冰棍！" prints.
- The last line is outside the braces, so it runs no matter what.

What you should see:

```
好热，吃冰棍！
程序结束
```

> 📖 **Condition**: a statement whose result is true or false, like `温度 > 30`. `如果` must be followed by a condition.

> ⚠️ **Careful**: conditions are **not** wrapped in parentheses. `如果 (温度 > 30)` is other languages' style — Rust errors on it. Write `如果 温度 > 30` directly.

### Two paths: 如果 / 否则

> 📖 **`否则`** (else): a keyword meaning "if the condition fails, take this path instead".

```rust
函数 主函数() {
    让 温度 = 15;
    如果 温度 > 30 {
        打印行!("吃冰棍");
    } 否则 {
        打印行!("穿外套");
    }
}
```

15 is not greater than 30 — the condition is false, so the `否则` path runs:

```
穿外套
```

> ⚠️ **Careful**: `否则` must hug the previous closing brace, written `} 否则 {`. Only spaces between them — don't put `否则` alone on a new line.

### More paths: 否则 如果

Two roads not enough? Chain more conditions with `否则 如果`:

```rust
函数 主函数() {
    让 温度 = 25;
    如果 温度 > 30 {
        打印行!("很热");
    } 否则 如果 温度 > 20 {
        打印行!("舒服");
    } 否则 如果 温度 > 10 {
        打印行!("有点凉");
    } 否则 {
        打印行!("冷");
    }
}
```

Execution works like **security gates**: check from top to bottom, and once you walk through the **first** gate that's true, all the later gates are ignored.

- 25 > 30? False. Next.
- 25 > 20? True! Enter, print "舒服". Everything after is skipped.

```
舒服
```

> 💡 **Metaphor**: a `否则 如果` chain is a vending machine's buttons — press down the row, the first lit button wins, the rest don't matter.

### Forks inside forks: nesting

An `如果` can hold another `如果`:

```rust
函数 主函数() {
    让 完成作业 = 真;
    让 天气好 = 真;
    如果 完成作业 {
        如果 天气好 {
            打印行!("出去玩！");
        } 否则 {
            打印行!("在家看书");
        }
    } 否则 {
        打印行!("先写作业");
    }
}
// 预期输出: 出去玩！
```

Output: `出去玩！`. Nesting works — but too many levels make people dizzy. Two levels is a good ceiling.

---

## 5.3 Comparisons and logic: writing complex conditions

### Six comparison symbols

| Symbol | Read as | Example | Result |
|---|---|---|---|
| `==` | equal to | `5 == 5` | 真 |
| `!=` | not equal to | `5 != 3` | 真 |
| `>` | greater than | `5 > 3` | 真 |
| `<` | less than | `5 < 3` | 假 |
| `>=` | greater than or equal | `5 >= 5` | 真 |
| `<=` | less than or equal | `5 <= 4` | 假 |

> ⚠️ **Careful**: "is equal" uses **two** equal signs `==`. One `=` is assignment (putting something in a box — Chapter 3). Getting them confused hurts: `如果 分数 = 90` errors out.

> ⚠️ **Careful**: both sides must have the same type. `5 == 5.0` errors — integers and floats can't be compared directly.

### Three logic symbols

To glue conditions together, use logic symbols. **They're symbols, not native words**:

| Symbol | Read as | Meaning | Life example |
|---|---|---|---|
| `&&` | and | True only if both sides are true | Homework done **and** weather nice → go out |
| `\|\|` | or | True if at least one side is true | Bus **or** subway — both get you there |
| `!` | not (negate) | True becomes false, false becomes true | **Not** raining |

```rust
函数 主函数() {
    让 完成作业 = 真;
    让 天气好 = 假;

    如果 完成作业 && 天气好 {
        打印行!("出去玩");        // not printed: 天气好 is false
    }
    如果 完成作业 || 天气好 {
        打印行!("至少满足一个");   // printed: 完成作业 is true
    }
    如果 !天气好 {
        打印行!("天气不好");       // printed: false negated is true
    }
}
```

What you should see:

```
至少满足一个
天气不好
```

> ⚠️ **Careful**: `&&` and `||` must be typed as English symbols. Writing the native word "而且" errors out. Key positions: `&&` is `Shift+7` twice; `||` is the vertical bar above Enter (`Shift+\`) twice.

> ✨ **Tip**: `&&` has a lazy superpower — when the left side is false, the right side isn't even looked at (the result is already decided). For example, in `如果 盘子 != 0 && 总数 / 盘子 > 5`, when 盘子 is 0 the division never happens — saving you from a "division by zero" accident.

---

## 5.4 A surprise: 如果 can be used as a "value"

Rust has a rare magic trick: the entire `如果` structure can **evaluate to a value**, straight into a box.

```rust
函数 主函数() {
    让 分数 = 87;
    让 等级 = 如果 分数 >= 90 { "优" } 否则 如果 分数 >= 80 { "良" } 否则 { "中" };
    打印行!("等级：{}", 等级);
}
```

The result: `等级：良`.

Line 3 means: compute a result based on the score, and put that result into `等级`. The score is 87, so we walk into the second branch, and `等级` holds `"良"`.

Two important concepts hide behind this:

> 📖 **Statement**: an instruction that does something but produces no value, ending with a semicolon. Like `打印行!(...);`.
>
> 📖 **Expression**: a piece of writing that evaluates to a value. The number `5` is an expression (value: 5), `2 + 3` is an expression (value: 5) — even `如果...{...} 否则 {...}` is one.

> ⚠️ **Careful: a semicolon swallows the value!** If the last line inside the braces has **no semicolon**, that line's value is the value of the whole `如果`. Add a semicolon and it becomes a statement — the value is gone:

```rust
让 等级 = 如果 分数 >= 90 { "优"; } 否则 { "中"; };   // ❌ error!
```

The error says, roughly: expected text, found "nothing". The semicolon swallowed `"优"`, leaving the braces empty.

One-sentence rule: **if you want the braces to hand over a value, the last line carries no semicolon.**

> 💡 **Metaphor**: an expression is "the answer to a question"; a statement is "an action performed". You ask `如果`: "what's the result?" It hands you the last line inside the braces (semicolon-free).

---

## 5.5 匹配: the vending machine

One value with many possibilities, each needing a different action? Use `匹配`.

> 📖 **`匹配`** (match): a keyword — take a value, compare it against a list, and run whichever line it hits.

```rust
函数 主函数() {
    让 名次 = 2;
    匹配 名次 {
        1 => 打印行!("金牌"),
        2 | 3 => 打印行!("领奖台"),
        _ => 打印行!("继续加油"),
    }
}
```

Line by line:

- `匹配 名次 {`: take the value of `名次` and compare it against the list below.
- `1 => 打印行!("金牌"),`: if the value is 1, run what follows the arrow. Each line is a **match arm**.
- `2 | 3 => ...`: the vertical bar `|` reads "or" — a value of 2 or 3 both hit this arm.
- `_ => ...`: the underscore `_` means "everything else" — the catch-all.

The rank is 2, so the output is:

```
领奖台
```

> 📖 **Wildcard**: the `_` here means "whatever it is, the rest belongs to me".

> 📖 **Exhaustiveness**: the `匹配` list must cover **every possibility** — either list them all, or catch the rest with `_`. Miss one possibility and the compiler errors. This is Rust forcing you to "think it through".

### 匹配 evaluates to a value too

`匹配` is an expression, just like `如果`:

```rust
让 分数 = 95;
让 等级 = 匹配 分数 {
    90..=100 => "优秀",
    80..=89 => "良好",
    70..=79 => "中等",
    60..=69 => "及格",
    _ => "不及格",
};
```

> 📖 `90..=100`: a range form — `..=` means **both ends included** (90 through 100 all count). Remember the slice's `..`, head in tail out? Add the equals sign and "the tail comes too".

> ⚠️ **Careful**: each arm's arrow `=>` (equals + greater-than) must not be written `->`.

---

## 5.6 循环: tireless repetition

### 循环: the bluntest repetition

> 📖 **`循环`** (loop): a keyword — repeat a block of code over and over until you call it off.

```rust
函数 主函数() {
    让 可变 次数 = 0;
    循环 {
        次数 += 1;
        打印行!("第{}遍", 次数);
        如果 次数 == 3 {
            中断;
        }
    }
    打印行!("收工");
}
```

What you should see:

```
第1遍
第2遍
第3遍
收工
```

> 📖 **`中断`** (break): a keyword meaning "jump out of the loop right now". Without it, `循环` runs until the end of time (an infinite loop).

### The loop can hand back a value too

`中断` can be followed by a value, handed to the entire loop:

```rust
函数 主函数() {
    让 可变 计数 = 0;
    让 翻倍结果 = 循环 {
        计数 += 1;
        如果 计数 == 5 {
            中断 计数 * 2;
        }
    };
    打印行!("翻倍结果：{}", 翻倍结果);
}
```

Line by line: the loop counts up one by one; at 5 it shouts "break" and throws out `5 * 2 = 10`. The whole `循环 {...}` evaluates to 10, stored in `翻倍结果`.

The result: `翻倍结果：10`.

---

## 5.7 当: repeat only while the condition holds

> 📖 **`当`** (while): a keyword — "keep repeating while the condition is true". When it turns false, stop automatically.

```rust
函数 主函数() {
    让 可变 倒数 = 3;
    当 倒数 > 0 {
        打印行!("倒数{}", 倒数);
        倒数 -= 1;
    }
    打印行!("出发！");
}
```

What you should see:

```
倒数3
倒数2
倒数1
出发！
```

How it executes: before every round, check `倒数 > 0`. 3, 2 and 1 all pass; at 0 the condition is false and the loop ends by itself.

> ⚠️ **Careful**: never forget to move the condition "toward false" inside the loop (here `倒数 -= 1`). Forget it and the condition stays true forever — the program hangs inside the loop. That's an **infinite loop**. Stuck? `Ctrl+C` (hold Ctrl, press C) force-stops the program.

`当` and `循环+如果+中断` can do similar jobs, but `当` writes the condition up front — "when does it stop" is visible at a glance. Clearer.

---

## 5.8 对于…在: one at a time, in line

Got a known count, or a row of things to process one by one? `对于…在` is the easiest.

> 📖 **`对于`** and **`在`**: keywords — read as "for each X, in some collection, do something in turn".

### With a range: repeat a fixed number of times

```rust
函数 主函数() {
    对于 编号 在 1..6 {
        打印行!("第{}圈", 编号);
    }
}
```

`1..6` is a range: **1 through 5** (remember? `..` is head in, tail out). To include 6, write `1..=6`.

What you should see:

```
第1圈
第2圈
第3圈
第4圈
第5圈
```

### With an array: process each element

Remember Chapter 4, adding five scores by hand in five lines? One line now:

```rust
函数 主函数() {
    让 分数 = [88, 95, 76, 90, 82];
    让 可变 总和 = 0;
    对于 单科 在 分数 {
        总和 += 单科;
    }
    打印行!("总分：{}", 总和);
}
```

`对于 单科 在 分数` means: take each element from the array in turn, put it into the box `单科`, and run the loop body once. After five rounds, the total is 431.

```
总分：431
```

> 💡 **Metaphor**: `对于…在` is like a teacher taking roll — "for each student on the list, in the classroom, answer in turn". The loop runs itself; you don't count.

---

## 5.9 继续: skip this round, start the next

> 📖 **`继续`** (continue): a keyword meaning "skip the rest of this round, jump straight to the next one".

```rust
函数 主函数() {
    让 分数表 = [88, 55, 76, 42, 90];
    让 可变 及格数 = 0;
    对于 分 在 分数表 {
        如果 分 < 60 {
            继续;      // failing scores skip the counting
        }
        及格数 += 1;
    }
    打印行!("及格{}门", 及格数);
}
```

When it meets 55 and 42, `继续` skips the counting and moves straight to the next score. Output:

```
及格3门
```

`中断` vs `继续`: `中断` is "the whole loop is done with me"; `继续` is "this round is done with me, next round please".

---

## 5.10 A complete example: the grade grader

### The complete code

```rust
函数 主函数() {
    让 分数表 = [88, 55, 76, 42, 90];

    // 第一遍：给每个分数评级
    打印行!("--- 评级 ---");
    对于 单科分数 在 分数表 {
        让 等级 = 匹配 单科分数 {
            90..=100 => "优秀",
            80..=89 => "良好",
            70..=79 => "中等",
            60..=69 => "及格",
            _ => "不及格",
        };
        打印行!("{}分 → {}", 单科分数, 等级);
    }

    // 第二遍：统计及格人数和最高分
    让 可变 及格数 = 0;
    让 可变 最高分 = 分数表[0];
    对于 单科分数 在 分数表 {
        如果 单科分数 >= 60 {
            及格数 += 1;
        }
        如果 单科分数 > 最高分 {
            最高分 = 单科分数;
        }
    }
    打印行!("及格{}人，最高分{}", 及格数, 最高分);
}
```

### Line by line

- Line 2: five subjects' scores into an array.
- Line 5: `对于…在` takes each score in turn, into the box `单科分数`.
- Lines 6–12: `匹配` as the grader. The range pattern `90..=100` includes both ends; `_` catches everything else (0–59). The whole `匹配` evaluates to a text value stored in `等级`.
- Line 13: print "score → grade".
- Line 17: the passing counter, mutable, starting at 0.
- Line 18: assume the first score is the highest, for now.
- Lines 19–26: walk the array again — count it if it's at least 60; update the max if it beats the current champion.
- Line 27: print the statistics.

### What you should see

```
--- 评级 ---
88分 → 良好
55分 → 不及格
76分 → 中等
42分 → 不及格
90分 → 优秀
及格3人，最高分90
```

---

## 5.11 Common mistakes and how to fix them

### Mistake 1: a match that misses cases (the exhaustiveness error)

```rust
匹配 名次 {
    1 => 打印行!("金牌"),
    2 => 打印行!("银牌"),
}   // ❌ the rank could also be 3, 4, 5…
```

The error says, roughly: the match isn't exhaustive — not all possibilities are covered.

**The fix**: add the remaining cases, or catch-all with `_ => ...`.

### Mistake 2: a semicolon swallowed the 如果's value

```rust
让 等级 = 如果 分数 >= 90 { "优"; } 否则 { "中"; };   // ❌
```

**The fix**: remove the semicolon from the last line inside the braces: `{ "优" } 否则 { "中" }`.

### Mistake 3: logic written as native words

```rust
如果 甲 而且 乙 { }   // ❌ error: expected `{`, found `而且`
```

**The fix**: use symbols: `如果 甲 && 乙`.

### Mistake 4: one equal sign instead of two

```rust
如果 分数 = 90 { }   // ❌ that's an assignment — error
```

**The fix**: `如果 分数 == 90`. Mantra: **double equals to compare, single equals to store**.

### Mistake 5: the infinite loop

A `当` or `循环` that never lets its condition change spins forever.

**The fix**: check the loop body for a statement that "pushes the condition toward the end"; if stuck, `Ctrl+C`.

---

## 5.12 Chapter glossary

| Term | Meaning |
|---|---|
| 当 | A loop that repeats while the condition is true — the `当` keyword |
| 对于…在 | The loop that takes collection elements one by one |
| Range | The `start..end` or `start..=end` forms |
| Match arm | One "if it hits, do this" line of a `匹配` list |
| Control flow | Statements that decide the program's execution order |
| Exhaustive | `匹配` must cover every possibility |
| Condition | A statement whose result is true or false |
| Wildcard | `_` — "everything else" in a match |
| 继续 | Skip this round, start the next |
| Infinite loop | A loop that never stops |
| Expression | A piece of writing that evaluates to a value |
| 循环 | Repeat until interrupted — the `循环` keyword |
| 匹配 | Dispatch by the value's cases — the `匹配` keyword |
| Nesting | Control flow inside control flow |
| 如果 | Choose a path by condition — the `如果` keyword |
| 否则 | The path taken when the condition fails — the `否则` keyword |
| Statement | An instruction that acts but produces no value |
| 中断 | Jump out of the loop immediately — the `中断` keyword |

---

## 5.13 Exercises

### Exercise 1: temperature advice

Write a program: temperature 28. Above 30 print "热", above 20 print "舒适", otherwise print "冷".

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 温度 = 28;
    如果 温度 > 30 {
        打印行!("热");
    } 否则 如果 温度 > 20 {
        打印行!("舒适");
    } 否则 {
        打印行!("冷");
    }
}
// 预期输出: 舒适
```

Output: `舒适`.

</details>

### Exercise 2: translate weekdays with 匹配

Create a variable `星期编号` = 3, and use `匹配` to print the matching "星期一" through "星期日"; any other number prints "无效".

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 星期编号 = 3;
    让 名字 = 匹配 星期编号 {
        1 => "星期一",
        2 => "星期二",
        3 => "星期三",
        4 => "星期四",
        5 => "星期五",
        6 => "星期六",
        7 => "星期日",
        _ => "无效",
    };
    打印行!("{}", 名字);
}
// 预期输出: 星期三
```

Output: `星期三`.

</details>

### Exercise 3: accumulate

Use `对于…在` to sum 1 through 100.

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 可变 总和 = 0;
    对于 数 在 1..=100 {
        总和 += 数;
    }
    打印行!("总和：{}", 总和);
}
// 预期输出: 总和：5050
```

Output: `总和：5050`. Note `1..=100` includes the 100.

</details>

### Exercise 4: find the evens

Print all even numbers from 1 to 10 (hint: use `%` to test divisibility by 2).

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    对于 数 在 1..=10 {
        如果 数 % 2 == 0 {
            打印行!("{}", 数);
        }
    }
}
```

Output: 2, 4, 6, 8, 10, one per line. `数 % 2 == 0` means "the remainder after dividing by 2 is 0".

</details>

### Exercise 5 (challenge): spot the bug

This code wants to print 1, 2, 3 and then stop — but it's broken. Find it:

```rust
函数 主函数() {
    让 可变 数 = 1;
    循环 {
        打印行!("{}", 数);
        数 += 1;
    }
}
```

<details>
<summary>Reference answer</summary>

There's no `中断` in the loop — it runs forever (an infinite loop). Add the condition:

```rust
函数 主函数() {
    让 可变 数 = 1;
    循环 {
        打印行!("{}", 数);
        数 += 1;
        如果 数 > 3 {
            中断;
        }
    }
}
```

Or simply switch to `对于 数 在 1..=3 { 打印行!("{}", 数); }` — much simpler.

</details>

---

## 5.14 FAQ

**Q1: 如果, 匹配, loops — how do I know which to use?**

Read the situation. **Choosing one of two/several paths** → `如果`. **One value with many cases** → `匹配`. **Repeating work** → a loop. Among the three loop siblings: a ready-made count or collection → `对于`; a complex stopping condition → `当`; "the loop hands back a value" → `循环`.

**Q2: Why does `1..6` specifically exclude 6?**

A big advantage: the range's length is exactly `6 - 1 = 5`, and one range's endpoint is exactly the next range's start (`1..4` meets `4..6`, no overlap, no gap). Slicing and pagination depend on this. Once used to it, it flows.

**Q3: Can 匹配 arms hold complex conditions?**

At the beginner stage, use `匹配` for "what does the value equal / which range does it fall in", and leave complex conditions to `如果`. `匹配` has more advanced powers (unpacking data inside enums) — see Chapter 11.

**Q4: Can a loop nest inside a loop?**

Yes. The multiplication table, for instance, is two loops: the outer one for rows, the inner one for columns. You can challenge yourself in this chapter's exercises.

**Q5: My program is stuck — I suspect an infinite loop. Now what?**

Press `Ctrl+C` in the terminal to force-stop. Then inspect the loop body: is there a statement pushing the condition "toward the end"? Will the `当` condition eventually turn false?

**Q6: What happens if `如果` has no `否则` and the condition is false?**

Nothing — it skips the braces and carries on. `否则` is optional.

---

## Chapter summary

1. **`如果 / 否则 如果 / 否则`**: the fork in the road; conditions go without parentheses.
2. **Logic symbols**: `&&` (and), `||` (or), `!` (not) — always symbols, never native words.
3. **Expressions**: `如果` and `匹配` both evaluate to values; the last line inside the braces carries no semicolon.
4. **`匹配`**: the vending machine — `|` combines cases, `..=` spans ranges, `_` catches the rest, exhaustiveness enforced.
5. **The three loop siblings**: `循环` (can return a value), `当` (condition up front), `对于…在` (one at a time).
6. **`中断`** exits the whole loop; **`继续`** skips only this round.

> 📖 If anything in this chapter confused you, check the chapter glossary above or the master glossary at the front of the book.

## What's next

In **Chapter 6, "Functions and Methods"**, you'll pack a set of instructions into a "magic box" and reuse it:

- **`声明 函数`**: define your own instructions, call them whenever.
- **Parameters and return values**: how the box takes materials in and hands results out.
- **Methods**: functions attached to types, called with a dot (the `.长度()` you've been using is one).
