# Chapter 7: Ownership

## 7.0 Learning goals

By the end of this chapter, you will be able to:

1. State **the three rules of ownership**;
2. Explain what happens to a value when its **scope** ends;
3. Tell **copying** (small data like integers) from **moving** (heap data like strings);
4. Read and fix an **E0382** ("value used after move") error;
5. Use **`.clone()`** to make an independent duplicate;
6. Say where ownership goes when a value **enters a function** or **returns from one**.

> ✨ This is Rust's most distinctive chapter — most other languages have nothing like it. Not understanding it on the first read is normal. Read it twice, run every example, and it will click.

---

## 7.1 Why ownership exists: who does the cleaning

💡 **Story**: the school organizes a big cleanup. Without **assigned duties**, two disasters happen:

1. **Nobody cleans**: some corner belongs to no one, and the trash piles up — in programs this is a **memory leak** (memory requested but never returned; memory usage grows until everything grinds to a halt);
2. **Two people clean the same spot**: one just wiped it, the other "fixes" it their own way — chaos. In programs this is a **data race** (covered properly in the multithreading chapters).

Rust's solution is blunt and simple: **everything must have exactly one owner. The owner is responsible; when the owner leaves, the thing is cleaned up automatically.**

📖 **Ownership**: Rust's core rule — every value has exactly one "owner" (a variable), and when the owner leaves the scene, the value is released automatically.

This is why Rust guarantees memory safety without a "garbage collector" (the automatic cleaner other languages hire): **everyone is responsible, and responsibility lands on exactly one person**.

---

## 7.2 The three rules of ownership

Memorize these three and half the chapter is won:

| # | Rule | Plain words |
|---|---|---|
| Rule 1 | Every value has an **owner** (a variable) | Everything has an owner |
| Rule 2 | At any moment there is **only one** owner | One thing can't have two owners |
| Rule 3 | When the owner leaves the **scope**, the value is **released automatically** | When the owner leaves, the thing gets cleaned up |

📖 **Scope**: the stretch of code where a name is "valid" — usually one pair of curly braces `{ }`. Like the playground: it's where you're allowed to act — step off it, and playground business is no longer yours.

---

## 7.3 Scope: the life of a value

Look at this code:

```rust
fn main() {
    {
        let temporary = String::from("only alive inside this block");
        println!("inside the block: {}", temporary);
    }
    // temporary no longer exists here
    println!("continuing outside the block");
}
```

Output:

```
inside the block: only alive inside this block
continuing outside the block
```

`temporary` is born inside the inner braces — and the moment those braces end, it **ceases to exist**. Its memory is returned automatically. Use `temporary` outside the block and the compiler reports "can't find this variable".

> 💡 **Metaphor**: a value's life is a hotel stay: **check in** (declaration) → use → **check out** (leaving the scope). At check-out the room is cleaned automatically (memory released) — nothing to worry about, and nobody can overstay.

> 📖 **Release**: the cleanup executed automatically when a value leaves its scope — the memory goes back to the system.

---

## 7.4 Two storage places: the pocket and the warehouse

Why does some data "copy" while other data "moves"? First, meet the two places data lives:

📖 **The stack**: a fast, small memory region. Like a **pocket** — easy to reach into, but nothing bulky fits.
📖 **The heap**: a slower, large memory region. Like a **warehouse** — it holds big things, but fetching requires a trip and some paperwork.

| Place | Holds | Metaphor |
|---|---|---|
| Stack (pocket) | Integers, floats, booleans, characters — small fixed-size things | Keys, an eraser |
| Heap (warehouse) | Strings, vectors — big things of varying size | A whole crate of books |

A `String`'s contents live on the **heap**: the row of letters sits in the warehouse, and the variable itself (in your pocket) holds only a **claim ticket** (recording the warehouse location and length).

With that picture, copy and move below make perfect sense.

---

## 7.5 Copy: pocket things duplicate for free

```rust
fn main() {
    let a = 5;
    let b = a;
    println!("a: {}, b: {}", a, b);
}
```

Output: `a: 5, b: 5`

Integers live in the **pocket**; duplicating one costs nothing. So `let b = a` is a **copy**: `a` still exists, and `b` is an independent duplicate. Both variables are free to use.

📖 **Copy**: the property of being automatically duplicated bit by bit when a value leaves its scope. Integers, floats, booleans and characters all have it.

