# Chapter 13: Traits

## 13.0 Learning Objectives

After reading this chapter, you will be able to:

1. Explain what a trait is using the **job posting** analogy;
2. Define new capabilities with the `trait` keyword (required + default methods);
3. Use **`impl Trait for Type`** to hand a type its "ID badge";
4. Accept "anyone with this capability" with both `<T: Trait>` bounds and the shorter `&impl Trait`;
5. Mix different types in one collection using **`dyn Trait`** (trait objects);
6. Read and fix **E0277** ("trait bound not satisfied");
7. See what the `#[derive(...)]` sticker really does: it writes an `impl` block for you.

---

## 13.1 Traits: The Job Posting

💡 **Analogy**: The school is throwing a talent show and needs to recruit "people who can introduce themselves." The help-wanted ad reads:

> **Position: Can Introduce**
> Requirement: must be able to provide a short self-introduction.

A student can apply. A teacher can apply. **It doesn't matter who you are — meet the requirement and you've got the job.**

📖 **Trait**: a set of method signatures that defines what a type **can do**. Think of it as a job posting: it lists requirements, without saying who will fill them.

The `Ord` and `Display` from Chapter 12 are traits the standard library **already defined for you**. This chapter, you learn to **define your own**.

## 13.2 Defining a Trait

```rust
trait Intro {
    fn intro(&self) -> String;
}
```

Reading: "There's a position called `Intro`. Whoever takes it must provide an `intro` method: it takes `&self` and returns a `String`."

Note: the method ends with a **semicolon**, with no body — a job posting states requirements; it doesn't do the work for you.

## 13.3 Implementing a Trait: Applying for the Job

```rust
struct Student {
    name: String,
    score: i32,
}

struct Teacher {
    name: String,
    subject: String,
}

impl Intro for Student {
    fn intro(&self) -> String {
        format!("Student {} ({} points)", self.name, self.score)
    }
}

impl Intro for Teacher {
    fn intro(&self) -> String {
        format!("{} teaches {}", self.name, self.subject)
    }
}
```

📖 The syntax **`impl TraitName for TypeName`**: read it as "implement this trait for this type."

From now on, both `Student` and `Teacher` are "badge holders" — anywhere an `Intro` is required, they can show up.

> ⚠️ **Careful**: defining a trait ≠ implementing it. A posting alone leaves the position empty; implementing for one type doesn't qualify the others.

## 13.4 Default Methods: The Posting Ships With a Template

Methods in a trait can **carry a default implementation** — applicants can use the template as-is, or override it:

```rust
trait Intro {
    fn intro(&self) -> String;

    fn polite_intro(&self) -> String {
        format!("Nice to meet you: {}", self.intro())
    }
}
```

Neither `Student` nor `Teacher` wrote `polite_intro`, yet both can call it — inside, it calls each type's own `intro`. Want a different greeting? Override it in the `impl` block.

## 13.5 Two Ways to Require a Trait

You want a function that only accepts "people who can introduce themselves." Two equivalent styles:

### Style One: Trait Bounds (from Chapter 12)

```rust
fn roll_call<T: Intro>(who: &T) {
    println!("{}", who.polite_intro());
}
```

### Style Two: `&impl Trait` (Shorter)

```rust
fn roll_call(who: &impl Intro) {
    println!("{}", who.polite_intro());
}
```

Reading: "the parameter is a borrow of any type, as long as that type implements `Intro`."

Which style to pick? **Single parameter, simple case** — Style Two reads better. **Multiple parameters that must share the same placeholder type** (say, "compare two people of the same type") — use Style One.

## 13.6 Trait Objects: Mixing Different Types

