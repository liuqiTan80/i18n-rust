# Chapter 17: Modules

## 17.0 Learning Objectives

After reading this chapter, you will be able to:

1. Explain what a module is with the **chest of drawers** metaphor (sorting + privacy);
2. Split code into areas with the **`mod`** keyword, nested as deep as you like;
3. Control who may use what with **`pub`**;
4. Find things with **`::` paths** and **`use`**;
5. Split a program into **several files**, one file per module;
6. Read and fix **E0603** (something on your path is private).

---

## 17.1 Why modules

Every program so far lived in one file. Real projects run to thousands of lines, and hunting a function in a single pile is like hunting a screwdriver in a junk drawer.

💡 **Metaphor**: Chapter 15's warehouse works better when its contents are sorted into a **chest of drawers** — cutlery in one drawer, bedding in another — and each drawer can be **locked** so only the people inside the household can reach in.

Modules solve three problems at once:

1. **grouping** — related code lives together;
2. **namespaces** — two different drawers can each hold a thing called "clip" without a fight;
3. **privacy** — whatever a drawer keeps unlisted stays out of strangers' hands.

## 17.2 Defining a module

```rust
mod kitchen {
    fn wash_veg() {
        println!("washing vegetables");
    }

    pub fn serve() {
        wash_veg();
        println!("serving!");
    }
}

fn main() {
    kitchen::serve();
}
// 预期输出:
// washing vegetables
// serving!
```

📖 A **module** is a named box around functions, structs, constants and other modules. Modules **nest** — a drawer inside a drawer:

```rust
mod kitchen {
    pub mod stove {
        pub fn stir_fry() {
            println!("stir-frying...");
        }
    }
}

fn main() {
    kitchen::stove::stir_fry();
}
// 预期输出: stir-frying...
```

## 17.3 Public and private: the lock

Rust's rule runs **opposite** to many languages:

> **Everything is private by default.** To let outsiders use something, say so with **`pub`**.

```rust
mod kitchen {
    fn wash_veg() { /* ... */ }        // 🔒 private: kitchen only
    pub fn serve() { /* ... */ }       // 🔓 public: the world may call it
}
```

From outside, you can reach one and not the other:

```text
kitchen::serve();      // ✅ a public function
kitchen::wash_veg();   // ❌ refuses: private
```

💡 **Metaphor**: the restaurant opens a **serving window** to customers, but how the kitchen washes vegetables is internal procedure, not on display. That is the payoff of privacy: **free to change inside, stable to depend on outside**.

## 17.4 Paths: the address of a thing

📖 A **path** is a chain of names joined by **`::`**, read like a postal address: "estate :: building :: flat number".

```rust
mod kitchen {
    pub mod stove {
        pub fn heat() {
            println!("heating up");
        }
    }
}

fn main() {
    kitchen::stove::heat();    // full address, every level public
}
// 预期输出: heating up
```

**Inside a module**, neighbours are addressed without the full chain — and the three
shortcuts below all compile and run in this one program:

```rust
mod kitchen {
    fn wash_veg() {
        println!("washing vegetables");
    }

    pub mod stove {
        pub fn cook() {
            super::wash_veg();   // up to the parent, then in
        }
    }

    pub fn serve() {
        wash_veg();              // same level: just the name
        stove::cook();           // a nested module right here: relative path
    }
}

fn main() {
    kitchen::serve();
}
// 预期输出:
// washing vegetables
// washing vegetables
```

| Write | Means |
|---|---|
| `stove::cook()` | from where I am, step into the neighbour `stove` |
| `crate::kitchen::serve()` | from the **root of this program**, follow the whole way |
| `super::wash_veg()` | one level **up**, then in — a child module reaching its parent |

`crate::` is the form you'll see in real projects: it works no matter where the
calling code sits, because it starts the address at the crate root.

Two things in that program are worth pausing on. `washing vegetables` prints **twice**,
because `serve` calls `wash_veg` directly and then `stove::cook`, which reaches the same
function through `super::`. And that second call is the fine print of privacy:
**private means invisible from outside, not from underneath** — `stove` lives inside
`kitchen`, so `super::wash_veg()` is allowed even though `wash_veg` has no `pub`.

## 17.5 `use`: the address book

Writing the whole address every time is tiring. **`use`** imports a path into the current file, after which you call the thing by name:

```rust
mod kitchen {
    pub fn serve() {
        println!("serving!");
    }
}

use kitchen::serve;

fn main() {
    serve();    // no kitchen:: in front
}
// 预期输出: serving!
```

Chapter 15's `use std::collections::HashMap;` was exactly this mechanism — dragging the hash map out of the standard library's depths to within reach.

> ✨ Small convention: importing a **function** names the function (`use kitchen::serve;`); importing a **type** names the type (`use std::collections::HashMap;`), and you call its methods with a dot afterwards.