**Which types copy?** Remember the mantra: **"numbers, booleans, single characters"** — integers (`i32`, `i64`…), floats (`f64`), booleans (`true`/`false`), characters. All pocket-dwellers; assignment duplicates them.

---

## 7.6 Move: there's only one claim ticket

A string lives in the **warehouse**; the variable holds only the claim ticket. Now:

```rust
fn main() {
    let sentence_a = String::from("Hello");
    let sentence_b = sentence_a;
    println!("sentence_b: {}", sentence_b);
}
```

`let sentence_b = sentence_a` is not a copy — it's a **move**: the claim ticket passed from `sentence_a`'s hand to `sentence_b`'s. There's only one ticket — the new owner is `sentence_b`.

💡 **Metaphor**: a movie ticket handed to a friend leaves your hand empty. Want in? Either your friend gives it back, or you buy another (the clone in 7.7).

If you try the old variable name after the move:

```rust
// 预期错误: E0382
fn main() {
    let sentence_a = String::from("Hello");
    let sentence_b = sentence_a;
    println!("sentence_a: {}", sentence_a);   // ❌
}
```

The compiler reports, mercilessly:

```
error[E0382]: borrow of moved value: `sentence_a`
 --> src/main.rs:4:27
 |
2 |     let sentence_a = String::from("Hello");
 |         ---------- move occurs because `sentence_a` has type `String`...
3 |     let sentence_b = sentence_a;
 |                      ---------- value moved here
4 |     println!("sentence_a: {}", sentence_a);
 |                                ^^^^^^^^^^ value borrowed here after move
```

📖 **Move**: handing a heap value's ownership from one variable to another; the original variable dies instantly.

> 📖 **E0382**: the error code for "value used after move". Seeing it means: some value changed owners, and you're still asking the old owner for it. (Notice how the error even draws the journey — where the value was created, where it moved, where you misused it.)

**Why is Rust so stingy?** Copying a whole crate of books (possibly hundreds of megabytes) slows the program; two people sharing one claim ticket means double-cleaning the same hotel room at check-out — an instant crash. Rust picks the cleanest scheme: **one thing, one owner; give it away and it's gone**.

---

## 7.7 Clone: buy another ticket

Want two independent copies? Use **`.clone()`** ("clone, duplicate"):

```rust
fn main() {
    let original = String::from("treasure map");
    let duplicate = original.clone();
    println!("original: {}, duplicate: {}", original, duplicate);
}
```

Output: `original: treasure map, duplicate: treasure map`

Clone **truly duplicates what's in the warehouse** — two claim tickets, each pointing at its own warehouse. Change one, the other is untouched.

> ⚠️ **Careful**: cloning takes time (photocopying the whole crate). If a move solves it, don't clone. At the beginner stage, clone freely — performance tuning comes later.

**Comparison table to remember**:

| Operation | Pocket data (integers etc.) | Warehouse data (strings etc.) |
|---|---|---|
| `let b = a` | Copy — both usable | Move — a dies |
| `let b = a.clone()` | Also clones (unnecessary) | Copy — both usable |

---

## 7.8 Ownership and functions: transfers at both doors

Values **entering a function** and **returning from one** transfer ownership the same way.

### Passing in: the value moves into the function

```rust
fn receive(sentence: String) {
    println!("received inside the function: {}", sentence);
}

fn main() {
    let blessing = String::from("Happy birthday");
    receive(blessing);
    // blessing has moved into the function — it can't be used here anymore
}
```

`receive(blessing)` transfers the whole blessing to the function's parameter `sentence`. When the call ends, `sentence` leaves its scope and the value is released. `blessing` in the main function is dead from then on.

### Returning: the value moves back to the caller

```rust
fn make_greeting(name: &str) -> String {
    String::from(name)
}

fn main() {
    let greeting = make_greeting("Xiaoming");
    println!("got: {}", greeting);
}
```

Output: `got: Xiaoming`

The string built inside the function is transferred via the trailing expression to `greeting` — its new owner.

