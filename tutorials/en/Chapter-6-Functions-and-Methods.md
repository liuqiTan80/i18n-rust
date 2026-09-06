# Chapter 6: Functions and Methods

## 6.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain what a function is using the **magic box** picture;
2. Write your own functions **with parameters and return values**;
3. Tell **statements** and **expressions** apart, and know how semicolons "swallow" return values;
4. Hand back a result early with the **`return`** keyword;
5. Write a **recursive** function (one that calls itself);
6. Attach **methods** to a type with an **`impl`** block, and recognize the three forms of **`self`**;
7. Meet the **unit type** `()`.

---

## 6.1 What is a function: the magic box

💡 **Metaphor**: imagine a **magic box**:

- You push things in through the **top opening** (these are **parameters**);
- The inside of the box does its work (this is the **function body**);
- Then it spits the result out of the **bottom opening** (this is the **return value**).

Take a "juicer" box: push in apples (parameters), it squeezes inside (the body), and out comes apple juice (the return value).

📖 **Function**: a named, reusable set of instructions — written **`fn`** in Rust.

You've been using functions all along — `println!` is one (a special "macro" function; Chapter 2 explained the exclamation mark). What you'll learn now is **building boxes of your own**.

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
fn greet() {
    println!("Hello!");
}

fn main() {
    greet();
    greet();
}
```

What you should see:

```
Hello!
Hello!
```

Five parts, taken apart:

| Part | Example | Notes |
|---|---|---|
| The `fn` keyword | `fn` | Tells the computer "a box is being built here" |
| Function name | `greet` | The box's name — you choose it (verbs recommended) |
| Parentheses | `()` | Where parameters go; none for now, so empty |
| Curly braces | `{ }` | The box's "body" — the actual instructions |
| The call | `greet();` | Name + parentheses + semicolon: make the box work |

> ⚠️ **Careful**: **defining** a function (building the box) and **calling** it (making the box work) are two different things. Define without calling and the box never starts. A program's entrance is always `main`.

---

## 6.3 Parameters: pushing things into the box

📖 **Parameter**: the input a function receives — the name written when defining it.
📖 **Argument**: the actual value pushed in when calling it.

💡 **Metaphor**: the juicer's manual says "please insert **fruit**" — "fruit" is the **parameter** (a placeholder name); the actual apple you push in is the **argument**.

### One parameter

```rust
fn greet_person(name: &str) {
    println!("Hello, {}!", name);
}

fn main() {
    greet_person("Xiaoming");
    greet_person("Xiaohong");
}
```

> 📖 **`&str`** (string slice): borrowed text, written as a `"..."` literal. Chapter 9 covers it — for now: text in double quotes belongs to this type.

Parameters are written **`name: type`** — Rust requires a **type for every parameter**, no exceptions.

### Multiple parameters

Separate them with **commas**:

```rust
fn sum(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    let total = sum(3, 7);
    println!("sum: {}", total);
}
// 预期输出: sum: 10
```

Output: `sum: 10`

Note the new symbol **`->`** (minus + greater-than, meaning "points to"):

- `-> i32` says "what this box spits out is an integer";
- No `->` means the box spits out nothing (strictly speaking, it spits out "empty" — section 6.8).

> ⚠️ **Careful**: the arguments at call time must **match the parameters in count and order**. `sum(3)` is one short; `sum("three", 7)` has the wrong type — both error out.

---

## 6.4 Return values: the box hands out a result

Three ways for a function to hand over a result.

### Way one: the trailing expression (no semicolon)

If the **last line** of the function body is an **expression** (something that evaluates to a value) and **carries no semicolon**, its value is the return value:

```rust
fn square(n: i32) -> i32 {
    n * n
}
```

`n * n` has no semicolon → its value is "spat out".

### Way two: the `return` keyword

📖 **`return`**: hand the value over immediately; the function ends right there.

```rust
fn max_of(a: i32, b: i32) -> i32 {
    if a > b {
        return a;
    }
    b
}
```

Read it as: "if a is bigger, **hand back** a immediately; otherwise walk to the last line and hand back b."

`return` is common for **early exits**: condition met, hand in the answer, leave — no need to run anything below.

### Way three: the block expression

Chapter 5 said: a braced block is itself an expression, and its last line (no semicolon) is the block's value:

```rust
let block_total = {
    let a = 10;
    let b = 20;
    a + b
};
println!("the block's value: {}", block_total);
// 预期输出: the block's value: 30
```

Output: `the block's value: 30`

