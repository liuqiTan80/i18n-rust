<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc : Compilateur dialectal Rust multilingue pour l'enseignement

Écrivez des programmes Rust dans votre langue maternelle. rzc les traduit automatiquement en Rust standard, compile et exécute — l'enseignement de la programmation revient à la pensée logique, pas à la mémorisation de l'anglais.

```rust
// src/main.fr — dialecte français de Rust pour l'enseignement
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

## ✨ Fonctionnalités

- **Programmation en langue maternelle** : écrivez des programmes Rust complets avec des mots-clés en français (`fonction`, `laisser`, `si`, `retourner`…)
- **Multilingue natif** : architecture supportant toute langue naturelle ; 11 paquets intégrés, autres installables à distance
- **Détection automatique par extension** : `.zh`, `.en`, `.de` associés automatiquement au paquet linguistique
- **Diagnostics localisés** : erreurs rustc traduites avec 💡 conseils pédagogiques ; erreurs de propriété visualisées via JSON
- **Visualisation de la propriété** : l'extension VS Code surligne les déplacements (jaune), réutilisations (rouge) et durées de vie (vert)
- **Support LSP complet** : complétion, survol, définition, références, renommage
- **Complétion automatique des macros** : le `!` peut être omis ; ajouté automatiquement
- **Transition progressive** : `eject` exporte le code Rust standard en une étape
- **Tutoriel complet** : 24 chapitres + 4 annexes, du débutant absolu au projet intégral

## 📦 Installation

### Via crates.io (recommandé)

```bash
cargo install rzc
```

### Depuis les sources

```bash
# International
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace
```

## 🚀 Démarrage rapide

```bash
rzc init mon-projet
cd mon-projet
rzc run src/main.fr
```

## 🛠️ Commandes

| Commande | Description |
|----------|-------------|
| `rzc init <nom>` | Créer un nouveau projet |
| `rzc run <fichier>` | Traduire et exécuter le code `.fr` |
| `rzc check <fichier>` | Vérification des types avec diagnostics |
| `rzc eject <fichier>` | Exporter en code `.rs` standard |
| `rzc lang list` | Lister les paquets linguistiques installés |
| `rzc lang install <source>` | Installer un paquet linguistique |
| `rzc lang remove <code>` | Supprimer un paquet linguistique |
| `rzc mapping auto <crate>` | Générer automatiquement les mappages de crates |

## 📖 Tutoriel

Tutoriel complet pour débutants : 24 chapitres + 4 annexes

| Étape | Chapitres |
|-------|-----------|
| **Bases** | Ch.1 Bonjour le monde · Ch.2 Variables et types · Ch.3 Types composés · Ch.4 Flux de contrôle · Ch.5 Fonctions et méthodes |
| **Noyau** | Ch.6 Propriété · Ch.7 Références et emprunt · Ch.8 Chaînes · Ch.9 Structures · Ch.10 Énumérations et filtrage |
| **Génériques** | Ch.11 Génériques · Ch.12 Traits · Ch.13 Durées de vie · Ch.14 Collections |
| **Erreurs et modules** | Ch.15 Gestion des erreurs · Ch.16 Système de modules · Ch.17 Gestion de paquets |
| **Avancé** | Ch.18 Pointeurs intelligents · Ch.19 Concurrence · Ch.20 Tests |
| **Expert** | Ch.21 Fermetures et itérateurs · Ch.22 Macros · Ch.23 Programmation asynchrone |
| **Projet** | Ch.24 Calculatrice en ligne de commande |
| **Annexes** | A Référence des mappages · B Glossaire · C Guide de migration · D FAQ et parcours d'apprentissage |

## ❓ FAQ

**Q : Pourquoi les macros peuvent-elles omettre le point d'exclamation ?**
Pour réduire la mémorisation. Le transpileur ajoute `!` automatiquement.

**Q : Puis-je utiliser des noms de variables en français ?**
Oui. `nombre` et `principale` sont des identifiants valides. Rust supporte les identifiants Unicode.

**Q : Comment installer d'autres paquets linguistiques ?**
`rzc lang install ja` (distant) ou `rzc lang install ./répertoire` (local).

## 🤝 Contribution

Retours via [GitHub Issues](https://github.com/liuqiTan80/i18n-rust/issues), PR bienvenues.

## 📄 Licence

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
