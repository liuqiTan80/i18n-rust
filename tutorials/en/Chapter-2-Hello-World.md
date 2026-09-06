# Chapter 2: Hello, World

> 💡 If you haven't set up the VS Code extension yet, read **Chapter 1 (Making the Most of the VS Code Extension)** first — this chapter assumes you can open a `.rs` file.

Welcome to Rust! In this chapter you will accomplish something remarkable — **writing a program that actually runs**.

If you have never written code before, don't worry. This chapter starts from "what is a computer" and "what is a keyboard".

---

## 2.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain in plain words what "programming" is.
2. Install all the tools you need on your computer.
3. Write and run your first Rust program.
4. Read the three most common kinds of error messages.

---

## 2.1 What is programming

### Writing notes for a robot

Imagine a robot standing in front of you. It's very obedient — and very dumb. It does **exactly what you say, word by word**, and never guesses what you meant.

Say "bring me a glass of water" and it may just stand there, dazed, if you didn't say "first pick up a glass".

**Programming is writing notes for that robot** — spelling out every step: pick up the glass, walk to the dispenser, press the button, turn the tap off when the glass is full…

> 💡 **Metaphor**: programming is like writing a super-detailed recipe. You can't write "make noodles" — you have to write "Step 1: turn on the tap. Step 2: fill the pot halfway. Step 3: …"

Each note you write is called a piece of **source code**.

> 📖 **Source code**: text you write for a computer to read.

The language you write source code in is called a **programming language**.

> 📖 **Programming language**: a language humans use to talk to computers. Just as humans have English, Chinese and Japanese, there are many programming languages, each with its own character.

### The language you'll learn: Rust

The language of this book is **Rust**, one of the most popular programming languages in the world.

Rust's keywords are English — which, for you, is the natural part:

| Keyword | What it means | Everyday analogy |
|---|---|---|
| `fn` | Define a set of instructions you can reuse | Name a sequence of actions, then trigger it by name |
| `let` | Create a "box" to store data | Grab a box, stick a label on it, put something inside |
| `if` | Do something only under a condition | If it's raining, take an umbrella |
| `for` | Repeat an action for each item | For every apple in the basket, wash it |
| `println!` | Show text on the screen | "Call out" to the screen |

Your code looks like English sentences — but it is **a program that really runs**:

```rust
fn main() {
    println!("Hello, world!");
}
```

> ✨ **Tip**: don't worry about the details of that code yet — section 1.6 explains it word by word. For now, all you need to know is: these lines are a program the computer can read.

> 🌍 **A note for the multilingual classroom**: for an English speaker, standard Rust *is* "Rust in your mother tongue". Speakers of other languages — Chinese, Japanese, Russian and seven more — write the same programs through the rzc translator, which swaps the keywords for their own words (their Chapter 2 shows `函数 主函数()` and friends). The teaching diagnostics you'll meet in this chapter are translated into all 10 languages by the same tool.

---

## 2.2 First, meet your computer

Before writing code, let's spend a few minutes on some basics. If you already know all this, jump straight to 2.3.

### Files: the "sheets of paper" inside a computer

> 📖 **File**: the basic unit of storage on a computer — like sheets of paper. A sheet can hold an essay or a drawing; a file can hold text or images.

Every file has a **file name**, like `essay.txt`. The `txt` after the dot is the **extension** — it says what type of file this is.

> 📖 **Extension**: the part after the last dot in a file name; it tells the computer what kind of file it is. `.txt` is plain text, `.jpg` is a picture. Rust source files use the extension **`.rs`**.

### Folders: the "drawers" of a computer

> 📖 **Folder**: a "drawer" that holds files — some systems call it a directory. You can put many files in one folder, like many sheets of paper in one drawer, to keep things tidy.

### The most important keys on your keyboard

Writing code mostly means typing. You will certainly use these keys:

| Key | Where | What it does |
|---|---|---|
| `Enter` | Right of the main key area | Start a new line; in the terminal it means "run this command" |
| `Space` | The longest key at the bottom | Type one space |
| `Backspace` | Above `Enter` | Delete one character before the cursor |
| `Shift` | One on each side | Hold it while pressing a letter key for capitals and upper symbols |
| `Ctrl` | One on each side | For combinations, like `Ctrl+S` to save |

> ⚠️ **Careful**: code must be typed with **English punctuation**! A Chinese input method's comma is `，`; the English one is `,`. Code itself always uses English punctuation. (Punctuation *inside quoted text* follows the language of that text.)

