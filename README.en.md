<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc: Multilingual Rust Teaching Dialect Compiler

Write Rust programs in your native language. rzc automatically translates them to standard Rust, then compiles and runs — programming education returns to logical thinking, not English memorization.

```rust
// src/main.zh — Chinese Rust teaching dialect
函数 主函数() {
    让 可变 数量 = 10;
    数量 = 数量 + 1;
    打印行!("数量是：{}", 数量);
}
```

```bash
$ rzc run src/main.zh
数量是：11
```

## ✨ Features

- **Native language programming**: Write complete Rust programs using Chinese keywords (`函数`, `让`, `如果`, `返回`…)
- **Multilingual by design**: Architecture natively supports any natural language; Chinese language pack built-in, others installable remotely
- **Auto extension detection**: `.zh`, `.ja`, `.ru` etc. automatically matched to the corresponding language pack
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
# China mirror
git clone https://gitcode.com/tan80/zrRust.git
# International
git clone https://github.com/liuqiTan80/i18n-rust.git

cd zrRust or i18n-rust
cargo build --release --workspace
# Binary at target/release/rzc
```

## 🚀 Quick Start

```bash
rzc init my-project
cd my-project
rzc run src/main.zh
```

## 🛠️ Commands

| Command | Description |
|---------|-------------|
| `rzc init <name>` | Create new project (with built-in Chinese language pack) |
| `rzc run <file>` | Translate and run `.zh` source |
| `rzc check <file>` | Type-check with localized teaching diagnostics |
| `rzc eject <file>` | Export to standard `.rs` code |
| `rzc lang list` | List installed language packs |
| `rzc lang install <source>` | Install a language pack |
| `rzc lang remove <code>` | Remove a user-installed language pack |
| `rzc mapping auto <crate>` | Auto-generate third-party crate mappings |

## 🌍 Language Pack Management

rzc ships with a Chinese language pack (115+ keywords, 496 stdlib mappings, 53 error code translations) out of the box.

- **Auto detection**: `main.zh` uses Chinese; after installing Japanese, `main.ja` auto-loads
- **Remote install**: `rzc lang install 俄语` downloads from GitCode, falls back to GitHub
- **Custom source**: Set `RZ_LANG_REPO` environment variable
- **Local install**: `rzc lang install ./my-lang-pack`
- **Priority**: `--lang-pack` arg > project `语言包/<code>/` > global user dir > built-in

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

Syntax highlighting, smart completion, error hints, one-click run/check/eject, and ownership visualization. Search `i18n-rust` in the VS Code marketplace.

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

**Q: Can I use Chinese variable and function names?**
Yes. Rust supports Unicode identifiers. `数量` and `主函数` are valid.

**Q: How to install other language packs?**
`rzc lang install 日本語` (remote) or `rzc lang install ./lang-pack-dir` (local).

**Q: How to migrate to standard Rust?**
`rzc eject src/main.zh` generates standard `src/main.rs`.

## 🤝 Contributing

Feedback via [GitHub Issues](https://github.com/liuqiTan80/i18n-rust/issues), PRs welcome.

- **Code**: Core engine (`crates/engine/`), CLI (`crates/cli/`), LSP proxy (`crates/lsp/`), VS Code extension (`tools/vscode-extension/`)
- **Language packs**: New language pack directory + `lang_info.toml`
- **Tutorial**: Chapters and appendices in `tutorials/`

## 📄 License

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
