# Chapter 11: Enums and Pattern Matching

## 11.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain the difference between enums and structs ("or" vs "and");
2. Define **simple enums** and **enums with data**;
3. Destructure enums with `match` and extract their data;
4. Read and fix **E0004** (non-exhaustive match);
5. Use the **`Option`** type for "there may be a value, or not";
6. Explain why Rust has no "null pointer" trap.

---

## 11.1 Enums: a multiple-choice question

Chapter 10's struct was "**and**": one student card **simultaneously** has a name, an age and a score.

But life is mostly "**or**":

- A traffic light is **either** red, **either** yellow, **either** green;
- A coin is **either** heads, **either** tails;
- An answer is **either** correct, **either** wrong, **either** unattempted.

📖 **Enum**: a type where a value can only be **one of the listed cases**. The keyword is `enum`.

💡 **Metaphor**: an enum is a **multiple-choice answer sheet** — you can only pick one of the given options. No "other", and definitely not two at once.

## 11.2 Simple enums: the traffic light

```rust
enum TrafficLight {
    Red,
    Yellow,
    Green,
}
```

📖 **Variant**: each possible value of an enum. `TrafficLight` has three variants.

Use them as `EnumName::VariantName`:

```rust
fn action(light: TrafficLight) {
    match light {
        TrafficLight::Red => println!("stop"),
        TrafficLight::Yellow => println!("get ready"),
        TrafficLight::Green => println!("go"),
    }
}

fn main() {
    action(TrafficLight::Red);
    action(TrafficLight::Yellow);
    action(TrafficLight::Green);
}
```

Output:

```
stop
get ready
go
```

Why is this better than 1, 2, 3 for the lights? **The names carry meaning**, and there's no way to pass a "light #4" — the compiler simply refuses.

## 11.3 Enums with data

Variants can also **carry luggage**:

```rust
enum Grade {
    Score(i32),        // carries an integer
    Absent,            // carries nothing
    Excused(String),   // carries a note (the excuse)
}
```

Read it as: "a Grade has three possible cases: a concrete score, absent, or excused (with a reason)."

Building values:

```rust
let a = Grade::Score(88);
let b = Grade::Absent;
let c = Grade::Excused(String::from("already studied this"));
```

