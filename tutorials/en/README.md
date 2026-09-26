# English Tutorials

English translations of the rzc tutorial series. Code blocks use **standard Rust**
(for an English reader, standard Rust IS their native dialect — see the
translation principles below), and the CI verifier
(`tools/verify-tutorials.py --dir tutorials/en --lang en`) compile-checks every
block.

| English | Original (Chinese) | Status |
|---|---|---|
| [Preface: How to Use This Book](Preface-How-to-Use-This-Book.md) | 开篇：这本书怎么用 | ✅ done |
| [Appendix D: Command Cheat Sheet](Appendix-D-Command-Cheatsheet.md) | 附录D：rzc命令速查 | ✅ done |
| [Chapter 1: Setting Up Your Editor](Chapter-1-Setting-Up-Your-Editor.md) | 第一章：用好VS Code扩展 | ✅ done (adapted: rust-analyzer + cargo for English readers) |
| [Chapter 2: Hello, World](Chapter-2-Hello-World.md) | 第二章：你好世界 | ✅ done (the standard-Rust reference template) |
| [Chapter 3: Variables and Types](Chapter-3-Variables-and-Types.md) | 第三章：变量与类型 | ✅ done |
| [Chapter 4: Compound Types](Chapter-4-Compound-Types.md) | 第四章：复合类型 | ✅ done |
| [Chapter 5: Control Flow](Chapter-5-Control-Flow.md) | 第五章：控制流 | ✅ done |
| [Chapter 6: Functions and Methods](Chapter-6-Functions-and-Methods.md) | 第六章：函数与方法 | ✅ done |
| [Chapter 7: Ownership](Chapter-7-Ownership.md) | 第七章：所有权 | ✅ done |
| [Chapter 8: References and Borrowing](Chapter-8-References-and-Borrowing.md) | 第八章：引用与借用 | ✅ done |
| [Chapter 9: Strings and Text](Chapter-9-Strings-and-Text.md) | 第九章：字符串与文本 | ✅ done |
| [Chapter 10: Structs](Chapter-10-Structs.md) | 第十章：结构体 | ✅ done |
| [Chapter 11: Enums and Pattern Matching](Chapter-11-Enums-and-Pattern-Matching.md) | 第十一章：枚举与模式匹配 | ✅ done |
| [Chapter 12: Generics](Chapter-12-Generics.md) | 第十二章：泛型 | ✅ done |
| [Chapter 13: Traits](Chapter-13-Traits.md) | 第十三章：特征 | ✅ done |
| Chapter 14–26, Appendices A–C & E | 第十四章…附录E | ⬜ pending (see [translation status](../../docs/translation-status.md)) |

Gate status (2026-09-26): 242 blocks extracted, 239 verified — 210 pass +
29 cross-block deps allowlisted, 0 failures (allowlist keyed by file+line
in `tools/expected-failures.json`).

Translation principles (revised 2026-09-06):

- **The code speaks the reader's mother tongue.** This is the project's core
  idea — so the English tutorial's code blocks are **standard Rust**
  (`fn main()`, `println!`, …): for an English speaker, standard Rust IS their
  native dialect. (A Russian tutorial would use the ru pack's Russian keywords;
  a Japanese tutorial the ja pack's — and so on for all 10 languages.)
- **Transcreation, not word-for-word**: prose rewritten the way a native
  English programming author would write it; program strings and outputs are
  English too (`println!("Hello, world!")`).
- **Every code block is still compile-verified** with
  `tools/verify-tutorials.py --dir tutorials/en` — standard Rust passes
  through rzc unchanged, and the gate still catches broken examples.
- Conversion status: **Chapters 1–13 are complete** — Chapter 2 is the
  reference template; Chapters 14+ are in progress, and the multilingual gate
  (zh/en/ja/ru) passes with 0 failures (see translation-status.md).
