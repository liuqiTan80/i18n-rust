# English Tutorials

English translations of the rzc tutorial series. Code blocks are **identical**
to the Chinese originals (same dialect, same examples) — so the CI verifier
(`tools/verify-tutorials.py --dir tutorials/en`) validates every block the
same way.

| English | Original (Chinese) | Status |
|---|---|---|
| [Preface: How to Use This Book](Preface-How-to-Use-This-Book.md) | 开篇：这本书怎么用 | ✅ done |
| [Appendix D: Command Cheat Sheet](Appendix-D-Command-Cheatsheet.md) | 附录D：rzc命令速查 | ✅ done |
| [Chapter 1: Making the Most of the VS Code Extension](Chapter-1-Making-the-Most-of-the-VS-Code-Extension.md) | 第一章：用好VS Code扩展 | ✅ done (all code blocks verified) |
| [Chapter 2: Hello, World](Chapter-2-Hello-World.md) | 第二章：你好世界 | ✅ done (16/16 code blocks verified) |
| [Chapter 3: Variables and Types](Chapter-3-Variables-and-Types.md) | 第三章：变量与类型 | ✅ done (44 blocks: 41 pass + 3 expected cross-block deps) |
| Chapter 4–26, Appendices A–C & E | 第四章…附录E | ⬜ pending (see [translation status](../../docs/translation-status.md)) |

Translation principles:

- **Transcreation, not word-for-word**: rewritten the way a native English
  programming author would write it, keeping the metaphor-driven teaching style.
- **Code is never translated** — every example stays byte-identical to the
  verified Chinese original.
- Chapter glossaries are adapted to English readers (the pinyin-lookup sections
  of the zh originals don't apply; English readers get the master glossary).
