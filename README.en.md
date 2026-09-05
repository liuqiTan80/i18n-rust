<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc: Multilingual Rust Teaching Dialect Compiler

> 🌍 **Write Rust in your native language.** · 10 languages · real Rust, real toolchain · graduate anytime with `rzc eject`

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

## 📦 Installation (build from source)

rzc has no prebuilt online installer — build it on your own machine (1–3 minutes).

### Prerequisites

| Tool | Purpose | Required? |
|---|---|---|
| **Rust toolchain** (rustc + cargo) | Build rzc itself | ✅ Yes |
| **git** | Get the source code | ✅ Yes (or download the source ZIP) |
| **Network** | Fetch dependencies on first build | ✅ Yes (first time) |
| **Node.js 18+ & npm** | Build the VS Code extension | Optional (IDE only) |
| **rust-analyzer** | IDE completion/diagnostics backend | Optional (IDE only) |

### 1. Install the Rust toolchain

[rustup](https://rustup.rs) is the official recommended installer — one command sets up rustc, cargo and rustup.

- **Linux / macOS** (in a terminal):

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

  Then run `source "$HOME/.cargo/env"` (or reopen the terminal) to activate.

- **Windows**: download [rustup-init.exe](https://rustup.rs) and follow the wizard (or run `winget install --id Rustlang.Rustup` in PowerShell).

Verify (after reopening the terminal):

```bash
rustc --version   # e.g. rustc 1.98.0
cargo --version
```

### 2. Get the source and build

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace
./target/release/rzc --version
```

The first build downloads dependencies and compiles all components (about 1–3 minutes). Binaries:

- `target/release/rzc` — the CLI tool
- `target/release/i18n-rust-lsp` — the language server (VS Code extension backend)

### 3. (Optional) Make rzc globally available

```bash
cargo install --path crates/cli   # builds locally and installs to ~/.cargo/bin
```

### 4. (Optional) Install rust-analyzer for IDE features

```bash
rzc install toolchain --ra-only --force
```

### 5. (Optional) Build the VS Code extension

Requires Node.js 18+ and npm (from [nodejs.org](https://nodejs.org)):

```bash
cd tools/vscode-extension
npm ci
npm run package    # produces i18n-rust-<version>.vsix
```

Install the `.vsix` via "Install from VSIX..." in VS Code; the language server is provided by `rzc install lsp`.

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
- **Multilingual by design**: 10 built-in language packs (zh/de/ja/ru/es/fr/pt/ko/ar/hi), auto-detected by file extension
- **Localized diagnostics**: `rzc check` translates rustc errors into the file's language, with 💡 teaching hints
- **VS Code extension**: Build it from source (`cd tools/vscode-extension && npm ci && npm run package`) and install the resulting `.vsix` via "Install from VSIX..." (see [install guide](tools/vscode-extension/)). Includes syntax highlighting, completion, ownership visualization (color-highlighted variable moves & reuse), one-click run/check, AI chat
- **Full LSP support**: completion, hover, go-to-definition, find references, rename, code formatting
- **Gradual transition**: `rzc eject` exports standard Rust code in one step

## 📖 Tutorial

A complete beginner-friendly Chinese tutorial, 26 chapters + glossary + 5 appendices — see [tutorials/](tutorials/).

## 📄 License

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
