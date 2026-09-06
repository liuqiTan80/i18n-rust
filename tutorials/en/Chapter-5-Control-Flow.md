# Chapter 5: Control Flow

Programs so far ran "in a straight line" — one line after another, top to bottom. This chapter, the program learns three new skills: **making choices**, **doing repetitive work**, and **dispatching by case**.

---

## 5.0 Learning goals

By the end of this chapter, you will be able to:

1. Use `if / else` to send the program down different paths based on a condition.
2. Combine comparison and logic symbols into complex conditions.
3. Use `match` to list every possible case of a value in one place.
4. Repeat work with `loop`, `while` and `for`, controlling the rhythm with `break` and `continue`.

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

> 📖 **Control flow**: the family of statements that steer the program's "footsteps". `if`, `match`, `loop`, `while` and `for` in this chapter are all control-flow statements.

---

## 5.2 if / else: the fork in the road

### The simplest choice: if

```rust
fn main() {
    let temperature = 32;
    if temperature > 30 {
        println!("So hot — popsicle time!");
    }
    println!("program over");
}
```

Line by line:

- Line 3: `if temperature > 30 {` — checks whether the statement "temperature is greater than 30" is true or false. True: run what's inside the braces. False: skip the whole block.
- The condition is true (32 really is greater than 30), so "So hot — popsicle time!" prints.
- The last line is outside the braces, so it runs no matter what.

What you should see:

```
So hot — popsicle time!
program over
```

> 📖 **Condition**: a statement whose result is true or false, like `temperature > 30`. `if` must be followed by a condition.

> ⚠️ **Careful**: conditions are **not** wrapped in parentheses. `if (temperature > 30)` is other languages' style — Rust errors on it. Write `if temperature > 30` directly.

### Two paths: if / else

> 📖 **`else`**: a keyword meaning "if the condition fails, take this path instead".

```rust
fn main() {
    let temperature = 15;
    if temperature > 30 {
        println!("popsicle");
    } else {
        println!("wear a coat");
    }
}
```

15 is not greater than 30 — the condition is false, so the `else` path runs:

```
wear a coat
```

> ⚠️ **Careful**: `else` must hug the previous closing brace, written `} else {`. Only spaces between them — don't put `else` alone on a new line.

### More paths: else if

Two roads not enough? Chain more conditions with `else if`:

```rust
fn main() {
    let temperature = 25;
    if temperature > 30 {
        println!("hot");
    } else if temperature > 20 {
        println!("comfortable");
    } else if temperature > 10 {
        println!("a bit cool");
    } else {
        println!("cold");
    }
}
```

Execution works like **security gates**: check from top to bottom, and once you walk through the **first** gate that's true, all the later gates are ignored.

- 25 > 30? False. Next.
- 25 > 20? True! Enter, print "comfortable". Everything after is skipped.

```
comfortable
```

> 💡 **Metaphor**: an `else if` chain is a vending machine's buttons — press down the row, the first lit button wins, the rest don't matter.

### Forks inside forks: nesting

An `if` can hold another `if`:

```rust
fn main() {
    let homework_done = true;
    let nice_weather = true;
    if homework_done {
        if nice_weather {
            println!("go out and play!");
        } else {
            println!("read at home");
        }
    } else {
        println!("homework first");
    }
}
// 预期输出: go out and play!
```

Output: `go out and play!`. Nesting works — but too many levels make people dizzy. Two levels is a good ceiling.

---

## 5.3 Comparisons and logic: writing complex conditions

### Six comparison symbols

| Symbol | Read as | Example | Result |
|---|---|---|---|
| `==` | equal to | `5 == 5` | true |
| `!=` | not equal to | `5 != 3` | true |
| `>` | greater than | `5 > 3` | true |
| `<` | less than | `5 < 3` | false |
| `>=` | greater than or equal | `5 >= 5` | true |
| `<=` | less than or equal | `5 <= 4` | false |

> ⚠️ **Careful**: "is equal" uses **two** equal signs `==`. One `=` is assignment (putting something in a box — Chapter 3). Getting them confused hurts: `if score = 90` errors out.

> ⚠️ **Careful**: both sides must have the same type. `5 == 5.0` errors — integers and floats can't be compared directly.