### Saving files

Code you type in the editor lives only in "memory" at first — switch the machine off and it's gone. You must **save** to write it to disk.

> 📖 **Save**: write what you're editing to the disk so it can't be lost. In almost every program, `Ctrl+S` (`Cmd+S` on macOS) saves.

> ✨ **Tip**: build the habit — press `Ctrl+S` every few lines.

---

## 2.3 Meet your tools

Writing code takes three tools — like cooking needs a kitchen, a spatula and a recipe.

### Tool one: the terminal — a window for talking to your computer

> 📖 **Terminal**: a text-only window. You type commands, the computer runs them and answers in text. Also called the "command line".

💡 **Metaphor**: the terminal is like a waiter who only takes written orders. You write "one glass of water", he brings the water and writes back "here it is".

How to open it:

- **Windows**: click the bottom-left corner, search "PowerShell", press Enter.
- **macOS**: press `Cmd+Space`, type "Terminal", press Enter.
- **Linux**: press `Ctrl+Alt+T`.

You'll see a blinking vertical line — the **cursor** — meaning "it's your turn to type".

> 📖 **Command**: one line of instruction you type into the terminal. Press Enter and the computer does it. Typing `cargo --version` and pressing Enter asks cargo to show its version.

### Tool two: the compiler — your translator

Source code is human-readable text, but computers only understand **binary**.

> 📖 **Binary**: representing everything with just 0 and 1 — like a light switch that's only ever on or off. Inside a computer it's "electricity" or "no electricity", written as 1 and 0.

So you need a **translator** to turn source code into instructions the machine can execute.

> 📖 **Compiler**: a translator program. It reads your source code, checks it for mistakes, then translates it into machine instructions.

> 📖 **Compile**: the act of translating code. "Compile it" = "have the translator do its job".

```
Your source code (text humans can read)
    ↓  the compiler translates
Machine code (01010101… — what computers read)
    ↓  the computer executes
Output (shown on screen)
```

Rust's compiler is called **rustc** — but you'll rarely call it directly. **cargo**, Rust's built-in project butler, invokes it for you: one `cargo run` compiles and runs.

> ✨ **Tip**: this book's companion tool **rzc** goes one step further for speakers of other languages — it translates *their* native keywords into standard Rust first, then hands over to the same rustc. For you, writing standard Rust, rzc's value is its teaching diagnostics: error messages and hints rendered friendlier, in any of 10 languages.

### Tool three: the text editor — your writing workbook

> 📖 **Editor**: software for writing text. The kind used for code is a "code editor" — it colors your code so it's easier to read.

This book recommends **VS Code**. It's free, powerful, and used by programmers everywhere.

> 📖 **Extension**: a small add-on for the editor — like a basket on your bicycle. The i18n-rust extension colors Rust code, checks errors live, and can translate diagnostics into 10 languages.

---

## 2.4 Installing the tools

This section is a step-by-step guide. Open your terminal and follow along.

### Step one: install Rust (rustup)

The extension needs the official Rust compiler (rustc) to work, so install Rust first.

#### Windows:
Open https://rust-lang.org/learn/get-started/ in a browser,
download the installer for your system and follow the prompts.

#### Linux / macOS:

Type this line in the terminal and press Enter:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

> 📖 What this line means: download an installer from the internet and run it. `curl` is the name of a download tool; the web address is where the installer lives. You don't need to understand every part — just type it in.

The installer will ask questions — **just press Enter** to pick the defaults. Depending on your network it usually takes a few minutes.

> ⚠️ **Careful**: after installing, you must **close the terminal and open a new one**. Freshly installed software is only found by newly opened windows.

Reopen the terminal and verify:

```bash
rustc --version
```

> 📖 `--version` means "tell me your version number". A version number is a piece of software's birthday tag, like 1.85.0.

If you see something like `rustc 1.85.0`, installation succeeded. An exact match doesn't matter.

> **Problems?** If it says "command not found", the window probably wasn't reopened. Close and reopen. Still nothing? Restart the computer and try again.

### Step two: create your first project (cargo)

Rust projects are created with cargo — the butler that manages code, dependencies and builds for you:

```bash
cargo new hello-world
cd hello-world
```

> 📖 Taking it apart: `cargo new` builds a ready-to-run project folder; `cd` walks into it. Inside you'll find `Cargo.toml` (the project's ID card) and `src/main.rs` — the file where your code lives.

Look inside `src/main.rs` — cargo has already written a Hello-World program for you:

