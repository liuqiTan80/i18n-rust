<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc: Multilingual Rust Teaching Dialect Compiler

Write Rust programs in your native language. rzc automatically translates them to standard Rust, then compiles and runs — programming education returns to logical thinking, not English memorization.

```rust
// src/main.en — English Rust teaching dialect
fn main() {
    let mut count = 10;
    count = count + 1;
    println!("Count: {}", count);
}
```

```bash
$ rzc run src/main.en
Count: 11
```

## ✨ Features

- **Native language programming**: Write complete Rust programs using English keywords (`fn`, `let`, `if`, `return`…)
- **Multilingual by design**: Architecture natively supports any natural language; 11 language packs built-in, others installable remotely
- **Auto extension detection**: `.zh`, `.en`, `.de` etc. automatically matched to the corresponding language pack
- **Localized error diagnostics**: rustc output translated with 💡 teaching hints; ownership errors (E0382/E0502/E0507) output narrative explanations with structured JSON for visualization
- **Ownership visualization**: VS Code extension highlights variable moves (yellow), reuse (red), and lifetimes (green)
- **Full LSP support**: Completion, hover, go-to-definition, find references, rename, code actions, document symbols, formatting
- **Macro auto-completion**: Macro calls can omit the exclamation mark; auto-added during transpilation
- **Gradual transition**: `eject` exports standard Rust code in one step for smooth migration
- **Complete tutorial**: 24 chapters + 4 appendices, from absolute beginner to comprehensive project

## 📦 Installation

### Via crates.io (recommended)

```bash
cargo install rzc
```

Requires Rust toolchain (stable via rustup).

### Build from source

```bash
# International
git clone https://github.com/liuqiTan80/i18n-rust.git

cd i18n-rust
cargo build --release --workspace
# Binary at target/release/rzc
```

## 🚀 Quick Start

```bash
rzc init my-project
cd my-project
rzc run src/main.en
```

## 🛠️ Commands

| Command | Description |
|---------|-------------|
| `rzc init <name>` | Create new project (with built-in English language pack) |
| `rzc run <file>` | Translate and run `.en` source |
| `rzc check <file>` | Type-check with localized teaching diagnostics |
| `rzc eject <file>` | Export to standard `.rs` code |
| `rzc lang list` | List installed language packs |
| `rzc lang install <source>` | Install a language pack |
| `rzc lang remove <code>` | Remove a user-installed language pack |
| `rzc mapping auto <crate>` | Auto-generate third-party crate mappings |

## 🌍 Language Pack Management

rzc ships with **11 language packs** out of the box — English, Chinese, German, Japanese, Russian, Spanish, French, Portuguese, Korean, Arabic, and Hindi (each pack includes error code translations with localized teaching hints).

- **Auto detection**: `main.en`/`main.zh`/`main.de` auto-load the corresponding built-in pack; error hints are localized to the source file's language
- **Remote install**: `rzc lang install <code>` downloads from GitCode, falls back to GitHub (all 11 packs are built in — no install needed)
- **Custom source**: Set `RZ_LANG_REPO` environment variable
- **Local install**: `rzc lang install ./my-lang-pack`
- **Priority**: `--lang-pack` arg > project `lang-packs/<code>/` > global user dir > built-in

## 🤖 Auto Mapping Generation

```bash
rzc mapping auto anyhow                 # AI mode (needs DEEPSEEK_API_KEY)
rzc mapping auto serde --provider 规则  # Offline rule-based mode
```

## 🎨 Ownership Visualization

| Color | Meaning |
|-------|---------|
| 🟡 Yellow | Move/borrow location |
| 🔴 Red | Variable reuse location |
| 🟢 Green | Lifetime range |

## 💻 VS Code Extension

Syntax highlighting for English (`.en`), Chinese (`.zh`), and German (`.de`) Rust, smart completion, error hints, one-click run/check/eject, and ownership visualization. Search `i18n-rust` in the VS Code marketplace.

## 📖 Tutorial

A complete beginner-friendly Chinese tutorial, 24 chapters + 4 appendices:

| Stage | Chapters |
|-------|----------|
| **Basics** | Ch.1 Hello World · Ch.2 Variables & Types · Ch.3 Compound Types · Ch.4 Control Flow · Ch.5 Functions & Methods |
| **Core** | Ch.6 Ownership · Ch.7 References & Borrowing · Ch.8 Strings · Ch.9 Structs · Ch.10 Enums & Pattern Matching |
| **Generics** | Ch.11 Generics · Ch.12 Traits · Ch.13 Lifetimes · Ch.14 Collections |
| **Errors & Modules** | Ch.15 Error Handling · Ch.16 Module System · Ch.17 Package Management |
| **Advanced** | Ch.18 Smart Pointers · Ch.19 Concurrency · Ch.20 Testing |
| **Expert** | Ch.21 Closures & Iterators · Ch.22 Macros · Ch.23 Async Programming |
| **Project** | Ch.24 Command-line Calculator |
| **Appendix** | A Mapping Reference · B Glossary · C Migration Guide · D FAQ & Learning Path |

## ❓ FAQ

**Q: Why can macros omit the exclamation mark?**
To reduce memorization for beginners. The transpiler auto-adds `!`; standard Rust export restores it.

**Q: Can I use English variable and function names?**
Yes. `count` and `main` are valid identifiers — Rust supports Unicode identifiers.

**Q: How to install other language packs?**
`rzc lang install ja` (remote) or `rzc lang install ./lang-pack-dir` (local).

**Q: How to migrate to standard Rust?**
`rzc eject src/main.en` generates standard `src/main.rs`.

## 🤝 Contributing

Feedback via [GitHub Issues](https://github.com/liuqiTan80/i18n-rust/issues), PRs welcome.

- **Code**: Core engine (`crates/engine/`), CLI (`crates/cli/`), LSP proxy (`crates/lsp/`), VS Code extension (`tools/vscode-extension/`)
- **Language packs**: New language pack directory + `lang_info.toml`
- **Tutorial**: Chapters and appendices in `tutorials/`

## 📄 License

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