### ⚠️ The semicolon trap: the most common mistake

What if you **add a semicolon** to `n * n`?

```rust
// 预期错误: E0308
fn square(n: i32) -> i32 {
    n * n;    // ❌ extra semicolon
}
```

The error: **E0308 mismatched types: expected `i32`, found `()`**.

💡 **Metaphor**: a semicolon is a "full stop" — it turns a sentence into a statement (done and gone, nothing left behind). The function promised to spit out an integer, but all it produced was a statement (the empty value `()`), so of course the compiler objects.

**The mantra**: **for a return value, the last line carries no semicolon.**

---

## 6.5 Functions can be defined anywhere

`main` can come first, with the functions it calls defined after:

```rust
fn main() {
    println!("area: {}", rectangle_area(3, 4));
}

fn rectangle_area(length: i32, width: i32) -> i32 {
    length * width
}
// 预期输出: area: 12
```

Output: `area: 12`

Unlike Chapter 5's `if` and `loop` — which must appear before use — **functions don't care about order**. The compiler "registers" all functions first; then call them however you like.

---

## 6.6 Recursion: a function calling itself

📖 **Recursion**: a function calling itself inside its own body.

💡 **Metaphor**: you ask Dad "what do I do about this?" Dad says "ask Grandpa", Grandpa says "ask Great-grandpa"… until the eldest, who answers directly — and the answer travels back down the chain.

The classic example: **factorial**. 5! = 5 × 4 × 3 × 2 × 1 = 120. The pattern: `factorial(5) = 5 × factorial(4)`, on down to `factorial(1) = 1`.

```rust
fn factorial(n: i32) -> i32 {
    if n <= 1 {
        return 1;          // reached the bottom — answer directly
    }
    n * factorial(n - 1)   // not at the bottom yet — ask "a smaller me"
}

fn main() {
    println!("factorial: {}", factorial(5));
}
// 预期输出: factorial: 120
```

Output: `factorial: 120`

> ⚠️ **Careful**: recursion needs a **stopping condition** (here `n <= 1`). Without one, the asking never ends and the program crashes — like a story where nobody ever gives an answer, so the question travels forever.

> ✨ It's fine if recursion doesn't click yet — loops (Chapter 5) solve most problems. Recursion is the "advanced style", and later chapters will meet it again.

---

## 6.7 Methods: functions attached to a type

The boxes so far are "universal" — anyone can use them. There's another kind of box that **belongs to one specific thing** — like a remote control belonging to one TV. A function attached to a type is called a **method**.

📖 **Method**: a function attached to a type. Like a toy's built-in button: the button is on the toy itself, and you need the toy before you can press it.

Chapter 10 properly teaches **structs** (custom types). Here, borrow the simplest possible struct to see what methods look like:

```rust
struct Scoreboard {
    score: i32,
    games: i32,
}
```

(Read: the Scoreboard type holds two pieces of data — a score and a game count.)

### The `impl` block

Attaching methods to a type takes an **`impl`** block ("implement — fit something with features"):

```rust
impl Scoreboard {
    // Associated function: no self parameter — builds a brand-new one
    fn new() -> Scoreboard {
        Scoreboard { score: 0, games: 0 }
    }

    // Method: &self = read-only borrow of self
    fn average(&self) -> i32 {
        if self.games == 0 {
            return 0;
        }
        self.score / self.games
    }

    // Method: &mut self = may modify self
    fn record_game(&mut self, points: i32) {
        self.score = self.score + points;
        self.games = self.games + 1;
    }
}
```

Two new words appeared:

📖 **`self`**: the parameter inside a method that stands for "this very instance". `average` is a Scoreboard method, and inside it `self` is "whichever scoreboard called it".

📖 **Associated function**: a function written inside an `impl` block but **without a `self` parameter**. It doesn't target an existing instance; it usually **builds new ones** — hence also called a **constructor**.

### The three forms of `self`

| Form | Meaning | Metaphor |
|---|---|---|
| `&self` | Read-only borrow: look, don't touch | Borrow a classmate's book to flip through — no writing in it |
| `&mut self` | Mutable borrow: may modify | Borrow a classmate's pen to write — return it after |
| `self` | Take it outright (consume) | Eat the apple — the apple is gone |

Chapter 7 *Ownership* and Chapter 8 *References and Borrowing* explain "borrowing" in depth. For now: **methods that modify themselves take `&mut self`; read-only ones take `&self`**.

### Calling: double colon vs dot

