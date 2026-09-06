# English Tutorials

English translations of the rzc tutorial series. Code blocks are **identical**
to the Chinese originals (same dialect, same examples) — so the CI verifier
(`tools/verify-tutorials.py --dir tutorials/en`) validates every block the
same way.

| English | Original (Chinese) | Status |
|---|---|---|
| [Preface: How to Use This Book](Preface-How-to-Use-This-Book.md) | 开篇：这本书怎么用 | ✅ done |
| [Appendix D: Command Cheat Sheet](Appendix-D-Command-Cheatsheet.md) | 附录D：rzc命令速查 | ✅ done |
| Chapter 1–26, Appendices A–C & E | 第一章…附录E | ⬜ pending (see [translation status](../../docs/translation-status.md)) |

Translation principles:

- **Transcreation, not word-for-word**: rewritten the way a native English
  programming author would write it, keeping the metaphor-driven teaching style.
- **Code is never translated** — every example stays byte-identical to the
  verified Chinese original.
- Chapter glossaries are adapted to English readers (the pinyin-lookup sections
  of the zh originals don't apply; English readers get the master glossary).