```rust
fn main() {
    println!("Hello, world!");
}
```

### Step three: run it

```bash
cargo run
```

> 📖 `cargo run` compiles (if needed) and runs the project in one step. The first run takes a few seconds; later runs are instant.

If everything went well, you'll see:

```
Hello, world!
```

**Congratulations — you just ran your first Rust program.** Programmers in every language start with "Hello, World". You've now joined that tradition.

### Step four (optional): the rzc toolchain for multilingual classrooms

If you're reading this book to *teach* Rust in another language — or you want friendlier diagnostics — install the companion tool:

```bash
cargo install rzc
```

rzc understands standard Rust (your code passes through unchanged) and adds: translated teaching diagnostics in 10 languages, a language server for the VS Code extension, offline packaging for classrooms, and a native-keyword dialect for speakers of those languages. None of it is required for this book's English chapters — it's there when you need it.

> ✨ **Tip**: after every change, run `cargo run` again to see the new result. Pressing the **up arrow** in the terminal recalls your last command — no retyping.

---

## 2.5 Your first program, word by word

The program cargo wrote for you is the whole lesson. Take it apart piece by piece:

**Line 1: `fn main() {`**

This line means: "I'm defining a set of instructions called 'main'; it takes no input, and the instructions start after the `{`."

| Code | What it is | What it does | Everyday analogy |
|---|---|---|---|
| `fn` | A keyword | Tells the computer "a set of instructions is being defined below" | Like saying "attention: here comes a routine" |
| space | A separator | Keeps the two words apart — computers tell words apart by spaces | Like writing "I·love·learning" to be clear |
| `main` | The function's name | The name of this instruction set. A program always starts running from "main" | Like a book's "Chapter 1" — reading starts here |
| `()` | A pair of parentheses | Holds the "parameters" (inputs the function needs). Empty here: no input | Like saying "this routine needs no props" |
| `{` | Left curly brace | Means "the instructions start here" | Like saying "and… begin!" |

> 📖 **Parameter**: the input a function needs. A juicer needs fruit before it works — the fruit is its parameter. Our main function takes no input, so the parentheses are empty. Chapter 6 covers this properly.

> 📖 **Curly braces**: the two symbols `{` and `}`. They always come in pairs, wrapping a section of content. `{` opens, `}` closes.

**Line 2: `    println!("Hello, world!");`**

This line means: "Show the text 'Hello, world!' on the screen."

| Code | What it is | What it does | Everyday analogy |
|---|---|---|---|
| the 4 leading spaces | Indentation | Marks this line as belonging inside the main function | Like indenting a new paragraph in an essay |
| `println!` | The name of a built-in tool | Shows text on screen, then moves to the next line | "Calling out" to the screen |
| `!` | Exclamation mark | Marks this tool as a **macro** | The macro's signature badge |
| `(...)` | Parentheses | Hold the content to display | The words you call out |
| `"Hello, world!"` | A string | A run of text wrapped in double quotes | Words written on the note |
| `;` | Semicolon | Ends this instruction | Like the full stop in a sentence |

> 📖 **Macro**: a special kind of built-in tool whose name ends with `!`. For now, just remember: **see an exclamation mark, it's a macro**. How macros differ from ordinary functions is Chapter 23's business — don't dig deeper yet.

> 📖 **String**: a run of text — "string" is meant literally. The double quotes `"` mark "this is text", like quotation marks around someone's spoken words.

> ⚠️ **Careful**: the `ln` in `println` means "line" — it prints a **line** and automatically moves to the next one.

**Line 3: `}`**

| Code | What it is | What it does | Everyday analogy |
|---|---|---|---|
| `}` | Right curly brace | The main function ends here | Like saying "and… scene!" |

> ⚠️ **Careful**: `{` and `}` must **come in pairs**. Every opening needs a closing. Missing one is the most common beginner mistake (section 2.9 covers it).

### Save and run

1. Press `Ctrl+S` to save.
2. In the terminal (inside the project folder), run:

```bash
cargo run
```

> ✨ **Tip**: after every change, run `cargo run` again to see the new result. Pressing the **up arrow** in the terminal recalls your last command — no retyping.

---

## 2.6 Behind the curtain: how does code run?

When you press `cargo run`, behind the scenes:

1. **Check**: the compiler reads your code and verifies every rule (types, names, brackets).
2. **Compile**: rustc translates the checked code into machine code.
3. **Run**: the computer executes the machine code and the screen shows the result.

It's all automatic. You just edit `src/main.rs`.