### Three logic symbols

To glue conditions together, use logic symbols. They're always symbols:

| Symbol | Read as | Meaning | Life example |
|---|---|---|---|
| `&&` | and | True only if both sides are true | Homework done **and** weather nice → go out |
| `\|\|` | or | True if at least one side is true | Bus **or** subway — both get you there |
| `!` | not (negate) | True becomes false, false becomes true | **Not** raining |

```rust
fn main() {
    let homework_done = true;
    let nice_weather = false;

    if homework_done && nice_weather {
        println!("go out and play");     // not printed: nice_weather is false
    }
    if homework_done || nice_weather {
        println!("at least one holds");  // printed: homework_done is true
    }
    if !nice_weather {
        println!("the weather is bad");  // printed: false negated is true
    }
}
```

What you should see:

```
at least one holds
the weather is bad
```

> ⚠️ **Careful**: `&&` and `||` must be typed as English symbols — never substitute words. Key positions: `&&` is `Shift+7` twice; `||` is the vertical bar above Enter (`Shift+\`) twice.

> ✨ **Tip**: `&&` has a lazy superpower — when the left side is false, the right side isn't even looked at (the result is already decided). For example, in `if plates != 0 && total / plates > 5`, when plates is 0 the division never happens — saving you from a "division by zero" accident.

---

## 5.4 A surprise: if can be used as a "value"

Rust has a rare magic trick: the entire `if` structure can **evaluate to a value**, straight into a box.

```rust
fn main() {
    let score = 87;
    let grade = if score >= 90 { "A" } else if score >= 80 { "B" } else { "C" };
    println!("grade: {}", grade);
}
```

The result: `grade: B`.

Line 3 means: compute a result based on the score, and put that result into `grade`. The score is 87, so we walk into the second branch, and `grade` holds `"B"`.

Two important concepts hide behind this:

> 📖 **Statement**: an instruction that does something but produces no value, ending with a semicolon. Like `println!(...);`.
>
> 📖 **Expression**: a piece of writing that evaluates to a value. The number `5` is an expression (value: 5), `2 + 3` is an expression (value: 5) — even `if...{...} else {...}` is one.

> ⚠️ **Careful: a semicolon swallows the value!** If the last line inside the braces has **no semicolon**, that line's value is the value of the whole `if`. Add a semicolon and it becomes a statement — the value is gone:

```rust
let grade = if score >= 90 { "A"; } else { "C"; };   // ❌ error!
```

The error says, roughly: expected text, found "nothing". The semicolon swallowed `"A"`, leaving the braces empty.

One-sentence rule: **if you want the braces to hand over a value, the last line carries no semicolon.**

> 💡 **Metaphor**: an expression is "the answer to a question"; a statement is "an action performed". You ask `if`: "what's the result?" It hands you the last line inside the braces (semicolon-free).

---

## 5.5 match: the vending machine

One value with many possibilities, each needing a different action? Use `match`.

> 📖 **`match`**: a keyword — take a value, compare it against a list, and run whichever line it hits.

```rust
fn main() {
    let rank = 2;
    match rank {
        1 => println!("gold medal"),
        2 | 3 => println!("on the podium"),
        _ => println!("keep going"),
    }
}
```

Line by line:

- `match rank {`: take the value of `rank` and compare it against the list below.
- `1 => println!("gold medal"),`: if the value is 1, run what follows the arrow. Each line is a **match arm**.
- `2 | 3 => ...`: the vertical bar `|` reads "or" — a value of 2 or 3 both hit this arm.
- `_ => ...`: the underscore `_` means "everything else" — the catch-all.

The rank is 2, so the output is:

```
on the podium
```

> 📖 **Wildcard**: the `_` here means "whatever it is, the rest belongs to me".

> 📖 **Exhaustiveness**: the `match` list must cover **every possibility** — either list them all, or catch the rest with `_`. Miss one possibility and the compiler errors. This is Rust forcing you to "think it through".

### match evaluates to a value too

`match` is an expression, just like `if`:

```rust
let score = 95;
let grade = match score {
    90..=100 => "excellent",
    80..=89 => "good",
    70..=79 => "fair",
    60..=69 => "pass",
    _ => "fail",
};
```

> 📖 `90..=100`: a range form — `..=` means **both ends included** (90 through 100 all count). Remember the slice's `..`, head in tail out? Add the equals sign and "the tail comes too".

> ⚠️ **Careful**: each arm's arrow `=>` (equals + greater-than) must not be written `->`.

---

## 5.6 loop: tireless repetition

### loop: the bluntest repetition

> 📖 **`loop`**: a keyword — repeat a block of code over and over until you call it off.

```rust
fn main() {
    let mut times = 0;
    loop {
        times += 1;
        println!("round {}", times);
        if times == 3 {
            break;
        }
    }
    println!("done");
}
```

What you should see:

```
round 1
round 2
round 3
done
```

> 📖 **`break`**: a keyword meaning "jump out of the loop right now". Without it, `loop` runs until the end of time (an infinite loop).

### The loop can hand back a value too

`break` can be followed by a value, handed to the entire loop:

```rust
fn main() {
    let mut count = 0;
    let doubled = loop {
        count += 1;
        if count == 5 {
            break count * 2;
        }
    };
    println!("doubled: {}", doubled);
}
```

Line by line: the loop counts up one by one; at 5 it shouts "break" and throws out `5 * 2 = 10`. The whole `loop {...}` evaluates to 10, stored in `doubled`.

The result: `doubled: 10`.

---

## 5.7 while: repeat only while the condition holds

> 📖 **`while`**: a keyword — "keep repeating while the condition is true". When it turns false, stop automatically.

```rust
fn main() {
    let mut countdown = 3;
    while countdown > 0 {
        println!("countdown {}", countdown);
        countdown -= 1;
    }
    println!("liftoff!");
}
```

What you should see:

```
countdown 3
countdown 2
countdown 1
liftoff!
```

How it executes: before every round, check `countdown > 0`. 3, 2 and 1 all pass; at 0 the condition is false and the loop ends by itself.

> ⚠️ **Careful**: never forget to move the condition "toward false" inside the loop (here `countdown -= 1`). Forget it and the condition stays true forever — the program hangs inside the loop. That's an **infinite loop**. Stuck? `Ctrl+C` (hold Ctrl, press C) force-stops the program.

`while` and `loop+if+break` can do similar jobs, but `while` writes the condition up front — "when does it stop" is visible at a glance. Clearer.

---

## 5.8 for…in: one at a time, in line

Got a known count, or a row of things to process one by one? `for…in` is the easiest.

> 📖 **`for`** and **`in`**: keywords — read as "for each X, in some collection, do something in turn".

### With a range: repeat a fixed number of times

```rust
fn main() {
    for lap in 1..6 {
        println!("lap {}", lap);
    }
}
```

`1..6` is a range: **1 through 5** (remember? `..` is head in, tail out). To include 6, write `1..=6`.

What you should see:

```
lap 1
lap 2
lap 3
lap 4
lap 5
```

### With an array: process each element

Remember Chapter 4, adding five scores by hand in five lines? One line now:

```rust
fn main() {
    let scores = [88, 95, 76, 90, 82];
    let mut total = 0;
    for subject in scores {
        total += subject;
    }
    println!("total: {}", total);
}
```

`for subject in scores` means: take each element from the array in turn, put it into the box `subject`, and run the loop body once. After five rounds, the total is 431.

```
total: 431
```

> 💡 **Metaphor**: `for…in` is like a teacher taking roll — "for each student on the list, in the classroom, answer in turn". The loop runs itself; you don't count.

---

## 5.9 continue: skip this round, start the next

> 📖 **`continue`**: a keyword meaning "skip the rest of this round, jump straight to the next one".

```rust
fn main() {
    let score_sheet = [88, 55, 76, 42, 90];
    let mut passing = 0;
    for score in score_sheet {
        if score < 60 {
            continue;      // failing scores skip the counting
        }
        passing += 1;
    }
    println!("{} subjects passed", passing);
}
```

When it meets 55 and 42, `continue` skips the counting and moves straight to the next score. Output:

```
3 subjects passed
```

`break` vs `continue`: `break` is "the whole loop is done with me"; `continue` is "this round is done with me, next round please".

---

## 5.10 A complete example: the grade grader

### The complete code

```rust
fn main() {
    let score_sheet = [88, 55, 76, 42, 90];

    // Pass one: grade every score
    println!("--- grading ---");
    for subject_score in score_sheet {
        let grade = match subject_score {
            90..=100 => "excellent",
            80..=89 => "good",
            70..=79 => "fair",
            60..=69 => "pass",
            _ => "fail",
        };
        println!("{} → {}", subject_score, grade);
    }

    // Pass two: count the passing and find the highest
    let mut passing = 0;
    let mut highest = score_sheet[0];
    for subject_score in score_sheet {
        if subject_score >= 60 {
            passing += 1;
        }
        if subject_score > highest {
            highest = subject_score;
        }
    }
    println!("{} passed, highest {}", passing, highest);
}
```

### Line by line

- Line 2: five subjects' scores into an array.
- Line 5: `for…in` takes each score in turn, into the box `subject_score`.
- Lines 6–12: `match` as the grader. The range pattern `90..=100` includes both ends; `_` catches everything else (0–59). The whole `match` evaluates to a text value stored in `grade`.
- Line 13: print "score → grade".
- Line 17: the passing counter, mutable, starting at 0.
- Line 18: assume the first score is the highest, for now.
- Lines 19–26: walk the array again — count it if it's at least 60; update the max if it beats the current champion.
- Line 27: print the statistics.

### What you should see

```
--- grading ---
88 → good
55 → fail
76 → fair
42 → fail
90 → excellent
3 passed, highest 90
```

---

## 5.11 Common mistakes and how to fix them

### Mistake 1: a match that misses cases (the exhaustiveness error)

```rust
match rank {
    1 => println!("gold medal"),
    2 => println!("silver medal"),
}   // ❌ the rank could also be 3, 4, 5…
```

The error says, roughly: the match isn't exhaustive — not all possibilities are covered.

**Fix**: add the remaining cases, or catch-all with `_ => ...`.

### Mistake 2: a semicolon swallowed the if's value

```rust
let grade = if score >= 90 { "A"; } else { "C"; };   // ❌
```

**Fix**: remove the semicolon from the last line inside the braces: `{ "A" } else { "C" }`.

### Mistake 3: logic written as words

```rust
if a and b { }   // ❌ error: there is no `and` keyword in Rust
```

**Fix**: use symbols: `if a && b`.

### Mistake 4: one equal sign instead of two

```rust
if score = 90 { }   // ❌ that's an assignment — error
```

**Fix**: `if score == 90`. Mantra: **double equals to compare, single equals to store**.

### Mistake 5: the infinite loop

A `while` or `loop` that never lets its condition change spins forever.

**Fix**: check the loop body for a statement that "pushes the condition toward the end"; if stuck, `Ctrl+C`.

---

## 5.12 Chapter glossary

| Term | Meaning |
|---|---|
| while | A loop that repeats while the condition is true |
| for…in | The loop that takes collection elements one by one |
| Range | The `start..end` or `start..=end` forms |
| Match arm | One "if it hits, do this" line of a match list |
| Control flow | Statements that decide the program's execution order |
| Exhaustive | match must cover every possibility |
| Condition | A statement whose result is true or false |
| Wildcard | `_` — "everything else" in a match |
| continue | Skip this round, start the next |
| Infinite loop | A loop that never stops |
| Expression | A piece of writing that evaluates to a value |
| loop | Repeat until interrupted — the `loop` keyword |
| match | Dispatch by the value's cases — the `match` keyword |
| Nesting | Control flow inside control flow |
| if | Choose a path by condition |
| else | The path taken when the condition fails |
| Statement | An instruction that acts but produces no value |
| break | Jump out of the loop immediately |

---

## 5.13 Exercises

> 💪 Try first, then peek.

### Exercise 1: temperature advice

Write a program: temperature 28. Above 30 print "hot", above 20 print "comfortable", otherwise print "cold".

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let temperature = 28;
    if temperature > 30 {
        println!("hot");
    } else if temperature > 20 {
        println!("comfortable");
    } else {
        println!("cold");
    }
}
// 预期输出: comfortable
```

Output: `comfortable`.

</details>

### Exercise 2: translate weekdays with match

Create a variable `weekday_number` = 3, and use `match` to print the matching "Monday" through "Sunday"; any other number prints "invalid".

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let weekday_number = 3;
    let name = match weekday_number {
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        6 => "Saturday",
        7 => "Sunday",
        _ => "invalid",
    };
    println!("{}", name);
}
// 预期输出: Wednesday
```

Output: `Wednesday`.

</details>

### Exercise 3: accumulate

Use `for…in` to sum 1 through 100.

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let mut total = 0;
    for number in 1..=100 {
        total += number;
    }
    println!("total: {}", total);
}
// 预期输出: total: 5050
```

Output: `total: 5050`. Note `1..=100` includes the 100.

</details>

### Exercise 4: find the evens

Print all even numbers from 1 to 10 (hint: use `%` to test divisibility by 2).

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    for number in 1..=10 {
        if number % 2 == 0 {
            println!("{}", number);
        }
    }
}
```

Output: 2, 4, 6, 8, 10, one per line. `number % 2 == 0` means "the remainder after dividing by 2 is 0".

</details>

### Exercise 5 (challenge): spot the bug

This code wants to print 1, 2, 3 and then stop — but it's broken. Find it:

```rust
fn main() {
    let mut number = 1;
    loop {
        println!("{}", number);
        number += 1;
    }
}
```

<details>
<summary>Reference answer</summary>

There's no `break` in the loop — it runs forever (an infinite loop). Add the condition:

```rust
fn main() {
    let mut number = 1;
    loop {
        println!("{}", number);
        number += 1;
        if number > 3 {
            break;
        }
    }
}
```

Or simply switch to `for number in 1..=3 { println!("{}", number); }` — much simpler.

</details>

---

## 5.14 FAQ

**Q1: if, match, loops — how do I know which to use?**

Read the situation. **Choosing one of two/several paths** → `if`. **One value with many cases** → `match`. **Repeating work** → a loop. Among the three loop siblings: a ready-made count or collection → `for`; a complex stopping condition → `while`; "the loop hands back a value" → `loop`.

**Q2: Why does `1..6` specifically exclude 6?**

A big advantage: the range's length is exactly `6 - 1 = 5`, and one range's endpoint is exactly the next range's start (`1..4` meets `4..6`, no overlap, no gap). Slicing and pagination depend on this. Once used to it, it flows.

**Q3: Can match arms hold complex conditions?**

At the beginner stage, use `match` for "what does the value equal / which range does it fall in", and leave complex conditions to `if`. `match` has more advanced powers (unpacking data inside enums) — see Chapter 11.

**Q4: Can a loop nest inside a loop?**

Yes. The multiplication table, for instance, is two loops: the outer one for rows, the inner one for columns. You can challenge yourself in this chapter's exercises.

**Q5: My program is stuck — I suspect an infinite loop. Now what?**

Press `Ctrl+C` in the terminal to force-stop. Then inspect the loop body: is there a statement pushing the condition "toward the end"? Will the `while` condition eventually turn false?

**Q6: What happens if `if` has no `else` and the condition is false?**

Nothing — it skips the braces and carries on. `else` is optional.

---

## Chapter summary

1. **`if / else if / else`**: the fork in the road; conditions go without parentheses.
2. **Logic symbols**: `&&` (and), `||` (or), `!` (not) — always symbols.
3. **Expressions**: `if` and `match` both evaluate to values; the last line inside the braces carries no semicolon.
4. **`match`**: the vending machine — `|` combines cases, `..=` spans ranges, `_` catches the rest, exhaustiveness enforced.
5. **The three loop siblings**: `loop` (can return a value), `while` (condition up front), `for…in` (one at a time).
6. **`break`** exits the whole loop; **`continue`** skips only this round.

> 📖 If anything in this chapter confused you, check the chapter glossary above or the master glossary at the front of the book.

## What's next

In **Chapter 6, "Functions and Methods"**, you'll pack a set of instructions into a "magic box" and reuse it:

- **`fn`**: define your own instructions, call them whenever.
- **Parameters and return values**: how the box takes materials in and hands results out.
- **Methods**: functions attached to types, called with a dot (the `.len()` you've been using is one).
