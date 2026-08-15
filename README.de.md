<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc: Mehrsprachiger Rust-Lehrdialekt-Compiler

Schreiben Sie Rust-Programme in Ihrer Muttersprache. rzc übersetzt sie automatisch in Standard-Rust, kompiliert und führt sie aus — Programmierbildung kehrt zum logischen Denken zurück, nicht zur Englisch-Memorierung.

```rust
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

## ✨ Funktionen

- **Muttersprachliche Programmierung**: Schreiben Sie vollständige Rust-Programme mit chinesischen Schlüsselwörtern
- **Mehrsprachig nativ**: Architektur unterstützt jede natürliche Sprache; chinesisches Paket integriert, andere ferninstallierbar
- **Automatische Erweiterungserkennung**: `.zh`, `.ja`, `.ru` werden automatisch dem Sprachpaket zugeordnet
- **Lokalisierte Diagnostik**: rustc-Fehler übersetzt mit 💡 Lehrhinweisen; Eigentumsfehler über JSON visualisiert
- **Eigentumsvisualisierung**: VS Code-Erweiterung hebt Verschiebungen (gelb), Wiederverwendung (rot) und Lebensdauern (grün) hervor
- **Vollständige LSP-Unterstützung**: Vervollständigung, Hover, Definitionssprung, Referenzsuche, Umbenennung
- **Makro-Autovervollständigung**: `!` kann weggelassen werden; wird beim Transpilieren automatisch ergänzt
- **Fließender Übergang**: `eject` exportiert Standard-Rust-Code in einem Schritt
- **Vollständiges Tutorial**: 24 Kapitel + 4 Anhänge, vom absoluten Anfänger bis zum Gesamtprojekt

## 📦 Installation

### Über crates.io (empfohlen)

```bash
cargo install rzc
```

### Aus dem Quellcode

```bash
# China-Spiegel
git clone https://gitcode.com/tan80/zrRust.git
# International
git clone https://github.com/liuqiTan80/i18n-rust.git
cd zrRust oder i18n-rust
cargo build --release --workspace
```

## 🚀 Schnellstart

```bash
rzc init mein-projekt
cd mein-projekt
rzc run src/main.zh
```

## 🛠️ Befehle

| Befehl | Beschreibung |
|--------|-------------|
| `rzc init <Name>` | Neues Projekt erstellen |
| `rzc run <Datei>` | `.zh`-Quellcode übersetzen und ausführen |
| `rzc check <Datei>` | Typprüfung mit Lehrdiagnostik |
| `rzc eject <Datei>` | In Standard-`.rs`-Code exportieren |
| `rzc lang list` | Installierte Sprachpakete auflisten |
| `rzc lang install <Quelle>` | Sprachpaket installieren |
| `rzc lang remove <Code>` | Sprachpaket entfernen |
| `rzc mapping auto <Crate>` | Drittanbieter-Crate-Mappings generieren |

## 📖 Tutorial

Vollständiges Anfänger-Tutorial: 24 Kapitel + 4 Anhänge

| Stufe | Kapitel |
|-------|---------|
| **Grundlagen** | Kap.1 Hallo Welt · Kap.2 Variablen & Typen · Kap.3 Zusammengesetzte Typen · Kap.4 Kontrollfluss · Kap.5 Funktionen & Methoden |
| **Kern** | Kap.6 Eigentümerschaft · Kap.7 Referenzen & Ausleihen · Kap.8 Zeichenketten · Kap.9 Strukturen · Kap.10 Aufzählungen & Musterabgleich |
| **Generika** | Kap.11 Generika · Kap.12 Traits · Kap.13 Lebensdauern · Kap.14 Sammlungen |
| **Fehler & Module** | Kap.15 Fehlerbehandlung · Kap.16 Modulsystem · Kap.17 Paketverwaltung |
| **Fortgeschritten** | Kap.18 Smart Pointer · Kap.19 Nebenläufigkeit · Kap.20 Tests |
| **Experte** | Kap.21 Closures & Iteratoren · Kap.22 Makros · Kap.23 Asynchrone Programmierung |
| **Projekt** | Kap.24 Kommandozeilen-Rechner |
| **Anhänge** | A Mapping-Referenz · B Glossar · C Migrationsleitfaden · D FAQ & Lernpfad |

## ❓ FAQ

**Q: Warum können Makros das Ausrufezeichen weglassen?**
Um den Lernaufwand zu reduzieren. Der Transpiler ergänzt `!` automatisch.

**Q: Kann ich chinesische Variablennamen verwenden?**
Ja. Rust unterstützt Unicode-Bezeichner.

**Q: Wie installiere ich andere Sprachpakete?**
`rzc lang install 日本語` (fern) oder `rzc lang install ./verzeichnis` (lokal).

## 🤝 Mitwirken

Rückmeldung über [GitHub Issues](https://github.com/liuqiTan80/i18n-rust/issues), PRs willkommen.

## 📄 Lizenz

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
