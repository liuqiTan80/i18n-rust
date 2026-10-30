# Chapter 15: Collection Types

## 15.0 Learning Objectives

After reading this chapter, you will be able to:

1. Name the **three ways a collection differs from an array** (heap-allocated, growable, one element type);
2. Keep a run of same-shaped data in a **`Vec<T>`**: create, push, pop, index, remove, iterate;
3. Look values up by key in a **`HashMap<K, V>`**: insert, get, test for a key, iterate pairs;
4. Write the classic **`entry(...).or_insert(0)`** counting idiom;
5. Explain a collection's **ownership** of what it holds;
6. Sidestep the borrow clash that comes from **changing a collection while looping over it**.

> ✨ Nothing in this chapter is tricky — it's inventory. Two containers carry almost all real programs, and the compiler is unusually friendly about telling you what went wrong. Type every example and run it.

---

## 15.1 Collections: a warehouse that resizes itself

Chapter 3's arrays came with a hard rule: **the length is decided at birth**. `[i32; 3]` holds three elements, no more and no less.

💡 **Metaphor**: an array is a row of **lockers with a fixed number of compartments** — buy three, keep three. A **collection** is a **warehouse that extends itself** — more stock arrives, it builds more shelving; stock leaves, it shrinks.

📖 A **collection** is a data structure stored on the **heap** (memory requested at runtime, as needed) whose length can change. Three properties:

| Property | Array | Collection |
|---|---|---|
| Where it lives | stack (small, fast, fixed) | heap (big, flexible) |
| Length | fixed at compile time | changes at runtime |
| Element type | must be single | must be single |

The two protagonists of this chapter: the **vector** (an ordered row of items) and the **hash map** (a phone book you query by name).

## 15.2 `Vec<T>`: the array that grows up

📖 A **vector** is an ordered, resizable run of elements that all share one type.

### Creating one

```rust
// Method one: build the empty warehouse, then load it
let mut scores: Vec<i32> = Vec::new();
scores.push(88);

// Method two: build it in one breath
let colors = vec!["red", "yellow", "blue"];
```

An empty warehouse has to learn what it stores. Method one says so up front (`Vec<i32>`); you can also drop the annotation and let the **first `push`** decide it — but a `Vec::new()` that never receives anything leaves Rust guessing, and it answers with **E0282: type annotations needed**.

> ⚠️ To put anything into a vector, the binding must be **`mut`** — you're extending the warehouse. `let mut scores = ...`, not plain `let scores = ...`.

### Push and pop

```rust
let mut colors = vec!["red", "yellow", "blue"];
colors.push("green");    // add at the tail

if let Some(color) = colors.pop() {
    println!("popped: {}", color);
}
// 预期输出: popped: green
```

📖 **`pop()`** takes the last element off and shortens the vector. Because the warehouse might already be empty, it returns an **`Option<T>`** — `Some(value)` or `None` — and `if let` is the tidiest way to open it.

### Reading an element

```rust
let scores = vec![88, 95, 76];
println!("first: {}", scores[0]);   // indexing: panics if out of bounds
match scores.get(0) {               // the polite way: returns an Option
    Some(value) => println!("got: {}", value),
    None => println!("no such position"),
}
// 预期输出:
// first: 88
// got: 88
```