```rust
let mut board = Scoreboard::new();   // associated functions use ::
board.record_game(80);               // methods use a dot
board.record_game(95);
board.record_game(76);
println!("average: {}", board.average());
```

Output: `average: 83`

The two calling styles compared:

| Style | Example | Used for |
|---|---|---|
| `Type::name()` | `Scoreboard::new()` | Associated functions (no `self`) |
| `instance.name()` | `board.average()` | Methods (first parameter is `self`, passed in automatically) |

> ⚠️ **Naming warning**: when naming associated functions, avoid clashing with names the standard library already defines for the same type (like `new` on your own type is fine, but shadowing well-known trait methods invites confusion). Pick clear, specific names.

---

## 6.8 The unit type: spitting out nothing

A function without `->` actually spits out a special value: the **unit type**, written `()`.

📖 **Unit type**: the type that holds nothing, written `()`. Its whole purpose is "so that every function has a return value" — you almost never touch it.

```rust
let nothing: () = ();
println!("nothing: {:?}", nothing);
// 预期输出: nothing: ()
```

Output: `nothing: ()`

Remember the semicolon trap in 6.4? The error said "expected `i32`, found `()`" — that `()` is the unit type. Now you know who it is.

### Diverging functions and the "never" type (`!`)

Beyond "spits out nothing", there's an even stranger kind of function: the **diverging function** — it never even gets to "spit", because it "leaves forever" before reaching the end.

📖 **Diverging function**: a function that never returns normally; its return type is written `!`. It either loops forever, panics outright, or ends the program.

```rust
fn never_stops() -> ! {
    loop {
        println!("here I am again!");
    }
}

fn surely_doomed() -> ! {
    panic!("this function is doomed");
}
```

The `!` type has a magic property: **it can stand in for any type**. For example, if one `match` arm returns `!`, the whole match's result type is decided by the other arms — the compiler allows it, because a `!` branch never actually produces a value:

```rust
fn describe(maybe_value: Option<i32>) -> String {
    match maybe_value {
        Some(value) => value.to_string(),
        None => panic!("there should have been a value"),   // this arm's type is ! (never returns), usable as any type
    }
}
```

`panic!`, `todo!` and `return` — these "leave early" expressions all have the type `!`, which is why they can appear anywhere any type is expected.

💡 **Metaphor**: `()` is "handing the recipient an empty box"; `!` is "the courier vanished halfway" — the box never arrives, so nobody needs to specify what it should contain.

---

## 6.9 A complete example: the three-subject score analyzer

Everything from this chapter at once: ordinary functions (parameters, return values, early returns) + struct methods (associated function, `&self`, `&mut self`):

```rust
// ============ Ordinary functions ============

// sum: two parameters, one return value
fn sum(a: i32, b: i32) -> i32 {
    a + b
}

// max_of: uses an early return
fn max_of(a: i32, b: i32) -> i32 {
    if a > b {
        return a;
    }
    b
}

// greet: no return value, uses Chapter 5's loop
fn greet(times: i32) {
    for _ in 0..times {
        println!("Hello!");
    }
}

// ============ Struct + methods ============

struct Scoreboard {
    score: i32,
    games: i32,
}

impl Scoreboard {
    // Associated function: build a brand-new scoreboard
    fn new() -> Scoreboard {
        Scoreboard { score: 0, games: 0 }
    }

    // Method: record one game (modifies self, so &mut self)
    fn record_game(&mut self, points: i32) {
        self.score = self.score + points;
        self.games = self.games + 1;
    }

    // Method: compute the average (read-only, so &self)
    fn average(&self) -> i32 {
        if self.games == 0 {
            return 0;    // no games yet — return 0 to avoid dividing by 0
        }
        self.score / self.games
    }

    // Method: report (a method can call other methods)
    fn report(&self) {
        println!("{} games, {} points total, average {}", self.games, self.score, self.average());
    }
}

// ============ Main ============

fn main() {
    println!("sum: {}", sum(3, 7));
    println!("the bigger one: {}", max_of(5, 9));
    greet(2);

    let mut board = Scoreboard::new();
    board.record_game(80);
    board.record_game(95);
    board.record_game(76);
    board.report();
    println!("the average is: {}", board.average());

    let nothing: () = ();
    println!("nothing: {:?}", nothing);
}
```

---

## 6.10 Line by line

