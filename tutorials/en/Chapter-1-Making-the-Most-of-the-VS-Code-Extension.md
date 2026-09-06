# Chapter 1: Making the Most of the VS Code Extension

## 1.0 Learning goals

By the end of this chapter, you will be able to:

1. Explain what an **extension** is and why you need one;
2. Install and verify the **i18n-rust** extension;
3. Run and check programs three ways — **buttons, right-click menu, shortcuts** — no more typing commands by hand;
4. Tidy messy code instantly with **one-key formatting**;
5. Type faster with **code snippets** (two letters that expand into a whole block);
6. Know every command the extension offers — which ones you need now, and which can wait.

> ✨ This chapter needs no new knowledge — just clicking. Every section has a "try it" box; follow along once and it's yours.

---

## 1.1 What is an extension

### A blank notebook + stickers

📖 **VS Code** (met in the preface) is a **code editor** — the place where you write code.

💡 **Metaphor**: a freshly installed VS Code is a **brand-new blank notebook**. It can hold anything but recognizes nothing: write native-language code and it sees plain text — no colors, no running, no error checking.

An **extension** is a **feature sticker** you stick onto the notebook. Each sticker teaches it a new trick:

- Stick on the "native Rust" sticker → the notebook recognizes `.<ext>` files and **colors** native keywords;
- The same sticker brings a **run button**, **formatting**, **code completion**…

