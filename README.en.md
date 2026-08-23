<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc: Multilingual Rust Teaching Dialect Compiler

Write Rust programs in your native language. rzc automatically translates them to standard Rust, compiles and runs — learn programming, not English.

```rust
// src/main.zh — the Chinese Rust teaching dialect
fn main() {
    let mut count = 10;
    count = count + 1;
    println!("Count: {}", count);
}
```

```bash
$ rzc run src/main.zh
Count: 11
```

## 📦 Installation

One command — globally available right after install:

```bash
cargo install rzc
```

> Requires the [Rust toolchain](https://www.rust-lang.org/tools/install) (stable via rustup). Language packs are built in — no extra configuration needed.

You can also build from source:

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace                # binary at target/release/rzc
```

## 🚀 Quick Start

```bash
rzc init my-project
cd my-project
rzc run src/main.zh
```

`rzc init` creates a complete runnable project skeleton (`Cargo.toml` + `src/main.zh`) — just run it.

## 🛠️ Commands

| Command | Description |
|---------|-------------|
| `rzc init <name>` | Create a new project |
| `rzc run <file>` | Translate and run the source file |
| `rzc check <file>` | Type-check with localized teaching diagnostics |
| `rzc eject <file>` | Export to standard Rust code |
| `rzc lang list` | List installed language packs |
| `rzc mapping auto <crate>` | Auto-generate third-party crate mappings |

## ✨ Features

- **Native-language programming**: write complete Rust programs using your own language keywords
- **Multilingual by design**: 11 built-in language packs (en/zh/de/ja/ru/es/fr/pt/ko/ar/hi), auto-detected by file extension
- **Localized diagnostics**: `rzc check` translates rustc errors into the file's language, with 💡 teaching hints
- **VS Code extension**: Download `i18n-rust.vsix` from [Releases](https://github.com/liuqiTan80/i18n-rust/releases), then in VS Code choose "Install from VSIX..." (see [install guide](tools/vscode-extension/)). Includes syntax highlighting, completion, ownership visualization (color-highlighted variable moves & reuse), one-click run/check, AI chat
- **Full LSP support**: completion, hover, go-to-definition, find references, rename, code formatting
- **Gradual transition**: `rzc eject` exports standard Rust code in one step

## 📖 Tutorial

A complete beginner-friendly Chinese tutorial, 24 chapters + 4 appendices — see [tutorials/](tutorials/).

## 📄 License

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
