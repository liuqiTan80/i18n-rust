# Chapter 18: Package Management and Build

## 18.0 Learning Objectives

After reading this chapter, you will be able to:

1. Name the three sections of **`Cargo.toml`** (identity, dependencies, config);
2. Add a **third-party library** to a project with one command;
3. Tell a **debug build** from a **release build**, and know which one you want;
4. Tell an **executable** from a **library**;
5. Say where the build output is hiding;
6. Read a **three-part version number** (major.minor.patch).

---

## 18.1 A project needs an ID card

A project is a pile of files. How does the compiler know what the pile is *called*, which
outside libraries it uses, and how it should be built?

💡 **Metaphor**: opening a company means filing **registration papers** first. In the Rust
world the papers are **`Cargo.toml`**, and the office that issues them is **Cargo** — the
build tool rzc drives underneath.

`rzc init greeter` files those papers for you:

```bash
rzc init greeter -l en
```

```text
✅ Project 'greeter' created successfully!
   Language packs are built in; no manual copying needed. To customize, create a lang-packs/en/ directory.
   Run with `rzc run src/main.en`.
```

and leaves this behind:

```text
greeter/
├── Cargo.toml         <- the registration papers
├── .gitignore         <- /target and friends, never committed
├── README.md          <- one line: how to run this project
├── rust-toolchain.toml<- which rustc/cargo version to use
└── src/
    └── main.en        <- your code
```

## 18.2 Reading Cargo.toml line by line

This is the real file that command just generated, word for word:

```toml
[package]
name = "greeter"
version = "0.1.0"
edition = "2024"

[dependencies]

[workspace]
```

| Field | Meaning |
|---|---|
| `[package]` | the identity section |
| `name` | the project's name — also the name of the binary it produces |
| `version` | your version number, three parts, see below |
| `edition` | which Rust language rules apply; `2024` is what this book uses |
| `[dependencies]` | the shopping list — the outside libraries this project uses |
| `[workspace]` | marks this project as its own build unit, so a folder you happen to sit inside cannot adopt it by accident |

> ✨ The empty `[workspace]` line is not decoration. Cargo looks *upward* for a workspace
> root that would claim this package as one of its members; declaring yourself a root ends
> that search, which keeps a tutorial project inside a bigger folder from being pulled into
> the bigger folder's build.

📖 A **dependency** is somebody else's library that your project uses. Write it on the
list; Cargo downloads, builds and links it for you at build time.

### The three-part version number

`0.1.0` reads as "major.minor.patch":

- **patch** (third digit) — bug fixed, usage unchanged;
- **minor** (second digit) — features added, old usage still works;
- **major** (first digit) — big change, old usage may break.

Custom: a `0.` version is still young and may reshuffle itself; `1.` and up is a promise
that minor bumps stay backwards compatible.

## 18.3 Adding a third-party library

Say you want a random-number library. Two ways in, and they end up in the same place.

### Way one: one command

```bash
rzc add rand
```

rzc wraps `cargo add` for you, so this also works and prints the same thing:

```bash
cargo add rand
```

What either command writes into `Cargo.toml`:

```toml
[dependencies]
rand = "0.10.3"
```

Pin a version when you need one (`name@version`):

```bash
rzc add serde_json@1
```

### Way two: edit the list by hand

Open `Cargo.toml`, add the line under `[dependencies]`. Nothing is downloaded until the
next build, so this is fine while you are offline — as long as the version you write
exists.

> ⚠️ Downloading dependencies needs network. Classrooms and corporate networks: check
> before the lesson starts.

### What the tool tells you back

`rzc add` does one thing `cargo add` doesn't: after editing the manifest it looks up
whether your language pack already has a native name for that crate, and says so. This is
the tail of a real run:

```text
    Updating crates.io index
      Adding rand v0.10.3 to dependencies
✅ Added rand. This crate has the native alias "rand" you can use directly in native source code
```

Read that last line as a door opening: the name after **native alias** is what you may
write in your own file instead of the English one. (In an English project the alias is the
crate's own name — there is nothing to translate. A Chinese project adding the same crate
is told, verbatim, 该 crate 有母语别名「随机数」，母语源码中可直接使用 — and its alias for
`serde_json` is JSON序列化.)

> 🔒 **A real trap**: run `rzc add` in a project you just created and it fails before
> Cargo reaches the registry. rzc notices the cause and puts a line of its own in front of
> Cargo's text (this is a real session in an English project; a Chinese project is told
> 这个项目还没有 Cargo 能读取的构建入口 … 先运行 `rzc check src/main.en` …):
>
> ```text
> This project has no build target Cargo can read: `Cargo.toml` declares no [lib]/[[bin]]
> and `src/` has no .rs file for src/main.en. Run `rzc check src/main.en` first to produce
> the standard-Rust entry, then retry
> error: failed to parse manifest at `demo2/Cargo.toml`
>
> Caused by:
>   no targets specified in the manifest
>   either src/lib.rs, src/main.rs, a [lib] section, or [[bin]] section must be present
> ```
>
> Nothing is wrong with your `Cargo.toml`. `rzc init` wrote `src/main.en`, and Cargo only
> recognises `src/main.rs` — the standard-Rust file that `rzc check` or `rzc run` produces.
> **Run `rzc check src/main.en` once, then `rzc add rand` again.** (Section 18.10, error
> two, is the same story, including when it comes straight from Cargo.)

