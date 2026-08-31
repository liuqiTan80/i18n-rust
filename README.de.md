<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc: Mehrsprachiger Compiler für den Rust-Lehrdialekt

Schreibe Rust-Programme in deiner Muttersprache — rzc übersetzt sie automatisch in Standard-Rust und kompiliert sie. Lerne Programmieren, nicht Englisch.

```rust
// src/main.de — deutscher Rust-Lehrdialekt
funktion hauptfunktion() {
    lass mutabel zahl = 10;
    zahl = zahl + 1;
    druckeZeile!("Zahl: {}", zahl);
}
```

```bash
$ rzc run src/main.de
Zahl: 11
```

## 📦 Installation (aus dem Quellcode bauen)

rzc bietet kein fertiges Online-Installationspaket — bauen Sie es auf Ihrem eigenen Rechner (1–3 Minuten).

### Voraussetzungen

| Werkzeug | Zweck | Erforderlich? |
|---|---|---|
| **Rust-Werkzeugkette** (rustc + cargo) | rzc selbst bauen | ✅ Ja |
| **git** | Quellcode holen | ✅ Ja (oder Quellcode-ZIP) |
| **Netzwerk** | Abhängigkeiten beim ersten Bau laden | ✅ Ja (beim ersten Mal) |
| **Node.js 18+ und npm** | VS-Code-Erweiterung bauen | Optional (nur IDE) |
| **rust-analyzer** | IDE-Backend für Vervollständigung/Diagnose | Optional (nur IDE) |

### 1. Rust-Werkzeugkette installieren

Empfohlen wird [rustup](https://rustup.rs) (installiert rustc, cargo und rustup auf einmal).

- **Linux / macOS** (im Terminal):

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

  Danach `source "$HOME/.cargo/env"` ausführen (oder Terminal neu öffnen).

- **Windows**: [rustup-init.exe](https://rustup.rs) herunterladen und dem Assistenten folgen (oder in PowerShell `winget install --id Rustlang.Rustup`).

Prüfen (nach dem Neuöffnen des Terminals):

```bash
rustc --version   # z. B. rustc 1.98.0
cargo --version
```

### 2. Quellcode holen und bauen

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace
./target/release/rzc --version
```

Der erste Bau lädt Abhängigkeiten und kompiliert alle Komponenten (1–3 Minuten). Binärdateien:

- `target/release/rzc` — das Befehlszeilenwerkzeug
- `target/release/i18n-rust-lsp` — der Sprachserver (Backend der VS-Code-Erweiterung)

### 3. (Optional) rzc global verfügbar machen

```bash
cargo install --path crates/cli   # lokal bauen und in ~/.cargo/bin installieren
```

### 4. (Optional) rust-analyzer für IDE-Funktionen installieren

```bash
rzc install toolchain --ra-only --force
```

### 5. (Optional) VS-Code-Erweiterung bauen

Erfordert Node.js 18+ und npm (von [nodejs.org](https://nodejs.org)):

```bash
cd tools/vscode-extension
npm ci
npm run package    # erzeugt i18n-rust-<Version>.vsix
```

Installieren Sie die `.vsix` über „Install from VSIX..." in VS Code; den Sprachserver stellt `rzc install lsp` bereit.

## 🚀 Schnellstart

```bash
rzc init mein-projekt
cd mein-projekt
rzc run src/main.de
```

`rzc init` erstellt ein vollständig lauffähiges Projektgerüst (`Cargo.toml` + `src/main.de`) — einfach ausführen.

## 🛠️ Wichtige Befehle

| Befehl | Beschreibung |
|--------|--------------|
| `rzc init <name>` | Neues Projekt erstellen |
| `rzc run <datei>` | Dialekt-Quellcode übersetzen und ausführen |
| `rzc check <datei>` | Typprüfung mit lokalisierter Lehrdiagnose |
| `rzc eject <datei>` | Als Standard-Rust-Code exportieren |
| `rzc lang list` | Installierte Sprachpakete auflisten |
| `rzc mapping auto <crate>` | Drittanbieter-Mappings automatisch generieren |

## ✨ Funktionen

- **Programmieren in der Muttersprache**: vollständige Rust-Programme mit Schlüsselwörtern deiner Sprache
- **Mehrsprachig**: 10 eingebaute Sprachpakete (de/zh/ja/ru/es/fr/pt/ko/ar/hi), automatische Erkennung per Dateiendung
- **Lokalisierte Diagnose**: `rzc check` übersetzt rustc-Fehler in die Sprache der Datei, mit 💡 Lehrhinweisen
- **Eigentums-Visualisierung**: VS-Code-Erweiterung (Suche `i18n-rust`) hebt Verschiebungen und Wiederverwendung von Variablen farbig hervor
- **Volle LSP-Unterstützung**: Vervollständigung, Hover, Gehe-zu-Definition, Referenzen, Umbenennen
- **Schrittweiser Übergang**: `rzc eject` exportiert Standard-Rust-Code in einem Schritt

## 📖 Tutorial

Ein vollständiges chinesisches Anfängertutorial (26 Kapitel + Glossar + 5 Anhänge) — siehe [tutorials/](tutorials/).

## 📄 Lizenz

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
