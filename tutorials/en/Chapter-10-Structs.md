# Chapter 10: Structs

## 10.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain what a struct is with the **record card** metaphor;
2. Define a struct, build **instances**, read and write fields with a dot;
3. Fit a struct with methods using an **`impl`** block (Chapter 6's setup finally pays off);
4. Make a struct printable with `{:?}` using **`#[derive(Debug)]`**;
5. Write **tuple structs**;
6. Build new instances quickly with the **`..old-instance`** update syntax;
7. Manage a batch of data with an array of structs.

---

## 10.1 Why structs exist: the pain of loose data

Say you want to record one student's information: name, age, score. With what you know so far, the only way is:

```rust
let name_a = "Xiaohua";
let age_a = 12;
let score_a = 95;

let name_b = "Xiaogang";
let age_b = 13;
let score_b = 76;
```

The problem is obvious: three variables form a group only through a **naming convention** (all carrying "_a"). With more students, variables fly everywhere, and which belongs to whom depends on sharp eyes.

💡 **Metaphor**: it's like the whole class's records **scattered on the floor** — a name slip here, an age strip there, a score sheet over there. What we need is a **record card**: one card per student, three fields, everything in its place.

📖 **Struct**: a custom "record card" type that packs related data together. The keyword is `struct`.

## 10.2 Defining a struct

```rust
struct Student {
    name: String,
    age: i32,
    score: i32,
}
```

Read it as: "there is a new type called `Student`, with three columns: name (String), age (i32), score (i32)."

> ⚠️ **Careful**: defining a struct only **designs the card layout** — no real data exists yet. Like a print shop finishing the card template — no one's information is written on any card.

## 10.3 Building instances: filling in a card

📖 **Instance**: one concrete piece of data built to the struct's layout — a card with its contents filled in.

### Way one: fill the literal directly

```rust
let xiaohua = Student {
    name: String::from("Xiaohua"),
    age: 12,
    score: 95,
};
```

The format: `TypeName { field: value, field: value, ... }`. **Every field must be filled** — miss one and it errors (E0063).

### Way two: an associated function (the constructor)

Seen in Chapter 6: a function without `self` inside an `impl` block is an associated function, built for creating instances:

```rust
impl Student {
    fn new(name: String, score: i32) -> Student {
        Student { name: name, age: 12, score: score }
    }
}

// 用起来：
let xiaohua = Student::new(String::from("Xiaohua"), 95);
```

The benefit: the instance-building logic (say, a default age) lives in one place, and the main function stays a clean single line.

## 10.4 Reading and writing fields: the dot

```rust
// 读
println!("{} is {} years old", xiaohua.name, xiaohua.age);

// 写（实例必须是 let mut）
let mut xiaohua = Student { ... };
xiaohua.score = 100;
```

> ⚠️ **Careful**: Rust has no "make just one field mutable" — to modify any field, the whole instance must be declared `let mut`.

## 10.5 Methods: fitting the card with features

A function with a `self` parameter inside an `impl` block is a **method**. Chapter 6's setup officially pays off now:

```rust
impl Student {
    // Read-only method
    fn is_passing(&self) -> bool {
        self.score >= 60
    }

    // Modifying method
    fn add_points(&mut self, how_many: i32) {
        self.score = self.score + how_many;
    }
}

// 调用：
xiaohua.add_points(5);
println!("{}", xiaohua.is_passing());
```

What makes methods better than ordinary functions? **Data and behavior live together.** The "is passing" check lives on the student type itself, rather than scattered in some corner as `check_passing(some_student)`.

## 10.6 Debug printing: `{:?}` and derive

Try printing a struct directly:

```rust
println!("{}", xiaohua);   // ❌ error: the Student type doesn't support {} display
```

`{}` requires the type to `, which structs lack by default. Two remedies — beginners use the first:

**Stick on the Debug sticker, print with `{:?}`**:

```rust
#[derive(Debug)]
struct Student { ... }

println!("{:?}", xiaohua);
```

Output:

```
Student { name: "Xiaohua", age: 12, score: 95 }
```

`{:?}` is the **debug format** — every field laid bare, a debugging superpower. Want a multi-line view for many fields? Use `{:#?}`:

```rust
println!("{:#?}", sky_blue);
```

```
Color {
    red: 0,
    green: 191,
    blue: 255,
}
```

> 📖 **Derive**: having the compiler automatically generate certain abilities for a type, written `#[derive(...)]`. `Debug` is the debug-printing ability. The trait names in the parentheses are English (`Debug`, `Clone`) — standard Rust here.

## 10.7 Tuple structs: packing without field names

Sometimes data is too simple to name each column:

```rust
struct Coordinate(i32, i32);

fn main() {
    let point = Coordinate(3, 5);
    println!("x: {}, y: {}", point.0, point.1);
}
// 预期输出: x: 3, y: 5
```

Output: `x: 3, y: 5`

Access by position with `.0`, `.1` (just like Chapter 4's tuples). Fits two fields or fewer whose meaning is self-evident (coordinates, colors…). Most of the time, use a regular struct with named fields — better readability.

## 10.8 The update syntax: change a few columns, copy the rest

Want a new card **mostly identical** to an old one:

```rust
let xiaohua_two = Student { score: 88, ..xiaohua };
```

Read it as: "the new card's score is 88, **all other fields copied from xiaohua**."

> ⚠️ **Ownership warning**: `..xiaohua` **moves** the old card's fields into the new one (remember Chapter 7). If a field is heap data like a `String`, `xiaohua` dies from then on. Want both cards usable? `xiaohua.clone()` first.

## 10.9 A complete example: the class roster

Structs + methods + arrays + loops, managing a whole class:

```rust
#[derive(Debug, Clone)]
struct Student {
    name: String,
    score: i32,
}

impl Student {
    fn new(name: String, score: i32) -> Student {
        Student { name: name, score: score }
    }

    fn is_passing(&self) -> bool {
        self.score >= 60
    }
}

fn main() {
    // An array holding three student instances
    let class = [
        Student::new(String::from("Xiaohua"), 95),
        Student::new(String::from("Xiaogang"), 58),
        Student::new(String::from("Xiaoli"), 88),
    ];

    // Print the roster
    for index in 0..3 {
        println!("{:?}, passing: {}", class[index], class[index].is_passing());
    }

    // Find the highest score
    let mut highest = 0;
    let mut champion = Student::new(String::from("nobody"), 0);
    for index in 0..3 {
        if class[index].score > highest {
            highest = class[index].score;
            champion = class[index].clone();
        }
    }
    println!("the champion is {} with {}", champion.name, champion.score);
}
```

## 10.10 Line by line

- **Line 1**: derive two abilities at once — `Debug` (enables `{:?}` printing) and `Clone` (enables `.clone()`), comma-separated.
- **Lines 2–5**: the `Student` struct, two fields.
- **Lines 8–10**: the associated function `new`, building an instance in one line.
- **Lines 12–14**: the read-only method `is_passing`.
- **Lines 19–23**: an **array of structs** — each element is a student card. Array syntax is still Chapter 3's `[元素, 元素, ...]`.
- **Lines 26–28**: loop-print. `{:?}` spits out the whole card.
- **Lines 31–32**: `champion` starts as a "nobody" placeholder, guarding against an empty run.
- **Lines 33–38**: the classic "king of the hill" max-finding. `class[index]` takes a card from the array; the array is read-only (not declared mutable), so cards can't be **taken away** — `.clone()` duplicates one for `champion`.
- **Line 40**: `champion.name` reads a field with a dot.

## 10.11 What you should see

```
Student { name: "Xiaohua", score: 95 }, passing: true
Student { name: "Xiaogang", score: 58 }, passing: false
Student { name: "Xiaoli", score: 88 }, passing: true
the champion is Xiaohua with 95
```

---

## 10.12 Common mistakes and how to fix them

### Mistake one: E0063, a missing field

```
error[E0063]: missing field `name` in initializer
```

**Fix**: fill in the field, or use `..旧实例` to copy the rest from an old one.

### Mistake two: printing a struct with `{}` errors

`println!("{}", xiaohua)` reports "Student doesn't implement Display". **Fix**: add `#[derive(Debug)]` and use `{:?}`; or print specific fields like `xiaohua.name`.

### Mistake three: assigning a field on an immutable instance

```rust
xiaohua.score = 100;   // ❌ xiaohua is not let mut
```

The error is E0594. **Fix**: add `mut` at the declaration: `let mut xiaohua = ...`.

### Mistake four: the old instance vanishes after the update syntax

`..xiaohua` moved the fields away, so the `String` field inside `xiaohua` was moved — using `xiaohua` again errors with E0382. **Fix**: `xiaohua.clone()` before the update, or reorder the uses.

### Mistake five: a misspelled derived trait name

`#[derive(Debugg)]` reports "can't find derive macro" — a typo in the trait name. **Fix**: check the spelling: `Debug`, `Clone`, `Copy`, `Hash`.

---

## 10.13 Chapter glossary

| Term | One-line meaning |
|---|---|
| Struct | A custom packing type |
| Field | Each column of data inside a struct |
| Instance | One concrete piece of data built to a struct's layout |
| Associated function | A function in an impl block without self — usually builds instances |
| Constructor | See "associated function" |
| Method | A function with a self parameter, attached to a type |
| Derive | Having the compiler generate trait implementations, written `#[derive(...)]` |
| Debug output | The `{:?}` format, paired with the debug derive for printing structs |
| Tuple struct | A struct without field names, accessed by position |
| Update syntax | `..旧实例` — copy the remaining fields to build a new instance quickly |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 10.14 Exercises

> 💪 Try first, then peek.

### Exercise one: a book card

Define a `Book` struct (title: String, pages: i32), write the associated function `new`, build one book and print it with `{:?}`.

<details>
<summary>🔍 View answer</summary>

```rust
#[derive(Debug)]
struct Book {
    title: String,
    pages: i32,
}

impl Book {
    fn new(title: String, pages: i32) -> Book {
        Book { title: title, pages: pages }
    }
}

fn main() {
    let book = Book::new(String::from("The Little Prince"), 97);
    println!("{:?}", book);
}
// 预期输出: Book { title: "The Little Prince", pages: 97 }
```

Output: `Book { title: "The Little Prince", pages: 97 }`

</details>

### Exercise two: a thick-book check

Add a method `is_thick(&self)` to `Book`: returns `true` when the page count exceeds 300.

<details>
<summary>🔍 View answer</summary>

```rust
fn is_thick(&self) -> bool {
    self.pages > 300
}
```

</details>

### Exercise three: tuple structs

Define a `Temperature(i32)` tuple struct, build two instances (Beijing 20, Harbin -5), and print their values.

<details>
<summary>🔍 View answer</summary>

```rust
struct Temperature(i32);

fn main() {
    let beijing = Temperature(20);
    let harbin = Temperature(-5);
    println!("Beijing: {} degrees, Harbin: {} degrees", beijing.0, harbin.0);
}
```

</details>

### Exercise four: the update syntax

Based on a `Book` instance `original`, use the update syntax to build a `revised` — changing only the page count to 120, copying the rest (hint: clone first to prevent a move).

<details>
<summary>🔍 View answer</summary>

```rust
#[derive(Debug, Clone)]
struct Book {
    title: String,
    pages: i32,
}

fn main() {
    let original = Book { title: String::from("The Little Prince"), pages: 97 };
    let revised = Book { pages: 120, ..original.clone() };
    println!("original: {:?}", original);
    println!("revised: {:?}", revised);
}
```

</details>

---

## 10.15 FAQ

**Q: Struct vs Chapter 4's tuple — what's the difference?**

Both pack. Tuple fields are **unnamed** (accessed `.0`, `.1`) — for temporary small groupings; struct fields are **named** — for real data models. Tuple structs sit between the two.

**Q: Can a struct hold a struct?**

Yes. For example, a `Class` struct holding an array of students. Data nests layer by layer, like record cards inside a filing cabinet. After Chapter 15's vectors (growable collections), it gets even more flexible.

**Q: What exactly is passed as `self`?**

The instance that called the method. In `xiaohua.is_passing()`, `self` = `xiaohua`. Write `&self` to borrow, write `self` to take it (consume). Chapter 8's knowledge is reused here in full.

**Q: Why is an associated function recommended over a literal for building instances?**

Two benefits — ① defaults (fix the age inside `new`); ② validation (page counts can't be negative). As projects grow, keep construction logic in associated functions.

**Q: Does the `derive(Debug)` sticker slow the program down?**

Practically no. It only generates printing code at compile time; if you never use `{:?}`, it never runs.

**Q: How do enums (Chapter 11) relate to structs?**

A struct is "**and**" — one card holds name AND age AND score. An enum is "**or**" — a value is one of several cases (passing / failing / absent). Next chapter settles it.

---

## What's next

Structs solved "packing several kinds together" (and); but many situations are "one of two, one of three" (or): a grade is either excellent, good or poor; a coin is either heads or tails.

**Chapter 11, "Enums and Pattern Matching"**, teaches:

- Defining "one-of-many" types with `enum`;
- Attaching data to each variant;
- `match` in its full glory: destructuring enums, extracting data;
- `Option` — Rust's standard answer to "maybe there's no value".

See you in the next chapter!
