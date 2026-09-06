# Chapter 2: Hello, World

> 💡 If you haven't set up the VS Code extension yet, read **Chapter 1 (Making the Most of the VS Code Extension)** first — this chapter assumes you can open a `.<ext>` file.

Welcome to native-language Rust! In this chapter you will accomplish something remarkable — **writing a program that actually runs, in your own language**.

If you have never written code before, don't worry. This chapter starts from "what is a computer" and "what is a keyboard".

---

## 2.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain in plain words what "programming" is.
2. Install all the tools you need on your computer.
3. Write and run your first Rust program in your native language.
4. Read the three most common kinds of error messages.

---

## 2.1 What is programming

### Writing notes for a robot

Imagine a robot standing in front of you. It's very obedient — and very dumb. It does **exactly what you say, word by word**, and never guesses what you meant.

Say "bring me a glass of water" and it may just stand there, dazed, if you didn't say "first pick up a glass".

**Programming is writing notes for that robot** — spelling out every step: pick up the glass, walk to the dispenser, press the button, turn the tap off when the glass is full…

> 💡 **Metaphor**: programming is like writing a super-detailed recipe. You can't write "make noodles" — you have to write "Step 1: turn on the tap. Step 2: fill the pot halfway. Step 3: …"

Each note you write is called a piece of **source code**.

> 📖 **Source code**: short for "source code" — text you write for a computer to read.

The language you write source code in is called a **programming language**.

> 📖 **Programming language**: a language humans use to talk to computers. Just as humans have Chinese, English and Japanese, there are many programming languages, each with its own character.

### Why native-language Rust exists

Most programming languages use English keywords.

> 📖 **Keyword**: a word **reserved in advance** by a programming language, with a special meaning. Like classrooms at school — they're reserved; you can't take one home. Keywords are reserved too, and each has a fixed job.

For example, standard Rust uses `let` (to create a variable) and `if` (to make a decision). For a native English speaker these words feel natural. But if your native language isn't English, you're learning programming logic *and* English vocabulary at the same time — like **studying two foreign languages at once**. Double the difficulty.

**Native-language Rust exists to solve exactly this.** It lets you write code with keywords from your own language:

| Native keyword | English | What it means | Everyday analogy |
|---|---|---|---|
| `函数` (function) | `fn` | Define a set of instructions you can reuse | Name a sequence of actions, then trigger it by name |
| `让` (let) | `let` | Create a "box" to store data | Grab a box, stick a label on it, put something inside |
| `如果` (if) | `if` | Do something only under a condition | If it's raining, take an umbrella |
| `对于` (for) | `for` | Repeat an action for each item | For every apple in the basket, wash it |
| `打印行` (println) | `println!` | Show text on the screen | "Call out" to the screen |

Your code looks like sentences in your own language — but it is **a program that really runs**:

```rust
函数 主函数() {
    打印行!("你好，世界！");
}
```

> ✨ **Tip**: don't worry about the details of that code yet — section 1.6 explains it word by word. For now, all you need to know is: these lines in your native language are a program the computer can read.

> ✨ **Tip**: in native-language Rust, the exclamation mark after a macro (a tool with a `!` — explained shortly) is optional; the tool fills it in for you. `打印行("你好")` and `打印行!("你好")` both work. This book always writes the `!` so you can recognize macros at a glance.

---

## 2.2 First, meet your computer

Before writing code, let's spend a few minutes on some basics. If you already know all this, jump straight to 2.3.

### Files: the "sheets of paper" inside a computer

> 📖 **File**: the basic unit of storage on a computer — like sheets of paper. A sheet can hold an essay or a drawing; a file can hold text or images.

Every file has a **file name**, like `essay.txt`. The `txt` after the dot is the **extension** — it says what type of file this is.