> ✨ **Tip**: whenever a program misbehaves, the problem is almost always in step 1 (a rule broken) — and the compiler's message is the map to the fix. Reading error messages calmly is *the* core programming skill, and this book drills it from day one.

---

## 2.7 Hands-on experiments

The best way to learn programming is to **change the code yourself**. Do all three experiments below.

### Experiment 1: change the output

**Goal**: see with your own eyes that "change the code → the output follows".

Change `src/main.rs` to:

```rust
fn main() {
    println!("Hello, I'm Xiaoming!");
    println!("I'm learning Rust.");
}
```

Line by line:

- Line 1: define the main function; the program starts here.
- Line 2: display "Hello, I'm Xiaoming!" and move to the next line.
- Line 3: display "I'm learning Rust." on the next line, then move on.
- Line 4: the main function ends.

Save and run; the screen shows:

```
Hello, I'm Xiaoming!
I'm learning Rust.
```

> **Observe**: the two lines appear on two lines, because `println!` always moves to a new line. Instructions run top to bottom — first written, first executed.

### Experiment 2: printing without a new line

**Goal**: understand the difference between "with newline" and "without".

```rust
fn main() {
    print!("Hello, ");
    print!("world!");
}
```

Line by line:

- Line 2: `print!` differs from `println!` in exactly one way — **no newline after printing**.
- Line 3: continues right where the last print stopped.

The screen shows:

```
Hello, world!
```

The two pieces joined into one line because nothing moved to the next line in between.

### Experiment 3: insert data into text

**Goal**: teach the program to "remember" data and use it later.

```rust
fn main() {
    let name = "Xiaoming";
    let age = 12;
    println!("My name is {}, and I am {} years old.", name, age);
}
```

Line by line:

- Line 2: `let` is a keyword meaning "create a box". This line says: take a box, label it "name", put "Xiaoming" inside.
- Line 3: likewise, a box labeled "age" with 12 inside.
- Line 4: `{}` is a **placeholder** (a space reserved in advance). `println!` fills the spaces with the data listed after the parentheses, in order.

> 📖 **Variable**: a box with a label. The label is the name; the box holds the data. `let name = "Xiaoming"` creates a variable called "name". Chapter 3 covers this in full.

> 📖 **Placeholder**: a space reserved in advance, waiting to be filled. `{}` is a placeholder — like a blank in a form waiting for an entry.

How the filling works:

```
"My name is {}, and I am {} years old."   ← the template, with two blanks
              ↑                ↑
            name             age        ← filled in order
         "Xiaoming"          12
              ↓                ↓
"My name is Xiaoming, and I am 12 years old."   ← the final result
```

After running, the screen shows:

```
My name is Xiaoming, and I am 12 years old.
```

---

## 2.8 Code style

Code should not only run — it should be pleasant to read. Like handwriting an essay neatly with clear paragraphs.

### Indentation: 4 spaces

Code inside curly braces is indented 4 spaces to the right, marking it as "belonging to" the function.

```rust
fn main() {
    println!("proper indentation");  // ✅ indented 4 spaces
}
```

Why indent? Indentation makes the code's structure obvious at a glance — like indenting the first line of every paragraph in a letter so its shape is clear.

> ✨ **Tip**: don't count spaces. With rust-analyzer active in VS Code, code is auto-formatted on save (or right-click → Format Document).

### Naming

| Kind | Style | Examples |
|---|---|---|
| Function names | Verbs or verb phrases | `calculate_total`, `get_user_input` |
| Variable names | Nouns | `name`, `age`, `count` |

> ⚠️ **Careful**: don't collide with keywords. You can't name a variable `fn` or `let` — those are reserved. Pick specific nouns — `total`, `greeting` — and you're safe.

### Comments: notes for humans

> 📖 **Comment**: text inside code written for humans; the compiler ignores it completely. A single-line comment starts with `//` (two slashes).

```rust
// this is a single-line comment; the compiler ignores it

fn main() {
    println!("Hello"); // comments can also sit at the end of a line
    // let name = "Xiaoming";  // commented-out code never runs
}
```

Why write comments? Code tells the computer "what to do"; comments tell humans "why". When you reread your own code in a few days, comments bring back what you were thinking.

---

## 2.9 Common mistakes and how to fix them

Write code and you will make mistakes — that's completely normal. Picture the compiler as a **strict but kind teacher**: marking your work, it tells you **where the mistake is, why it's wrong, and how to fix it**.

First, learn the check-only command:

