<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

> ⚠️ Cette traduction peut être obsolète. Reportez-vous à la [version chinoise](README.md) ou à la [version anglaise](README.en.md) pour les informations les plus récentes.

# rzc : Compilateur multilingue du dialecte pédagogique Rust

> 🌍 **Écris Rust dans ta langue maternelle.** · 10 langues · du vrai Rust, une vraie toolchain · obtiens ton diplôme quand tu veux avec `rzc eject`

Écrivez des programmes Rust dans votre langue maternelle : rzc les traduit automatiquement en Rust standard et les compile. Apprenez la programmation, pas l'anglais.

```rust
// src/main.fr — dialecte pédagogique Rust en français
fonction principale() {
    laisser mutable nombre = 10;
    nombre = nombre + 1;
    afficher_ligne!("Nombre : {}", nombre);
}
```

```bash
$ rzc run src/main.fr
Nombre : 11
```

## 📦 Installation (compilation depuis les sources)

rzc ne fournit pas d'installeur précompilé en ligne — compilez-le sur votre machine (1 à 3 minutes).

### Prérequis

| Outil | Rôle | Requis ? |
|---|---|---|
| **Toolchain Rust** (rustc + cargo) | Compiler rzc lui-même | ✅ Oui |
| **git** | Récupérer les sources | ✅ Oui (ou télécharger le ZIP) |
| **Réseau** | Télécharger les dépendances à la première compilation | ✅ Oui (première fois) |
| **Node.js 18+ et npm** | Compiler l'extension VS Code | Optionnel (IDE uniquement) |
| **rust-analyzer** | Backend de complétion/diagnostics de l'IDE | Optionnel (IDE uniquement) |

### 1. Installer la toolchain Rust

La méthode recommandée est [rustup](https://rustup.rs) (installe rustc, cargo et rustup en une fois).

- **Linux / macOS** (dans un terminal) :

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

  Ensuite exécutez `source "$HOME/.cargo/env"` (ou rouvrez le terminal).

- **Windows** : téléchargez [rustup-init.exe](https://rustup.rs) et suivez l'assistant (ou exécutez `winget install --id Rustlang.Rustup` dans PowerShell).

Vérification (après avoir rouvert le terminal) :

```bash
rustc --version   # ex. rustc 1.98.0
cargo --version
```

### 2. Récupérer les sources et compiler

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace
./target/release/rzc --version
```

La première compilation télécharge les dépendances et compile tous les composants (1 à 3 minutes). Binaires :

- `target/release/rzc` — l'outil en ligne de commande
- `target/release/i18n-rust-lsp` — le serveur de langage (backend de l'extension VS Code)

### 3. (Optionnel) Rendre rzc disponible globalement

```bash
cargo install --path crates/cli   # compile localement et installe dans ~/.cargo/bin
```

### 4. (Optionnel) Installer rust-analyzer pour les fonctions IDE

```bash
rzc install toolchain --ra-only --force
```

### 5. (Optionnel) Compiler l'extension VS Code

Nécessite Node.js 18+ et npm (de [nodejs.org](https://nodejs.org)) :

```bash
cd tools/vscode-extension
npm ci
npm run package    # génère i18n-rust-<version>.vsix
```

Installez le `.vsix` via « Install from VSIX... » dans VS Code ; le serveur de langage est fourni par `rzc install lsp`.

## 🚀 Démarrage rapide

```bash
rzc init mon-projet
cd mon-projet
rzc run src/main.fr
```

`rzc init` crée un squelette de projet exécutable (`Cargo.toml` + `src/main.fr`) — lancez-le directement.

## 🛠️ Commandes

| Commande | Description |
|----------|-------------|
| `rzc init <nom>` | Créer un nouveau projet |
| `rzc run <fichier>` | Traduire et exécuter le code source du dialecte |
| `rzc check <fichier>` | Vérification de types avec diagnostic pédagogique localisé |
| `rzc eject <fichier>` | Exporter en code Rust standard |
| `rzc lang list` | Lister les packs de langue installés |
| `rzc mapping auto <crate>` | Générer automatiquement les mappages tiers |

## ✨ Fonctionnalités

- **Programmation en langue maternelle** : écrivez des programmes Rust complets avec les mots-clés de votre langue
- **Multilingue par conception** : 10 packs de langue intégrés (fr/zh/de/ja/ru/es/pt/ko/ar/hi), détection automatique par extension
- **Diagnostics localisés** : `rzc check` traduit les erreurs rustc dans la langue du fichier, avec 💡 conseils pédagogiques
- **Visualisation de la propriété** : l'extension VS Code (recherchez `i18n-rust`) surligne les déplacements et réutilisations de variables
- **Support LSP complet** : complétion, survol, aller à la définition, références, renommage
- **Transition progressive** : `rzc eject` exporte le code Rust standard en une étape

## 📖 Tutoriel

Un tutoriel complet en chinois pour débutants (26 chapitres + glossaire + 5 annexes) — voir [tutorials/](tutorials/).

## 📄 Licence

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
