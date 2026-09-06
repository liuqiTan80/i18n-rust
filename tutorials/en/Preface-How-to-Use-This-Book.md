# Preface: How to Use This Book

Welcome, future programmer!

This book was written for you — someone who has **never programmed before**. Even if you have never written a single line of code in your life, you can read it from cover to cover and learn to write Rust programs in your own native language.

> 💡 **Metaphor**: think of this book as a dictionary plus a recipe collection. The dictionary part means: every word you don't understand can be looked up right here. The recipe part means: every step is written out so precisely that if you follow it, it works.

---

## 0.1 What this book teaches

This book teaches you **programming**.

(What does "programming" mean? Writing instructions for a computer so it does what you have in mind. Chapter 1 explains this properly — for now, just remember: programming = writing instructions for a computer.)

The language you will learn is **Rust**, one of the most popular programming languages in the world.

Ordinary Rust uses English keywords. This book teaches you to write **Rust in your native language** — the keywords are words you already know.

Code you write in your native language is automatically translated into standard Rust by a tool called **rzc**, and then it becomes a program that actually runs.

> 💡 **Metaphor**: it's like writing a letter in your own language, with a translator who converts it into English before mailing it. The recipient reads English — but all *you* ever needed was your own language.

---

## 0.2 Who this book is for

This book assumes:

- **You have never written a program.** That's fine — we start from zero.
- **You don't know any jargon.** That's fine — every technical term is explained the first time it appears.
- **You're not confident around computers.** That's fine — operations like "open a file" or "save a file" are walked through step by step.
- **You don't want to hunt for other resources.** That's fine — this book is *self-contained*: every answer you need lives inside it.

> 📖 **Self-contained**: meaning "everything it needs, it carries with itself" — like a toolbox that never has to borrow from the neighbors. Every concept, every definition, every likely problem: it's all in here.

---

## 0.3 How the book is organized

The book has four parts — 26 chapters plus 5 appendices (reference material that comes after the main text).

| Part | Chapters | What you will learn |
|---|---|---|
| **Part 1: Getting started** | 1–5 | Setting up tools, using the VS Code extension, your first program, variables, control flow |
| **Part 2: Writing real programs** | 6–11 | Functions, ownership, strings, structs, enums |
| **Part 3: Growing your powers** | 12–18 | Generics, traits, error handling, modules, package management |
| **Part 4: Advanced topics & a real project** | 19–26 | Smart pointers, threads, testing, async, a capstone project, unsafe Rust & FFI |

> 💡 **Metaphor**: learning to program is like building a house. Part 1 lays the foundation, Part 2 raises the walls, Part 3 fits the doors and windows, Part 4 does the finishing — and at the end you have a house people can actually live in.

**Please read the chapters in order.** Every chapter builds on the one before it; skipping ahead may land you on words that were never explained.

---

## 0.4 What every chapter looks like

Every chapter follows the same nine-part structure, like fixed building blocks:

1. **Learning goals** — what this chapter covers, in plain words.
2. **The lesson** — first a story from everyday life, then how the story maps onto code.
3. **A complete example** — a full program you can run as-is.
4. **Line-by-line explanation** — what every single line does, one at a time.
5. **What you should see** — what appears on screen when the program runs.
6. **Common mistakes** — the traps beginners fall into, and how to climb out.
7. **Chapter glossary** — every new word from this chapter, defined.
8. **Exercises** — a few hands-on tasks, all with answers.
9. **FAQ** — the questions learners actually ask, answered.

("FAQ" is short for "frequently asked questions".)

Every chapter closes with the same reminder:

> 📖 If anything in this chapter confused you, check the chapter glossary or the master glossary at the front of the book.

---

## 0.5 What the special symbols mean

While reading, you'll run into these markers:

| Symbol | Name | Meaning |
|---|---|---|
| 💡 | Metaphor | An everyday story that makes an abstract idea click |
| 📖 | Term | A new word with a formal definition — look it up in the glossary |
| ⚠️ | Careful | A place beginners most often trip — pay extra attention |
| ✅ | Correct | This is the right way to write it |
| ❌ | Wrong | This is the wrong way to write it |
| ✨ | Tip | A bonus nugget — nice to know, safe to skip |

---

## 0.6 How to look up a word you don't know

The book has two "dictionaries":

1. **The master glossary** — at the very front, listing every term in the book.
2. **Chapter glossaries** — at the end of each chapter, listing only that chapter's new words. Faster to search.

**How to look things up:**

- Hit an unfamiliar word? Check that chapter's glossary first.
- Not there? Go to the master glossary at the front.
- Appendix B is an **index**: every term, and the chapter where it first appears.

> 💡 **Metaphor**: the chapter glossary is the small dictionary in your backpack — always at hand. The master glossary is the library's big dictionary — it has everything.

---

## 0.7 Two words you'll see everywhere: "code" and "run"

These two appear on almost every page. For now, just get acquainted (Chapter 2 explains them properly):

> 📖 **Code**: text you write for a computer to read — like a note you pass to a classmate, except the recipient is a machine.

> 📖 **Run**: making the computer execute your code so you can see the result — like flipping the switch on a toy and watching it move.

---

## 0.8 What you need

Just three things:

1. A computer with internet access (Windows, macOS or Linux all work — Chapter 1 walks through setup for each).
2. About 30 minutes a day.
3. A willingness to try things with your own hands.

> ✨ **Tip**: the secret to learning programming is doing. Only reading is like watching someone swim without ever getting in the water. Type in every example yourself — really.

---

## 0.9 When you get stuck

1. **A word you don't understand** → check the chapter glossary or the master glossary.
2. **An error message** → don't panic! Check the "Common mistakes" section, or Appendix C, *A Dictionary of Common Error Messages*. An error message is the computer proofreading your homework — it's help, not criticism.
3. **The output is wrong** → compare against the line-by-line explanation and check your code one line at a time.
4. **You forgot a command** → Appendix D, *The rzc Command Cheat Sheet*.

---

## 0.10 One last word

Programming is not a talent reserved for geniuses. It's like riding a bicycle: you'll wobble at first, but step by step, anyone can learn.

This book holds your hand the whole way. The only thing you need to do is **open Chapter 1 and follow along**.

See you in Chapter 1!

---

📖 **Words from this preface**:
- **Programming**: writing instructions for a computer so it does what you have in mind.
- **Self-contained**: carrying everything it needs, borrowing nothing.
- **Code**: text written for a computer to read.
- **Run**: making the computer execute code and show the result.
- **FAQ**: frequently asked questions.
- **Appendix**: reference material that comes after the main text.