> 📖 **Extension**: the part after the last dot in a file name; it tells the computer what kind of file it is. `.txt` is plain text, `.jpg` is a picture. The code files in this book use the extension for your language (for the Chinese pack it's `.zh`).

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

> ⚠️ **Careful**: code must be typed with **English punctuation**! In a Chinese input method the comma is `，`; in English it's `,`. Code itself always uses English punctuation. (Chinese punctuation is fine *inside quoted text*, because that part is shown to humans.)

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

> 📖 **Command**: one line of instruction you type into the terminal. Press Enter and the computer does it. Typing `rzc --help` and pressing Enter asks rzc to show its help text.

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

Native-language Rust has **two** translators working in a relay:

```
Native source code (a .<ext> file)
    ↓  first translator, rzc: your language → English
Standard Rust source code (a .rs file)
    ↓  second translator, rustc: English → machine code
Executable program
    ↓  run
Output
```

The first translator is **rzc** (say "r-z-c", three letters). It's this project's native-language compiler: it swaps native keywords for English ones.

The second translator is **rustc**, the official Rust compiler: it turns the English code into machine code.

You never have to direct them separately. One call to `rzc run` and both steps happen automatically.

> ✨ **Tip**: the "r" in rzc stands for Rust; "zc" comes from the pinyin initials of the Chinese word for "native" — the project began as a Chinese-teaching tool and now speaks 10 languages.

### Tool three: the text editor — your writing workbook

> 📖 **Editor**: software for writing text. The kind used for code is a "code editor" — it colors your code so it's easier to read.

This book recommends **VS Code**. It's free, powerful, and used by programmers everywhere.

> 📖 **Extension**: a small add-on for the editor — like a basket on your bicycle. VS Code has a dedicated native-Rust extension: once installed, your code gets beautiful syntax highlighting and automatic formatting.

---

## 2.4 Installing the tools

This section is a step-by-step guide. Open your terminal and follow along.

### Option A: the offline package (no network needed)

Download the package for your platform from **the Releases page or a file host** (on Windows pick `rzc-<version>-windows-x86_64.zip` — it contains rzc, the language server and **rust-analyzer**, whose official download is unreliable, which is why it ships in the box). Unpack and go:

1. Enter the unpacked folder and run `rzc --version` to verify;
2. Add that folder to your PATH (Windows: Settings → System → Advanced system settings → Environment variables) so `rzc` works from anywhere.

> 💡 The offline package already bundles rust-analyzer and all language packs — **no need to download rust-analyzer again**. But rustc/cargo must be installed separately (Option B below), or later run `rzc install toolchain` once you're online. The build-from-source instructions below are for when no offline package is available.

### Option B: hands-on (rustup + build from source, the fallback)

rzc needs the official Rust compiler (rustc) to work, so install Rust first.

#### Windows:
Open https://rust-lang.org/zh-CN/learn/get-started/ in a browser,
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

### Option B, step two: build rzc from source

rzc's source code lives on GitHub (a website for storing code). We download it and compile it with cargo — that's "building from source".

#### Sub-step 1: get the source

Open a terminal, go to a suitable folder (like your home directory), and type:

```bash
git clone https://github.com/liuqiTan80/i18n-rust
cd i18n-rust
```

> 📖 **git**: a "version control" tool for downloading and updating code. The first command means "copy this whole repository into the current folder"; the second means "enter that folder".
>
> If git isn't installed, go to the repository page, click the green `Code` button → `Download ZIP`, unpack it, then `cd` into the unpacked `i18n-rust` folder (no git needed for that).

#### Sub-step 2: build

```bash
cargo build --release --workspace
```

> 📖 This command means "use cargo to build the whole project as a release". `--release` means "the optimized version" (runs faster), `--workspace` means "the whole project" (it builds rzc and the language server together).

The first build downloads dependencies and compiles everything — about 1–3 minutes. When it finishes, the executable is in the `target/release/` folder.

#### Sub-step 3: verify

```bash
./target/release/rzc --help
```

> 📖 `--help` means "tell me how to use you". Almost every command supports it. The `./` prefix means "from the current folder" — rzc isn't in the system's search path yet, so you must give the path.

If a wall of help text appears — congratulations, rzc built successfully!

#### Sub-step 4 (optional): make rzc available everywhere

Typing `./target/release/rzc` every time is tedious. Install it into the system search path:

```bash
cargo install --path crates/cli
```

Now `rzc` works from any folder.

> ✨ **Tip**: the language packs are built into the program — it works immediately after compiling, no extra configuration. To upgrade later, return to the repository folder, run `git pull` to fetch the latest code, then `cargo build --release --workspace` again.

### Option B, step three (optional): install the VS Code extension

> Extension installation was covered in **Chapter 1, section 1.2** (the `.vsix` offline install); Option B users install it the same way, so we won't repeat it here.

---

## 2.5 Create your first project

### What is a project

> 📖 **Project**: a folder containing every file needed for one job. We're writing a program, so we start with a project folder.

Type in the terminal:

```bash
rzc init hello-world
```

> 📖 Taking the command apart: `rzc` calls the translator to work; `init` is short for "initialize" ("set everything up from scratch") — it creates a new project; `hello-world` is the name you chose.

The command creates a folder named `hello-world` with this structure:

```
hello-world/
├── Cargo.toml          ← the project's ID card (name and other info)
├── rust-toolchain.toml ← records the Rust version needed
├── README.md           ← project description
└── src/
    └── main.<ext>      ← your source file (where you write code)
```

> 📖 **Directory tree**: a diagram like this, drawn with lines to show folder layers, is called a directory tree. The vertical and horizontal lines mean "contains", like a family tree. The slash after `src/` marks it as a folder.

It means: the `hello-world` folder holds four things, `src` is a folder, and inside it lives `main.<ext>`.

The only file you need to care about is **`src/main.<ext>`** — that's where you write code. The other files are managed by the tools; ignore them for now.

### Open the project in VS Code

1. Open VS Code.
2. Top-left menu "File" → "Open Folder".
3. Find the `hello-world` folder you just created and confirm.
4. In the left-hand file list, open `src`, then click `main.<ext>` — the code appears in the big central area.

---

## 2.6 Your first program, word by word

### The complete code

Open `src/main.<ext>` and type these three lines (the file already contains some generated code — you can delete it all and retype):

```rust
函数 主函数() {
    打印行!("你好，世界！");
}
```

> ⚠️ **Careful** while typing:
> 1. There is a **space** between `函数` and `主函数`.
> 2. `()`, `{}`, `""`, `!` and `;` must all be typed in **English input mode**. (With the native-language extension installed, Chinese input is auto-converted to English punctuation.)
> 3. The text inside the quotes — "你好，世界！" — is shown to humans, so Chinese punctuation is fine there.

### Line by line, word by word

**Line 1: `函数 主函数() {`**

This line means: "I'm defining a set of instructions called 'main'; it takes no input, and the instructions start after the `{`."

Broken down piece by piece:

| Code | What it is | What it does | Everyday analogy |
|---|---|---|---|
| `函数` | A keyword | Tells the computer "a set of instructions is being defined below" | Like saying "attention: here comes a routine" |
| space | A separator | Keeps the two words apart — computers tell words apart by spaces | Like writing "I love learning" as "I · love · learning" to be clear |
| `主函数` | The function's name | The name of this instruction set. A program always starts running from "main" | Like a book's "Chapter 1" — reading starts here |
| `()` | A pair of parentheses | Holds the "parameters" (inputs the function needs). Empty here: no input | Like saying "this routine needs no props" |
| `{` | Left curly brace | Means "the instructions start here" | Like saying "and… begin!" |

> 📖 **Parameter**: the input a function needs. A juicer needs fruit before it works — the fruit is its parameter. Our main function takes no input, so the parentheses are empty. Chapter 6 covers this properly.

> 📖 **Curly braces**: the two symbols `{` and `}`. They always come in pairs, wrapping a section of content. `{` opens, `}` closes.

**Line 2: `    打印行!("你好，世界！");`**

This line means: "Show the text '你好，世界！' on the screen."

| Code | What it is | What it does | Everyday analogy |
|---|---|---|---|
| the 4 leading spaces | Indentation | Marks this line as belonging inside the main function | Like indenting a new paragraph in an essay |
| `打印行` | The name of a built-in tool | Shows text on screen, then moves to the next line | "Calling out" to the screen |
| `!` | Exclamation mark | Marks this tool as a **macro** | The macro's signature badge |
| `(...)` | Parentheses | Hold the content to display | The words you call out |
| `"你好，世界！"` | A string | A run of text wrapped in double quotes | Words written on the note |
| `;` | Semicolon | Ends this instruction | Like the full stop in a sentence |

> 📖 **Macro**: a special kind of built-in tool whose name ends with `!`. For now, just remember: **see an exclamation mark, it's a macro**. How macros differ from ordinary functions is Chapter 23's business — don't dig deeper yet.

> 📖 **String**: a run of text — "string" is meant literally. The double quotes `"` mark "this is text", like quotation marks around someone's spoken words.

> ⚠️ **Careful**: the 行 in `打印行` means "line" — it prints a **line** and automatically moves to the next one.

**Line 3: `}`**

| Code | What it is | What it does | Everyday analogy |
|---|---|---|---|
| `}` | Right curly brace | The main function ends here | Like saying "and… scene!" |

> ⚠️ **Careful**: `{` and `}` must **come in pairs**. Every opening needs a closing. Missing one is the most common beginner mistake (section 1.10 covers it).

### Save and run

1. Press `Ctrl+S` to save.
2. In the terminal, make sure you're inside the `hello-world` folder. Type:

```bash
cd hello-world
```

> 📖 `cd` is short for "change directory" — "walk into a folder". Like walking into your own bedroom: everything you do next happens there.

3. Then run:

```bash
rzc run src/main.<ext>
```

> 📖 Taking it apart: `rzc` calls the translator; `run` means "run"; `src/main.<ext>` says "translate and run this file".

If everything went well, you'll see:

```
你好，世界！
```

**Congratulations — you just ran your first Rust program in your native language.** Programmers in every language start with "Hello, World". You've now joined that tradition.

> ✨ **Tip**: from now on, after every change, run `rzc run src/main.<ext>` again to see the new result. Pressing the **up arrow** in the terminal recalls your last command — no retyping.

---

## 2.7 Behind the curtain: how does native-language code run?

You might wonder: the computer doesn't understand my language — how does my code run?

When you press `rzc run`, three things happen automatically:

```
┌──────────────────────────────────┐
│ ① Your native source file main.<ext>  │
│    函数 主函数() { ... }          │
└────────────┬─────────────────────┘
             │ rzc swaps native keywords for English
             ▼
┌──────────────────────────────────┐
│ ② Standard Rust source code      │
│    fn main() { ... }             │
└────────────┬─────────────────────┘
             │ rustc turns English code into machine code
             ▼
┌──────────────────────────────────┐
│ ③ Machine code (0s and 1s) → run → output │
│    The screen shows: 你好，世界！  │
└──────────────────────────────────┘
```

1. **Translate**: rzc consults a "native ↔ English mapping table", replacing `函数` with `fn` and `打印行` with `println!`.
2. **Compile**: rustc translates the English code into machine code.
3. **Run**: the computer executes the machine code and the screen shows the result.

> 📖 **Mapping table**: the "native ↔ English table" above has a formal name — the mapping table. It maps native words to English words. It lives in the project's language pack; Appendix A contains the full listing.

It's all automatic. You just write your `.<ext>` files.

### Curious what the translation looks like? Use the eject command

To see the translated English code with your own eyes:

```bash
rzc eject src/main.<ext>
```

> 📖 `eject` means "pop out, export". The command generates a `src/main.rs` file (`.rs` is the standard Rust extension) containing the translated English code (shown here in a plain code fence, since it's standard Rust rather than dialect):

```
fn main() {
    println!("你好，世界！");
}
```

> ✨ **Tip**: notice — the text inside the quotes, "你好，世界！", was **not** translated. rzc only translates keywords and identifiers (words with special meaning in code); text inside quotes stays exactly as written.

> ✨ **Tip**: `main.rs` is just for looking at. Keep editing `main.<ext>` as usual.

---

## 2.8 Hands-on experiments

The best way to learn programming is to **change the code yourself**. Do all three experiments below.

### Experiment 1: change the output

**Goal**: see with your own eyes that "change the code → the output follows".

Change `main.<ext>` to:

```rust
函数 主函数() {
    打印行!("你好，我是小明！");
    打印行!("我正在学中文 Rust。");
}
```

Line by line:

- Line 1: define the main function; the program starts here.
- Line 2: display "你好，我是小明！" and move to the next line.
- Line 3: display "我正在学中文 Rust。" on the next line, then move on.
- Line 4: the main function ends.

Save and run; the screen shows:

```
你好，我是小明！
我正在学中文 Rust。
```

> **Observe**: the two lines appear on two lines, because `打印行` always moves to a new line. Instructions run top to bottom — first written, first executed.

### Experiment 2: printing without a new line

**Goal**: understand the difference between "with newline" and "without".

```rust
函数 主函数() {
    打印!("你好，");
    打印!("世界！");
}
```

Line by line:

- Line 2: `打印!` differs from `打印行!` in exactly one way — **no newline after printing**.
- Line 3: continues right where the last print stopped.

The screen shows:

```
你好，世界！
```

The two pieces joined into one line because nothing moved to the next line in between.

### Experiment 3: insert data into text

**Goal**: teach the program to "remember" data and use it later.

```rust
函数 主函数() {
    让 名字 = "小明";
    让 年龄 = 20;
    打印行!("我叫{}，今年{}岁。", 名字, 年龄);
}
```

Line by line:

- Line 2: `让` is a keyword meaning "create a box". This line says: take a box, label it "名字" (name), put "小明" inside.
- Line 3: likewise, a box labeled "年龄" (age) with 20 inside.
- Line 4: `{}` is a **placeholder** (a space reserved in advance). `打印行!` fills the spaces with the data listed after the parentheses, in order.

> 📖 **Variable**: a box with a label. The label is the name; the box holds the data. `让 名字 = "小明"` creates a variable called "名字" (name). Chapter 3 covers this in full.

> 📖 **Placeholder**: a space reserved in advance, waiting to be filled. `{}` is a placeholder — like a blank in a form waiting for an entry.

How the filling works:

```
"我叫{}，今年{}岁。"     ← the template, with two blanks
      ↑        ↑
    名字      年龄        ← filled in order
"小明"      20
      ↓        ↓
"我叫小明，今年20岁。"   ← the final result
```

After running, the screen shows:

```
我叫小明，今年20岁。
```

---

## 2.9 Code style

Code should not only run — it should be pleasant to read. Like handwriting an essay neatly with clear paragraphs.

### Indentation: 4 spaces

Code inside curly braces is indented 4 spaces to the right, marking it as "belonging to" the function.

```rust
函数 主函数() {
    打印行!("正确缩进");  // ✅ indented 4 spaces
}
```

Why indent? Indentation makes the code's structure obvious at a glance — like indenting the first line of every paragraph in a letter so its shape is clear.

> ✨ **Tip**: don't count spaces. With the VS Code extension installed, right-click in the code → "Format", and the editor tidies it for you.

### Naming

| Kind | Style | Examples |
|---|---|---|
| Function names | Native verbs or verb phrases | `计算总和`, `获取用户输入` |
| Variable names | Native nouns | `名字`, `年龄`, `数量` |
| Type names | Native nouns | `学生信息`, `图书` |

> ⚠️ **Careful**: don't collide with keywords. For example, don't name a variable `结果` — it's a reserved word in the language pack (Chapter 11 explains). Pick specific nouns — `总和`, `问候语` — and you're safe.

### Comments: notes for humans

> 📖 **Comment**: text inside code written for humans; the compiler ignores it completely. A single-line comment starts with `//` (two slashes, typed in English mode).

```rust
// 这是单行注释，编译器会忽略

函数 主函数() {
    打印行!("你好"); // 行尾也可以写注释
    // 让 名字 = "小明";  // 被注释掉的代码不会执行
}
```

Why write comments? Code tells the computer "what to do"; comments tell humans "why". When you reread your own code in a few days, comments bring back what you were thinking.

---

## 2.10 Common mistakes and how to fix them

You will make mistakes — that's completely normal. Picture the compiler as a **strict but kind teacher**: marking your work, it tells you **where the mistake is, why it's wrong, and how to fix it**.

> 📖 **Error message**: the text the compiler shows when it finds a problem. It's not criticism — it's help finding the bug.

First, learn the check-only command:

```bash
rzc check src/main.<ext>
```

> 📖 `check` means "check". It translates and validates but doesn't actually run — like proofreading your homework before handing it in.

### Mistake 1: a missing bracket

```rust
函数 主函数() {
    打印行!("你好"   // ❌ missing the closing ) and the ;
}
```

Running `rzc check` reports:

```
错误: mismatched closing delimiter: `}`
  --> main.<ext>:3:1
```

**How to read it**:

- `mismatched closing delimiter`: "the closing symbol doesn't match" — a bracket wasn't closed.
- After `-->` is the location: `main.<ext>`, line 3, column 1.
- This kind of **syntax error** (a broken writing rule) has no error code; it shows in the original English.

**The fix**: count the brackets, left and right. Rule of thumb: **every opened bracket must be closed**.

### Mistake 2: a misspelled tool name

```rust
函数 主函数() {
    打应行!("世界");  // ❌ "打应行" doesn't exist — it should be 打印行
}
```

The report:

```
错误: cannot find macro `打应行` in this scope
  --> main.<ext>:2:5
```

**How to read it**: `cannot find` = "can't find"; `macro` = "macro". Together: **no macro called `打应行` exists in this scope**. Almost certainly a typo.

**The fix**: check the tool names in this chapter and correct it to `打印行!`.

### Mistake 3: a type mismatch (the most common)

```rust
// 预期错误: E0308
函数 主函数() {
    让 数量 = 10;        // 数量 holds a number
    数量 = "你好";    // ❌ trying to stuff text into a number box
}
```

The report:

```
错误[E0308]: 类型不匹配：期望 `整数`，实际得到 `字符串引用`
💡 请检查变量类型是否与上下文要求一致。
```

**How to read it**:

- `错误[E0308]`: the bracketed part is the **error code**. Each code covers one family of common mistakes. Errors with codes are translated into your language.
- `类型不匹配` (type mismatch): the essence of the problem — different kinds of things got mixed.
- The `💡` line is the fix suggestion.

> 📖 **Type**: the "kind" of data. Numbers are one kind, text another. A box labeled for one kind only holds that kind. Chapter 3 explains in full.

**Remember this**: numbers are numbers, text is text — don't mix them. Like a box labeled "apples" that must not be stuffed with bananas.

> 📖 For plain-language explanations of more error codes, see **Appendix C: A Dictionary of Common Error Messages**.

---

## 2.11 Chapter glossary

Every new word from this chapter, sorted alphabetically:

| Term | Meaning |
|---|---|
| Save | Write what you're editing to disk (`Ctrl+S`) so it isn't lost |
| Error message | Text the compiler shows when it finds a problem |
| Compile | The translator turning source code into machine instructions |
| Compiler | The program that translates code, such as rustc |
| Editor | The software you write code in — this book uses VS Code |
| Variable | A labeled box, created with the `让` keyword |
| Parameter | The input a function needs, written in parentheses |
| Operating system | The base software managing the computer — Windows, macOS, Linux |
| 打印 (print) | Show text on screen — the `打印!` macro |
| 打印行 (println) | Show text and automatically move to the next line — the `打印行!` macro |
| Code | Text written for a computer to read |
| Code style | Writing conventions that keep code easy to read |
| Binary | Representing everything with only 0 and 1 |
| Semicolon | `;` — the marker that an instruction has ended |
| Cursor | The blinking vertical line meaning "your turn to type" |
| Curly braces | `{` and `}` — a paired wrapper around a block of content |
| Macro | A special built-in tool with a `!` after its name |
| Newline | After showing text, jump to the start of the next line |
| Machine code | The 0-and-1 instructions a computer executes directly |
| Check command | `rzc check` — check without running |
| Keyboard | The device you type on |
| Keyword | A word reserved by the language with special meaning |
| Extension (editor) | A small add-on for the editor |
| Extension (file) | The part after the last dot in a file name, marking the file type |
| Type | The kind of data — numbers, text, etc. |
| Command | One line of instruction typed into the terminal |
| Terminal | A text window for talking to the computer, a.k.a. the command line |
| Directory tree | A diagram drawn with lines showing folder layers |
| Run | Make the computer execute the program and see the result |
| Placeholder filling | `{}` placeholders filled with data in order |
| Configuration file | The project's ID-card file, such as Cargo.toml |
| String | A run of text wrapped in double quotes |
| Comment | Notes for humans, starting with `//`; ignored by the compiler |
| Indentation | Leading spaces marking structure — 4 spaces by convention |
| Project | The folder holding every file for one job |
| Mapping table | The table mapping native words to English words |
| Source code | The code files you write (the `.<ext>` files) |
| Parentheses | `(` and `)` — holders of parameters and content |
| Placeholder | A space reserved in advance, waiting to be filled — `{}` |
| Terminal | Another name for the command-line window |
| rzc | The native-Rust compiler — translates your code into standard Rust |
| rustc | The official Rust compiler — turns Rust code into machine code |
| cargo | Rust's built-in package butler |
| crates.io | Rust's official package repository |
| VS Code | The free code editor this book recommends |
| eject | rzc's export command — produces the translated English code file |
| init | rzc's initialization command — creates a new project |
| FAQ | Frequently asked questions |

---

## 2.12 Exercises

### Exercise 1: introduce yourself

Write a program that outputs your self-introduction, at least three lines:

```
大家好，我叫小华。
我今年 12 岁。
我正在学习中文 Rust！
```

<details>
<summary>Reference answer (try it yourself before opening)</summary>

```rust
函数 主函数() {
    打印行!("大家好，我叫小华。");
    打印行!("我今年 12 岁。");
    打印行!("我正在学习中文 Rust！");
}
```

Line by line: line 1 defines the main function; lines 2–4 each print one sentence with a newline; line 5 closes the function.

</details>

### Exercise 2: formatted output

Create two variables `城市` (city) and `温度` (temperature), and use placeholders to output one sentence:

```
北京今天气温 25 度。
```

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    让 城市 = "北京";
    让 温度 = 25;
    打印行!("{}今天气温 {} 度。", 城市, 温度);
}
```

Line by line: line 2 creates the variable `城市` holding "北京"; line 3 creates `温度` holding 25; line 4's two `{}` are filled by `城市` and `温度` in order.

</details>

### Exercise 3: join without newlines

Use two `打印!` (no "行") plus one `打印行!` to output:

```
甲乙丙
```

<details>
<summary>Reference answer</summary>

```rust
函数 主函数() {
    打印!("甲");
    打印!("乙");
    打印行!("丙");
}
```

Line by line: the first two `打印!` calls don't add newlines, so three characters join into one line; the final `打印行!` prints "丙" and closes with a newline.

</details>

### Exercise 4: draw a triangle

Use `打印行!` to output:

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
函数 主函数() {
    打印行!("*");
    打印行!("**");
    打印行!("***");
    打印行!("****");
    打印行!("*****");
}
```

