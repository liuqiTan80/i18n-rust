<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc : Compilateur multilingue du dialecte pédagogique Rust

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

## 📦 Installation

Une seule commande — disponible globalement dès l'installation :

```bash
cargo install rzc
```

> Nécessite la [toolchain Rust](https://www.rust-lang.org/tools/install) (stable via rustup). Les packs de langue sont intégrés — aucune configuration supplémentaire requise.

Vous pouvez aussi compiler depuis les sources :

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace                # binaire : target/release/rzc
```

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
- **Multilingue par conception** : 11 packs de langue intégrés (fr/zh/en/de/ja/ru/es/pt/ko/ar/hi), détection automatique par extension
- **Diagnostics localisés** : `rzc check` traduit les erreurs rustc dans la langue du fichier, avec 💡 conseils pédagogiques
- **Visualisation de la propriété** : l'extension VS Code (recherchez `i18n-rust`) surligne les déplacements et réutilisations de variables
- **Support LSP complet** : complétion, survol, aller à la définition, références, renommage
- **Transition progressive** : `rzc eject` exporte le code Rust standard en une étape

## 📖 Tutoriel

Un tutoriel complet en chinois pour débutants (24 chapitres + 4 annexes) — voir [tutorials/](tutorials/).

## 📄 Licence

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