The trade-off between the two readers: use `[index]` when you are **sure** the index is legal (it's shorter and faster); use `.get()` when you're not — it hands you `None` instead of panicking.

### Removing, and how long is it now?

```rust
let mut scores = vec![88, 95, 76];
scores.remove(1);                  // take away index 1; the rest slide forward
println!("len: {}", scores.len());
// 预期输出: len: 2
```

### Iterating

```rust
let scores = vec![88, 95, 76];
for item in &scores {              // borrow each item: look, don't touch
    println!("score: {}", item);
}
// 预期输出:
// score: 88
// score: 95
// score: 76
```

`&scores` means "lend me every element in turn" — neither the vector nor its items get carried away. (Chapter 8's borrow slips, applied to a container.)

## 15.3 A vector owns its cargo

A vector **owns** each of its elements: when the vector is dropped, everything inside it is dropped with it.

```rust
fn main() {
    {
        let fruits = vec!["apple", "banana"];
        println!("in the block: {} fruits", fruits.len());
    }
    // the block ends — `fruits` is dropped, and its elements went with it
    // println!("{}", fruits[0]);   // ← this line would not compile
}
// 预期输出: in the block: 2 fruits
```

Putting **owned** data in a vector (`String`, a struct, another vector) is trouble-free. Putting a **borrow slip** (`&str`) in one means the whole collection inherits the slip's expiry date — which is Chapter 14's annotation coming due. **Beginner rule: store `String`, not `&str`.**

## 15.4 `HashMap<K, V>`: the phone book

📖 A **hash map** stores a pile of "key → value" pairs and finds the value the instant you name the key.

💡 **Metaphor**: a phone book that maps "name → number". You don't read from page one; you say the name and the right page is already open.

### Bring it in first

A hash map isn't available by default the way `i32` is — you have to import it:

```rust
use std::collections::HashMap;
```

That line means "move the phone-book toolset into this file." Every program below that uses a hash map starts with it.

### Insert and get

```rust
use std::collections::HashMap;

let mut roster = HashMap::new();
roster.insert("amy", 100);    // key -> value
roster.insert("bob", 95);

match roster.get("amy") {
    Some(score) => println!("amy: {}", score),
    None => println!("no such person"),
}
println!("is bob in it: {}", roster.contains_key("bob"));
// 预期输出:
// amy: 100
// is bob in it: true
```

Notice: `.get()` returns an **`Option<&V>`** — the key may not exist — so you open it with `match` or `if let`. When you only want to know *whether* it's there, not *what* it is, call `.contains_key()`.

### Iterating pairs

```rust
use std::collections::HashMap;

let mut roster = HashMap::new();
roster.insert("amy", 100);
for (name, score) in &roster {
    println!("{} -> {}", name, score);
}
// 预期输出: amy -> 100
```

> ⚠️ A hash map's iteration **order is not guaranteed** — two runs may print the pairs in different orders. (The example above holds a single entry, so there's nothing to reorder.) Need things sorted? Use a **`BTreeMap`**, covered in 15.5.

### Counting words: `entry(...).or_insert(0)`

Counting how often each word appears is *the* hash map idiom:

```rust
use std::collections::HashMap;

let words = ["apple", "banana", "apple", "orange", "apple", "banana"];
let mut freq = HashMap::new();

for word in words {
    // entry: stand at that word's doorway; or_insert: put a 0 there if empty
    let count = freq.entry(word).or_insert(0);
    *count = *count + 1;
}
println!("apple appeared {} times", freq["apple"]);
// 预期输出: apple appeared 3 times
```

How to read the two-call combo: `.entry(word)` positions you at that word's slot; `.or_insert(0)` says "if nothing's there yet, start it at 0"; what comes back is a **mutable reference to the value's slot**, so `*count` adds one right where it lives. A word's first visit writes 0 then makes it 1; every later visit just adds one. The final `freq["apple"]` is the indexing shorthand for reading a map — handy when you are sure the key is present, since a missing key panics.

---

## 15.5 More collections

Beyond vectors and hash maps, the standard library keeps a drawer full of containers. Four new ones, plus a trick that makes a vector faster.

### `HashSet`: only who showed up

💡 **Metaphor**: a hash map is a phone book of "name → number"; a **hash set** is the **visitor log by the door** — it records who came, and nothing else.

📖 A **`HashSet<T>`** is a group of **non-repeating** elements, superb at answering "is this one in there?".

```rust
use std::collections::HashSet;

fn main() {
    let mut visitors = HashSet::new();
    visitors.insert("zhang");
    visitors.insert("li");
    visitors.insert("zhang");
    println!("visitor count: {}", visitors.len());
    println!("is zhang here: {}", visitors.contains(&"zhang"));
    println!("is wang here: {}", visitors.contains(&"wang"));
    for name in &visitors {
        println!("{}", name);
    }
}
```

Output:

```text
visitor count: 2
is zhang here: true
is wang here: false
zhang
li
```

The repeated `insert("zhang")` was **deduplicated on arrival** — one line in the log, so the count stays 2. The last two lines may print in either order: like a hash map, a hash set makes no promises about iteration order. One syntax detail: `.contains()` asks for a *reference* to what's stored, hence `visitors.contains(&"zhang")` — the set holds `&str`, so you pass `&&str`.

### `VecDeque`: in and out at both ends

A vector's `push`/`pop` work at one end (the tail). A **`VecDeque`** takes and gives at **both** ends, which is what a queue needs:

```rust
use std::collections::VecDeque;

fn main() {
    let mut queue = VecDeque::new();
    queue.push_back(1);
    queue.push_front(0);
    queue.push_back(2);
    println!("front: {}", queue.pop_front().unwrap());
    println!("back: {}", queue.pop_back().unwrap());
}
// 预期输出:
// front: 0
// back: 2
```

The four methods are `push_back`, `push_front`, `pop_front`, `pop_back`. The queue was `0 → 1 → 2`; we served the head (0) first, then took the tail (2), leaving `1` in the middle.

### `BinaryHeap`: always pops the biggest

A **`BinaryHeap`** is a self-sorting sack: however you throw things in, `.pop()` always hands back **the largest one currently inside**:

```rust
use std::collections::BinaryHeap;

fn main() {
    let mut heap = BinaryHeap::new();
    heap.push(3);
    heap.push(9);
    heap.push(5);
    println!("biggest: {}", heap.pop().unwrap());
    println!("next: {}", heap.pop().unwrap());
}
// 预期输出:
// biggest: 9
// next: 5
```

📖 A **max-heap**: a structure that always yields the greatest remaining value first. Use it when "handle the most urgent thing next" is the requirement (task scheduling, for instance).

### `BTreeMap`: keys standing in line

A hash map doesn't order anything. When you need to **iterate in key order**, use a **`BTreeMap`**:

```rust
use std::collections::BTreeMap;

fn main() {
    let mut prices = BTreeMap::new();
    prices.insert("orange", 5);
    prices.insert("apple", 3);
    prices.insert("banana", 2);
    for (name, price) in &prices {
        println!("{} -> {}", name, price);
    }
}
// 预期输出: apple -> 3
// banana -> 2
// orange -> 5
```

The API is the same as a hash map's (`new`, `insert`, `get`); the only difference is that iteration comes out **sorted by key** (here, by byte order). The price is slightly slower inserts and lookups — pay it only when you actually need order.

### Capacity and reserving room

A vector **grows itself automatically**: when the shelving is full it rents a bigger room and carries everything over. Moving costs something, so a vector can also reserve space up front:

📖 **Capacity**: how many compartments the vector has rented right now, whether or not they're filled. `.len()` is what's in there; `.capacity()` is what could be.

```rust
fn main() {
    let mut list: Vec<i32> = Vec::new();
    println!("initial capacity: {}", list.capacity());
    list.reserve(100);
    println!("after reserve: {}", list.capacity());
    list.push(1);
    list.push(2);
    println!("len: {}, capacity: {}", list.len(), list.capacity());
}
// 预期输出:
// initial capacity: 0
// after reserve: 100
// len: 2, capacity: 100
```

`reserve(100)` rents a hundred slots in one go; the two pushes after it leave the capacity untouched (98 slots still free). **Pre-allocating**: when you roughly know how much you'll store, `reserve` first and skip the moving bills.

### Common mistake: indexing a `HashSet`

A hash set isn't stored by position, so `visitors[0]` fails to compile:

```rust
// 预期错误: E0608
use std::collections::HashSet;

fn main() {
    let mut visitors = HashSet::new();
    visitors.insert("zhang");
    visitors.insert("li");
    println!("first: {}", visitors[0]);
}
```

```text
error[E0608]: this type does not support indexing
   |     println!("first: {}", visitors[0]);
💡 `value[index]` requires the type to implement the `Index` trait. For collections use `.get(index)`, or use a slice instead.
```

**Fix**: a hash set answers "is it in there?" (`.contains()`), never "which one is number one?". To walk it in order, use a `Vec`, or switch to a sorted container; to simply visit every element, `for name in &visitors` already works.

---

## 15.6 Complete Example: the score ledger

Vector and hash map in one program — collect a class's scores, total them, and count how often each score came up:

```rust
use std::collections::HashMap;

fn main() {
    // —— a Vec: collect the whole class's scores ——
    let mut scores: Vec<i32> = Vec::new();
    scores.push(88);
    scores.push(95);
    scores.push(76);
    scores.push(88);
    scores.push(95);

    // add them up
    let mut total = 0;
    for item in &scores {
        total = total + *item;
    }
    println!("{} students, total {}", scores.len(), total);

    // —— a HashMap: how many students got each score ——
    let mut tally: HashMap<i32, i32> = HashMap::new();
    for item in &scores {
        let count = tally.entry(*item).or_insert(0);
        *count = *count + 1;
    }
    for (score, times) in &tally {
        println!("{} appeared {} time(s)", score, times);
    }

    // —— pop the last one off ——
    if let Some(last) = scores.pop() {
        println!("the last score was {}", last);
        println!("now {} students left", scores.len());
    }
}
```

## 15.7 Line-by-Line Walkthrough

- **Lines 5–10**: five pushes into an empty vector — the warehouse extends itself, you never declare a size.
- **Lines 13–16**: a borrowing walk (`&scores`), so `item` is an `&i32`; `*item` opens the slip to add the number itself.
- **Lines 20–24**: the counting idiom — `entry(*item)` stands at that score's doorway, `or_insert(0)` covers a first visit, `*count` increments the value in place. The `*item` peels the borrowed integer off so it can be a key.
- **Lines 25–27**: walking the tally. Key is the score, value is the count.
- **Lines 30–33**: `.pop()` takes the last score off the end; `if let` handles the "suppose it was already empty" case.
- **Line 1**: `use std::collections::HashMap;` — the single import that makes `HashMap` on line 20 and its `entry` call on line 22 legal.

## 15.8 Output

```text
5 students, total 442
88 appeared 2 time(s)
95 appeared 2 time(s)
76 appeared 1 time(s)
the last score was 95
now 4 students left
```

(The three tally lines may come out in a different order — a hash map doesn't guarantee iteration order, and that's normal, not a bug.)

---

## 15.9 Common Errors and Fixes

### Error One: mixing types in one vector

```rust
// 预期错误: E0308
fn main() {
    let mixed = vec![1, "hello"];
    println!("{}", mixed.len());
}
```

```text
error[E0308]: mismatched types: expected `integer`, found `&str`
   |     let mixed = vec![1, "hello"];
💡 Check if the variable type matches what the context requires. Try using `as` for type casting.
```

A collection holds exactly **one** type. Want numbers and text in the same container? That's Chapter 11's job — define an enum with one variant per shape and put the variants in the vector.

### Error Two: pushing into an immutable vector

```rust
// 预期错误: E0596
fn main() {
    let scores = vec![88, 95, 76];
    scores.push(60);
}
```

```text
error[E0596]: cannot borrow `scores` as mutable, as it is not declared as mutable
   |     scores.push(60);
💡 Only a mutable binding can be borrowed mutably. Add `mut` to the declaration, turning `let value = ...;` into `let mut value = ...;`
```

`push`, `pop` and `remove` all reshape the container, so they need **`mut`**. Change the binding to `let mut scores = ...`.

### Error Three: changing the vector while walking it

```rust
// 预期错误: E0502
fn main() {
    let mut scores = vec![88, 95, 76];
    for item in &scores {
        println!("{}", item);
        scores.push(60);
    }
}
```

```text
error[E0502]: cannot borrow `scores` as mutable because it is also borrowed as immutable
   |         scores.push(60);
📌 Variable `scores`: the mutable borrow on line 6 overlaps the immutable borrow on line 4; Rust does not allow both at once.
💡 Rust does not allow mutable and immutable borrows at the same time. Try reducing the scope of the immutable borrow.
```

The two line numbers in the 📌 line are the two ends of one clash: line 6 is where `push` asks for a mutable borrow, line 4 is the walk that is still holding the immutable one.

`for item in &scores` holds an **immutable** borrow for the whole loop; `scores.push(60)` inside it asks for a **mutable** one, and Chapter 8's iron rule says those never overlap. **Fix**: collect what you want to add in a separate vector, then extend after the loop finishes.

### Error Four: indexing past the end

```rust
// 预期行为: 运行失败
fn main() {
    let scores = vec![88, 95, 76];
    println!("{}", scores[99]);
}
```

This one **compiles** and dies at runtime:

```text
thread 'main' panicked at src/main.rs:3:26:
index out of bounds: the len is 3 but the index is 99
```

**Fix**: when the length isn't certain, use `.get(index)` and handle the `None`. A panic message can be reworded for your language, so the safe move is not to depend on it.

### Error Five: forgetting to import the hash map

```rust
// 预期错误: E0433
fn main() {
    let mut roster = HashMap::new();
    roster.insert("amy", 100);
}
```

```text
error[E0433]: failed to resolve: use of undeclared type `HashMap`
   |     let mut roster = HashMap::new();
💡 Make sure you have imported the required module or type. Use the `use` keyword to import.
```

**Fix**: add `use std::collections::HashMap;` at the top of the file. The same applies to `HashSet`, `VecDeque`, `BinaryHeap` and `BTreeMap` — only `Vec` lives in the prelude.

---

## 15.10 Chapter Glossary

| Term | One-sentence explanation |
|---|---|
| Collection | Heap-stored structure whose length can change |
| Vector (`Vec<T>`) | Ordered, resizable run of same-typed elements |
| `vec!` macro | Build and fill a vector in one expression |
| push | Add an element at the tail |
| pop | Take the tail element out; returns an `Option` |
| get | Read by index, safely — returns an `Option` |
| remove | Take an element out by index; the rest slide forward |
| HashMap | Key → value collection, looked up by key |
| entry | Stand at one key's slot, so you can initialize it if absent |
| or_insert | Insert a default when the key is missing; gives a mutable reference to the value |
| contains_key | Ask whether a key exists; returns a bool |
| HashSet | Non-repeating elements; answers "is it in there?" |
| VecDeque | Queue you can push and pop at both ends |
| BinaryHeap | Max-heap: always pops the largest remaining item |
| BTreeMap | Key-sorted map — use it when iteration order matters |
| capacity / reserve | Slots rented vs. slots used; reserve to skip regrowth |

> 📖 **Reminder**: any term that feels rusty is in the Master Glossary at the front of this book.

---

## 15.11 Exercises

> 💪 Try first, then look at the answer.

### Exercise One: a shopping list

Build a vector of `String`s, push three items, print how many there are and the last one, then pop one and print it.

<details>
<summary>🔍 See Answer</summary>

```rust
fn main() {
    let mut list: Vec<String> = Vec::new();
    list.push(String::from("milk"));
    list.push(String::from("bread"));
    list.push(String::from("eggs"));
    println!("{} items, last one: {}", list.len(), list[2]);
    if let Some(gone) = list.pop() {
        println!("popped: {}", gone);
    }
}
// 预期输出:
// 3 items, last one: eggs
// popped: eggs
```

</details>

### Exercise Two: a mini dictionary

Build a `HashMap` from English words to their meaning. Look up `"apple"` (print the meaning) and `"moon"` (report that it isn't in there).

<details>
<summary>🔍 See Answer</summary>

```rust
use std::collections::HashMap;

fn main() {
    let mut glossary: HashMap<&str, &str> = HashMap::new();
    glossary.insert("apple", "a round fruit with red or green skin");
    glossary.insert("book", "pages bound inside a cover");

    match glossary.get("apple") {
        Some(text) => println!("apple: {}", text),
        None => println!("not found"),
    }
    match glossary.get("moon") {
        Some(text) => println!("moon: {}", text),
        None => println!("not found"),
    }
}
// 预期输出:
// apple: a round fruit with red or green skin
// not found
```

</details>

### Exercise Three: counting votes

Given `["amy", "bob", "amy", "cat", "bob", "amy"]`, use `entry(...).or_insert(0)` to count votes per candidate, then print each candidate's total by looking it up by name.

<details>
<summary>🔍 See Answer</summary>

```rust
use std::collections::HashMap;

fn main() {
    let ballots = ["amy", "bob", "amy", "cat", "bob", "amy"];
    let mut votes = HashMap::new();

    for name in ballots {
        let count = votes.entry(name).or_insert(0);
        *count = *count + 1;
    }
    println!("amy got {}", votes["amy"]);
    println!("bob got {}", votes["bob"]);
    println!("cat got {}", votes["cat"]);
}
// 预期输出:
// amy got 3
// bob got 2
// cat got 1
```

Walking the map with `for (name, count) in &votes` works too, but prints in no particular order — which is why this answer looks candidates up by name instead.

</details>

---

## 15.12 Frequently Asked Questions

**Q: Why does a vector live on the heap? Isn't the stack faster?**

A: Stack space is small, and its size must be known at compile time. A vector *grows* — ten elements or a hundred thousand — and only the heap can offer that elasticity. Reaching an element is just as fast either way, because a vector's contents are one contiguous run of memory.

**Q: What happens when a vector fills up?**

A: There is no "up". When the space runs out, the vector rents a bigger room, moves everything over, and carries on — automatically. That's the self-extending warehouse.

**Q: What can be a hash map key?**

A: Anything you can **compare for equality** and **hash**: integers, booleans, strings, and so on. To use your own struct as a key, derive all three at once above its definition: `#[derive(PartialEq, Eq, Hash)]`.

**Q: `scores[0]` or `scores.get(0)`?**

A: If the index is guaranteed legal (a loop bound, a fixed-size input), index — it's shorter and never returns an `Option` you must unwrap. If the number came from user input, use `.get()` — it can't panic.

**Q: Can I change values while iterating a hash map?**

A: Yes, with a mutable walk: `for (key, value) in &mut map` hands you `value` as `&mut V`, so you can assign through it. What you may *not* do is add or remove keys mid-walk — that's the iron rule again. Collect the changes and apply them after the loop.

**Q: Can a collection hold another collection?**

A: Freely. `Vec<Vec<i32>>` is a grid; a hash map whose values are vectors is a simple index. Read nested type names **from the inside out**: `Vec<Vec<i32>>` is "a vector of vectors of integers".

---

## Next Chapter Preview

So far, every failure in our programs has been the compiler's problem. Soon they'll be yours: files that aren't there, input that isn't a number, network calls that never answer.

Chapter 16, **Error Handling**, gives you Rust's two regular armies:

- **recoverable errors**: `Result<T, E>`, and the `?` operator as a relay pass;
- **unrecoverable errors**: when `panic!` is the right call, and how to trigger it on purpose;
- defining **your own error type** for your own problem domain;
- a decision table for "should this panic, or return a `Result`?".

See you in the next chapter!