Line by line: five print instructions run top to bottom, each line one star longer than the last. (After Chapter 5 introduces loops, the same output takes three lines of code.)

</details>

### Exercise 5: spot the bug

This code has one mistake. Can you find it?

```rust
// 预期错误: any
函数 主函数() {
    打印行!("找错游戏"）
}
```

<details>
<summary>Reference answer</summary>

The closing bracket is a **Chinese bracket** `）` — it should be the English `)`. And the line is missing its semicolon `;`. Correct version:

```rust
函数 主函数() {
    打印行!("找错游戏");
}
```

(The semicolon would actually be auto-inserted, but make a habit of writing it. The Chinese bracket must be fixed — it errors out immediately.)

</details>

---

## 2.13 FAQ

**Q1: My screen says "rzc is not recognized as a command". What now?**

The terminal can't find rzc. Close and reopen the terminal; if that fails, restart the computer; if it still fails, the build didn't succeed — return to section 2.4 Option B: enter the repository folder, run `cargo build --release --workspace` again, then `cargo install --path crates/cli`.

**Q2: I write in my own language — how can the computer understand?**

It doesn't, really. rzc first swaps your native keywords for English ones (via the mapping table), then rustc compiles to machine code. The "native code" you see is a friendly interface written for you; what runs is the translated English code.