## 18.4 Building: two flavours

```bash
cargo build              # debug: fast to compile, keeps debug info
cargo build --release    # release: optimised, slower to compile, faster to run
```

| | debug (default) | release |
|---|---|---|
| output directory | `target/debug/` | `target/release/` |
| compile time | short | longer |
| optimisations | mostly off | on |
| good for | editing, running again and again | the binary you hand out |

💡 **Metaphor**: debug is the **prototype kitchen** — quick to change, always open;
release is the **packaged product** — slower to make, built to sit on a shelf.

Numbers from one run of this chapter's own project (one tiny function, nothing exotic):

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.57s
    Finished `release` profile [optimized] target(s) in 1.93s
```

```text
target/debug/greeter      4514376 bytes
target/release/greeter     457952 bytes
```

The release binary is roughly **a tenth** of the debug one. Build times on a machine that
has already compiled the dependencies are not a fair comparison; the size gap is.

> ✨ `target/` can be deleted at any moment — it is all build output, and the next build
> recreates it. That is why `.gitignore` lists `/target`. When it balloons:
>
> ```bash
> cargo clean
> ```
>
> ```text
>      Removed 184 files, 55.8MiB total
> ```

## 18.5 An executable or a library?

One project can produce two kinds of output:

| Kind | Entry file | Result |
|---|---|---|
| executable | `src/main.en` | a program you can run |
| library | `src/lib.en` | code for other people to use |

`rzc init` always builds you an executable, because the only file rzc hands to Cargo as a
fixed entry is `src/main.<ext>` (or its native keyword alias, `src/主函数.zh`). `lib.en`
gets no special treatment: it transpiles to `.rz/gen/src/lib.rs` like any other file, so a
folder holding *only* a dialect library still has nothing for Cargo to read:

```text
Error: The manifest declares no build target, and neither src/main.rs nor src/lib.rs exists.
Write your program in src/main.rs, or declare [[bin]]/lib in Cargo.toml.
```

To ship a library written in the dialect, tell Cargo where the product lands — one extra
section in `Cargo.toml`, then one `rzc check` to create the file it points at (this is the
real session, `src/lib.en` holding `pub fn greet() -> i32 { 3 }`):

```toml
[lib]
path = ".rz/gen/src/lib.rs"
```

```bash
rzc check src/lib.en   # ✅ Compilation succeeded, no errors.
cargo build            # Compiling greetlib v0.1.0 … Finished `dev` profile
```

Many real projects do both: the library holds the logic, and a thin `main.en` is the
command-line front door. In practice, though, most dialect code that is *meant to be
reused* simply lives in a module file (`src/utils.en`) reached with `mod utils;` from
`main.en` — Chapter 17 — and no `[lib]` section is needed at all.

What Cargo ultimately reads is always the standard-Rust product, which is why any Cargo
command only works after one `rzc check` (error two in 18.10).

Two programs from one project is a manifest question, and it works with dialect paths —
this is the real session:

```toml
[[bin]]
name = "server"
path = "src/bin/server.en"
```

```bash
rzc check src/bin/server.en   # ✅ Compilation succeeded, no errors.
cargo build                   # Compiling b v0.1.0
./target/debug/server         # serving
```

rzc writes the transpiled product under `.rz/gen/` (here `.rz/gen/src/bin/server.rs`) and
points Cargo at it, so the binary named in `[[bin]]` is what you get in `target/debug/`.

## 18.6 Command cheat sheet

| Command | What it does |
|---|---|
| `rzc init name` | new project, with Cargo.toml and a starter main file |
| `rzc run src/main.en` | transpile + compile + run |
| `rzc check src/main.en` | compile only, no binary (fastest loop) |
| `rzc add rand` | put a third-party library on the dependency list |
| `cargo build` | build the debug binary |
| `cargo build --release` | build the optimised binary |
| `cargo test` | run tests (Chapter 21) |
| `cargo clean` | throw away `target/` |
| `rzc eject src/main.en` | write the standard-Rust product out as a file you can keep |
| `rzc doctor` | check your toolchain: rustc, cargo, language server |

---

## 18.7 Full example: from nothing to a shipped binary

Every line below is a real session on one project.

Create it:

```bash
rzc init demo -l en
cd demo
```

Write `src/main.en`:

```rust
fn main() {
    println!("Hello, world!");
}
```

Check it, then run it:

```bash
rzc check src/main.en
rzc run src/main.en
```

```text
✅ Compilation succeeded, no errors.
Hello, world!
```

Now — and only now — ask Cargo to build it:

```bash
cargo build --release
```

```text
   Compiling demo v0.1.0 (/home/you/demo)
    Finished `release` profile [optimized] target(s) in 1.93s
