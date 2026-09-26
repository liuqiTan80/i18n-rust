<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

[![CI](https://github.com/liuqiTan80/i18n-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/liuqiTan80/i18n-rust/actions)
[![crates.io](https://img.shields.io/crates/v/rzc.svg)](https://crates.io/crates/rzc)
[![Docs](https://img.shields.io/badge/Docs-Online%20documentation-blue)](https://liuqiTan80.github.io/i18n-rust/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

> 🪞 **Repository-Spiegel**: Das Projekt wird synchron auf GitHub ([liuqiTan80/i18n-rust](https://github.com/liuqiTan80/i18n-rust)) und GitCode ([tan80/i18n-rust](https://gitcode.com/tan80/i18n-rust)) gepflegt. `rzc lang install` bevorzugt standardmäßig die GitCode-Quelle (schneller aus China) und weicht bei Fehlschlag automatisch auf GitHub aus.

> ⚠️ Diese Übersetzung basiert auf der [chinesischen Version](README.md) und kann ihr gegenüber verzögert sein.

# rzc: Mehrsprachiger Compiler für den Rust-Lehrdialekt

> 🌍 **Schreibe Rust in deiner Muttersprache.** · 10 Sprachen · echtes Rust, echte Toolchain · jederzeit mit `rzc eject` abschließen

**rzc ist ein mehrsprachiger Rust-Dialekt-Compiler**: Du schreibst Code in deiner Muttersprache, rzc übersetzt ihn in Echtzeit in Standard-Rust, die offizielle Toolchain kompiliert und führt ihn aus — und jede Diagnosemeldung kommt in deiner Sprache mit Lernhinweisen zurück. Lerne Programmieren, nicht Englisch.

- 🌍 **Kein Pseudocode**: Muttersprachlicher Code ist vollständig isomorph zu Standard-Rust — Kompilierung, Ausführung, Abhängigkeiten und Ökosystem sind zu 100 % echt
- 🎓 **Für Einsteiger**: Fehlermeldungen werden zu „Was als Nächstes zu tun ist“-Hinweisen statt zu einer Wand aus Englisch
- 🚪 **Jederzeit abschließen**: `rzc eject` exportiert Standard-Rust in einem Schritt — kein Lock-in, voll kompatibel mit dem Mainstream-Ökosystem

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

Fehler passieren — die Meldungen bleiben trotzdem in deiner Sprache:

```
Fehler[E0384]: kann der unveränderlichen Variablen `zahl` nicht zweimal zuweisen
  --> src/main.de:3:5
💡 Wenn Sie den Wert einer Variablen ändern möchten, deklarieren Sie sie mit `lass mutabel`.
```

Dasselbe Programm läuft in allen 10 eingebauten Dialekten — z. B. auf Chinesisch (`函数 主函数()`, `打印行!`) oder Japanisch (`関数 主関数()`, `表示行!`). Standard-Rust (`fn main()`) wird immer unverändert akzeptiert.

**Hier anfangen**: 🚀 [Schnellstart](#schnellstart) · 🌐 [Online-Dokumentation](https://liuqiTan80.github.io/i18n-rust/) (4 Sprachen) · 📚 [Feishu KB](https://my.feishu.cn/wiki/space/7689728327082314704) (chinesisches Tutorial, ohne Anmeldung) · 📖 [Schritt für Schritt lernen](#tutorial) · 🤝 [Mitwirken](#mitwirken)

---

## 📦 Installation

Zwei Optionen: **Option 1 — Installation über crates.io** (empfohlen, für die meisten Nutzer); **Option 2 — aus dem Quellcode bauen** (neuester Entwicklungsstand oder lokale Builds nach Änderungen an rzc).

**Option 1: Installation über crates.io (empfohlen)** — auf crates.io veröffentlicht; mit installierter Rust-Toolchain genügt ein Befehl:

```bash
cargo install rzc        # von crates.io holen und lokal bauen (1–3 Minuten)
rzc --version            # Versionsnummer = Erfolg
rzc init mein-projekt && cd mein-projekt && rzc run src/main.de
```

> Sprachpakete sind eingebaut — keine zusätzliche Konfiguration. Crate-Seite: <https://crates.io/crates/rzc>; Upgrade: `cargo install rzc --force`. Noch keine Rust-Toolchain? Zuerst „1.“ unten.

**Option 2: Aus dem Quellcode bauen (Entwickler)** — für den neuesten Entwicklungsstand oder lokale Builds nach Änderungen an rzc. **Kürzester Weg** (mit installierter Rust-Toolchain — 4 Befehle bis zum ersten Programm):

```bash
git clone https://github.com/liuqiTan80/i18n-rust && cd i18n-rust
cargo build --release --workspace   # ca. 1–3 Minuten
cargo install --path crates/cli     # macht rzc global verfügbar (alternativ ./target/release/rzc direkt nutzen)
rzc init mein-projekt && cd mein-projekt && rzc run src/main.de
```

Die vollständige Schritt-für-Schritt-Anleitung für Option 2 steht unten (Voraussetzungen → Build → Prüfung); „1.“ gilt für beide Optionen.

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

Der offizielle standalone rust-analyzer wird nach `~/.rz/toolchain` installiert — rzc und der Sprachserver bevorzugen ihn automatisch.

### 5. (Optional) VS-Code-Erweiterung bauen

Erfordert Node.js 18+ und npm (von [nodejs.org](https://nodejs.org)):

```bash
cd tools/vscode-extension
npm ci
npm run package    # erzeugt i18n-rust-<Version>.vsix
```

Installieren Sie die `.vsix` über „Install from VSIX...“ in VS Code — für Syntaxhervorhebung, Vervollständigung, Diagnosen, Hover und Eigentums-Visualisierung; den Sprachserver stellt `rzc install lsp` bereit (findet die eingebaute Toolchain automatisch).

### Vollständige Funktionsausstattung (je ein Befehl)

```bash
rzc install lsp          # Sprachserver (Backend für Vervollständigung/Diagnose/Hover in VS Code)
rzc install toolchain    # eingebaute offizielle Toolchain (standalone rustc/cargo/rust-analyzer nach ~/.rz/toolchain)
rzc doctor               # Zustand der Toolchain-Umgebung (eingebaut / PATH / Versionsvergleich)
```

Nach der Installation bevorzugen rzc und der Sprachserver automatisch die eingebaute Toolchain; Einzeldatei-Projekte rufen rustc direkt auf, ohne cargo-Index.

### Umgebungsvariablen (optional, meist nicht nötig)

| Variable | Zweck |
|---|---|
| `RZ_LANG_DIR` | Verzeichnis der Sprachpakete (Standard: die eingebauten) |
| `RUST_ANALYZER_PATH` | Pfad zu rust-analyzer (wenn die automatische Erkennung fehlschlägt) |

Die VS-Code-Einstellung `i18n-rust.serverPath` gibt den LSP-Binärpfad explizit an (falls die automatische Erkennung fehlschlägt).

### Einzelne Komponenten der Toolchain aktualisieren

| Szenario | Befehl |
|---|---|
| rustc/cargo aktualisieren (z. B. 1.98 → 1.99) | `rzc install toolchain --version 1.99.0 --force` |
| Nur rust-analyzer aktualisieren (ohne 300-MB-Neudownload) | `rzc install toolchain --ra-tag <Datums-Tag> --ra-only --force` |
| Aktuelle Versionen und Status anzeigen | `rzc doctor` |

### Editor wechseln (VSCodium / Cursor und andere VS-Code-Familie)

Die i18n-rust-Erweiterung (.vsix) ist mit allen VS-Code-basierten Editoren kompatibel; rzc, Sprachserver und Toolchain sind editorunabhängig:

1. Im neuen Editor: Erweiterungen → „Install from VSIX“ → `i18n-rust-<Version>.vsix` wählen;
2. Komponenten an die Standardorte anbinden (mit dem gebauten rzc):
   ```bash
   rzc install lsp        # Sprachserver → ~/.cargo/bin
   rzc install toolchain  # eingebaute Toolchain → ~/.rz/toolchain (online; oder aus dem Offline-Paket eines Distributors kopieren)
   ```
3. Eine `.de`-Datei öffnen — fertig (die Erweiterung findet Server und Toolchain automatisch; `RUST_ANALYZER_PATH` oder die Einstellung `i18n-rust.serverPath` können überschreiben).

### Für Distributoren: Offline-Release-Paket (Verteilung für Schulungsräume)

Ein Ein-Kommando-Paketierskript erzeugt ein plattformspezifisches Release-Paket (für Releases / Dateifreigaben):

| Plattform | Befehl | Artefakt |
|---|---|---|
| Linux / macOS | `./release-offline.sh` | `release/rzc-<Version>-linux/macos-<Architektur>.tar.gz` |
| Windows (PowerShell) | `.\release-offline.ps1` | `release/rzc-<Version>-windows-x86_64.zip` |

Voraussetzung: ein Release-Build (Schritt 2 oben) und ein Online-Lauf von `rzc install toolchain --ra-only --force` (das Skript kopiert den rust-analyzer der Plattform ins Paket).

Das Skript erkennt die Plattform automatisch (Linux / Darwin / Windows); das Paket enthält rzc, i18n-rust-lsp, rust-analyzer, alle 11 Sprachpakete (10 natürliche Sprachen + `en`-Identitätspaket) und das Tutorial — auspacken und loslegen (Details in Tutorial-Anhang D.5).

## 🚀Schnellstart

```bash
rzc init mein-projekt        # erstellt ein lauffähiges Projektgerüst (Cargo.toml + src/main.de)
cd mein-projekt
rzc run src/main.de          # übersetzen → kompilieren → ausführen
```

In drei Schritten läuft dein erstes Muttersprach-Rust-Programm.

---

## 🛠️ Wichtige Befehle

| Befehl | Beschreibung |
|------------------------------------|------------------------------------------------------|
| `rzc init <Name>` | Neues Projekt erstellen (Toolchain-Version auf die lokale fixiert; IDE-bereit) |
| `rzc run <Datei>` | Übersetzen und ausführen; Warnungen/Fehler/Build-Fortschritt komplett in deiner Sprache |
| `rzc check <Datei>` | Typprüfung mit lokalisierter Lehrdiagnose |
| `rzc eject <Datei>` | Als Standard-Rust-Code exportieren (schrittweiser Übergang) |
| `rzc transpile <Datei>` | Nur übersetzen — Standard-Rust auf stdout |
| `rzc cheat <Sprache>` | Muttersprache ↔ Rust-Spickzettel (`--markdown` zum Einbetten) |
| `rzc add <Crate>[@Version]` | Abhängigkeit hinzufügen (wrappt `cargo add`, mit Hinweisen zu Muttersprach-Mappings) |
| `rzc doctor` | Toolchain-Umgebung diagnostizieren (eingebaut / PATH / Versionsvergleich) |
| `rzc lang list` | Installierte Sprachpakete auflisten |
| `rzc lang install <Code/Verzeichnis>` | Sprachpaket installieren (Remote-Registry oder lokales Verzeichnis) |
| `rzc lang search [Stichwort]` | Installierbare Remote-Sprachpakete durchsuchen |
| `rzc lang remove <Code>` | Benutzerinstalliertes Sprachpaket entfernen |
| `rzc mapping auto <Crate> [--target-version <Version>]` | Muttersprach-Mappings für eine Drittanbieter-Crate automatisch generieren (AI/Regeln); Basisversion der Generierung fixieren |
| `rzc mapping check [Ziel]` | Mapping-Qualität prüfen (doppelte Schlüssel / Schlüsselwort-Kollisionen / Dateikonflikte / Pflichtabschnitte) |
| `rzc mapping coverage [--lang <Sprache>]` | Sprachpaket-Abdeckung an echtem Quellcode messen; fehlende Mappings auflisten (im Repo-Root ausführen) |
| `rzc mapping scaffold <Quelle> <Ziel>` | Übersetzungsgerüst für eine neue Sprache erzeugen; `--provider deepseek` für AI-Übersetzung |
| `rzc crate search [Stichwort]` | Von der Community geteilte Drittanbieter-Mappings durchsuchen (Registry) |
| `rzc crate install <Crate> --lang <Sprache>` | Einzelnes Drittanbieter-Mapping aus der Registry ins globale Sprachpaket installieren |
| `rzc crate list` | Installierte Community-Mappings auflisten |
| `rzc crate remove <Crate> --lang <Sprache>` | Installiertes Community-Mapping entfernen |
| `rzc crate update` | Alle installierten Mappings laut Manifest neu laden (Updates holen) |
| `rzc crate publish <Crate> --lang <Sprache>` | Lokale Mappings in die Registry veröffentlichen (zuvor Qualitätsgate passieren) |
| `rzc install <lsp\|toolchain>` | Begleitkomponenten installieren (Sprachserver / eingebaute offizielle Toolchain) |

Vollständige Referenz: [Anhang D: rzc-Befehlsspickzettel (Chinesisch)](tutorials/附录D：rzc命令速查.md).

---

## ✨ Funktionen

### Programmieren in der Muttersprache

Schreibe vollständige Programme mit deutschen Schlüsselwörtern (`funktion`, `lass`, `wenn`, `abgleich`…) und Muttersprach-Standardbibliothek (`zeichenkette`, `vektor::neu()`, `nutze standard::sammlungen::hashmap`). Makros, Lebenszeiten, Generics und Traits werden vollständig unterstützt.

### 10 eingebaute Sprachen

| Sprache | Endung | Sprache   | Endung |
|---------|--------|-----------|--------|
| 中文    | `.zh` | Español   | `.es`  |
| Deutsch | `.de` | Français  | `.fr`  |
| 日本語  | `.ja` | Português | `.pt`  |
| 한국어  | `.ko` | العربية   | `.ar`  |
| Русский | `.ru` | हिन्दी    | `.hi`  |

Rust selbst ist in Englisch geschrieben, deshalb ist Englisch kein Lehrdialekt (eine Identitätszuordnung hat keinen Lehrwert); im Sprachpaketverzeichnis gibt es ein separates `en`-Identitätspaket (Endung `.en`) für Identitätsmapping-Szenarien und englische UI-Texte. Die 10 natürlichen Sprachen oben werden automatisch per Dateiendung erkannt und können in einem Projekt gemischt werden.

### Lehrdiagnose

- **Doppelte Übersetzung: Fehlercodes + Meldungen**: deckt rustc-Fehlercodes, codelose lint-Warnungen und help-Phrasen ab
- **Lokalisierte Typnamen**: `std::fmt::Display` → `standard::formatierung::anzeigbar`
- **💡 Lehrhinweise**: zu jedem Fehler ein „Was als Nächstes“-Hinweis; Eigentumsfehler mit 📌 Move/Borrow-Narrativ
- **Abhängigkeitshilfe**: erkennt nicht deklarierte Drittanbieter-Crates und schlägt `rzc add <Crate>` vor

### Vollständige IDE-Erfahrung (VS Code / Qoder-Erweiterung)

Syntaxhervorhebung, intelligente Vervollständigung, Hover-Dokumentation, Gehe-zu-Definition, Referenzsuche, Umbenennen, Code-Formatierung, Ein-Klick-Ausführen/Prüfen, automatische Umwandlung von Vollbreiten-Satzzeichen, KI-gestützte Übersetzung.

### Drittanbieter-Crates in deiner Sprache

`rzc mapping auto` extrahiert öffentliche APIs installierter Crates und generiert Muttersprach-Namen (AI); Produktions-Mappings fixieren ihre Basisversion mit `--target-version` (im Dateikopf vermerkt); Community-Mappings passieren das Qualitätsgate `rzc mapping check`.

### Warum das kein „Spielzeug“ ist

| | Typische „Muttersprach“-Spielzeuge | rzc |
|---|---|---|
| Codeform | Erfundene Syntax oder Pseudocode | Vollständig isomorph zu Standard-Rust — nur Identifikatoren lokalisiert |
| Kompilieren & Ausführen | Eigener Interpreter / nur transpilieren | Offizielles rustc/cargo — echtes Kompilieren und Ausführen |
| Ökosystem | Geschlossen oder beschnitten | Ganzes crates.io (`rzc add` + Muttersprach-Mappings) |
| Fehlererlebnis | Englisch pur oder Eigenbau-Hinweise | Übersetzte Meldungen + Fehlercodes + Lehrhinweise |
| Ausstiegskosten | Neu schreiben | `rzc eject` exportiert in einem Schritt Standard-Rust |

---

## 📖Tutorial

Ein vollständiges chinesisches Anfängertutorial: **26 Kapitel + Glossar + 5 Anhänge** — siehe [tutorials/](tutorials/). Von „Hallo, Welt“ über Ownership, Closures, Async und Makros bis zum großen Praxisprojekt; alle Beispiele sind in chinesischem Rust geschrieben.

🌐 **Online lesen**: [Online-Dokumentation](https://liuqiTan80.github.io/i18n-rust/) (mdBook, 4 Sprachen mit Umschalter in der oberen Leiste; lokale Vorschau mit `make site-serve`).

**Übersetzungen in Arbeit**: Englisch (Vorwort, Kapitel 1–13, Anhang D, 15/33), Japanisch (Kapitel 1–11, 11/33), Russisch (Kapitel 1–15, 15/33) — Fortschritt und nächste Charge in [translation-status.md](docs/translation-status.md). Übersetzungen ins Deutsche sind willkommen — siehe „Mitwirken“ unten.

> Tutorial-Qualität wird von CI überwacht ([tools/verify-tutorials.py](tools/verify-tutorials.py)): jeder Codeblock muss kompilieren, Fehlerbeispiele müssen den annotierten erwarteten Fehlercode ausgeben (`// 预期错误: EXXXX`). Lokale Prüfung nach Änderungen: `make tutorials` (Chinesisch) oder `make tutorials-all` (Englisch/Japanisch/Russisch).

> FAQ und Lern-Roadmap: [Anhang E (Chinesisch)](tutorials/附录E：常见问题、迁移指南与学习路线.md).

---

## 🏗️ Projektstruktur und Funktionsweise

```text
Muttersprachlicher Quellcode (.de)
   │  Lexer-Übersetzung → Modulpfad-Ersetzung → Alias-Ersetzung        ← engine (sprachunabhängig)
   ▼
Standard-Rust-Quellcode
   │  cargo build / run (offizielle Toolchain)
   ▼
JSON-Diagnosen → Fehlercode-/Meldungsübersetzung + Typ-Lokalisierung + Lehrhinweise → Ausgabe in der Muttersprache
```

| Verzeichnis                | Zuständigkeit                                                                   |
|----------------------------|---------------------------------------------------------------------------------|
| `crates/engine`            | Sprachunabhängige Kern-Engine: Übersetzungspipeline, Mapping-Verwaltung, Diagnoseübersetzung, Inkrement-Cache, Unicode-Sicherheitsprüfungen |
| `crates/cli`               | das `rzc`-Befehlszeilenwerkzeug |
| `crates/lsp`               | `i18n-rust-lsp`: proxyt den offiziellen Sprachserver (rust-analyzer), übersetzt Positionen und Diagnosen bidirektional |
| `crates/engine/lang-packs` | 11 eingebaute Sprachpakete (10 natürliche Sprachen + `en`-Identitätspaket: Schlüsselwörter / Standardbibliothek / Modulpfade / Fehlerübersetzungen / UI-Texte) |
| `tools/vscode-extension`   | VS Code / Qoder-Erweiterung |
| `tools`                    | Gates und Build-Skripte: Tutorial-Verifikation, Benchmark-Regression, Doku-Site-Aufbau ([tools/README.md](tools/README.md)) |
| `tutorials`                | 26 Kapitel chinesisches Tutorial mit Anhängen (en/ja/ru-Übersetzungen in Arbeit) |
| `book`                     | Aufbauquelle der Doku-Site (mdBook, [book/README.md](book/README.md)) |
| `docs`                     | Referenz- und Entwicklerdokumentation, [Projektkarte](docs/project-map.md); Betrieb & Roadmap: [docs/strategy/](docs/strategy/README.md) |

**Designprinzip**: Die Engine verdrahtet keine konkrete Sprache fest — eine natürliche Sprache hinzufügen = ein Sprachpaketverzeichnis hinzufügen (wird beim Build automatisch eingebettet). Wie man dieses Paradigma auf andere Programmiersprachen überträgt (z. B. chinesisches Python): siehe [Dialekt-Framework-Blaupause](docs/dialect-framework-blueprint.md).

---

## 🤝Mitwirken

- **Zum ersten Mal hier?** Starte mit der [Projektkarte (Maintainer-Handbuch)](docs/project-map.md) — „wo ändere ich X und wie verifiziere ich es“; Entwicklungsumgebung und Commit-Konventionen: [CONTRIBUTING.md](CONTRIBUTING.md)
- **Leitfaden für fehlende Wörter (App-Entwickler)**: [docs/missing-mapping-guide.md](docs/missing-mapping-guide.md) (Wörter ergänzen / Mappings anpassen beim Schreiben einer App — für Einsteiger)
- **Sprachpaket hinzufügen**: [docs/contributing-lang-pack.md](docs/contributing-lang-pack.md) (inkl. AI-Übersetzungsflow mit `rzc mapping scaffold`)
- **Drittanbieter-Crate-Mappings**: [docs/third-party-mapping.md](docs/third-party-mapping.md)
- **Community-Registry**: [docs/third-party-registry.md](docs/third-party-registry.md) (eigene Mappings hoch- und herunterladen)
- **Tutorial übersetzen**: Quelle ist `tutorials/`, Kapitelstruktur beibehalten; Fortschrittsübersicht: [translation-status.md](docs/translation-status.md)
- Vor dem Einreichen muss `make gate` vollständig grün sein (entspricht dem CI-Testgate)

---

## ⭐ Projekt unterstützen

Wenn rzc dir hilft oder dich inspiriert, freuen wir uns über einen Star; und das Tutorial in deine Muttersprache zu übersetzen ist die beste Unterstützung.

---

## 📄 Lizenz

[MIT](LICENSE) © tan80