```bash
cargo check
```

> 📖 `check` means "verify without building an executable" — like proofreading your homework before handing it in. Faster than `cargo run`.

### Mistake 1: a missing bracket

```rust
fn main() {
    println!("Hello"   // ❌ missing the closing ) and the ;
}
```

Running `cargo check` reports:

```
error: mismatched closing delimiter: `}`
 --> src/main.rs:2:22
```

**How to read it**:

- `mismatched closing delimiter`: "the closing symbol doesn't match" — a bracket wasn't closed.
- After `-->` is the location: `src/main.rs`, line 2, column 22.
- This kind of **syntax error** (a broken writing rule) has no error code; it shows as a plain message.

**The fix**: count the brackets, left and right. Rule of thumb: **every opened bracket must be closed**.

### Mistake 2: a misspelled tool name

```rust
fn main() {
    printnl!("world!");  // ❌ "printnl" doesn't exist — it's println
}
```

The report:

```
error: cannot find macro `printnl` in this scope
 --> src/main.rs:2:5
```

**How to read it**: `cannot find` = "can't find"; `macro` = "macro". Together: **no macro called `printnl` exists in this scope**. Almost certainly a typo.

**The fix**: check the tool names in this chapter and correct it to `println!`.

### Mistake 3: a type mismatch (the most common)

```rust
fn main() {
    let count: i32 = 10;      // count holds a number
    count = "Hello";          // ❌ trying to stuff text into a number box
}
```

The report:

```
error[E0308]: mismatched types: expected `i32`, found `&str`
```

**How to read it**:

- `error[E0308]`: the bracketed part is the **error code**. Each code covers one family of common mistakes.
- `mismatched types`: the essence of the problem — different kinds of things got mixed.
- The note lines under it explain and suggest.

> 📖 **Type**: the "kind" of data. Numbers are one kind, text another. A box labeled for one kind only holds that kind. Chapter 3 explains in full.

**Remember this**: numbers are numbers, text is text — don't mix them. Like a box labeled "apples" that must not be stuffed with bananas.

> 📖 If you use the i18n-rust extension, the same messages can be translated into any of 10 languages with teaching hints attached — that's rzc's gift to the multilingual classroom.

---

## 2.10 Chapter glossary

| Term | One-line meaning |
|---|---|
| Save | Write what you're editing to disk (`Ctrl+S`) so it isn't lost |
| Error message | Text the compiler shows when it finds a problem |
| Compile | The translator turning source code into machine instructions |
| Compiler | The program that translates code — rustc for Rust |
| Editor | The software you write code in — this book uses VS Code |
| Variable | A labeled box, created with the `let` keyword |
| Parameter | The input a function needs, written in parentheses |
| Operating system | The base software managing the computer — Windows, macOS, Linux |
| print | Show text on screen — the `print!` macro |
| println | Show text and automatically move to the next line — the `println!` macro |
| Code | Text written for a computer to read |
| Code style | Writing conventions that keep code easy to read |
| Binary | Representing everything with only 0 and 1 |
| Semicolon | `;` — the marker that an instruction has ended |
| Cursor | The blinking vertical line meaning "your turn to type" |
| Curly braces | `{` and `}` — a paired wrapper around a block of content |
| Macro | A special built-in tool with a `!` after its name |
| Newline | After showing text, jump to the start of the next line |
| Machine code | The 0-and-1 instructions a computer executes directly |
| Check command | `cargo check` — verify without running |
| Keyboard | The device you type on |
| Keyword | A word reserved by the language with special meaning |
| Extension (editor) | A small add-on for the editor |
| Extension (file) | The part after the last dot in a file name, marking the file type |
| Type | The kind of data — numbers, text, etc. |
| Command | One line of instruction typed into the terminal |
| Terminal | A text window for talking to the computer, a.k.a. the command line |
| Directory tree | A diagram drawn with lines showing folder layers |
| Run | Make the computer execute the program and see the result |
| Placeholder | A space reserved in advance, waiting to be filled — `{}` |
| Project | The folder holding every file for one job |
| Configuration file | The project's ID-card file — `Cargo.toml` |
| String | A run of text wrapped in double quotes |
| Comment | Notes for humans, starting with `//`; ignored by the compiler |
| Indentation | Leading spaces marking structure — 4 spaces by convention |
| cargo | Rust's built-in project butler: new, run, check, dependencies |
| crates.io | Rust's official package repository |
| rustc | The official Rust compiler |
| VS Code | The free code editor this book recommends |
| rzc | The companion tool adding translated teaching diagnostics in 10 languages |