```

Run the finished product directly, with no toolchain in sight:

```bash
./target/release/demo
```

```text
Hello, world!
```

## 18.8 What each step did

- **`rzc init demo -l en`** wrote five files (18.1) and chose `en` because of `-l en`.
  Without `-l`, `init` reads your **system language** — so a machine set to Chinese gets
  `src/主函数.zh` even when you ask for English *menus*. In an English chapter, keep `-l en`
  on the habit.
- **`rzc check src/main.en`** transpiled your file into `src/main.rs` and ran a
  check-only compile. It is the fast loop: edit, check, edit, check.
- **`rzc run src/main.en`** did the same transpile, then built and executed. Between the
  two of them, `src/` now holds **both** `main.en` and `main.rs` — see the FAQ.
- **`cargo build --release`** is Cargo speaking: it compiled the `src/main.rs` that rzc
  left behind and put the binary in `target/release/`.
- **`./target/release/demo`** is your program. The name comes from
  `name = "demo"` in `Cargo.toml`.

## 18.9 What the terminal said

`rzc check` on a clean file, and `rzc run`:

```text
✅ Compilation succeeded, no errors.
```

```text
Hello, world!
```

A release build of a project that also pulls one dependency looks like this
(the `Compiling` lines are the dependency list being built first):

```text
   Compiling getrandom v0.4.3
   Compiling cpufeatures v0.3.1
   Compiling chacha20 v0.10.2
   Compiling rand v0.10.3
   Compiling greeter v0.1.0 (/home/you/greeter)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.57s
```

---

## 18.10 Common Errors and Fixes

### Error one: the manifest is fine, the folder is not a project

`rzc add` (and a few other commands) need a `Cargo.toml` in this folder or a parent:

```text
Error: No Cargo project found (no Cargo.toml in this directory or its parents); run inside a project
```

**Fix**: `cd` into the project `rzc init` made, or create one. Running commands in the
folder that *contains* your projects is the usual cause.

### Error two: "no targets specified in the manifest"

The loudest beginner roadblock in a dialect project, and it can come from `cargo build`,
`cargo add`, or any Cargo command:

```text
error: failed to parse manifest at `demo/Cargo.toml`

Caused by:
  no targets specified in the manifest
  either src/lib.rs, src/main.rs, a [lib] section, or [[bin]] section must be present
```

Cargo has never heard of `.en` files. It wants `src/main.rs`, which only exists after rzc
transpiles. **Fix**: run `rzc check src/main.en` once — it writes the standard-Rust
product — then repeat the Cargo command.

`rzc add` says this in your own language before Cargo's text appears, because this is the
first thing a fresh project runs into (the sentence is 18.3's trap, same in every UI
language). Cargo itself is not localised, so `cargo build` straight from the shell still
quotes the internals above — read the two lines under **Caused by** as "no `.rs` entry yet",
and reach for `rzc check`.

### Error three: the library is used but never declared

You `use` a crate that isn't on the dependency list:

```rust
use serde::Serialize;

fn main() {
    println!("Hello, world!");
}
// 预期错误: E0432
```

What rzc prints includes a hand up:

```text
error[E0432]: the import path cannot be resolved
  --> main.en:1:5
   | use serde::Serialize;
💡 The imported module or item does not exist. Check the path spelling, and confirm the module is declared (`mod xxx;`) and visible.
💡 Suggestion: you might be missing a crate named `serde`

Hint: "serde" looks like an undeclared third-party library; run `rzc add serde` to add the dependency
```

**Fix**: `rzc add serde` — the hint names the crate and the command. If the crate is not
third-party but one of your own modules, the missing piece is `mod name;` (Chapter 17).

### Error four: the version you asked for does not exist

```bash
cargo add serde@99.0
```

```text
    Updating crates.io index
error: the crate `serde@99.0` could not be found in registry index.
```

**Fix**: let the tool pick — `cargo add serde` with no version takes the newest. Note the
separator is `@`; `serde==99.0` is not a version question but a name question, and Cargo
answers `error: invalid character `=` in package name`.

### Error five: a typo inside Cargo.toml

Cargo reads the manifest before anything else, and a broken field stops the whole build
with a message pointing at the line:

```text
error: package name cannot be empty
 --> Cargo.toml:2:8
  |
2 | name = ""
  |        ^^