**Q3: Will text inside quotes be translated?**

No. Text inside quotes stays exactly as written. rzc only translates the keywords and identifiers of the code itself.

**Q4: What's the difference between `打印` and `打印行`?**

`打印行!` moves to the next line after printing; `打印!` doesn't. Use `打印!` when you want two pieces of text on one line.

**Q5: Is the exclamation mark `!` required?**

It can be omitted — the tool fills it in. But write it: seeing `!` means "this is a macro".

**Q6: Can I skip VS Code and write code in Notepad?**

You can, but it's not recommended. Notepad has no highlighting, no auto-formatting, no one-key run — it's exhausting. A craftsman needs good tools.

**Q7: Can a code mistake break my computer?**

Absolutely not. The worst a code error does is make the program fail to run — fix it and move on. Experiment freely.

**Q8: I can't remember all the words in this chapter!**

You don't have to memorize anything! The glossary exists for looking things up. Use the words and they stick — like riding a bicycle, nobody memorizes "push off with the left foot first".

---

## Chapter summary

Congratulations on finishing Chapter 2! To recap:

1. **Programming** = writing detailed instruction notes for a computer.
2. **Native-language Rust** = writing Rust with keywords in your own language; rzc translates to English automatically.
3. **Three tools**: the terminal (issuing commands), the compilers rzc + rustc (translating), and the VS Code editor (writing).
4. **Your first program**: `函数 主函数() { 打印行!("..."); }`.
5. **Run command**: `rzc run src/main.<ext>`; check command: `rzc check src/main.<ext>`.
6. **Reading an error in three steps**: the position (which line) → the kind (what's wrong) → the suggestion (how to fix it).

> 📖 If anything in this chapter confused you, check the chapter glossary above or the master glossary at the front of the book.

## What's next

In **Chapter 3, "Variables and Types"**, you'll teach the program to "remember" more data:

- The full rules for creating variables with **`让`** (the "labeled box").
- Making boxes changeable with **`可变`** (mutable).
- The kinds of data: **integers**, **floating-point numbers**, **booleans**, **characters**.
- Making the program do math: add, subtract, multiply, divide.

You'll write a program that can really *count*!