- **Lines 4–6**: `sum` takes two integers; the last line `a + b` carries no semicolon, so its value is the return value.
- **Lines 9–14**: in `max_of`, if `a > b` holds, `return a;` ends early; otherwise fall through and hand back `b`.
- **Lines 17–21**: `greet` has no `->`, returning the unit type. `for _ in 0..times`: the `_` (underscore) means "the loop variable is unused — no name needed".
- **Lines 25–28**: define the struct `Scoreboard`, two integer fields.
- **Lines 32–34**: the associated function `new` has no `self` parameter; it returns a brand-new "0 points, 0 games" scoreboard.
- **Lines 37–40**: `record_game` takes `&mut self`, because it modifies both of its own fields.
- **Lines 43–48**: `average` guards against division by zero (returns 0 early when `games == 0`), then does integer division.
- **Lines 51–53**: `report` shows **a method calling another method** with `self.average()`.
- **Lines 58–60**: calls to the three ordinary functions.
- **Line 62**: `let mut` — the following calls to `&mut self` methods modify it, so it must be mutable.
- **Lines 63–65**: dot-call the methods; the scoreboard's internal data updates.
- **Line 68**: the unit type `()` demo, printed with the `{:?}` debug format.

---

## 6.11 What you should see

Store the code in `src/main.rs` and run (`cargo run` in the terminal, or the top-right ▶):

```
sum: 10
the bigger one: 9
Hello!
Hello!
3 games, 251 points total, average 83
the average is: 83
nothing: ()
```

Checking against the answers:

- `sum: 10`: 3 + 7;
- `the bigger one: 9`: of 5 and 9, the bigger is 9;
- Two lines of `Hello!`: `greet(2)` loops twice;
- `3 games, 251 points total, average 83`: 80 + 95 + 76 = 251, and 251 ÷ 3 = 83 (integer division drops the decimal — 251 ÷ 3 is 83.66…, taking 83);
- `nothing: ()`: the unit type prints as a pair of parentheses.

---

## 6.12 Common mistakes and how to fix them

### Mistake one: E0308 return type mismatch

```rust
// 预期错误: E0308
fn doubled(n: i32) -> i32 {
    "it got bigger"
}
```

The error:

```
error[E0308]: mismatched types: expected `i32`, found `&str`
```

**Cause**: promised an integer, handed over text. **Fix**: check the return value against the type after `->`. Usually one of two things: the type is wrong, or a semicolon is extra/missing (see next).

### Mistake two: forgot to drop the semicolon (E0308 + `()`)

```rust
// 预期错误: E0308
fn square(n: i32) -> i32 {
    n * n;
}
```

The error: `mismatched types: expected i32, found ()`.

**Fix**: delete the semicolon from the return-value line. Whenever the error contains `()`, suspect the semicolon first.

### Mistake three: a parameter without a type

```rust
fn square(n) -> i32 {   // ❌ n is missing ": i32"
```

The error complains about a missing type annotation. **Fix**: in Rust, **every function parameter must have a type** — no exceptions: `(n: i32)`.

### Mistake four: arguments don't match at call time

`sum(3)` is one short, `sum(3, 7, 9)` is one too many — both error with "this function takes N arguments". **Fix**: count the parameters at the definition and pass exactly that many.

### Mistake five: division by zero doesn't error?

Without guarding `games == 0` in `average`, the first run **panics** (a runtime crash — Chapter 3 mentioned it). Integer division by zero in Rust is a runtime error; the compiler doesn't intercept it early. **Fix**: always check the divisor before dividing.

### Mistake six: method not found (a naming collision)

Name your own method with a word the standard library already uses for the same purpose, and you may get E0599 "method not found on this type" when definitions and call sites disagree. **Fix**: keep your own function names clear and specific — fresh words, no collisions.

---

## 6.13 Chapter glossary

| Term | One-line meaning |
|---|---|
| Function | A named, reusable instruction block |
| Parameter | The input name written at definition — type required |
| Argument | The actual value passed at call time |
| Return value | The result a function hands back after running |
| return | Hand back a result early and end the function |
| Expression | A piece of writing that evaluates to a value (last line without semicolon can be the return value) |
| Statement | An instruction that does one thing, ends with a semicolon, produces no value |
| Recursion | A function calling itself — must have a stopping condition |
| Method | A function attached to a type |
| impl | The block that fits a type with methods |
| self | The parameter inside a method standing for "this instance itself" |
| Associated function | A function in an impl block without self — usually builds new instances |
| Constructor | See "associated function" |
| Unit type | The type holding nothing, written () |
| Panic | The program crashing out on an error (at runtime) |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 6.14 Exercises

