<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

[![CI](https://github.com/liuqiTan80/i18n-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/liuqiTan80/i18n-rust/actions)
[![crates.io](https://img.shields.io/crates/v/rzc.svg)](https://crates.io/crates/rzc)
[![Docs](https://img.shields.io/badge/Docs-Online%20documentation-blue)](https://liuqiTan80.github.io/i18n-rust/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

> 🪞 **Miroir du dépôt** : le projet est maintenu en synchronisation sur GitHub ([liuqiTan80/i18n-rust](https://github.com/liuqiTan80/i18n-rust)) et GitCode ([tan80/i18n-rust](https://gitcode.com/tan80/i18n-rust)). `rzc lang install` privilégie par défaut la source GitCode (plus rapide depuis la Chine) et bascule automatiquement vers GitHub en cas d'échec.

> ⚠️ Cette traduction est basée sur la [version chinoise](README.md) et peut être en retard par rapport à celle-ci.

# rzc : Compilateur multilingue du dialecte pédagogique Rust

> 🌍 **Écris Rust dans ta langue maternelle.** · 10 langues · du vrai Rust, une vraie toolchain · obtiens ton diplôme quand tu veux avec `rzc eject`

**rzc est un compilateur multilingue de dialectes Rust** : vous écrivez du code dans votre langue maternelle, rzc le traduit en temps réel en Rust standard, la toolchain officielle le compile et l'exécute, et chaque diagnostic revient traduit dans votre langue avec des conseils pédagogiques. Apprenez la programmation, pas l'anglais.

- 🌍 **Pas du pseudo-code** : le code en langue maternelle est entièrement isomorphe au Rust standard — compilation, exécution, dépendances et écosystème sont 100 % réels
- 🎓 **Pour les débutants** : les messages d'erreur deviennent des conseils « quoi faire ensuite » au lieu d'un mur d'anglais
- 🚪 **Diplômé quand vous voulez** : `rzc eject` exporte le Rust standard en une étape — aucun verrouillage, pleinement compatible avec l'écosystème

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

Une erreur ? Pas grave — les messages sont aussi dans votre langue :

```
Erreur[E0384]: Impossible d'assigner deux fois la variable immuable `nombre`
  --> src/main.fr:3:5
💡 Si vous devez modifier la valeur d'une variable, déclarez-la avec `laisser mutable`.
```

Le même programme fonctionne dans les 10 dialectes intégrés — par exemple en chinois (`函数 主函数()`, `打印行!`) ou en japonais (`関数 主関数()`, `表示行!`). Le Rust standard (`fn main()`) est toujours accepté tel quel.

**Commencez ici** : 🚀 [Démarrage rapide](#démarrage-rapide) · 🌐 [Documentation en ligne](https://liuqiTan80.github.io/i18n-rust/) (4 langues) · 📚 [Feishu KB](https://my.feishu.cn/wiki/space/7689728327082314704) (tutoriel en chinois, sans connexion) · 📖 [Apprendre pas à pas](#tutoriel) · 🤝 [Contribuer](#contribuer)

---

## 📦 Installation

Deux options au choix : **Option 1 — installation via crates.io** (recommandée, pour la plupart des utilisateurs) ; **Option 2 — compilation depuis les sources** (version de développement la plus récente ou compilation locale après modification de rzc).

**Option 1 : installation via crates.io (recommandée)** — publié sur crates.io ; avec une toolchain Rust installée, une seule commande suffit :

```bash
cargo install rzc        # récupère depuis crates.io et compile localement (1 à 3 minutes)
rzc --version            # un numéro de version = succès
rzc init mon-projet && cd mon-projet && rzc run src/main.fr
```

> Les packs de langue sont intégrés — aucune configuration supplémentaire. Page du crate : <https://crates.io/crates/rzc> ; mise à jour : `cargo install rzc --force`. Pas encore de toolchain Rust ? Voir « 1. » ci-dessous.

**Option 2 : compilation depuis les sources (développeurs)** — pour la dernière version de développement ou compiler après avoir modifié rzc. **Chemin le plus court** (avec une toolchain Rust installée — 4 commandes jusqu'au premier programme) :

```bash
git clone https://github.com/liuqiTan80/i18n-rust && cd i18n-rust
cargo build --release --workspace   # environ 1 à 3 minutes
cargo install --path crates/cli     # rend rzc global (ou utilisez ./target/release/rzc directement)
rzc init mon-projet && cd mon-projet && rzc run src/main.fr
```

La procédure complète de l'option 2 est ci-dessous (prérequis → compilation → vérification) ; le « 1. » est commun aux deux options.

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

Le rust-analyzer standalone officiel est installé dans `~/.rz/toolchain` ; rzc et le serveur de langage le préfèrent automatiquement.

### 5. (Optionnel) Compiler l'extension VS Code

Nécessite Node.js 18+ et npm (de [nodejs.org](https://nodejs.org)) :

```bash
cd tools/vscode-extension
npm ci
npm run package    # génère i18n-rust-<version>.vsix
```

Installez le `.vsix` via « Install from VSIX... » dans VS Code — vous obtenez coloration syntaxique, complétion, diagnostics, survol et visualisation de la propriété ; le serveur de langage est fourni par `rzc install lsp` (il localise la toolchain intégrée automatiquement).

### Configuration complète (une commande par composant)

```bash
rzc install lsp          # serveur de langage (backend complétion/diagnostics/survol de VS Code)
rzc install toolchain    # toolchain officielle intégrée (rustc/cargo/rust-analyzer standalone dans ~/.rz/toolchain)
rzc doctor               # état de l'environnement (intégré / PATH / comparaison de versions)
```

Après l'installation, rzc et le serveur de langage préfèrent automatiquement la toolchain intégrée ; les projets mono-fichier appellent rustc directement, sans index cargo.

### Variables d'environnement (optionnelles, généralement inutiles)

| Variable | Rôle |
|---|---|
| `RZ_LANG_DIR` | Répertoire des packs de langue (par défaut : les packs intégrés) |
| `RUST_ANALYZER_PATH` | Chemin vers rust-analyzer (si la détection automatique échoue) |

Le réglage VS Code `i18n-rust.serverPath` indique explicitement le chemin du binaire LSP (utilisé si la détection automatique échoue).

### Mettre à jour un composant de la toolchain

| Scénario | Commande |
|---|---|
| Mettre à jour rustc/cargo (ex. 1.98 → 1.99) | `rzc install toolchain --version 1.99.0 --force` |
| Mettre à jour uniquement rust-analyzer (sans re-télécharger 300 Mo) | `rzc install toolchain --ra-tag <tag-de-date> --ra-only --force` |
| Afficher les versions et l'état actuels | `rzc doctor` |

### Changer d'éditeur (VSCodium / Cursor et autres de la famille VS Code)

L'extension i18n-rust (.vsix) est compatible avec tous les éditeurs basés sur VS Code ; rzc, le serveur de langage et la toolchain sont indépendants de l'éditeur :

1. Dans le nouvel éditeur : Extensions → « Install from VSIX » → choisissez `i18n-rust-<version>.vsix` ;
2. Branchez les composants aux emplacements standards (avec le rzc compilé) :
   ```bash
   rzc install lsp        # serveur de langage → ~/.cargo/bin
   rzc install toolchain  # toolchain intégrée → ~/.rz/toolchain (en ligne ; ou copiez depuis le pack hors ligne d'un distributeur)
   ```
3. Ouvrez un fichier `.fr` et c'est parti (l'extension localise serveur et toolchain automatiquement ; `RUST_ANALYZER_PATH` ou le réglage `i18n-rust.serverPath` peuvent remplacer).

### Pour les distributeurs : pack de release hors ligne (distribution en salle de classe)

Le dépôt fournit un script d'empaquetage en une commande (compilation locale, pack pour la plateforme courante, pour Releases) :

| Plateforme | Commande | Artefact |
|---|---|---|
| Linux / macOS | `./release-offline.sh` | `release/rzc-<version>-linux/macos-<architecture>.tar.gz` |
| Windows (PowerShell) | `.\release-offline.ps1` | `release/rzc-<version>-windows-x86_64.zip` |

Prérequis : une compilation release (étape 2 ci-dessus) et une exécution en ligne de `rzc install toolchain --ra-only --force` (le script copie le rust-analyzer de la plateforme dans le pack).

Le script détecte automatiquement la plateforme (Linux / Darwin / Windows) ; le pack contient rzc, i18n-rust-lsp, rust-analyzer, les 11 packs de langue intégrés (10 langues naturelles + le pack identité `en`) et le tutoriel — décompressez et utilisez (détails dans l'annexe D.5 du tutoriel).

## 🚀Démarrage rapide

```bash
rzc init mon-projet        # crée un squelette de projet exécutable (Cargo.toml + src/main.fr)
cd mon-projet
rzc run src/main.fr        # traduire → compiler → exécuter
```

En trois étapes, votre premier programme Rust en langue maternelle tourne.

---

## 🛠️ Commandes

| Commande | Description |
|------------------------------------|------------------------------------------------------|
| `rzc init <nom>` | Créer un projet (version de la toolchain fixée à la locale ; prêt pour l'IDE) |
| `rzc run <fichier>` | Traduire et exécuter ; avertissements/erreurs/progression de compilation dans votre langue |
| `rzc check <fichier>` | Vérification de types avec diagnostic pédagogique localisé |
| `rzc eject <fichier>` | Exporter en code Rust standard (transition progressive) |
| `rzc transpile <fichier>` | Traduire seulement — Rust standard sur stdout |
| `rzc cheat <langue>` | Aide-mémoire langue ↔ Rust (`--markdown` pour l'intégrer dans la documentation) |
| `rzc add <crate>[@version]` | Ajouter une dépendance (enveloppe `cargo add`, avec conseils de mappage natif) |
| `rzc doctor` | Diagnostiquer l'environnement de la toolchain (intégré / PATH / comparaison de versions) |
| `rzc lang list` | Lister les packs de langue installés |
| `rzc lang install <code/dossier>` | Installer un pack de langue (registre distant ou dossier local) |
| `rzc lang search [mot-clé]` | Rechercher les packs de langue distants installables |
| `rzc lang remove <code>` | Supprimer un pack de langue installé par l'utilisateur |
| `rzc mapping auto <crate> [--target-version <version>]` | Générer automatiquement les mappages natifs d'un crate tiers (IA/règles) ; figer la version de base de génération |
| `rzc mapping check [cible]` | Vérifier la qualité des mappages (clés dupliquées / collisions de mots-clés / conflits entre fichiers / sections obligatoires) |
| `rzc mapping coverage [--lang <langue>]` | Mesurer la couverture du pack de langue sur du code réel ; lister les mappages manquants (exécuter à la racine du dépôt) |
| `rzc mapping scaffold <source> <cible>` | Générer un squelette de traduction pour une nouvelle langue ; `--provider deepseek` pour la traduction IA |
| `rzc crate search [mot-clé]` | Rechercher les mappages tiers partagés par la communauté (registre) |
| `rzc crate install <crate> --lang <langue>` | Installer un mappage tiers du registre dans le pack de langue global |
| `rzc crate list` | Lister les mappages communautaires installés |
| `rzc crate remove <crate> --lang <langue>` | Supprimer un mappage communautaire installé |
| `rzc crate update` | Re-télécharger tous les mappages installés selon le manifeste (obtenir les mises à jour) |
| `rzc crate publish <crate> --lang <langue>` | Publier vos mappages locaux dans le registre (passe d'abord le contrôle qualité) |
| `rzc install <lsp\|toolchain>` | Installer les composants compagnons (serveur de langage / toolchain officielle intégrée) |

Référence complète : [annexe D : aide-mémoire des commandes rzc (chinois)](tutorials/附录D：rzc命令速查.md).

---

## ✨ Fonctionnalités

### Programmation en langue maternelle

Écrivez des programmes complets avec des mots-clés français (`fonction`, `laisser`, `si`, `correspondre`…) et la bibliothèque standard en votre langue (`chaine`, `vecteur::nouveau()`, `utiliser standard::collections::dictionnaire`). Macros, durées de vie, génériques et traits : tout est pris en charge.

### 10 langues intégrées

| Langue  | Ext.  | Langue    | Ext.   |
|---------|-------|-----------|--------|
| 中文    | `.zh` | Español   | `.es`  |
| Deutsch | `.de` | Français  | `.fr`  |
| 日本語  | `.ja` | Português | `.pt`  |
| 한국어  | `.ko` | العربية   | `.ar`  |
| Русский | `.ru` | हिन्दी    | `.hi`  |

Rust lui-même est écrit en anglais ; l'anglais n'est donc pas un dialecte pédagogique (un mappage identité n'a pas de valeur pédagogique) ; le répertoire des packs de langue contient un pack identité `en` séparé (extension `.en`) pour les scénarios de mappage identité et les textes d'interface en anglais. Les 10 langues naturelles ci-dessus sont détectées automatiquement par l'extension de fichier et peuvent coexister dans un même projet.

### Diagnostics de niveau pédagogique

- **Double traduction : codes + messages** : couvre les codes d'erreur rustc, les avertissements lint sans code et les phrases d'aide
- **Noms de types localisés** : `std::fmt::Display` → `standard::format::affichable`
- **💡 Conseils pédagogiques** : chaque erreur inclut une suggestion « quoi faire ensuite » ; les erreurs de propriété ajoutent un récit 📌 de déplacement/emprunt
- **Guidage des dépendances** : détecte les crates tiers non déclarés et suggère `rzc add <crate>`

### Expérience IDE complète (extension VS Code / Qoder)

Coloration syntaxique, complétion intelligente, documentation au survol, aller à la définition, recherche de références, renommage, formatage du code, exécution/vérification en un clic, conversion automatique de la ponctuation pleine largeur, traduction assistée par IA.

### Crates tiers dans votre langue

`rzc mapping auto` extrait les API publiques des crates installés et génère des noms dans votre langue (IA) ; les mappages de production fixent leur version de base avec `--target-version` (consignée dans l'en-tête du fichier) ; les mappages communautaires passent le contrôle qualité `rzc mapping check`.

### Pourquoi ce n'est pas un « langage jouet »

| | « Langages natifs » jouets typiques | rzc |
|---|---|---|
| Forme du code | Syntaxe inventée ou pseudo-code | Entièrement isomorphe au Rust standard — seuls les identifiants sont localisés |
| Compilation et exécution | Interpréteur maison / traduction seule | rustc/cargo officiels — vraie compilation et exécution |
| Écosystème | Fermé ou tronqué | Tout crates.io (`rzc add` + mappages natifs) |
| Expérience d'erreur | Anglais brut ou astuces maison | Messages traduits + codes d'erreur + conseils pédagogiques |
| Coût de sortie | Tout réécrire | `rzc eject` exporte le Rust standard en une étape |

---

## 📖Tutoriel

Un tutoriel complet en chinois pour débutants : **26 chapitres + glossaire + 5 annexes** — voir [tutorials/](tutorials/). De « Bonjour le monde » à l'ownership, aux closures, à l'async et aux macros, jusqu'au projet final ; tous les exemples sont écrits en Rust chinois.

🌐 **Lecture en ligne** : [documentation en ligne](https://liuqiTan80.github.io/i18n-rust/) (mdBook, 4 langues avec sélecteur en barre supérieure ; aperçu local avec `make site-serve`).

**Traductions en cours** : anglais (préface, chapitres 1–13, annexe D, 15/33), japonais (chapitres 1–11, 11/33), russe (chapitres 1–15, 15/33) — progression et prochaine vague dans [translation-status.md](docs/translation-status.md). Les traductions en français sont bienvenues — voir « Contribuer ».

> La qualité du tutoriel est protégée par la CI ([tools/verify-tutorials.py](tools/verify-tutorials.py)) : chaque bloc de code doit compiler, les exemples d'erreur doivent émettre le code d'erreur attendu annoté (`// 预期错误: EXXXX`). Vérification locale après modification : `make tutorials` (chinois) ou `make tutorials-all` (anglais/japonais/russe).

> FAQ et parcours d'apprentissage : [annexe E (chinois)](tutorials/附录E：常见问题、迁移指南与学习路线.md).

---

## 🏗️ Structure du projet et fonctionnement

```text
Code source en langue maternelle (.fr)
   │  traduction lexicale → remplacement des chemins de modules → remplacement des alias   ← engine (agnostique de la langue)
   ▼
Code Rust standard
   │  cargo build / run (toolchain officielle)
   ▼
Diagnostics JSON → traduction codes/messages + localisation des types + conseils pédagogiques → sortie en langue maternelle
```

| Dossier                    | Responsabilité                                                                  |
|----------------------------|---------------------------------------------------------------------------------|
| `crates/engine`            | Moteur central agnostique : pipeline de traduction, gestion des mappages, traduction des diagnostics, cache incrémental, contrôles de sécurité Unicode |
| `crates/cli`               | l'outil en ligne de commande `rzc` |
| `crates/lsp`               | `i18n-rust-lsp` : proxifie le serveur de langage officiel (rust-analyzer), traduit positions et diagnostics dans les deux sens |
| `crates/engine/lang-packs` | 11 packs de langue intégrés (10 langues naturelles + le pack identité `en` : mots-clés / bibliothèque standard / chemins de modules / traductions d'erreurs / textes d'interface) |
| `tools/vscode-extension`   | extension VS Code / Qoder |
| `tools`                    | portes et scripts de build : vérification du tutoriel, régression de benchmarks, assemblage du site de documentation ([tools/README.md](tools/README.md)) |
| `tutorials`                | tutoriel chinois en 26 chapitres avec annexes (traductions en/ja/ru en cours) |
| `book`                     | source d'assemblage du site de documentation (mdBook, [book/README.md](book/README.md)) |
| `docs`                     | documentation de référence et de développement, [carte du projet](docs/project-map.md) ; opérations et feuille de route dans [docs/strategy/](docs/strategy/README.md) |

**Principe de conception** : le moteur ne code en dur aucune langue — ajouter une langue naturelle = ajouter un dossier de pack de langue (intégré automatiquement à la compilation). Pour porter ce paradigme vers d'autres langages de programmation (ex. Python en chinois), voir [le plan du framework de dialectes](docs/dialect-framework-blueprint.md).

---

## 🤝Contribuer

- **Première fois ?** Commencez par la [carte du projet (manuel du mainteneur)](docs/project-map.md) — « où modifier X et comment vérifier » ; environnement de développement et conventions de commit dans [CONTRIBUTING.md](CONTRIBUTING.md)
- **Guide des mots manquants (développeurs d'applications)** : [docs/missing-mapping-guide.md](docs/missing-mapping-guide.md) (ajouter des mots / personnaliser les mappages en écrivant votre application — pour débutants)
- **Ajouter un pack de langue** : [docs/contributing-lang-pack.md](docs/contributing-lang-pack.md) (avec le flux de traduction IA de `rzc mapping scaffold`)
- **Mappages de crates tiers** : [docs/third-party-mapping.md](docs/third-party-mapping.md)
- **Registre communautaire** : [docs/third-party-registry.md](docs/third-party-registry.md) (upload/téléchargement de mappages communautaires)
- **Traduire le tutoriel** : partez de `tutorials/`, gardez la structure des chapitres ; tableau de progression dans [translation-status.md](docs/translation-status.md)
- Avant de soumettre, assurez-vous que `make gate` est entièrement vert (équivalent de la porte de test CI)

---

## ⭐ Soutenir le projet

Si rzc vous aide ou vous inspire, une Star est la meilleure récompense ; et traduire le tutoriel dans votre langue est la meilleure façon de soutenir le projet.

---

## 📄 Licence

[MIT](LICENSE) © tan80