The wildcard form pours in everything a module makes public:

```rust
mod kitchen {
    pub fn serve() {
        println!("serving!");
    }
    pub fn clear_table() {
        println!("table cleared");
    }
}

use kitchen::*;

fn main() {
    serve();
    clear_table();
}
// 预期输出:
// serving!
// table cleared
```

⚠️ `use kitchen::*;` is legal but unpolite to your future self: two globbed modules can carry the same name, and then the compiler has to ask *you* which one you meant. List what you use.

## 17.6 A file is a module

When the pile grows, give each module its own file. **File name = module name**:

```text
my_project/
├── Cargo.toml
└── src/
    ├── main.en          <- the entry point
    └── cutlery.en       <- a new file = a new module
```

`cutlery.en` holds the module's body — no `mod` wrapper, the file *is* the module:

```text
pub fn grab_fork() {
    println!("grabbed a fork");
}
```

`main.en` declares it with **`mod cutlery;`** — note the **semicolon**, not braces, meaning "there is a file called `cutlery` with that module inside":

```text
mod cutlery;

fn main() {
    cutlery::grab_fork();
}
```

Run that two-file program and it prints `grabbed a fork`. The same shape written in one file — which is what the rest of this chapter's examples do, and what the book's verifier checks:

```rust
mod cutlery {
    pub fn grab_fork() {
        println!("grabbed a fork");
    }
}

fn main() {
    cutlery::grab_fork();
}
// 预期输出: grabbed a fork
```

📖 Inline modules (`mod name { ... }`) and file modules (`mod name;` + a file) are the **same module** written two ways. Rust looks for `cutlery.en` (in a dialect project) or `cutlery.rs` (plain Rust) next to the file that declares it.

---

## 17.7 Full example: the little restaurant

Grouping, nesting, privacy, paths and imports in one kitchen:

```rust
mod kitchen {
    // —— a nested drawer ——
    pub mod stove {
        pub fn stir_fry() {
            println!("stir-frying...");
        }
    }

    // —— private: internal procedure ——
    fn wash_veg() {
        println!("washing vegetables");
    }

    // —— public: the only window to the outside ——
    pub fn serve() {
        wash_veg();          // inside the module: private neighbours are fine
        stove::stir_fry();   // nested module, named from next door
        println!("serving!");
    }
}

// write the serving window into the address book
use kitchen::serve;

fn main() {
    serve();                 // reached through `use`
    kitchen::stove::stir_fry();   // full path, no address book needed
}
// 预期输出:
// washing vegetables
// stir-frying...
// serving!
// stir-frying...
```

## 17.8 Line-by-line explanation

- **Line 1**: `mod kitchen {` — the first drawer opens.
- **Lines 2–7**: the nested module `stove`, `pub` at **both** levels — otherwise the outside can't reach `stir_fry` at all (one locked level blocks the whole corridor).
- **Lines 9–12**: `wash_veg` has no `pub`, so it is private by birth.
- **Lines 14–19**: `serve` is public and uses the shortcuts inside its own module: a same-level name and a relative `stove::stir_fry()`.
- **Line 23**: `use kitchen::serve;` (line 22 is the comment above it) — from here on, "serve" alone is enough.
- **Line 26**: `serve()` runs the whole sequence: wash → stir-fry → serve.
- **Line 27**: bypassing the window and walking straight into the stove — the path is written in full, and every level on it is public.

## 17.9 What it prints

```text
washing vegetables
stir-frying...
serving!
stir-frying...
```

The first three lines come from `serve()`; the last one is the standalone full-path call.

---

## 17.10 Common Errors and Fixes

### Error one: E0603 — something on the path is private

```rust
mod lab {
    mod shelf {
        pub fn take_reagent() {
            println!("taken");
        }
    }
}

fn main() {
    lab::shelf::take_reagent();
}
// 预期错误: E0603
```

The compiler names the level that is locked:

```text
error[E0603]: `shelf` is private
```

**Fix**: walk the path left to right and put `pub` on **every** level — the module you step through, the function you call, and the fields you read. One missing link ends the trip.

### Error two: wrong path, name not found

```rust
mod kitchen {
    fn wash_veg() {
        println!("washing vegetables");
    }
}

fn main() {
    wash_veg();
}
// 预期错误: E0425
```

`error[E0425]: cannot find \`wash_veg\` in this scope` — the name exists, just not *here*. **Fix**: think from where you are standing: same level → plain name; elsewhere → full path (`kitchen::wash_veg()`) or a `use` import first.

### Error three: a file module written with braces

`mod cutlery { ... }` defines a module **in place**; a file module must be `mod cutlery;` — semicolon, and the file name has to match the module name. Swap the two and either the compiler can't find the file, or your file sits unused while an empty in-place module shadows it.

### Error four: the struct is public, the fields aren't

