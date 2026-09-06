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
| [Chapter 4: Compound Types](Chapter-4-Compound-Types.md) | 第四章：复合类型 | ✅ done (78 blocks total in en dir: 70 pass + 8 expected deps) |
| [Chapter 5: Control Flow](Chapter-5-Control-Flow.md) | 第五章：控制流 | ✅ done (all Ch5 blocks verified, incl. 5 output examples) |
| [Chapter 6: Functions and Methods](Chapter-6-Functions-and-Methods.md) | 第六章：函数与方法 | ✅ done (2 known cross-block deps allowlisted) |
| [Chapter 7: Ownership](Chapter-7-Ownership.md) | 第七章：所有权 | ✅ done (1 known cross-block dep allowlisted) |
| [Chapter 8: References and Borrowing](Chapter-8-References-and-Borrowing.md) | 第八章：引用与借用 | ✅ done (1 known cross-block dep allowlisted) |
| [Chapter 9: Strings and Text](Chapter-9-Strings-and-Text.md) | 第九章：字符串与文本 | ✅ done (15/15 blocks verified, no allowlist needed) |
| [Chapter 10: Structs](Chapter-10-Structs.md) | 第十章：结构体 | ✅ done (6 known deps allowlisted, incl. 1 教学演示) |
| [Chapter 11: Enums and Pattern Matching](Chapter-11-Enums-and-Pattern-Matching.md) | 第十一章：枚举与模式匹配 | ✅ done (4 known cross-block deps allowlisted, same lines as zh) |
| Chapter 12–26, Appendices A–C & E | 第十二章…附录E | ⬜ pending (see [translation status](../../docs/translation-status.md)) |

Translation principles:

- **Transcreation, not word-for-word**: rewritten the way a native English
  programming author would write it, keeping the metaphor-driven teaching style.
- **Code is never translated** — every example stays byte-identical to the
  verified Chinese original.
- Chapter glossaries are adapted to English readers (the pinyin-lookup sections
  of the zh originals don't apply; English readers get the master glossary).