```

**Fix**: read the line number Cargo gives you, and fix the manifest — not your source.

---

## 18.11 Chapter Glossary

| Term | One-line explanation |
|---|---|
| Cargo | Rust's build tool and package manager |
| Cargo.toml | the project's manifest: identity, dependencies, settings |
| package | one project with a Cargo.toml |
| dependency | a library your package uses |
| crates.io | the official registry of published crates |
| crate | one published or one compiled library/executable unit |
| debug build | fast to compile, keeps debug info, big binary |
| release build | optimised, slower to compile, small and fast |
| target/ | the directory where all build output lands |
| semantic versioning | the major.minor.patch promise |

> 📖 **Reminder**: any term you don't recognise is in the master glossary at the front of
> the book.

---

## 18.12 Exercises

> 💪 These are hands-on. Type them in a terminal, not in the book.

### Exercise 1: file the papers

Create a project called `workbook`, then open its `Cargo.toml` and point at each field
from section 18.2 while reading the table.

<details>
<summary>🔍 Show the answer</summary>

```bash
rzc init workbook -l en
```

```text
✅ Project 'workbook' created successfully!
   Language packs are built in; no manual copying needed. To customize, create a lang-packs/en/ directory.
   Run with `rzc run src/main.en`.
```

The manifest it writes:

```toml
[package]
name = "workbook"
version = "0.1.0"
edition = "2024"

[dependencies]

[workspace]
```

</details>

### Exercise 2: build the same program twice

In that project run `rzc check src/main.en`, then `cargo build`, then
`cargo build --release`. Look at both binaries in `target/`.

<details>
<summary>🔍 Show the answer points</summary>

Two executables with the same name in two folders:

```text
target/debug/workbook
target/release/workbook
```

On the project used for this chapter, the same ratio showed up: 4514376 bytes debug,
457952 bytes release. The release one is the file you hand to a user; the debug one is
the file you keep for yourself while editing.

</details>

### Exercise 3: add a library

Add the random-number crate and read what changed in `Cargo.toml`.

<details>
<summary>🔍 Show the answer</summary>

```bash
rzc add rand
```

One line appears under `[dependencies]`, with the version the registry served you:

```toml
rand = "0.10.3"
```

If the project was freshly created, `rzc add` fails with "no targets specified in the
manifest" first — run `rzc check src/main.en`, then the `rzc add` again (error two above).
A `Cargo.lock` file also appears: the exact snapshot of every version chosen.

</details>

---

## 18.13 Frequently Asked Questions

**Q: What is the relationship between Cargo and rzc?**

A: rzc is the front door for a native-language dialect: it transpiles your `.en` (or
`.zh`, `.ru`, …) file into standard Rust and translates compiler messages back into your
language. Everything after that — resolving dependencies, compiling, linking — is Cargo
and rustc doing their normal jobs. That is why Cargo can only work once rzc has produced
`src/main.rs`.

**Q: Why did `src/main.rs` appear next to my `src/main.en`?**

A: It is the transpile product, written so Cargo and your editor's analysis have something
standard to read. Treat it as generated. If you hand-edit it, rzc keeps your work rather
than silently discarding it:

```text
backed up src/main.rs to src/main.rs.bak (the original content was overwritten by the new transpile result)
```

The rule to remember: **edit the dialect file, let the product be regenerated.**

**Q: What is `Cargo.lock`?**

A: A snapshot of the exact versions used in this build, every crate in the tree. Commit it
for an application so everyone (and CI) builds the same thing.

**Q: Can a dependency be written in a dialect too?**

A: Published crates are standard Rust, and rzc never touches them — it only transpiles the
source files inside your project. Your dialect code can call them normally, provided your
language pack has a mapping for the names you want to say natively (that is what
`rzc mapping` and the shared registry in Chapter 25 are for).

**Q: Why is `target/` so huge?**

A: It holds every intermediate artifact for every profile you have built, so hundreds of
megabytes is normal. `cargo clean` deletes it (55.8 MiB in this chapter's run) and the
next build refills it.

**Q: What do I send someone?**

A: An executable: the file under `target/release/` — but binaries are
operating-system-specific, so build on (or for) the platform your user has. A library:
publish the source (e.g. to crates.io), or hand over the folder.

**Q: Can I set `edition = "2015"`?**

A: You can, and it will compile — but the newer editions are what this book, rzc's
generated template and current learning material assume. There is no reason to start a new
project on an old edition.

---

## Next chapter preview

Your project can now grow past one file and pull in other people's code. The next thing
to understand is a type you have been using without meeting it: pointers that **own**.

Chapter 19, **Smart Pointers**, covers:

- `Box<T>`: putting a value on the heap in a small box you hold;
- reference counting (`Rc<T>`): how several owners share one value, and what they pay for it;
- when a plain borrow is not enough;
- automatic cleanup with `Drop`.

See you in the next chapter!
