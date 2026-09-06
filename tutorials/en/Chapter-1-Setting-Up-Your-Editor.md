# Chapter 1: Setting Up Your Editor

## 1.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain what a **code editor** is and why Notepad isn't enough;
2. Install **VS Code** and the **rust-analyzer** extension;
3. Create a practice folder and your first `.rs` file;
4. Know the **rzc** companion tool and when it matters (multilingual classrooms);
5. Know where to find every command later — the command palette.

> ✨ This chapter needs no programming knowledge — just clicking. Every section has a "try it" box; follow along once and it's yours.

---

## 1.1 Why an editor at all

### A blank notebook + stickers

💡 **Metaphor**: writing code in Notepad is like writing an essay with your finger in the sand — possible, but painful. A **code editor** is a notebook designed for code: it colors your words, spots mistakes as you type, and runs your programs with one click.

The editor this book uses is **VS Code** — free, powerful, and the most popular editor in the world.

📖 **Extension**: a small add-on for the editor — like a basket on your bicycle. VS Code alone is a fine notebook; extensions teach it new tricks.

Two extensions matter for this book:

- **rust-analyzer** — the official Rust extension: syntax highlighting, error squiggles, auto-completion. Essential.
- **i18n-rust** — this project's companion extension: it translates Rust's error messages into 10 languages with teaching hints, and adds AI-assisted explanations. Optional for English readers (rustc's errors are already English) — invaluable if you're setting up a **multilingual classroom** (Chapter 2 explains how the same tool lets students write Rust in their own native language).

---

## 1.2 Installing the tools

### Step one: install VS Code

1. Visit https://code.visualstudio.com and download the installer for your system;
2. Install it (default options are fine) and open it.

You'll see a welcome page. Ignore it for now.

### Step two: install the rust-analyzer extension

1. Click the **Extensions** icon in the left sidebar (four squares);
2. Search `rust-analyzer`;
3. Click **Install** on the one published by "The Rust Programming Language" (or "rust-lang").

### Step three: install the Rust toolchain

The extension needs the Rust compiler to work. If you haven't yet, install Rust now (the full walkthrough is Chapter 2, section 2.4):

- **Linux / macOS**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Windows**: download rustup-init from https://rustup.rs and run it.

After installing, **close and reopen** VS Code (freshly installed tools are only found by new processes), then verify:

1. In VS Code, open the built-in terminal: menu **Terminal → New Terminal**;
2. Type `rustc --version` and press Enter — a version number means success.

### Step four (optional): the rzc companion tool

If you're setting up a **multilingual classroom** — teaching Rust to speakers of Chinese, Japanese, Russian and seven more — install rzc:

```bash
cargo install rzc
```

rzc understands standard Rust (your code passes through unchanged) and adds: translated teaching diagnostics in 10 languages, offline packaging for classrooms without internet, and a native-keyword dialect so each student can write Rust in their own mother tongue. English readers don't need it for this book — it's there for the classroom.

> 💡 The i18n-rust extension (the one this project publishes) hooks into rzc and provides: error-message translation with teaching hints, AI-assisted explanations (your own API key), and code snippets for the non-English dialects. English-only readers can skip it — rust-analyzer covers you.

---

## 1.3 Your first look at syntax highlighting

📖 **Syntax highlighting**: the editor paints different parts of the code in different colors. Like a textbook with **bold key terms** and **red vocabulary** — your eye separates structure instantly.

### Try it

Create your practice file:

1. Press `Ctrl + N` for a new blank file;
2. Press `Ctrl + S`, name it `test.rs`, save it in your practice folder (**the extension must be `.rs`**);
3. Type this program (letter by letter — good typing practice):

```rust
fn main() {
    let name = "Xiaoming";
    let age = 20;
    println!("My name is {}, and I am {} years old.", name, age);
}
```

Save (`Ctrl + S`) and you'll notice:

- `fn`, `let` — **keywords** are one color;
- `"My name is..."` — **strings** are another;
- `20` — **numbers** are yet another.

The exact palette depends on your theme — but it will definitely **no longer be all white**.

> 💡 Why color? Real code gets long; color shows "keyword vs content" at a glance. Like a map with green parks and blue rivers — far easier to read than an all-gray one.

### The `.rs` extension matters

rust-analyzer only activates for recognized Rust files. If the file is `test.txt`, no colors and no error squiggles.

**How to check**: does the file name end in `.rs`? Does the bottom-right corner say `Rust`?

---

## 1.4 Running programs: three ways

There are three ways to run your program — start with whichever feels easiest, then pick a favorite.

### Way one: the ▶ button

With `test.rs` open, look **top-right of the editor tab**: a ▶ (play) button and a ✓ (check) button (provided by rust-analyzer).

- ▶ = run the program (same as `cargo run` in a cargo project; for a single file it runs `rustc` directly);
- ✓ = check only (same as `cargo check`).

The output appears in the **terminal panel** below.

> 📖 **Terminal panel**: the terminal embedded at the bottom of VS Code — a black command-line window, moved inside so you never switch windows.

### Way two: the right-click menu

**Right-click** in the code area: you'll find "Run", "Check", and formatting commands in the context menu.

### Way three: the terminal (the honest way)

For a single file:

```bash
rustc test.rs && ./test
```

For a cargo project:

```bash
cargo run
```

> 📖 **Shortcut**: a key combination that completes an action — no mouse. Like knowing your times tables: faster than long addition. rust-analyzer also offers "Run" as a clickable hint above `fn main`.

### Try it: break it on purpose, watch the red squiggle

Add this line to the code:

```rust
// 预期错误: E0308
let age: i32 = "twenty";
```

After saving, a **red squiggle** appears under the line; hover the mouse and the reason pops up — a type mismatch, visible without running anything.

> 📖 **Diagnostic**: the editor's real-time error check, drawn as squiggles. Like red flags on a doctor's report.

> ⚠️ **Note**: squiggles need the rust-analyzer language server running. If no squiggle appears a few seconds after opening a file, see section 1.9's troubleshooting.

### Errors you can click: the Problems panel

Press `Ctrl + Shift + M` to open the **Problems panel** — every error and warning in the current file. Click one and the cursor jumps to the broken line.

---

## 1.5 One-key formatting: messy code tidies itself

📖 **Formatting**: automatic adjustment of your code's **indentation and spacing** — uniform and tidy. Like a teacher aligning your paragraphs and straightening your punctuation — **not one word changes, only the layout**.

### Try it

Mess up the code in `test.rs` on purpose (delete indentation, add random spaces):

```rust
fn main() {
let name = "Xiaoming";
        println!("Hello, {}!", name);
}
```

Then:

- **Right-click → Format Document**; or
- `Shift + Alt + F` (Mac: `Shift + Option + F`).

The code instantly returns to tidy indentation:

```rust
fn main() {
    let name = "Xiaoming";
    println!("Hello, {}!", name);
}
```

> 💡 **Metaphor**: formatting is like tidying a desk. Same books (the code didn't change), but everything is back where it belongs — easier to find, nicer to look at.

> ✨ **Good habit**: press the format shortcut after finishing each stretch of code. Tidy code is easier to debug, and easier for others — including future you — to read.

---

## 1.6 Auto-completion: the editor finishes your words

📖 **Auto-completion**: as you type, the editor suggests what you might mean. Type `pr` inside `main` and a list pops up with `println!` — press `Tab` or `Enter` and it completes, placeholders and all.

### Try it

Inside `main`, type `pr` and watch the list. Pick `println!` with the arrow keys, press Enter, and type "Hello" inside the quotes.

> 💡 **Metaphor**: auto-completion is like a helpful assistant finishing your sentences — you say the first syllable, it knows the rest. rust-analyzer knows every standard-library function, so it can suggest things you haven't learned yet: read the little documentation box beside the list.

---

## 1.7 The command palette: every command, one box

`Ctrl + Shift + P` opens the **command palette** — type any command's name to run it. Three you'll use now:

| Command | What it does |
|---|---|
| Format Document | Tidy the code (section 1.5) |
| Problems: Focus on Problems View | The errors panel (section 1.4) |
| Rust Analyzer: Restart Server | When squiggles vanish or completion dies |

And three for later:

| Command | What it does |
|---|---|
| i18n-rust commands | Diagnostics translation, AI assistance (if installed) |
| Terminal: Create New Terminal | The built-in terminal |
| Reload Window | The universal "turn it off and on again" |

> ✨ No need to memorize: press `Ctrl + Shift + P` and type the first letters of what you want.

---

## 1.8 A complete workflow example

Stringing the chapter together — this is your **standard workflow** from now on:

1. **Open the project**: VS Code → File → Open Folder → pick your project;
2. **Open the code**: click `src/main.rs` in the file tree;
3. **Write**: let auto-completion finish words; watch for red squiggles as you type;
4. **Save**: `Ctrl + S`;
5. **Tidy**: `Shift + Alt + F` to format;
6. **Run**: click the top-right ▶ (or `cargo run` in the terminal);
7. **See the result**: the terminal panel shows the output;
8. **On error**: `Ctrl + Shift + M` opens the Problems panel; click an error to jump there, fix it, back to step 4.

> 💡 Steps 4 → 8 repeat many, many times. That's what programming is: **write a little → check → run → fix → write a little more** — a snowball rolling downhill.

---

## 1.9 Common mistakes and how to fix them

### Mistake one: no colors or squiggles on a `.rs` file

**Cause**: rust-analyzer isn't active, or the file isn't recognized as Rust.

**Fix, in order**:

1. Check the bottom-right corner says `Rust`. If not, click it and select `Rust` manually;
2. Make sure the file extension really is `.rs` (Windows may hide extensions — the file might secretly be `test.rs.txt`; enable "show file extensions" in File Explorer);
3. Press `Ctrl + Shift + P`, type `Reload Window`, Enter.

### Mistake two: no red squiggles, errors go undetected

**Cause**: the rust-analyzer language server didn't start — usually because **rustc isn't installed or can't be found**.

**Fix, in order**:

1. Open VS Code's built-in terminal (menu: Terminal → New Terminal) and type `rustc --version`:
   - "command not found" → rustc isn't installed or isn't on PATH; redo Chapter 2, section 2.4, and reopen VS Code;
2. Press `Ctrl + Shift + P` and run **Rust Analyzer: Restart Server**;
3. Still nothing? `Reload Window`.

> 📖 **PATH**: the operating system's "directory list" for finding commands. If a command isn't on that list, the terminal can't find it.

### Mistake three: the run button errors with "rustc not found"

Same root as mistake two. If rustc lives in an unusual place, point the system at it:

- Make sure rustup installed correctly (section 1.2, step three);
- In VS Code settings, search `rust` and check the toolchain-related paths if you use a non-standard setup.

### Mistake four: formatting does nothing

**Cause**: the file isn't recognized as Rust.

**Fix**: run mistake-one's steps first, then format.

### Mistake five: a shortcut does nothing

**Likely cause**: another app or your input method stole the shortcut (`Ctrl + Shift + C` means "copy" in some layouts, or "convert simplified/traditional" in Chinese input methods).

**Fix**: use the menus instead — identical behavior. Or rebind the shortcut in VS Code Settings → Keyboard Shortcuts.

---

## 1.10 Chapter glossary

| Term | One-line meaning |
|---|---|
| Code editor | Software designed for writing code — this book uses VS Code |
| Extension | A plug-in adding features to the editor |
| rust-analyzer | The official Rust extension: highlighting, squiggles, completion |
| i18n-rust | This project's companion extension: translated diagnostics and AI hints for 10 languages |
| Language server | The background program providing squiggles and completion (rust-analyzer) |
| Command palette | The command search box opened with `Ctrl + Shift + P` |
| Terminal panel | The terminal embedded at the bottom of VS Code |
| Problems panel | The panel listing all errors and warnings (`Ctrl + Shift + M`) |
| Formatting | Auto-tidying indentation and spacing, content unchanged |
| Syntax highlighting | Coloring different parts of the code differently |
| Diagnostic | Real-time error checking, drawn as squiggles |
| PATH | The OS's directory list for finding commands |
| Reload Window | Restarting the VS Code interface without closing it |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 1.11 Exercises

> 💪 Try first, then peek.

### Exercise one: the full setup

Install VS Code, rust-analyzer and the Rust toolchain (if not already done), create `setup.rs` with a program that prints `"My editor is ready!"`, and run it **two ways**: the ▶ button, and `rustc setup.rs && ./setup` in the terminal.

<details>
<summary>🔍 View answer</summary>

```rust
fn main() {
    println!("My editor is ready!");
}
```

Output (both ways):

```
My editor is ready!
```

</details>

### Exercise two: formatting practice

Paste this badly-formatted code into the editor and tidy it with the formatter:

```rust
fn main() {
let a = 3;
      let b = 7;
println!("the sum is {}", a + b);
}
```

<details>
<summary>🔍 View answer</summary>

Right-click → Format Document (or `Shift + Alt + F`):

```rust
fn main() {
    let a = 3;
    let b = 7;
    println!("the sum is {}", a + b);
}
```

</details>

### Exercise three: completion practice

Using only the keyboard: type `pr`, complete to `println!`, and build a program that prints `completion works!`. Then add an intentional error (`let age: i32 = "twenty";`), find it in the Problems panel, and fix it.

<details>
<summary>🔍 View answer</summary>

```rust
fn main() {
    println!("completion works!");
}
```

The intentional error shows as E0308 (type mismatch) in the Problems panel; changing `"twenty"` to `20` fixes it.

</details>

---

## 1.12 FAQ

**Q: Does the extension change my files? Will my code be modified?**

No. rust-analyzer only **colors, checks and completes**. Your `.rs` file stays exactly as you wrote it. Formatting rearranges whitespace only.

**Q: Can I use both the terminal and the editor buttons on the same project? Will results differ?**

No difference. The buttons run the same rustc/cargo commands underneath — they just save you typing. Use either freely.

**Q: My classmate uses a different editor (like JetBrains IDEs). Does this book still apply?**

Yes. The concepts (projects, compilers, error messages) are identical; the buttons live in different places. rust-analyzer also has a JetBrains counterpart (RustRover).

**Q: What is rzc for, if I'm writing standard Rust?**

For the multilingual classroom: rzc and the i18n-rust extension let students write Rust in **their** native language (10 available), translate error messages with teaching hints, and package everything offline. If your classroom is English-only, standard Rust tooling is all you need — and that's exactly what this book teaches.

**Q: Why does nothing happen when I click features like AI chat?**

AI features need your own AI service key and address configured — an optional track. Beginners don't need them at all; skipping changes nothing about writing and running code.

**Q: After upgrading the extension, my code suddenly shows a wall of errors?**

First run "Rust Analyzer: Restart Server", then `Reload Window`. Still broken? Check that your toolchain is intact (`rustc --version`).

---

## What's next

Tools ready! In **Chapter 2, "Hello, World"**, you'll create your first project with cargo, write your first program, and learn to read error messages calmly — the book's core skill from day one. Then **Chapter 3** teaches the program to "remember" data with variables.

See you in the next chapter!