> ⚠️ **Careful**: prefer `String` over `&str` in variant luggage — the latter drags in lifetime annotations (Chapter 14's headache). Beginners should steer clear.

### Enums vs structs

| | Struct | Enum |
|---|---|---|
| Meaning | These fields hold **at the same time** | These cases are **one of** |
| Example | Student = name + age + score | Grade = score / absent / excused |

The two can also **nest** (you'll see it in Chapter 13's traits and Chapter 15's collections) — powerful combinations.

## 11.4 match: unpacking the luggage

Chapter 5 taught `match` basics (matching integers, ranges). For enums, `match` can also **open a variant's luggage and take the data out**:

```rust
fn grade_report(one: Grade) {
    match one {
        Grade::Score(value) => println!("score: {}", value),
        Grade::Absent => println!("absent, no grade"),
        Grade::Excused(reason) => println!("excused: {}", reason),
    }
}
```

`Grade::Score(value)` reads: "if this is a score-carrying grade, **take the integer out and call it `value`**." The parentheses are the unpacking action.

Output:

```
score: 88
absent, no grade
excused: already studied this
```

## 11.5 Exhaustiveness: not one case missed

Rust's iron law: **the match must cover every variant**. Miss one and you get **E0004**:

```rust
enum Light { Red, Green }

match current {
    Light::Red => println!("stop"),
    // Green is missing ❌
}
```

The real error:

```
error[E0004]: non-exhaustive patterns: pattern `Light::Green` not covered
```

**Two fixes**: ① list every variant; ② catch-all with `_` (Chapter 5):

```rust
match current {
    Light::Red => println!("stop"),
    _ => println!("every other case"),
}
```

> 💡 **Why so strict?** Imagine adding a "blinking" variant to `TrafficLight` later — every match point that ignores it instantly errors with E0004. The compiler **marches you** through updating every corner; none escapes. That's the biggest source of an enum's sense of safety.

## 11.6 Bonus: the full life of patterns

So far, patterns only appeared inside `match`. Actually patterns show up in a **bunch of other places**, and come in many flavors. This section completes the craft — from now on, wherever you see "unpacking a value" in code, you'll recognize a pattern at work.

### if let: caring about one case only

`match` demands exhaustiveness (just covered in 11.5). But if you only care about **one case** and lump the rest together, `match` is wordy:

```rust
let score: Option<i32> = Some(95);

if let Some(value) = score {
    println!("has a score: {}", value);
} else {
    println!("no score");
}
```

📖 **`if let`**: `if let PATTERN = value { ... } else { ... }` — try `value` against `PATTERN`: **on a match** unpack and run the first block; **on failure** walk the `else`.

💡 **Metaphor**: `match` is a vending machine where you press **every** button; `if let` is pressing **one** button — it dispenses on a hit; miss, and you move to the next machine.

`if let Some(value) = score` reads: "if `score` is a `Some(...)`, take the inner value and call it `value`." Identical to `match score { Some(value) => ..., None => ... }` — just shorter.

### while let: the loop version of if let

```rust
let mut remaining: Option<i32> = Some(3);
while let Some(value) = remaining {
    println!("{} remaining", value);
    remaining = None;
}
```

📖 **`while let`**: `while let PATTERN = value { ... }` — as long as `value` matches `PATTERN`, unpack, run the body once, then **try again**; on failure the loop ends.

(The example above deliberately sets `remaining` to `None`, so the loop runs once; real scenarios usually repeatedly draw from a "changing value", like pulling from an iterator.)

### let else: if it won't unpack, leave

Sometimes "it won't unpack" means **there's no point running the rest**. Rust 1.65 added a syntax for this:

```rust
let input: Option<i32> = Some(7);
let Some(number) = input else {
    return;
};
println!("let-else got {}", number);
```

📖 **`let else`**: `let PATTERN = value else { exit code };` — the pattern **must** match; on a match, continue; on failure, run the `else` block (usually `return` or `break`), and **the code after never executes**.

> ⚠️ **Note**: the `else` block **must end in a non-returning way** (`return`, `break`, `continue`, `panic!` etc.) — the compiler must guarantee "if it didn't unpack, we can never reach the code below".

### Match guards: a threshold on a match arm

Patterns handle "is it this shape", but sometimes you also need a **condition**. A guard is an `if` placed before the `=>`:

```rust
let number = 7;
match number {
    value if value > 5 => println!("greater than 5: {}", value),
    _ => println!("not greater than 5"),
}
```

Read: the first arm's full threshold is "matched a `value` **and** `value > 5`". If the condition fails, fall to the next arm.

### @ bindings: match and keep the name

Want "the range matched successfully, AND I keep the matched value"? `@` exists for this:

```rust
let number = 7;
match number {
    value @ 1..=5 => println!("1 to 5: {}", value),
    _ => println!("out of range"),
}
```

📖 **@ binding**: `name @ PATTERN` — store the matched value **simultaneously** into `name`. `value @ 1..=5` reads "if the number is between 1 and 5, call the number itself `value`".

Without `@`, `1..=5` can only judge "in range or not" — the concrete number is lost. `@` fills that gap.

### Or-patterns: one arm, several values

Several **same-type** values sharing one path? Use `|` (vertical bar) to merge them into one arm:

```rust
let number = 7;
match number {
    1 | 2 => println!("1 or 2"),
    _ => println!("other"),
}
```

Enums work the same: `Grade::Absent | Grade::Excused(_) => println!("no concrete score")` — both "no score" cases share one arm.

### Structs and tuples: unpacking packed data

Patterns unpack structs and tuples too, not just enums:

```rust
struct Point {
    x: i32,
    y: i32,
}

let point_a = Point { x: 10, y: 20 };
match point_a {
    Point { x, y } => println!("x={} y={}", x, y),
}

let (a, b) = (1, 2);
println!("{} {}", a, b);
```

- The struct pattern `Point { x, y }`: unpacks **by field name** — order doesn't matter;
- The tuple pattern `(a, b)`: unpacks **by position**, one-to-one.

💡 **Metaphor**: enum patterns are like opening delivery boxes (which courier is it? what's inside?); struct patterns pick items **by label**; tuple patterns call names **by seat number**.

### Complete example one: if let, while let and let else

```rust
fn main() {
    // if let
    let score: Option<i32> = Some(95);
    if let Some(value) = score {
        println!("has a score: {}", value);
    } else {
        println!("no score");
    }

    // while let
    let mut remaining: Option<i32> = Some(3);
    while let Some(value) = remaining {
        println!("{} remaining", value);
        remaining = None;
    }

    // let else
    let input: Option<i32> = Some(7);
    let Some(number) = input else {
        return;
    };
    println!("let-else got {}", number);
}
```

Output:

```
has a score: 95
3 remaining
let-else got 7
```

### Complete example two: guards, @ and destructuring

Guards, `@`, `|`, struct destructuring and tuple destructuring in one program:

```rust
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    // Match guard
    let number = 7;
    match number {
        value if value > 5 => println!("greater than 5: {}", value),
        _ => println!("not greater than 5"),
    }

    // @ binding
    match number {
        value @ 1..=5 => println!("1 to 5: {}", value),
        _ => println!("out of range"),
    }

    // Or-pattern
    match number {
        1 | 2 => println!("1 or 2"),
        _ => println!("other"),
    }

    // Struct destructuring
    let point_a = Point { x: 10, y: 20 };
    match point_a {
        Point { x, y } => println!("x={} y={}", x, y),
    }

    // Tuple destructuring
    let (a, b) = (1, 2);
    println!("{} {}", a, b);
}
```

Output:

```
greater than 5: 7
out of range
other
x=10 y=20
1 2
```

The first three are `7` passing through the guard, the range and `|` in turn; the last two show how a struct and a tuple unpack all their data in one breath.

### Refutable vs irrefutable: the two faces of patterns

One last concept. Patterns come in two kinds:

📖 **Refutable**: might fail. Like `Some(value)` — if the value is `None`, no match.
📖 **Irrefutable**: **always** matches. Like `x`, `(a, b)` — any value unpacks.

**Position matters**:

- `let` bindings, function parameters, `for` loops: require **irrefutable** patterns (the compiler checks);
- `match` arms, `if let`, `while let`: **either kind** (a miss walks another path or ends).

The reverse also fails — an irrefutable pattern inside `if let` makes the compiler grumble that it's pointless (hinting "this pattern always succeeds; no need for if let").

**The most common trap**: writing `let Some(value) = score;` — the compiler reports **E0005** directly:

```
error[E0005]: refutable pattern in local binding: `None` not covered
```

**Fix**: leave the "might not unpack" shell-splitting to `match` / `if let` / `let else`; use plain `let` only for things that are "definitely there".

---

## 11.7 Option: handling "maybe there's nothing"

A common situation: **the value may not exist**. Like "the class's highest score" — what if nobody's in the class?


Many languages use "null pointers" for "nothing", birthing the billion-dollar mistake: forget to check for null and the program crashes on the spot. Rust's answer is a built-in enum:

```rust
enum Option<T> {
    Some(T),    // there is a value, wrapped inside
    None,       // there is no value
}
```

(`<T>` is a generic — Chapter 12 explains. For now, read it as "an option of what type the value is".)

📖 **Option**: the type for "there may be a value, or not". `Some` = there is; `None` = there isn't.

```rust
let has_score: Option<i32> = Some(95);
let no_score: Option<i32> = None;

match has_score {
    Some(value) => println!("the score is: {}", value),
    None => println!("no score"),
}
match no_score {
    Some(value) => println!("the score is: {}", value),
    None => println!("no score"),
}
```

Output:

```
the score is: 95
no score
```

The key idea: **"no value" is itself a value that must be handled explicitly**. You can't pretend `None` doesn't exist — to take a value out of an option, you must use `match` and care for both cases (or Chapter 16's convenience methods). "Forgot to check for null" becomes **impossible** in Rust.

---

## 11.8 A complete example: grading the answer sheet

An enum array + match + loops, grading a 10-question answer sheet:

```rust
// Each question's outcome (derive Copy: data-free enums can be copied casually)
#[derive(Copy, Clone)]
enum Answer {
    Correct,
    Wrong,
    Unanswered,
}

// The scoring rule per question
fn points_for(answer: Answer) -> i32 {
    match answer {
        Answer::Correct => 5,
        Answer::Wrong => 0,
        Answer::Unanswered => 0,
    }
}

// The overall rating
enum Rating { Excellent, Good, Average, Poor }

fn rating_for(total: i32) -> Rating {
    if total >= 45 { return Rating::Excellent; }
    if total >= 35 { return Rating::Good; }
    if total >= 20 { return Rating::Average; }
    Rating::Poor
}

fn main() {
    // A 10-question answer sheet
    let sheet = [
        Answer::Correct, Answer::Correct, Answer::Wrong, Answer::Correct, Answer::Unanswered,
        Answer::Correct, Answer::Correct, Answer::Wrong, Answer::Correct, Answer::Correct,
    ];

    let mut total = 0;
    let mut correct_count = 0;
    for index in 0..10 {
        total = total + points_for(sheet[index]);
        match sheet[index] {
            Answer::Correct => correct_count = correct_count + 1,
            Answer::Wrong => {}
            Answer::Unanswered => {}
        }
    }
    println!("{} correct, {} points", correct_count, total);

    match rating_for(total) {
        Rating::Excellent => println!("overall: excellent — great job!"),
        Rating::Good => println!("overall: good — nice!"),
        Rating::Average => println!("overall: average — keep going"),
        Rating::Poor => println!("overall: poor — work harder"),
    }
}
```

## 11.9 Line by line

- **Line 2**: `#[derive(Copy, Clone)]` — `Answer` carries no data, so it can be **casually copied** like an integer. This matters: array elements can't be taken out by default (E0508), but with `Copy`, `sheet[index]` automatically duplicates one for handing to functions.
- **Lines 10–16**: `points_for` maps the three cases to numbers via match. The arms' right sides are bare expressions `5` and `0`.
- **Line 19**: `Rating` is a pure label enum.
- **Lines 21–26**: `rating_for` converts a total score to a rating via Chapter 5's `if` chain, with `Rating::Poor` as the floor.
- **Lines 30–33**: the enum array — 10 elements.
- **Lines 37–43**: one loop does two jobs — accumulate the total, count correct answers. `Answer::Wrong => {}` and `Answer::Unanswered => {}` are empty braces: match arms **must** list all three cases, so even these "do nothing" arms must be written out.
- **Lines 46–51**: match the rating enum into well-wishes — all four variants, none missed.

## 11.10 What you should see

```
7 correct, 35 points
overall: good — nice!
```

7 correct × 5 points = 35, landing in the 35–44 range → good.

---

## 11.11 Common mistakes and how to fix them

### Mistake one: E0004 non-exhaustive match

A variant is missing. **Fix**: list it, or catch-all with `_`. The error lists the "uncovered patterns" — copy them in.

### Mistake two: E0508 can't move out of an array

`函数(数组[下标])` tries to **take** the element, but the array refuses. **Fix**: derive `Copy` for the enum (when it has no data), or pass a reference `&sheet[index]` (adjust the parameter), or `.clone()`.

### Mistake three: a variant holding `&str` errors with E0106

Writing `Excused(&str)` in a variant demands lifetime annotations. **Fix**: use `String`: `Excused(String)`, building with `String::from("...")`.

### Mistake four: printing an enum directly

`println!("{}", Answer::Correct)` errors. **Fix**: `#[derive(Debug)]` + `{:?}` (Chapter 10's old trick).

### Mistake five: assuming the option's value

`Some(95)` can't be used directly as `95` — it's still wrapped in a shell. **Fix**: unpack with `match`, or Chapter 16's `.expect()` and friends.

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
| Option | The type for "there is or there isn't" |
| Some | The option variant for "there is a value" |
| None | The option variant for "no value" |
| Null pointer | Other languages' dangerous way of expressing "nothing" — replaced by Option in Rust |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 11.13 Exercises

> 💪 Try first, then peek.

### Exercise one: the coin

Define an enum `Coin { Heads, Tails }`, and write a function `announce(coin: Coin)` printing the result via match.

<details>
<summary>🔍 View answer</summary>

```rust
enum Coin { Heads, Tails }

fn announce(coin: Coin) {
    match coin {
        Coin::Heads => println!("heads!"),
        Coin::Tails => println!("tails!"),
    }
}

fn main() {
    announce(Coin::Heads);
    announce(Coin::Tails);
}
```

</details>

### Exercise two: shapes with data

Define an enum `Shape { Circle(i32), Rect(i32, i32) }` (circle carries a radius; rectangle carries length and width), and a function `area`: circles use `3 * radius * radius` as an approximation, rectangles length × width.

<details>
<summary>🔍 View answer</summary>

```rust
enum Shape {
    Circle(i32),
    Rect(i32, i32),
}

fn area(shape: Shape) -> i32 {
    match shape {
        Shape::Circle(radius) => 3 * radius * radius,
        Shape::Rect(length, width) => length * width,
    }
}

fn main() {
    println!("{}", area(Shape::Circle(10)));    // 300
    println!("{}", area(Shape::Rect(3, 4)));    // 12
}
```

</details>

### Exercise three: unpacking the option

Write a function `report_score(score: Option<i32>)`: with a value, print "scored N points"; without, print "no record". Call it with `Some(72)` and `None`.

<details>
<summary>🔍 View answer</summary>

```rust
fn report_score(score: Option<i32>) {
    match score {
        Some(value) => println!("scored {} points", value),
        None => println!("no record"),
    }
}

fn main() {
    report_score(Some(72));
    report_score(None);
}
```

</details>

---

## 11.14 FAQ

**Q: What's the advantage of enums over strings?**

Strings accept anything — `"excellent"`, `"excellent "`, `"Excellent"` are all different values, and typos compile happily. Enums have a fixed set of values; a typo won't compile, and the exhaustive check guarantees every case is handled. **Far safer.**

**Q: Why must data-free enums derive `Copy` to be taken from an array?**

Array elements are "look, yes; take, no" by default. `Copy` tells the compiler "this type is dirt cheap to duplicate (it's just a label)", so taking automatically duplicates and the array stays put. Enums with data might be costly to copy, so you must state your intent (`clone` or derive `Clone`).

**Q: Option is so cumbersome — why not just allow "no value"?**

Because "forgot to check for nothing" is history's most expensive defect source. Rust makes "nothing" a **type that must be handled explicitly** — the hassle moves to writing time, and crashes never happen in front of users. Once fluent, the unpacking is two lines.

**Q: match or an if chain?**

Discrete cases (enums, a few fixed values) suit `match`; continuous ranges (score bands) flow better as an `if` chain. Both are legal — whichever reads clearer.

**Q: Can an enum hold a struct?**

Yes! A variant's luggage can be any type — structs, even other enums. The capstone projects later lean heavily on this combination play.

**Q: Is the `Result` type also an enum?**

Yes! `Result` = `Ok(value)` / `Err(reason)` — same pattern as `Option`, specialized for "operations that might fail". Chapter 16, *Error Handling*, is its home turf.

---

## What's next

You can build all kinds of types now. But one chore repeats: the `max_of` function must be written once for integers, again for floats? Can "can be compared" be abstracted into one piece of code that serves every type?

**Chapter 12, "Generics"**, answers:

- The full syntax of `<T>` type parameters;
- Generic functions, structs and enums;
- Monomorphization: why generics are **not slower**;
- Laying groundwork for next chapter's "trait bounds".

See you in the next chapter!