---

## 2.11 Exercises

> 💪 Try first, then peek.

### Exercise one: introduce yourself

Write a program that outputs your self-introduction, at least three lines:

```
Hello everyone, I'm Xiaohua.
I am 12 years old.
I am learning Rust!
```

<details>
<summary>Reference answer (try it yourself before opening)</summary>

```rust
fn main() {
    println!("Hello everyone, I'm Xiaohua.");
    println!("I am 12 years old.");
    println!("I am learning Rust!");
}
```

Line by line: line 1 defines the main function; lines 2–4 each print one sentence with a newline; line 5 closes the function.

</details>

### Exercise two: formatted output

Create two variables `city` and `temperature`, and use placeholders to output one sentence:

```
The temperature in Beijing is 25 degrees.
```

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    let city = "Beijing";
    let temperature = 25;
    println!("The temperature in {} is {} degrees.", city, temperature);
}
```

Line by line: line 2 creates the variable `city` holding "Beijing"; line 3 creates `temperature` holding 25; line 4's two `{}` are filled by `city` and `temperature` in order.

</details>

### Exercise three: join without newlines

Use two `print!` (no "ln") plus one `println!` to output three characters on one line:

```
一二三
```

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    print!("一");
    print!("二");
    println!("三");
}
```

Line by line: the first two `print!` calls don't add newlines, so the three characters join into one line; the final `println!` closes with a newline. (Yes — the characters themselves can be Chinese; Rust strings hold any language.)

</details>

### Exercise four: draw a triangle

Use `println!` to output:

```
*
**
***
****
*****
```

<details>
<summary>Reference answer</summary>

```rust
fn main() {
    println!("*");
    println!("**");
    println!("***");
    println!("****");
    println!("*****");
}
```

Line by line: five print instructions run top to bottom, each line one star longer than the last. (After Chapter 5 introduces loops, the same output takes three lines of code.)

</details>

---

## 2.12 FAQ

**Q: My screen says "cargo is not recognized as a command". What now?**

The terminal can't find cargo. Close and reopen the terminal; if that fails, restart the computer; if it still fails, the installation didn't succeed — return to section 2.4 and reinstall Rust.

**Q: I write English — how can the computer understand?**

The computer understands nothing directly. rustc first verifies your code follows Rust's rules, then translates it into machine code. The friendly text you type is an interface for you; what runs is the translated machine code.

**Q: Will text inside quotes be translated?**

No. Text inside quotes stays exactly as written. The compiler only processes the code itself — keywords and identifiers.

**Q: What's the difference between `print` and `println`?**

`println!` moves to the next line after printing; `print!` doesn't. Use `print!` when you want two pieces of text on one line.

**Q: Is the exclamation mark `!` required?**

For `println!`/`print!`, yes — they're macros, and macros carry the `!`. You'll meet non-macro calls (like `s.len()`) without one later.

**Q: Can I skip VS Code and write code in Notepad?**

You can, but it's not recommended. Notepad has no highlighting, no auto-formatting, no one-key run — it's exhausting. A craftsman needs good tools.

**Q: Can a code mistake break my computer?**

Absolutely not. The worst a code error does is make the program fail to run — fix it and move on. Experiment freely.

**Q: I can't remember all the words in this chapter!**

You don't have to memorize anything! The glossary exists for looking things up. Use the words and they stick — like riding a bicycle, nobody memorizes "push off with the left foot first".

---

## Chapter summary

1. **Programming** = writing detailed instruction notes for a computer.
2. **Rust** = a language whose keywords are already English — `fn`, `let`, `println!`.
3. **Three tools**: the terminal (issuing commands), the compiler (translating), and the VS Code editor (writing).
4. **Your first program**: `fn main() { println!("Hello, world!"); }`.
5. **Run command**: `cargo run`; check command: `cargo check`.
6. **Reading an error in three steps**: the position (which line) → the kind (what's wrong) → the suggestion (how to fix it).

> 📖 If anything in this chapter confused you, check the chapter glossary above or the master glossary at the front of the book.

## What's next

In **Chapter 3, "Variables and Types"**, you'll teach the program to "remember" more data:

- The full rules for creating variables with **`let`** (the "labeled box").
- Making boxes changeable with **`mut`**.
- The kinds of data: **integers**, **floating-point numbers**, **booleans**, **characters**.
- Making the program do math: add, subtract, multiply, divide.

You'll write a program that can really *count*!