```rust
mod school {
    pub struct Student {
        pub name: String,
        age: u32,
    }

    pub fn new_student() -> Student {
        Student { name: String::from("amy"), age: 20 }
    }
}

fn main() {
    let s = school::new_student();
    println!("{}", s.age);
}
// 预期错误: E0616
```

Outside `school` you may *have* a `Student`, but you cannot touch `age`: **fields are private individually** and each needs its own `pub`. **Fix**: mark the field public, or better, keep it private and expose a `pub fn age(&self) -> u32` — then the module still controls how its data is read.

---

## 17.11 Chapter Glossary

| Term | One-line explanation |
|---|---|
| module | a named box of code, `mod name { ... }` or one file |
| `pub` | keyword that lets the outside use an item (default is private) |
| private | the default: reachable only inside its own module |
| path | names joined by `::`, the address of a thing |
| `use` | import a path once, then refer to it by short name |
| crate | one compilable unit — the root module every `crate::` path starts from |
| file module | `mod name;` plus a file called `name.rs` (or `name.en` here) |

> 📖 **Reminder**: any term you don't recognise is in the master glossary at the front of the book.

---

## 17.12 Exercises

> 💪 Try it first, then look at the answer.

### Exercise 1: the pencil case

Make a module `pencil_case` with a private function `sharpen()` and a public function `write()` that calls `sharpen()` first. Call `write()` from `main`.

<details>
<summary>🔍 Show the answer</summary>

```rust
mod pencil_case {
    fn sharpen() {
        println!("sharpening");
    }

    pub fn write() {
        sharpen();
        println!("scribble scribble");
    }
}

fn main() {
    pencil_case::write();
}
// 预期输出:
// sharpening
// scribble scribble
```

</details>

### Exercise 2: fix the lock with the fewest changes

The code below fails with E0603. Repair it with as few edits as possible:

```rust
// 预期错误: E0603
mod lab {
    mod shelf {
        pub fn take_reagent() {
            println!("taken");
        }
    }
}

fn main() {
    lab::shelf::take_reagent();
}
```

<details>
<summary>🔍 Show the answer</summary>

Three levels, so three `pub` — the outer module, the inner module and the function are all needed on this path:

```rust
mod lab {
    pub mod shelf {
        pub fn take_reagent() {
            println!("taken");
        }
    }
}

fn main() {
    lab::shelf::take_reagent();
}
// 预期输出: taken
```

`lab` itself doesn't need `pub`: the call starts *inside* the crate root, where `lab` is already visible.

</details>

### Exercise 3: split into two files

Turn Exercise 1 into two files: `main.en` and `pencil_case.en`.

<details>
<summary>🔍 Show the answer</summary>

`pencil_case.en` (the file is the module body):

```text
pub fn write() {
    println!("scribble scribble");
}
```

`main.en`:

```text
mod pencil_case;

fn main() {
    pencil_case::write();
}
```

Two files, one program: it prints `scribble scribble`. Move `sharpen()` into `pencil_case.en` as a private `fn` and `write()` keeps working — the lock is per module, and both functions now live inside the same one.

</details>

---

## 17.13 Frequently Asked Questions

**Q: Why is everything private by default? Most languages default to public.**

A: Because defaulting to private forces you to decide what you **promise** to the outside. Anything unlisted is internal, so you can rewrite it without breaking a caller. It feels strict for a week and then it feels safe.

**Q: Must modules and files line up one to one?**

A: No. One file may hold several `mod name { ... }` blocks, and one module may spread over a directory of files. "One file, one module" is the tidiest habit, so keep it while learning.

**Q: Does `use` make the program slower?**

A: No. `use` is a compile-time name binding — a shortcut for the reader and the compiler, with zero runtime cost.

**Q: Why do I have to `use std::collections::HashMap;` but never `use` for `Vec` or `String`?**

A: The standard library pre-loads a small **prelude** into every file: `Vec`, `String`, `Option`, `Result` and a handful of other everyday items. `HashMap` isn't in it, so it has to be imported by path. Same mechanism, two shelves.

**Q: The standard library's paths are deep (`std::collections::HashMap`). How is it organised?**

A: Exactly the way this chapter teaches: `std` is the crate root, `collections` is a module inside it, `HashMap` a type inside that. Nothing about your syntax changes when you read library source.

**Q: Private functions can't be called from outside — how do I test them, then?**

A: Code inside a module reaches everything inside it, tests included — that's what Chapter 21, *Testing*, uses when it puts `mod tests` next to the code it checks.

---

## Next chapter preview

Modules keep your code tidy. The next thing to manage is **other people's code**: how do you pull in a ready-made library, and how do you publish your own?

Chapter 18, **Package Management and Build**, covers:

- `Cargo.toml`: a project's ID card and shopping list;
- adding third-party crates (`cargo add` and the rzc helpers);
- where the build output goes (debug and release);
- libraries versus executables.

See you in the next chapter!