Arrays demand that all elements share **one type** (Chapter 3's iron rule). `Student` and `Teacher` are two different families — how do you fit them into one roster?

Answer: **trait objects** — don't look at the concrete type, look at the badge:

```rust
let roster: [&dyn Intro; 2] = [&xiaohua, &laowang];

for i in 0..2 {
    println!("Present: {}", roster[i].intro());
}
```

📖 **dyn**: a keyword meaning "the concrete type is only known at runtime." `&dyn Intro` reads as "a borrow of any `Intro` implementer."

💡 **Analogy**: a trait bound is like **building one production line per job title** — orders arrive from students and teachers, and the compiler builds two dedicated lines (monomorphization, Chapter 12). A trait object is like **one universal line** — whatever comes down the belt, as long as it has a badge, gets handled. One line saves code, but every item needs a badge check on the spot — a tiny bit slower.

| | Generic `<T: Trait>` | Trait object `dyn Trait` |
|---|---|---|
| When the type is decided | Compile time | Runtime |
| Performance | Faster (dedicated code) | Slightly slower (on-the-spot check) |
| Can mix different types | No | Yes |

## 13.7 The Truth Behind `#[derive(...)]`

That `#[derive(Debug)]` sticker from Chapter 10, and `#[derive(Clone)]` from Chapter 7 — now we can explain them: **derive = the compiler writes the `impl` block for you**.

`#[derive(Debug)]` is roughly the compiler generating:

```rust
impl Debug for Student {
    // ... the compiler writes the print logic for every field
}
```

One sticker saves you dozens of lines. The usual suspects you can derive: `Debug` (debug printing), `Clone` (cloning), `Copy` (copy-by-value), `PartialEq` (equality comparison).

---

## 13.8 Associated Types and Operator Overloading

When defining a trait, a method signature can mention "a type that isn't pinned down yet" — that's what generics are for. But there's another kind of placeholder: **associated types**.

### Associated Types: The Type Placeholder Inside a Trait

Start with the standard library's most famous trait — `Iterator`. It's the job posting for a "conveyor belt":

```rust
// Sketch: the standard library's Iterator trait, roughly like this
trait Iterator {
    type Item;                          // associated type: the cargo this belt carries

    fn next(&mut self) -> Option<Self::Item>;   // take the next item
}
```

`Item` here is an **associated type**: the posting only says "this belt carries a cargo called `Item`" — whether that's apples or bricks is **decided by each applicant**.

📖 **Associated type**: a placeholder type declared inside a trait with `type Name;`. When implementing, you fill in the real thing with `type Item = i32;`.

💡 **Analogy**: a trait is the spec sheet for a conveyor belt — "carries cargo of type `Item`". Implementing the trait is like signing the contract: **your type** writes in the blank, "cargo type = integers". Same trait, different types fill in different cargo — no interference.

Give a "counter" its belt:

```rust
struct Counter {
    current: i32,
    limit: i32,
}

// Hand Counter a "conveyor belt" badge
impl Iterator for Counter {
    type Item = i32;

    fn next(&mut self) -> Option<i32> {
        if self.current < self.limit {
            self.current += 1;
            Some(self.current - 1)
        } else {
            None
        }
    }
}

fn main() {
    let count = Counter { current: 0, limit: 4 };
    for n in count {
        println!("{}", n);
    }
}
```

Output:

```
0
1
2
3
```

Implementing `Iterator` takes two fills: `type Item = i32;` (the cargo is integers) and `fn next` (produce one number per call). Once filled, `Counter` really is a conveyor belt — `for n in count` keeps calling `next` until it returns `None`.

> ⚠️ **Reminder**: `Item` and `Output` are names the standard library fixed long ago — **copy them exactly**. Your own traits can call their associated types anything, but implementing `Iterator` or `Add` requires these exact names.

### Supertraits: Traits That Come With Prerequisites

A trait can "extend" another trait. Suppose you want a `FancyDisplay`: it first requires `Describable` (can describe itself), then adds "fancier description":

```rust
trait Describable {
    fn describe(&self) -> String;
}

// FancyDisplay requires Describable first: you must be able to describe before you can decorate
trait FancyDisplay: Describable {
    fn fancy(&self) -> String;
}

struct Product {
    name: String,
    price: i32,
}

impl Describable for Product {
    fn describe(&self) -> String {
        format!("{} ({} yuan)", self.name, self.price)
    }
}

impl FancyDisplay for Product {
    fn fancy(&self) -> String {
        format!("[Featured] {}", self.describe())
    }
}

fn welcome(who: &impl FancyDisplay) {
    println!("{}", who.fancy());
}

fn main() {
    let item = Product { name: String::from("Pen"), price: 5 };
    welcome(&item);
}
```

Output:

```
[Featured] Pen (5 yuan)
```

📖 **Supertrait**: `trait Child: Parent { ... }` — after the colon comes the **prerequisite**. Any type that implements `FancyDisplay` **must also implement `Describable`**.

Two things to note:

- The inherited `Describable` still needs **its own impl block** (inheritance ≠ auto-implementation — it still has to apply for that job too);
- Inside `FancyDisplay`'s default method you can call `self.describe()` directly — the compiler knows that "implementing `FancyDisplay` implies `Describable`."

### Operator Overloading: Teaching a Struct to Support `+`

Operators like `+`, `-`, `*` are really traits: `+` corresponds to the `Add` trait. Implement it for a struct, and `+` works on your own type:

```rust
use std::ops::Add;

struct Point {
    x: i32,
    y: i32,
}

// Make Point support +
impl Add for Point {
    type Output = Point;

    fn add(self, other: Point) -> Point {
        Point { x: self.x + other.x, y: self.y + other.y }
    }
}

fn main() {
    let a = Point { x: 1, y: 2 };
    let b = Point { x: 3, y: 4 };
    let c = a + b;
    println!("({}, {})", c.x, c.y);
}
```

Output:

```
(4, 6)
```

`Add` needs two fills: the associated type `Output` (what comes out of the addition) and the `add` method (how to add). From then on, `a + b` is legal — the compiler rewrites it as `Add::add(a, b)`.

> ⚠️ **Reminder**: `Output` and `Item` are fixed names from the standard library — **copy them exactly**. Addition, multiplication, comparison… the whole family of operator traits lives in `std::ops`, all following the same recipe.

### Common Errors

**Error one: forgetting the associated type when implementing**

You implement `Iterator` but only write `fn next`, leaving out `type Item = ...` — the compiler reports "missing associated type" (E0191). **Fix**: check the trait definition and add `type Item = ConcreteType;`.

**Error two: implementing a child trait without its parent**

You implement `FancyDisplay` but never `Describable` — the compiler reports "the trait `Describable` is not implemented." **Fix**: implement the parent trait too; a parent's requirement is hard, there's no dodging it.

---

## 13.9 Complete Example: School Roll Call

Tie it all together — trait definition, two implementations, default method, both call styles, and trait objects:

```rust
// —— The job posting ——
trait Intro {
    // Required: self-introduction
    fn intro(&self) -> String;

    // Default: polite version (calls the required intro above)
    fn polite_intro(&self) -> String {
        format!("Nice to meet you: {}", self.intro())
    }
}

// —— Two kinds of people ——
struct Student {
    name: String,
    score: i32,
}

struct Teacher {
    name: String,
    subject: String,
}

// —— Both apply for the job ——
impl Intro for Student {
    fn intro(&self) -> String {
        format!("Student {} ({} points)", self.name, self.score)
    }
}

impl Intro for Teacher {
    fn intro(&self) -> String {
        format!("{} teaches {}", self.name, self.subject)
    }
}

// —— Generic function: badge holders only ——
fn roll_call(who: &impl Intro) {
    println!("{}", who.polite_intro());
}

// —— Trait objects: mix different types ——
fn full_roll_call(roster: [&dyn Intro; 2]) {
    for i in 0..2 {
        println!("Present: {}", roster[i].intro());
    }
}

fn main() {
    let xiaohua = Student { name: String::from("Xiaohua"), score: 95 };
    let laowang = Teacher { name: String::from("Wang"), subject: String::from("Math") };

    // Style Two: &impl Trait
    roll_call(&xiaohua);
    roll_call(&laowang);

    // Trait object array
    let roster: [&dyn Intro; 2] = [&xiaohua, &laowang];
    full_roll_call(roster);
}
```

## 13.10 Line-by-Line Walkthrough

- **Lines 2–10**: trait `Intro` — one required method (semicolon) + one default method (with a body).
- **Lines 13–21**: two structs, with completely different fields.
- **Lines 24–34**: two `impl ... for ...` blocks, each filling in its own `intro`; both use the default `polite_intro`.
- **Lines 37–39**: `roll_call` uses the `&impl Intro` style — student and teacher both hold the badge, both can pass in.
- **Lines 42–46**: `full_roll_call` takes an array of "Intro-holder borrows." `&dyn Intro` is what makes mixing the two types legal.
- **Lines 49–50**: create one student, one teacher.
- **Lines 53–54**: call roll on each — both go through the default `polite_intro`.
- **Lines 57–58**: build the roster array and call the full roll. The array holds **borrows** (`&`) — Xiaohua and Wang themselves are still alive and well.

## 13.11 Output

```
Nice to meet you: Student Xiaohua (95 points)
Nice to meet you: Wang teaches Math
Present: Student Xiaohua (95 points)
Present: Wang teaches Math
```

The first two lines go through the default method (the "Nice to meet you" prefix), the last two call `intro` directly — one trait, two calling postures.

---

## 13.12 Common Errors and Fixes

### Error One: E0277 Trait Bound Not Satisfied

```rust
// Expected error E0277 — `Stone` doesn't implement `CanFly`
trait CanFly {
    fn fly(&self) -> String;
}

struct Stone;

fn launch(thing: &impl CanFly) {
    println!("{}", thing.fly());
}

fn main() {
    launch(&Stone);
}
```

```
error[E0277]: the trait bound `Stone: CanFly` is not satisfied
```

**Fix**: either add an `impl CanFly for Stone` (actually give it wings), or pass a type that already implements it.

### Error Two: E0046 Missing Required Method

```rust
// Expected error E0046 — `refuel` has no default, so it must be written
trait CanFly {
    fn fly(&self) -> String;
    fn land(&self) -> String;
    fn refuel(&self);
}

struct Bird;

impl CanFly for Bird {
    fn fly(&self) -> String {
        String::from("flap")
    }
    fn land(&self) -> String {
        String::from("touch down")
    }
}

fn main() {
    let b = Bird;
    println!("{}", b.fly());
}
```

```
error[E0046]: a trait implementation is missing required items
```

**Fix**: compare against the trait definition and fill everything in. If you genuinely can't write it yet, `todo!()` works as a placeholder (it panics if actually called at runtime).

### Error Three: E0404 Wrong Order in `impl`

```rust
// Expected error E0404 — the order is `impl Trait for Type`, not the other way
trait Intro {
    fn intro(&self) -> String;
}

struct Student;

impl Student for Intro {
    fn intro(&self) -> String {
        String::from("hi")
    }
}

fn main() {
    let s = Student;
    println!("{}", s.intro());
}
```

```
error[E0404]: expected a trait, but found a type
```

**Fix**: the correct form is **`impl Trait for Type`** — trait first, type second. Swapping the order makes the compiler read `Student` as a trait and complain it isn't one.

### Error Four: Missing Semicolon on a Required Method

A required method in a trait ends with a **semicolon** (no body); writing `{}` silently turns it into a default method. Both are legal — just be clear which one you mean.

### Error Five: E0308 Storing Trait Objects by Value

```rust
// Expected error E0308 — trait objects need references or Box
trait Greeter {
    fn greet(&self) -> String;
}

struct Cat;

impl Greeter for Cat {
    fn greet(&self) -> String {
        String::from("meow")
    }
}

fn main() {
    let zoo: [dyn Greeter; 2] = [Cat, Cat];
    println!("{}", zoo[0].greet());
}
```

```
error[E0308]: mismatched types: expected `dyn Greeter`, found `Cat`
```

Trait objects are **unsized** — nobody knows at compile time how big the actual animal is, so `[dyn Greeter; 2]` can't lay them out in memory. **Fix**: store **borrows** (`[&dyn Greeter; 2]`) or smart pointers (`Box<dyn Greeter>` — the box from Chapter 19).

---

## 13.13 Chapter Glossary

| Term | One-sentence explanation |
|---|---|
| Trait | A set of method signatures defining what a type "can do" |
| Impl block (trait) | `impl Trait for Type` — hands a type its ID badge |
| Default method | A method with a body provided by the trait; use as-is or override |
| Trait bound | `<T: Trait>` — requires a generic type to have a capability |
| &impl Trait | Shorthand trait bound in parameter position |
| dyn | Keyword marking "concrete type known only at runtime" |
| Trait object | A value of type `dyn Trait`, letting different types mix |
| Associated type | A placeholder type declared inside a trait, filled in by the impl |
| Supertrait | `trait Child: Parent` — implementing Child requires Parent |
| derive | A sticker (`#[derive(...)]`) that makes the compiler write impl blocks |

> 📖 **Reminder**: For unfamiliar terms, refer back to the "Master Glossary" at the front of this book.

---

## 13.14 Exercises

> 💪 Try first, then check the answers.

### Exercise One: Animals That Speak

Define a trait `Speak { fn speak(&self) -> String; }`, implement it for structs `Cat` and `Dog` (returning "Meow" and "Woof"), and write a function `listen(who: &impl Speak)` that prints the sound.

<details>
<summary>🔍 See Answer</summary>

```rust
trait Speak {
    fn speak(&self) -> String;
}

struct Cat;
struct Dog;

impl Speak for Cat {
    fn speak(&self) -> String {
        String::from("Meow")
    }
}

impl Speak for Dog {
    fn speak(&self) -> String {
        String::from("Woof")
    }
}

fn listen(who: &impl Speak) {
    println!("{}", who.speak());
}

fn main() {
    listen(&Cat);
    listen(&Dog);
}
```

(`struct Cat;` is an "empty struct" with no fields — handy when a type exists only to carry methods.)

</details>

### Exercise Two: Default Methods

Add a default method `speak_three_times` to `Speak`: print the sound three times. Don't override it — just call it.

<details>
<summary>🔍 See Answer</summary>

```rust
trait Speak {
    fn speak(&self) -> String;

    fn speak_three_times(&self) {
        for _ in 0..3 {
            println!("{}", self.speak());
        }
    }
}

// In main: cat.speak_three_times();
```

</details>

### Exercise Three: Mixed Menagerie

Use a trait object to put a cat and a dog into one array, then loop and let them take turns speaking.

<details>
<summary>🔍 See Answer</summary>

```rust
fn main() {
    let zoo: [&dyn Speak; 2] = [&Cat, &Dog];
    for i in 0..2 {
        println!("{}", zoo[i].speak());
    }
}
```

Output: `Meow`, `Woof`

</details>

---

## 13.15 Frequently Asked Questions

**Q: Is a trait the same as an "interface" in other languages?**

A: The idea is the same (defining a capability contract), but Rust traits are stronger: they can carry default methods, cooperate deeply with generics, and can even require things like "this type has a known size" or "this type is copyable." And types that aren't classes — integers, strings — can implement traits too, which most languages' interfaces can't express.

**Q: Can one type implement multiple traits?**

A: Yes — one impl block each. And the reverse works too: many types implementing the same trait. Traits and types are a **many-to-many** relationship.

**Q: Where am I allowed to implement a trait? Any file?**

A: There's a rule (the orphan rule): either the trait is yours, or the type is yours — at least one of the two. This stops two libraries from both pairing "someone else's type" with "someone else's trait" and fighting. Early on, your code lives in your own crate, so you'll rarely touch this rule.

**Q: Generic bounds or trait objects — which do real projects use?**

A: Want speed with a fixed cast of types → generic bounds. Need to **mix** different types (say, "put everything printable in one list") → trait objects. This book uses both heavily in later chapters.

**Q: Do I need to memorize standard traits like `Ord` and `Display`?**

A: Just the working set: `Debug` (struct printing), `Clone`/`Copy` (duplication), `PartialEq`/`Ord` (comparison), `Display` (`{}` printing), `Default` (make a default value). You'll pick them up through use — and Appendix A has a cheat sheet.

**Q: Can traits have fields?**

A: No. A trait only manages "can do" (methods), never "has" (fields). Need fields? Use a struct. Need "struct + capability"? Combine the two: the struct holds the fields, the impl block attaches the traits.

---

## Next Chapter Preview

There's still one symbol the compiler keeps nagging you about: **`'a`**. When a function returns a reference, when a struct holds a reference, the compiler asks you to annotate lifetimes. It's not black magic — it's a "validity map."

Chapter 14, **Lifetimes**, makes it clear:

- How the lifetime of a reference is actually calculated;
- The syntax and reading of `'a` annotations;
- When a function signature must annotate, and when the compiler infers it;
- `'static` — references that live as long as the program.

See you in the next chapter!