> 💡 **Metaphor**: a function is a parcel station. Passing in = you **mail the parcel in** (it leaves your hands); returning = the station **mails a new parcel out to you** (it's yours from then on).

> ✨ **You may find this annoying right now: can't I just lend things?** You can! Chapter 8, *References and Borrowing*, is entirely about "how to lend" — lend, return, no ownership transfer.

---

## 7.9 A complete example: the parcel station

All the ownership knowledge, strung into a "mailing a parcel" story:

```rust
// derive(Clone): the compiler generates the clone ability automatically
// (the trait names in derive are English: Clone, Debug, …)
#[derive(Clone)]
struct Parcel {
    contents: String,
    weight: i32,
}

impl Parcel {
    fn new(contents: String, weight: i32) -> Parcel {
        Parcel { contents: contents, weight: weight }
    }

    fn introduce(&self) {
        println!("parcel: {}, {} grams", self.contents, self.weight);
    }
}

// sign_for = taking the parcel's ownership in; after signing, the parcel is "used up"
fn sign_for(parcel: Parcel) {
    parcel.introduce();
    println!("signed for!");
}

fn main() {
    // Build a parcel
    let a = Parcel::new(String::from("toys"), 500);
    a.introduce();

    // Clone one to mail out, keep the original
    let copy = a.clone();
    sign_for(copy);

    // The original is still here, still usable
    a.introduce();

    // Finally mail out the original too
    sign_for(a);
    // From now on, `a` is dead
}
```

---

## 7.10 Line by line

- **Line 3**: `#[derive(Clone)]` is a "sticker" (formally an **attribute**) telling the compiler to generate a `.clone()` method for `Parcel` automatically.
- **Lines 4–7**: the struct `Parcel` has two fields. `contents` is a `String` (lives in the warehouse); `weight` is an integer (lives in the pocket).
- **Lines 10–13**: the associated function `new`, returning a new parcel (ownership handed to the caller).
- **Lines 15–17**: `introduce` takes `&self` — **only borrows**, a look but no take, so the parcel survives the call (borrowing details in Chapter 8).
- **Lines 21–24**: `sign_for`'s parameter type is `Parcel` (no `&`) — the incoming parcel's ownership belongs to `parcel`, and when the function ends, it is released. The sender has lost this parcel.
- **Line 28**: `String::from("toys")` builds a string, owned by `a` along with the parcel.
- **Line 32**: `a.clone()` duplicates a fully independent `copy` (the inner string included).
- **Line 33**: `sign_for(copy)` — the copy is collected and released.
- **Line 36**: `a` is intact and can still introduce itself.
- **Line 39**: `sign_for(a)` — the original is collected too. Writing `a.introduce()` after this line would be an E0382.

---

## 7.11 What you should see

```
parcel: toys, 500 grams
parcel: toys, 500 grams
signed for!
parcel: toys, 500 grams
parcel: toys, 500 grams
signed for!
```

Mapping it: line 1 is the self-introduction after creation; lines 2–3 are the cloned copy being signed for; lines 4–6 are the original introducing itself and then being signed for too. The two parcels hold identical contents — but they are **two independent values**.

---

## 7.12 Common mistakes and how to fix them

### Mistake one: E0382, used after move

```
error[E0382]: borrow of moved value: `sentence_a`
```

**Three repair routes**:

1. **Just want to look** → pass a reference, `&sentence_a` (Chapter 8);
2. **Both sides need a copy** → `sentence_a.clone()`;
3. **Truly giving it to one person** → reorder the code so the last use comes before the move.

### Mistake two: applying integer behavior to strings

"Why can't I use `a` anymore? `let b = a` worked fine with integers!" — because integers **copy**, strings **move**. Revisit 7.4's mantra: only "numbers, booleans, single characters" copy.

### Mistake three: cloned, but "method not found"

Calling `.clone()` on your own struct errors because structs **don't** clone by default. Add the sticker `#[derive(Clone)]` before the struct definition.

### Mistake four: a misspelled derived trait name

`#[derive(Clonee)]` reports "can't find derive macro" — a typo in the trait name. Supported names: `Clone`, `Debug`, `Copy`, `Hash` and more.

---

## 7.13 Chapter glossary

| Term | One-line meaning |
|---|---|
| Ownership | Every value has exactly one owner; when the owner leaves, the value is released |
| Scope | The stretch of code where a name is valid — usually one pair of braces |
| Release | The automatic memory cleanup when a value leaves its scope |
| Stack | The fast, small memory region for fixed-size data — the pocket |
| Heap | The large, slower memory region for variable-size data — the warehouse |
| Copy | How stack data assigns — both sides keep an independent copy |
| Move | How heap data assigns — the claim ticket handed over, the original variable dies |
| Clone | Manually duplicating the full heap copy |
| E0382 | The error code for "value used after move" |
| Attribute | A "sticker" on code, written `#[...]`, giving the compiler extra instructions |
| Derive | Having the compiler generate trait implementations automatically, like `#[derive(Clone)]` |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 7.14 Exercises

> 💪 Try first, then peek.

### Exercise one: who can still be used

Decide which variables are still usable after each line (write your answers on paper first):

```rust
let a = 10;
let b = a;
let c = String::from("Hello");
let d = c;
let e = d.clone();
```

<details>
<summary>🔍 View answer</summary>

- `a`: usable (integers copy);
- `b`: usable;
- `c`: **not usable** (moved to d);
- `d`: usable;
- `e`: usable (an independent clone).

</details>

### Exercise two: fix the E0382

This code errors — fix it **two different ways** (hint: one uses clone, one reorders the uses):

```rust
// 预期错误: E0382
fn main() {
    let blessing = String::from("Happy new year");
    let backup = blessing;
    println!("{}", blessing);
}
```

<details>
<summary>🔍 View answer</summary>

Way one: clone.

```rust
let backup = blessing.clone();
```

Way two: print before moving.

```rust
let blessing = String::from("Happy new year");
println!("{}", blessing);
let backup = blessing;
```

</details>

### Exercise three: a parcel between functions

Write a function `pack` (takes an integer weight, returns a parcel) and a function `unpack` (takes a parcel, prints its contents). In the main function, pack a "building block", unpack it, then try to use the original variable — and watch the compiler remind you.

<details>
<summary>🔍 View answer</summary>

```rust
struct Parcel {
    contents: String,
    weight: i32,
}

fn pack(weight: i32) -> Parcel {
    Parcel { contents: String::from("building blocks"), weight: weight }
}

fn unpack(parcel: Parcel) {
    println!("unpacked: {}", parcel.contents);
}

fn main() {
    let gift = pack(800);
    unpack(gift);
    // The next line would error with E0382: the gift's ownership
    // has already been transferred to unpack's parameter
    // println!("{}", gift.weight);
}
```

Output: `unpacked: building blocks`

</details>

---

## 7.15 FAQ

**Q: Is the ownership check at runtime or compile time?**

**Compile time**. The compiler traces every ownership transfer before you run; non-compliant programs don't even compile. So Rust programs carry no ownership-checking cost at runtime — the "zero cost" philosophy in action.

**Q: Do integers really never move?**

Correct. Integers, floats, booleans and characters — "pocket data" — always copy on assignment. Later you'll learn the precise rule: types implementing the `Copy` trait all copy.

**Q: Is the string literal `"Hello"` a move too?**

A literal is a **string slice** (`&str`) — a "borrow". Borrowed things only copy the borrow slip on assignment; both sides stay usable. What truly moves is the `String` type, like what `String::from("Hello")` builds. Chapter 9 draws the line precisely.

**Q: What's the real difference between clone and copy?**

Copy is **automatic and cheap** (the compiler photocopies a few pocket bytes); clone is **manual and pricier** (a full warehouse photocopy). Types that copy can also clone — but needn't; types that move must clone if you want two.

**Q: Won't thinking about ownership for every variable be exhausting?**

At first, yes — it's the biggest gate in learning Rust. The good news: after two or three chapters of projects, "who owns whom" becomes intuition. And the compiler is a free partner — the moment you forget, it reminds you, with a fix suggestion attached.

**Q: Why don't other languages (Python, JavaScript) have any of this?**

Most of them use a "garbage collector": a background cleaner hired while the program runs, patrolling and sweeping up unused values. Convenient — but you pay runtime performance costs and unpredictability. Rust solves it at compile time, so at runtime it's fast and steady.

---

## What's next

Ownership is safe, but "give it away and it's gone" is inconvenient — must you transfer ownership just to let someone *look* at your data?

**Chapter 8, "References and Borrowing"**, teaches Rust's lending rules:

- A **reference** `&`: borrow to look; the owner is untouched;
- A **mutable reference** `&mut`: borrow to modify — with strict limits;
- **One iron rule**: at any moment, either one mutable borrow or any number of read-only borrows;
- Why that rule prevents data from fighting.

See you in the next chapter!
