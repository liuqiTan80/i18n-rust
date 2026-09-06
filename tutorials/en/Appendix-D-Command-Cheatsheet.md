# Appendix D: The rzc Command Cheat Sheet

For when a command slips your mind. Deeper explanations live in Chapter 1 (rzc basics) and Chapter 2 (the VS Code extension).

---

## D.1 The rzc command line

| Command | What it does | Example |
|---|---|---|
| `rzc init name` | Create a new project in your language (generates Cargo.toml + the main source file) | `rzc init my-project` |
| `rzc run file.<ext>` | Translate + compile + run | `rzc run src/main.<ext>` |
| `rzc check file.<ext>` | Check only, no executable produced (faster) | `rzc check src/main.<ext>` |
| `rzc eject file.<ext>` | Export as standard Rust (generates a .rs file) | `rzc eject src/main.<ext>` |
| `rzc transpile file.<ext>` | Translate only — prints the standard Rust to the screen | `rzc transpile src/main.<ext>` |
| `rzc cheat <lang>` | Native ↔ Rust mapping cheat sheet (`--markdown` for pasting into docs) | `rzc cheat zh --markdown` |
| `rzc lang list` | List available language packs | |
| `rzc lang install <lang>` | Install a language pack | `rzc lang install ja` |
| `rzc add <crate>` | Add a third-party dependency (wraps cargo add, with native-mapping hints) | `rzc add rand` |
| `rzc mapping auto <crate>` | Generate a mapping file for a crate automatically (AI or rule mode) | `rzc mapping auto rand --provider rule` |
| `rzc mapping check` | Validate third-party mapping files | |
| `rzc mapping coverage` | Check a pack's coverage against real backend source code, listing missing native mappings (run from the i18n-rust repo root) | `rzc mapping coverage --lang zh` |
| `rzc crate search <keyword>` | Search the community registry for shared crate mappings | `rzc crate search serde` |
| `rzc crate install <crate>@<lang>` | Install a community mapping into your global language pack | |
| `rzc crate publish` | Publish your local mappings to the registry (quality-gated) | |
| `rzc install lsp` | Install the language server (VS Code completions) | |
| `rzc install toolchain` | One-command bundled official toolchain (standalone rustc/cargo/rust-analyzer, no rustup needed) | |
| `rzc doctor` | Diagnose the toolchain environment (bundled / PATH / version comparison) | |
| `rzc --version` | Show the version | |

### The daily loop

```bash
rzc init notebook && cd notebook    # create a project
rzc run src/main.<ext>              # write a little, run a little
rzc check src/main.<ext>            # quick checks while editing
rzc eject src/main.<ext> && cargo test   # run tests (Chapter 21)
cargo build --release               # optimized build for release (Chapter 18)
```

---

## D.2 Everyday cargo commands

| Command | What it does |
|---|---|
| `cargo build` | Build (debug) |
| `cargo build --release` | Build (optimized, for shipping) |
| `cargo test` | Run all tests |
| `cargo add <crate>` | Add a third-party dependency |
| `cargo clean` | Wipe the target directory |

---

## D.3 VS Code extension commands

Open the command palette (`Ctrl+Shift+P`) and type "i18n" or "rzc":

| Command | What it does | Shortcut |
|---|---|---|
| Run current file (run) | Compile and run | `Ctrl+Shift+R` |
| Check current file (check) | Check only | `Ctrl+Shift+C` |
| Export standard Rust (eject) | Convert to .rs | |
| Select language pack (selectLanguagePack) | Switch between 10 languages | |
| Restart language server (restartServer) | When the language server acts up | |
| AI chat (aiChat) | AI assistance (needs configuration) | |
| Mapping check (mappingCheck) | Validate a mapping file | |
| Mapping scaffold (mappingScaffold) | Template for a new language | |
| Install language pack (langInstall) | Install a community language pack | |
| Add dependency (cargoAdd) | Add a dependency graphically | |

Right-clicking a `.<ext>` file in the editor also shows the run/check/eject menu. The extension's 20 snippets are listed in Chapter 2, section 2.5.

---

## D.4 Emergency troubleshooting, three moves

1. **An error you don't understand** → Appendix C, *A Dictionary of Common Error Messages*;
2. **Suspect a mapping collision** → `rzc eject` and read the translated English code;
3. **The language server hangs** → run "Restart language server" from the command palette, or restart VS Code.

---

## D.5 Building your own offline package

Official offline packages cover Windows / Linux / macOS (uploaded manually by the maintainers). If you need a build for a **specific platform or with specific changes** — for a school computer lab, say — you can package it yourself on that platform:

1. **Get the source**: `git clone https://github.com/liuqiTan80/i18n-rust`
2. **Install the Rust toolchain** (rustup), then build:
   ```bash
   cargo build --release -p rzc
   ```
3. **Download rust-analyzer for your platform** (the packaging script copies it from `~/.rz/toolchain/bin`):
   ```bash
   ./target/release/rzc install toolchain --ra-only --force
   ```
4. **Run the packaging script** (auto-detects the platform, no arguments needed):
   - Linux / macOS: `./release-offline.sh`
   - Windows (PowerShell): `.\release-offline.ps1`
5. **The artifact** lands in `release/` (e.g. `rzc-0.6.3-windows-x86_64.zip`) — rzc, the language server, rust-analyzer, all 10 language packs and the tutorials, ready to unpack and use. Upload it to a Release page or a file host to distribute.

> 💡 The script targets the platform it runs on. Cross-compiling (building Windows packages on Linux) needs extra target toolchains and isn't recommended — just run the script on the target platform.