The sticker this book uses is called **i18n-rust** ("i18n" is the standard shorthand for "internationalization" — making software speak every language; "rust" is the language you're learning).

> 📖 **Extension**: a plug-in that adds features to VS Code — literally "something that extends it".

### What can it actually do for me?

Here's the full menu; we'll learn it one section at a time:

| Ability | In one sentence | Learned in |
|---|---|---|
| Syntax highlighting | Native keywords turn colorful — structure at a glance | 2.3 |
| Run button | One click ▶ to run — no typing commands | 2.4 |
| Type-check button | One click to check for errors | 2.4 |
| One-key formatting | Messy code tidies itself | 1.5 |
| Code snippets | Type a short name, press Tab, get a whole block | 1.6 |
| Full-width auto-fix | Chinese brackets turn into English ones automatically | 1.7 |
| Error squiggles | Mistakes get underlined in red instantly | 2.4 |
| Command palette | The entrance to all the advanced commands | 1.8 |

---

## 1.2 Installing the extension

### Step one: install rzc (the offline package, recommended)

1. Download the package for your platform from **the Releases page or a file host** (on Windows pick `rzc-<version>-windows-x86_64.zip`);
2. Unpack it to get the rzc executable — open a terminal (`Win + R`, type `cmd`, Enter), and **work from that folder** (or add it to PATH: Settings → System → Advanced system settings → Environment variables);
3. Verify: type `rzc --version` in the terminal — a version number means success.

> 💡 The offline package already bundles rust-analyzer (its official release downloads are unreliable, so it ships in the box) plus the language server and all 10 language packs; rustc/cargo must be installed separately (Chapter 2, section 2.4, Option B), or run `rzc install toolchain` once online.

### Step two: install the extension (offline .vsix install)

> 📌 The i18n-rust extension is distributed as a `.vsix` file (GitHub Releases / the same file hosts as the offline package) — **it is not yet on the VS Code Marketplace**. To upgrade, download the new version from the same place.
### Offline install (from a downloaded .vsix file)

📖 **A .vsix file**: the extension's "installer" — an archive with the `.vsix` extension. Like a phone app's installer; no internet needed.

1. Open VS Code;
2. Press `Ctrl + Shift + P` (Mac: `Cmd + Shift + P`) to open the **command palette** — memorize this shortcut now; section 1.8 uses it again;

> 📖 **Command palette**: a search box at the top of VS Code — type a command's name to run it. Like a phone's voice assistant: say what you want done, and it's done.

3. Type **`Install from VSIX`** and pick **"Extensions: Install from VSIX..."**;
4. In the file picker, find the `.vsix` file (e.g. `i18n-rust-<version>.vsix`) and confirm;
5. A notification pops up in the bottom-right corner when it's done.

### Try it: verify the installation

1. Press `Ctrl + N` for a new blank file;
2. Press `Ctrl + S` to save it as `test.zh` (**the extension must be `.zh`**), in your practice folder;
3. Look at the **bottom-right corner** of the VS Code window: it should say **`Rust (中文)`**.

![示意]右下角语言标识

If it says "Plain Text", the extension isn't active or the file isn't `.zh` — click that text and pick **Rust (中文)** from the list.

> ✅ Seeing `Rust (中文)` means the extension is working.

---

### Environment variables (optional, usually not needed)

| Variable | Purpose |
|---|---|
| `RZ_LANG_DIR` | Point to a language pack directory (defaults to the built-in packs) |
| `RUST_ANALYZER_PATH` | Point to rust-analyzer (when auto-detection fails) |

The VS Code setting `i18n-rust.serverPath` can also pin the language server's location explicitly.

---
## 1.3 Syntax highlighting: code turns colorful

📖 **Syntax highlighting**: the editor paints different parts of the code in different colors. Like a textbook with **bold key terms** and **red vocabulary** — your eye separates structure instantly.

### Try it

In the `test.zh` file, type the Chapter-2 program (letter by letter — good typing practice):

```rust
函数 主函数() {
    让 名字 = "小明";
    让 年龄 = 20;
    打印行!("我叫{}，今年{}岁。", 名字, 年龄);
}
```

After saving (`Ctrl + S`) you'll notice:

- Keywords like `函数` and `让` are one color;
- **Strings** like `"我叫{}，今年{}岁。"` are another;
- **Numbers** like `20` are yet another.

The exact palette depends on your theme — but it will definitely **no longer be all white**.

> 💡 Why color? Real code gets long; color shows "keyword vs content" at a glance. Like a map with green parks and blue rivers — far easier to read than an all-gray one.

### The `.zh` extension matters

The extension only recognizes `.zh` files. If the file is `test.txt` or has no extension, no colors and no run button.

**How to check**: does the file name end in `.zh`? Does the bottom-right corner say `Rust (中文)`?

---

## 1.4 Running programs: three ways

Chapter 2 ran programs with `rzc run` in the terminal. With the extension, there are three easier ways. Practice with the Chapter 2 project (it contains `src/主函数.zh`).

### Way one: the ▶ button, top right

Open `src/主函数.zh` and look at the **top-right of the editor tab**: a ▶ (play) button and a ✓ (check) button.

- ▶ = run the program (same as `rzc run`);
- ✓ = type-check only (same as `rzc check`).

The output appears in the **terminal panel** below.

> 📖 **Terminal panel**: the terminal embedded at the bottom of VS Code — Chapter 1's black window, moved inside so you never switch windows.

### Way two: the right-click menu

**Right-click** in the code area:

| Menu item | What it does |
|---|---|
| i18n: 运行 (Run) | Run the program |
| i18n: 类型检查 (Check) | Check for errors |
| i18n: 导出为 Rust 源码 | Generate the English Rust file (the eject command from Chapter 2, section 2.7) |
| 格式化文档 | Next section |
| i18n: AI 对话 | AI assistance (needs configuration — ignore for now) |

### Way three: keyboard shortcuts (fastest)

| Shortcut (Windows/Linux) | Mac | What it does |
|---|---|---|
| `Ctrl + Shift + R` | `Cmd + Shift + R` | Run the program |
| `Ctrl + Shift + C` | `Cmd + Shift + C` | Type-check |

> 📖 **Shortcut**: press a few keys to complete an action — no mouse. Like knowing your times tables: faster than long addition.

### Try it: break it on purpose, watch the red squiggle

Add this line to the code:

```rust
// 预期错误: E0308
让 年龄: 整数 = "二十";
```

After saving, a **red squiggle** appears under the line; hover the mouse and the reason pops up — the same E0308 from Chapter 2's terminal, but now visible without running anything.

> 📖 **Diagnostic**: the editor's real-time error check, drawn as squiggles. Like red flags on a doctor's report.

> ⚠️ **Note**: squiggles need the extension to start a "language server" program in the background (called `i18n-rust-lsp`). If no squiggle appears a few seconds after opening a project, see section 1.9's troubleshooting.

### Errors you can click: the Problems panel

Press `Ctrl + Shift + M` to open the **Problems panel** — every error and warning in the current file. Click one and the cursor jumps to the broken line.


---

## 1.5 One-key formatting: messy code tidies itself

📖 **Formatting**: automatic adjustment of your code's **indentation and spacing** — uniform and tidy. Like a teacher aligning your paragraphs and straightening your punctuation — **not one word changes, only the layout**.

### Try it

Mess up the code in `test.zh` on purpose (delete indentation, add random spaces):

```rust
函数 主函数() {
让 名字 = "小明";
        打印行!("你好，{}！", 名字);
}
```

Then:

- **Right-click → Format Document**; or
- `Shift + Alt + F` (Mac: `Shift + Option + F`).

The code instantly returns to tidy indentation:

```rust
函数 主函数() {
    让 名字 = "小明";
    打印行!("你好，{}！", 名字);
}
```

> 💡 **Metaphor**: formatting is like tidying a desk. Same books (the code didn't change), but everything is back where it belongs — easier to find, nicer to look at.

> ✨ **Good habit**: press the format shortcut after finishing each stretch of code. Tidy code is easier to debug, and easier for others — including future you — to read.

---

## 1.6 Code snippets: two letters for a whole block

📖 **Code snippet**: a pre-stored code template. Type a **short name**, press `Tab`, and it expands into complete code. Like a chat app's quick replies: type "address" and the whole home address appears.

### Try it

On a new line, type:

```
主函数
```

A **suggestion list** pops up (the top entry is marked as a snippet). Press `Tab` or `Enter`, and it expands into:

```rust
函数 主函数() {
    
}
```

The cursor even lands inside the braces, ready for you to keep writing.

### The full snippet list

| Type this | Expands into |
|---|---|
| `主函数` | The main function skeleton |
| `打印行` | `打印行!("...");` |
| `让变量` | `让 名字 = 值;` |
| `让可变变量` | `让 可变 名字 = 值;` |
| `如果表达式` | `如果 条件 { }` |
| `如果否则` | `如果 条件 { } 否则 { }` |
| `对于循环` | `对于 项 在 集合 中 { }` |
| `当循环` | `当 条件 { }` |
| `循环` | `循环 { }` |
| `匹配表达式` | `匹配 值 { }` |
| `结构体定义` | Struct skeleton (Chapter 10) |
| `枚举定义` | Enum skeleton (Chapter 11) |
| `特征定义` | Trait skeleton (Chapter 13) |
| `实现块` | Impl-block skeleton (Chapter 10) |
| `向量创建` | Vector creation (Chapter 15) |
| `选项有值` / `选项无` | The option type's two values (Chapter 16) |
| `结果成功` / `结果错误` | The result type's two values (Chapter 16) |
| `打印格式化` | `打印格式化!(...)` |

> ✨ Don't memorize the later chapters' snippets — you'll meet each one in its own chapter.

---

## 1.7 Full-width auto-fix: a gift from the Chinese input method

Chapter 2 said: code must use **English (half-width) symbols**. But typing in a Chinese input method makes it easy to slip out **Chinese (full-width) symbols**:

| Full-width (wrong) | Half-width (right) |
|---|---|
| `（）` | `()` |
| `；` | `;` |
| `“ ”` | `" "` |
| `｛｝` | `{}` |

💡 **Metaphor**: full-width symbols are "symbols wearing winter coats" — they look similar, but the computer can't recognize them and reports an error every time.

The i18n-rust extension has a caring feature: when you type a full-width symbol in code, it **automatically swaps in the half-width one** (on by default; the setting is `i18n-rust.autoConvertFullWidthSymbols`).

### Try it

With your Chinese input method on, type a Chinese semicolon `；` in the code area (not inside a string) — it instantly becomes the English `;`.

> ⚠️ **Note**: full-width symbols **inside strings `"..."` are not converted** — Chinese punctuation in strings is legal (the `！` in `打印行!("你好！")` is supposed to be Chinese). The auto-fix only applies to the code part.

---

## 1.8 The command palette: more commands

Section 1.2 introduced `Ctrl + Shift + P` for the **command palette**. Type `i18n` and every extension command appears, sorted by "do I need it now?":

### You'll use these daily

| Command | What it does | When |
|---|---|---|
| i18n: 运行 (Run) | Run the program | Every day (also a button and a shortcut) |
| i18n: 类型检查 (Check) | Check for errors | Every day |
| i18n: 导出为 Rust 源码 | Generate the English Rust file | When you want to see "the translation" (Chapter 2, section 2.7) |

### Occasionally useful

| Command | What it does | When |
|---|---|---|
| i18n: 选择语言包 | Switch the language you write in (Chinese/Japanese/German etc. — 10 of them) | When you want to try Rust in another language |
| i18n: 重启语言服务器 | Restart the background checker | When squiggles vanish or completion dies (section 1.9) |
| i18n: 添加依赖 (cargo add) | Add a third-party library to the project | Chapter 17, package management |

### For later (just know they exist)

| Command | What it does |
|---|---|
| i18n: 校验第三方库映射 (mapping check) | Check a mapping file's correctness (Appendix D) |
| i18n: 生成新语言翻译脚手架 (mapping scaffold) | Generate a template for a new language (Appendix D) |
| i18n: 安装语言包 (lang install) | Install a community language pack (Appendix D) |
| i18n: AI 对话 / 选择 AI 提供商 / 获取 AI 模型列表 | AI assistance — needs your own AI service configured (Appendix D) |

> ✨ No need to memorize: when you need one, press `Ctrl + Shift + P`, type `i18n`, and look.

---

## 1.9 Common mistakes and how to fix them

### Mistake one: no run button on a `.<ext>` file

**Cause**: the extension doesn't recognize the file.

**Fix, in order**:

1. Check the bottom-right corner. Not `Rust (中文)`? Click it and select `Rust (中文)` manually;
2. Make sure the extension really is `.<ext>` (Windows may hide extensions — the file might secretly be `test.zh.txt`; enable "show file extensions" in File Explorer);
3. Press `Ctrl + Shift + P`, type `Reload Window`, Enter.

### Mistake two: no red squiggles, errors go undetected

**Cause**: the background language server didn't start. The most common reason: **it can't find the `rzc` command**.

**Fix, in order**:

1. Open VS Code's built-in terminal (menu: Terminal → New Terminal) and type `rzc --version`:
   - "not recognized as a command / command not found" → rzc isn't installed or isn't on PATH; reinstall per Chapter 2, section 2.4;
2. If rzc works, press `Ctrl + Shift + P` and run **i18n: 重启语言服务器**;
3. Still nothing? `Reload Window`.

> 📖 **PATH**: the operating system's "directory list" for finding commands (mentioned in Chapter 2). If a command isn't on that list, the terminal can't find it.

### Mistake three: the run button errors with "rzc not found"

Same root as mistake two. If rzc lives in an unusual place, point the extension at it:

1. Press `Ctrl + ,` (Mac: `Cmd + ,`) to open Settings;
2. Search `i18n-rust`;
3. Find **i18n-rust.rzcPath** and enter rzc's full path, e.g.:
   - Windows: `C:\Users\yourname\.cargo\bin\rzc.exe`
   - macOS/Linux: `/home/yourname/.cargo/bin/rzc`

(The same settings page has `i18n-rust.serverPath` for the language server's location — usually leave it alone.)

### Mistake four: formatting does nothing

**Cause**: the file isn't recognized as `Rust (中文)`.

**Fix**: run mistake-one's steps first, then format.

### Mistake five: a shortcut does nothing

**Likely cause**: another app or your input method stole the shortcut (`Ctrl + Shift + C` means "convert simplified/traditional" in some input methods).

**Fix**: use the button or the right-click menu — identical behavior. Or rebind `i18n-rust.run` in VS Code Settings → Keyboard Shortcuts.

---

## 1.10 A complete workflow example

Stringing the chapter together — this is your **standard workflow** from now on:

1. **Open the project**: VS Code → File → Open Folder → pick your project;
2. **Open the code**: click `src/主函数.zh` in the file tree;
3. **Write**: `打印行` + Tab for a snippet; full-width symbols auto-fixed;
4. **Save**: `Ctrl + S`; watch for red squiggles;
5. **Tidy**: `Shift + Alt + F` to format;
6. **Run**: click the top-right ▶ (or `Ctrl + Shift + R`);
7. **See the result**: the terminal panel shows the output;
8. **On error**: `Ctrl + Shift + M` opens the Problems panel; click an error to jump there, fix it, back to step 4.

> 💡 Steps 4 → 8 repeat many, many times. That's what programming is: **write a little → check → run → fix → write a little more** — a snowball rolling downhill.

---


## 1.11 Chapter glossary

| Term | One-line meaning |
|---|---|
| Extension | A plug-in adding features to VS Code — ours is i18n-rust |
| .vsix file | The extension's offline installer |
| Command palette | The command search box opened with `Ctrl + Shift + P` |
| Syntax highlighting | Coloring different parts of the code differently |
| Terminal panel | The terminal embedded at the bottom of VS Code |
| Shortcut | A key combination that completes an action |
| Diagnostic | Real-time error checking, drawn as squiggles |
| Problems panel | The panel listing all errors and warnings (`Ctrl + Shift + M`) |
| Formatting | Auto-tidying indentation and spacing, content unchanged |
| Code snippet | A short name that expands into a complete code block |
| Full-width / half-width | Chinese / English symbols — code must use half-width |
| Language server | The checker running in the background (i18n-rust-lsp) |
| PATH | The OS's directory list for finding commands |
| Setting rzcPath | Tells the extension where rzc is installed |

> 📖 **Reminder**: any unfamiliar word — look it up in the master glossary at the front of the book.

---

## 1.12 Exercises

> 💪 Try first, then peek.

### Exercise one: verify the extension

Create a `练习.zh` file with a program that prints `"扩展装好了！"`, and run it **all three ways**.

<details>
<summary>🔍 View answer</summary>

```rust
函数 主函数() {
    打印行!("扩展装好了！");
}
```

Three ways: ① the top-right ▶ button; ② right-click → i18n: 运行 (Run); ③ `Ctrl + Shift + R`. All three print:

```
扩展装好了！
```

</details>

### Exercise two: formatting practice

Paste this badly-formatted code into the editor and tidy it with the formatter:

```rust
函数 主函数() {
让 甲 = 3;
      让 乙 = 7;
打印行!("和是{}", 甲 + 乙);
}
```

<details>
<summary>🔍 View answer</summary>

Right-click → Format Document (or `Shift + Alt + F`):

```rust
函数 主函数() {
    让 甲 = 3;
    让 乙 = 7;
    打印行!("和是{}", 甲 + 乙);
}
```

</details>

### Exercise three: snippets

Using only the keyboard: type a snippet name + Tab to expand an if/else structure, then fill it in so it prints "及格" when the score is at least 60, and "不及格" otherwise.

<details>
<summary>🔍 View answer</summary>

Type `如果否则`, press Tab, and fill in the blanks:

```rust
函数 主函数() {
    让 分数 = 75;
    如果 分数 >= 60 {
        打印行!("及格");
    } 否则 {
        打印行!("不及格");
    }
}
// 预期输出: 及格
```

Output: `及格`

</details>

### Exercise four: hunt an error

Write a program with an error (say `让 年龄: 整数 = "二十";`), and practice using the **Problems panel** (`Ctrl + Shift + M`) to find it and jump to it.

<details>
<summary>🔍 View answer</summary>

The Problems panel lists the E0308 type-mismatch error. Click it — the cursor jumps to the broken line. Change `"二十"` to `20` and it's fixed.

</details>

---

## 1.13 FAQ

**Q: Does the extension "translate" my code? Will my file be modified?**

No. The extension only **colors, checks, runs and formats**. Your `.<ext>` file stays exactly as you wrote it. Translation to English Rust happens only for an instant inside memory (by rzc, during run/check) — or when you explicitly use "导出为 Rust 源码", which creates a separate `.rs` file.

**Q: Can I use both `rzc run` in the terminal and the extension buttons on the same project? Will results differ?**

No difference. The buttons call the same rzc commands underneath — they just save you typing. Use either freely.

**Q: My classmate's VS Code shows different menu names than mine. What now?**

No problem. The command names (like "i18n: 运行 (Run)") are identical in the command palette, and shortcuts match too. Can't find a menu item? The command palette always works.

**Q: Can I turn off full-width auto-conversion?**

Yes. Settings → search `i18n-rust.autoConvertFullWidthSymbols` and untick it. But we recommend leaving it on — it eliminates an entire family of "Chinese symbol" errors.

**Q: The snippet suggestion list doesn't pop up?**

Check two things: ① the bottom-right corner says `Rust (中文)`; ② VS Code's completion isn't disabled (Settings → search `suggestOnTriggerCharacters`, make sure it's ticked). You can also trigger manually with `Ctrl + Space`.

**Q: Which languages does the extension support? Only Chinese?**

Ten: Chinese, 日本語, Deutsch, Español, Français, Português, Русский, 한국어, हिन्दी, العربية. Switch with "i18n: 选择语言包"; each has its own file extension (`.zh`, `.ja`, …). This book teaches the Chinese dialect.

**Q: Why does nothing happen when I click features like AI chat?**

AI features need your own AI service key and address configured — an optional track. Beginners don't need them at all; skipping changes nothing about writing and running code.

**Q: After upgrading the extension, my code suddenly shows a wall of errors?**

First run "i18n: 重启语言服务器", then `Reload Window`. Still broken? Check that rzc was upgraded too (`rzc --version`) — extension and rzc versions should stay in step.

---

## What's next

All tools ready! In **Chapter 2, "Hello, World"**, you'll run programs with buttons and shortcuts, format code in one key, and let the editor auto-complete native keywords — no more typing commands. Then **Chapter 3, "Variables and Types"** teaches the program to "remember" data:

- The full rules for creating variables with **`让`** (the "labeled box").
- Making boxes changeable with **`可变`**.
- Declaring never-changing values with **`常量`**.
- Meeting integers, floats, booleans, characters and strings.
- **Type conversion**: turning integers into floats for division.

See you in the next chapter!
