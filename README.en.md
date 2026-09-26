<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

[![CI](https://github.com/liuqiTan80/i18n-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/liuqiTan80/i18n-rust/actions)
[![crates.io](https://img.shields.io/crates/v/rzc.svg)](https://crates.io/crates/rzc)
[![Docs](https://img.shields.io/badge/Docs-Online%20documentation-blue)](https://liuqiTan80.github.io/i18n-rust/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

> 🪞 **Repository mirror**: maintained on both GitHub ([liuqiTan80/i18n-rust](https://github.com/liuqiTan80/i18n-rust)) and GitCode ([tan80/i18n-rust](https://gitcode.com/tan80/i18n-rust)). `rzc lang install` prefers the GitCode source and falls back to GitHub automatically.

# rzc: Multilingual Rust Teaching Dialect Compiler

> 🌍 **Write Rust in your native language.** · 10 languages · real Rust, real toolchain · graduate anytime with `rzc eject`

**rzc is a multilingual Rust dialect compiler**: you write code in your native language; rzc translates it to standard Rust in real time, the official toolchain compiles and runs it, and every diagnostic comes back translated into your language with teaching hints.

- 🌍 **Not pseudocode**: native-language code is fully isomorphic to standard Rust — compilation, execution, dependencies and the ecosystem are 100% real
- 🎓 **Made for beginners**: error messages become "what to do next" guidance instead of a wall of English
- 🚪 **Graduate anytime**: `rzc eject` exports standard Rust in one step — no lock-in, fully compatible with the mainstream ecosystem

```rust
// src/main.zh — the Chinese teaching dialect
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

Mistakes are reported in your language too:

```
错误[E0384]: 不可变变量 `数量` 被重复赋值
  --> src/main.zh:3:5
💡 如果需要修改变量的值，请使用 `让 可变` 声明变量。
```

The same program runs in any of the 10 built-in dialects — e.g. Japanese (`関数 主関数()`, `表示行!`) or Russian (`функция главная()`, `печатай_строку!`). Standard Rust (`fn main()`) is always accepted as-is.

**Start here**: 🚀 [Quick start](#quick-start) · 🌐 [Online docs](https://liuqiTan80.github.io/i18n-rust/) (4 languages) · 📚 [Feishu KB](https://my.feishu.cn/wiki/space/7689728327082314704) (Chinese tutorial, no login required) · 📖 [Learn step by step](#tutorials) · 🤝 [Contributing](#contributing)

---

## 📦 Installation

Two options: **Option 1 — install from crates.io** (recommended for most users); **Option 2 — build from source** (latest development build, or when modifying rzc).

**Option 1: Install from crates.io (recommended)** — published on crates.io; one command gives you a global `rzc`:

```bash
cargo install rzc        # fetch the source from crates.io and build locally (1–3 minutes)
rzc --version            # verify (a version number means success)
rzc init my-project && cd my-project && rzc run src/main.zh
```

> Language packs are built in — no extra configuration. Crate page: <https://crates.io/crates/rzc>; upgrade an installed version with `cargo install rzc --force`. No Rust toolchain yet? See step 1 below.

**Option 2: Build from source (developers)** — get the latest development build, or build after modifying rzc. **Shortest path** (with a Rust toolchain installed — four commands to your first program):

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git && cd i18n-rust
cargo build --release --workspace   # 1–3 minutes
cargo install --path crates/cli     # make rzc globally available (or use ./target/release/rzc directly)
rzc init my-project && cd my-project && rzc run src/main.zh
```

The full walkthrough below covers Option 2 (prerequisites → build → verify); Option 1 only needs step 1 for the toolchain.

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

The official standalone rust-analyzer is installed into `~/.rz/toolchain`, and both rzc and the LSP prefer it automatically.

### 5. (Optional) Build the VS Code extension

Requires Node.js 18+ and npm (from [nodejs.org](https://nodejs.org)):

```bash
cd tools/vscode-extension
npm ci
npm run package    # produces i18n-rust-<version>.vsix
```

Install the `.vsix` via "Install from VSIX..." in VS Code to get syntax highlighting, completion, diagnostics, hover and ownership visualization; the language server is provided by `rzc install lsp`.

### Full-featured setup (one command each)

```bash
rzc install lsp          # language server (VS Code completion/diagnostics/hover backend)
rzc install toolchain    # embedded official toolchain (standalone rustc/cargo/rust-analyzer into ~/.rz/toolchain)
rzc doctor               # inspect the toolchain environment (embedded / PATH / version diff)
```

After installation, rzc and the LSP automatically prefer the embedded toolchain — single-file projects are compiled straight with rustc, no cargo index needed.

### Environment variables (optional)

| Variable | Purpose |
|---|---|
| `RZ_LANG_DIR` | Custom language-pack directory (defaults to the built-in packs) |
| `RUST_ANALYZER_PATH` | Explicit rust-analyzer path (when auto-detection fails) |

The VS Code setting `i18n-rust.serverPath` explicitly points to the LSP binary (used when auto-detection fails).

### Upgrading components

| Task | Command |
|---|---|
| Upgrade rustc/cargo (e.g. 1.98 → 1.99) | `rzc install toolchain --version 1.99.0 --force` |
| Upgrade rust-analyzer only (skip the 300 MB re-download) | `rzc install toolchain --ra-tag <date-tag> --ra-only --force` |
| Show current versions and status | `rzc doctor` |

### Switching editors (VSCodium / Cursor / other VS Code-based)

The i18n-rust extension (`.vsix`) works in every VS Code-based editor; rzc, the language server and the toolchain are editor-agnostic:

1. In the new editor: Extensions → "Install from VSIX" → pick `i18n-rust-<version>.vsix`;
2. Wire the components into standard locations (using your built rzc):
   ```bash
   rzc install lsp        # language server → ~/.cargo/bin
   rzc install toolchain  # embedded toolchain → ~/.rz/toolchain (online; or copy from a distributor's offline package)
   ```
3. Open a `.zh` file and go — the extension locates the server and toolchain automatically (`RUST_ANALYZER_PATH` or the `i18n-rust.serverPath` setting can override).

### For distributors: offline release package (classroom distribution)

One-shot local packaging scripts produce a platform-specific release bundle for Releases:

| Platform | Command | Artifact |
|---|---|---|
| Linux / macOS | `./release-offline.sh` | `release/rzc-<version>-linux/macos-<arch>.tar.gz` |
| Windows (PowerShell) | `.\release-offline.ps1` | `release/rzc-<version>-windows-x86_64.zip` |

Prerequisite: a release build (step 2 above) and one online run of `rzc install toolchain --ra-only --force` (the packager copies the platform rust-analyzer into the bundle). The package contains rzc, i18n-rust-lsp, rust-analyzer, all 11 language packs and the tutorial — unzip and use (details in tutorial Appendix D.5).

## 🚀Quick Start

```bash
rzc init my-project
cd my-project
rzc run src/main.zh      # translate → compile → run
```

`rzc init` creates a complete runnable project skeleton (`Cargo.toml` + `src/main.zh`) with the toolchain version pinned to your local one — just run it.

---

## 🛠️ Commands

| Command | Description |
|---|---|
| `rzc init <name>` | Create a new project (toolchain version pinned to your local one; IDE-ready) |
| `rzc run <file>` | Translate and run; warnings/errors/build progress all in your language |
| `rzc check <file>` | Type-check with localized teaching diagnostics |
| `rzc eject <file>` | Export to standard Rust code (gradual transition) |
| `rzc transpile <file>` | Translate only — print standard Rust to stdout |
| `rzc cheat <lang>` | Native ↔ Rust mapping cheat sheet (`--markdown` for embedding) |
| `rzc add <crate>[@version]` | Add a dependency (wraps `cargo add`, with localized mapping hints) |
| `rzc doctor` | Diagnose the toolchain environment (embedded / PATH / version diff) |
| `rzc lang list` | List installed language packs |
| `rzc lang install <code/dir>` | Install a language pack (remote registry or local directory) |
| `rzc lang search [keyword]` | Search installable remote language packs |
| `rzc lang remove <code>` | Remove a user-installed language pack |
| `rzc mapping auto <crate> [--target-version <v>]` | Auto-generate native mappings for a third-party crate (AI/rules); pin the generation baseline version |
| `rzc mapping check [target]` | Validate mapping quality (duplicate keys / keyword collisions / cross-file conflicts / required sections) |
| `rzc mapping coverage [--lang <lang>]` | Measure language-pack coverage against real source code; list missing mappings (run at repo root) |
| `rzc mapping scaffold <source> <target>` | Generate a translation skeleton for a new language; `--provider deepseek` for AI translation |
| `rzc crate search [keyword]` | Search community-shared third-party mapping packages (registry) |
| `rzc crate install <crate> --lang <lang>` | Install a single third-party mapping into your global language pack |
| `rzc crate list` | List installed community mappings |
| `rzc crate remove <crate> --lang <lang>` | Remove an installed community mapping |
| `rzc crate update` | Re-fetch all installed mappings from the manifest (get updates) |
| `rzc crate publish <crate> --lang <lang>` | Publish your local mappings to the registry (passes the quality gate first) |
| `rzc install <lsp\|toolchain>` | Install companion components (language server / embedded official toolchain) |

Full reference: [Appendix D: rzc Command Cheat Sheet](tutorials/en/Appendix-D-Command-Cheatsheet.md) (English).

---

## ✨ Features

### Native-language programming
Write complete programs using native keywords (`函数`, `让`, `如果`, `匹配`, …) and a native standard library (`字符串`, `向量::新建()`, `使用 标准集合::哈希映射`). Macros, lifetimes, generics and traits are all supported.

### 10 built-in languages

| Language | Ext. | Language | Ext. |
|----------|------|----------|------|
| 中文 Chinese | `.zh` | Español Spanish | `.es` |
| Deutsch German | `.de` | Français French | `.fr` |
| 日本語 Japanese | `.ja` | Português Portuguese | `.pt` |
| 한국어 Korean | `.ko` | العربية Arabic | `.ar` |
| Русский Russian | `.ru` | हिन्दी Hindi | `.hi` |

Rust itself is written in English, so English is not a teaching dialect (an identity mapping has no teaching value); an `en` identity pack (extension `.en`) exists for identity-mapping scenarios and English UI strings. The 10 natural languages above are auto-matched by file extension, and can be mixed within one project.

### Teaching-grade diagnostics
- **Dual-track translation of error codes + messages**: covers rustc error codes, codeless lint warnings and help phrases
- **Localized type names**: `std::fmt::Display` → `标准库::格式化::可显示` (per-language equivalent)
- **💡 Teaching hints**: every error comes with a next-step suggestion; ownership errors include 📌 move/borrow narratives
- **Dependency guidance**: detects undeclared third-party crates and suggests `rzc add <crate>`

### Full IDE experience (VS Code / Qoder extension)
Syntax highlighting, smart completion, hover docs, go-to-definition, find references, rename, code formatting, one-click run/check, full-width punctuation auto-conversion, AI-assisted translation.

### Third-party crates in your language
`rzc mapping auto` extracts public APIs from installed crates and generates native names (AI); production mappings can pin their baseline version with `--target-version` (recorded in the file header); community mappings pass the `rzc mapping check` quality gate.

### Why this is not a "toy language"

| | Typical "native-language programming" toys | rzc |
|---|---|---|
| Code shape | Invented syntax or pseudocode | Fully isomorphic to standard Rust — only identifiers are localized |
| Compilation | Own interpreter / transpile-only | Official rustc/cargo — truly compiled and executed |
| Ecosystem | Closed or trimmed | Full crates.io (`rzc add` + native mappings) |
| Diagnostics | Raw English or ad-hoc tips | Translated messages + error codes + teaching hints |
| Exit cost | Rewrite from scratch | `rzc eject` exports standard Rust in one step |

---

## 📖Tutorials

A complete beginner-friendly **Chinese tutorial** — 26 chapters + glossary + 5 appendices — lives in [tutorials/](tutorials/): from "Hello, World" through ownership, closures, async and macros, up to a full capstone project. Every example is written in Chinese Rust.

🌐 **Read online**: [documentation site](https://liuqiTan80.github.io/i18n-rust/) (mdBook, 4 languages with a top-bar switcher; preview locally with `make site-serve`) · 📚 **For readers in China**: [Feishu knowledge base](https://my.feishu.cn/wiki/space/7689728327082314704) (33-part Chinese tutorial, publicly readable without a Feishu account).

**Translations in progress**: English (Preface, Chapters 1–13, Appendix D — 15/33), 日本語 and Русский rolling; see [translation-status.md](docs/translation-status.md) for progress and the next batch.

> Tutorial quality is enforced by CI ([tools/verify-tutorials.py](tools/verify-tutorials.py)): every code block must compile, and error examples must emit the annotated expected error code (`// 预期错误: EXXXX`). Verify locally after editing a tutorial:
>
> ```bash
> make tutorials       # Chinese gate
> make tutorials-all   # multi-language gate: en / ja / ru
> ```

> Translations of the tutorial into more languages are very welcome — see "Contributing" below.

---

## 🏗️ Project Structure & How It Works

```text
Native source (.zh)
   │  lexer transpile → module-path rewrite → alias rewrite    ← engine (language-agnostic)
   ▼
Standard Rust source
   │  cargo build / run (official toolchain)
   ▼
JSON diagnostics → error-code/message translation + type localization + teaching hints → native output
```

| Directory | Responsibility |
|---|---|
| `crates/engine` | Language-agnostic core engine: transpile pipeline, mapping management, diagnostic translation, incremental cache, Unicode safety checks |
| `crates/cli` | the `rzc` command-line tool |
| `crates/lsp` | `i18n-rust-lsp`: proxies the official language server (rust-analyzer), translating positions and diagnostics in both directions |
| `crates/engine/lang-packs` | 11 built-in language packs (10 natural languages + the `en` identity pack: keywords / stdlib / module paths / error translations / UI strings) |
| `tools/vscode-extension` | VS Code / Qoder extension |
| `tools` | gates and build scripts: tutorial verification, benchmark regression, docs-site assembly ([tools/README.md](tools/README.md)) |
| `tutorials` | 26-chapter Chinese tutorial + appendices (en/ja/ru translations in progress) |
| `book` | docs-site assembly source (mdBook, see [book/README.md](book/README.md)) |
| `docs` | reference docs, dev docs and the [project map](docs/project-map.md); operations & roadmap in [docs/strategy/](docs/strategy/README.md) |

**Design principle**: the engine hardcodes no specific language — adding a natural language means adding one language-pack directory, with zero code changes (packs are embedded automatically at build time). To port the paradigm to other programming languages (e.g. Chinese Python), see the [dialect-framework blueprint](docs/dialect-framework-blueprint.md).

---

## 🤝Contributing

- **First time here?** Start with the [project map (maintainer handbook)](docs/project-map.md) — where to change X and how to verify; dev setup and commit conventions in [CONTRIBUTING.md](CONTRIBUTING.md)
- **Missing-mapping guide for app developers**: [docs/missing-mapping-guide.md](docs/missing-mapping-guide.md) — how to add entries / customize mappings while writing your app (beginner-friendly)
- **Adding a language pack**: [docs/contributing-lang-pack.md](docs/contributing-lang-pack.md) (includes the `rzc mapping scaffold` AI translation flow)
- **Third-party crate mappings**: [docs/third-party-mapping.md](docs/third-party-mapping.md)
- **Community mapping registry**: [docs/third-party-registry.md](docs/third-party-registry.md) — upload/download community-translated mappings
- **Translating the tutorial**: start from `tutorials/`, keep the chapter structure aligned
- Dev setup and local checks: see [CONTRIBUTING.md](CONTRIBUTING.md); before submitting, run `make gate` (the full CI test gate).

---

## ⭐ Support

If rzc helps you or sparks ideas, a Star is the best encouragement — and translating the tutorial into your native language is a great way to contribute.

---

## 📄 License

[MIT](LICENSE) © tan80
