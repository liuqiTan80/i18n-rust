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

## 📦 Installation

Ein Befehl — danach sofort global verfügbar:

```bash
cargo install rzc
```

> Erfordert die [Rust-Werkzeugkette](https://www.rust-lang.org/tools/install) (stable via rustup). Sprachpakete sind eingebaut — keine zusätzliche Konfiguration nötig.

Alternativ aus dem Quellcode bauen:

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace                # Binärdatei: target/release/rzc
```

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
- **Mehrsprachig**: 11 eingebaute Sprachpakete (de/zh/en/ja/ru/es/fr/pt/ko/ar/hi), automatische Erkennung per Dateiendung
- **Lokalisierte Diagnose**: `rzc check` übersetzt rustc-Fehler in die Sprache der Datei, mit 💡 Lehrhinweisen
- **Eigentums-Visualisierung**: VS-Code-Erweiterung (Suche `i18n-rust`) hebt Verschiebungen und Wiederverwendung von Variablen farbig hervor
- **Volle LSP-Unterstützung**: Vervollständigung, Hover, Gehe-zu-Definition, Referenzen, Umbenennen
- **Schrittweiser Übergang**: `rzc eject` exportiert Standard-Rust-Code in einem Schritt

## 📖 Tutorial

Ein vollständiges chinesisches Anfängertutorial (24 Kapitel + 4 Anhänge) — siehe [tutorials/](tutorials/).

## 📄 Lizenz

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