> 💪 Try first, then peek.

### Exercise one: a multiplication function

Write a function `multiply` taking two integers and returning their product. Print `multiply(6, 7)` in the main function.

<details>
<summary>🔍 View answer</summary>

```rust
fn multiply(a: i32, b: i32) -> i32 {
    a * b
}

fn main() {
    println!("{}", multiply(6, 7));
}
// 预期输出: 42
```

Output: `42`

</details>

### Exercise two: even or odd

Write a function `is_even` taking one integer and returning a boolean. Hint: `n % 2` is the remainder after dividing by 2 — remainder 0 means even.

<details>
<summary>🔍 View answer</summary>

```rust
fn is_even(n: i32) -> bool {
    n % 2 == 0
}

fn main() {
    println!("is 8 even: {}", is_even(8));
    println!("is 7 even: {}", is_even(7));
}
```

Output:

```
is 8 even: true
is 7 even: false
```

</details>

### Exercise three: max of three

Using this chapter's `max_of` function, find the biggest of three numbers. Hint: take the max of the first two, then compare with the third.

<details>
<summary>🔍 View answer</summary>

```rust
fn max_of(a: i32, b: i32) -> i32 {
    if a > b {
        return a;
    }
    b
}

fn max_of_three(a: i32, b: i32, c: i32) -> i32 {
    max_of(max_of(a, b), c)
}

fn main() {
    println!("{}", max_of_three(3, 9, 5));
}
// 预期输出: 9
```

Output: `9`

</details>

### Exercise four: a counter method

For a struct `Counter` (one field `value: i32`), write: the associated function `new` (starting from 0), the method `increment` (`&mut self`), and the method `report` (prints the current value). In the main function, add three times and print.

<details>
<summary>🔍 View answer</summary>

```rust
struct Counter {
    value: i32,
}

impl Counter {
    fn new() -> Counter {
        Counter { value: 0 }
    }
    fn increment(&mut self) {
        self.value = self.value + 1;
    }
    fn report(&self) {
        println!("now at: {}", self.value);
    }
}

fn main() {
    let mut counter = Counter::new();
    counter.increment();
    counter.increment();
    counter.increment();
    counter.report();
}
// 预期输出: now at: 3
```

Output: `now at: 3`

</details>

---

## 6.15 FAQ

**Q: Can function names be descriptive? Any limits?**

Yes — use clear names throughout. Start with a **verb** (`sum`, `print`, `check`) so the box's purpose is obvious at a glance. Rust convention is snake_case for functions and variables.

**Q: How do functions relate to Chapter 5's `match` and `loop`?**

`if` and `loop` are **control flow** (deciding execution order); functions are **code organization** (packing instructions under a name). Function bodies can use control flow freely, and control flow can call functions. They cooperate.

**Q: Can a function return multiple values?**

Strictly, one value only. But pack several into a **tuple** (Chapter 4) and return that, e.g. `-> (i32, i32)`. After Chapter 10's structs, you can also return a struct.

**Q: `return` or the trailing expression — which is better?**

Simple computations suit the trailing expression (more concise); use `return` when you need an **early exit** (condition met, stop calculating). Both are everyday tools.

**Q: What does the `&` in `&self` mean?**

It means "borrow" — the method merely **borrows** the instance to look at or modify it, then gives it back; it doesn't consume it. Chapter 7 *Ownership* and Chapter 8 *References and Borrowing* spend two full chapters on this. For now, treat it as fixed spelling.

**Q: Which is faster, recursion or a loop?**

Most of the time, loops are faster and lighter on memory. Recursion wins on elegance and clarity (especially for "layers within layers" problems). At the beginner stage, learn both; don't fuss over performance.

**Q: Why does `greet` use `_` as the loop variable?**

`_` means "this value goes unused". The loop repeats `times` times, but we don't care which round it is — `_` skips the naming. It also avoids the "defined but never used" warning.

---

## What's next

Your boxes are multiplying — but the data inside them has a big question: **who owns it? And who cleans up when it's done?**

In **Chapter 7, "Ownership"**, you'll learn Rust's most distinctive rules:

- Every value has exactly one **owner**;
- When the owner leaves, the value is **cleaned up** automatically (released);
- Hand a value to someone else and **you can no longer use it** (moving);
- `clone()` copies out an independent duplicate.

This is what makes Rust unlike any other language — and it's the secret weapon of its memory safety. See you in the next chapter!
