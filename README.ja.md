<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

> ⚠️ この翻訳は最新の内容に追いついていない場合があります。最新情報は[中国語版](README.md)または[英語版](README.en.md)をご参照ください。

# rzc：多言語 Rust 教学方言コンパイラ

あなたの母語で Rust プログラムを書き、rzc が標準 Rust に自動翻訳してコンパイル・実行します——プログラミングを学ぶのに英語は不要。

```rust
// src/main.ja —— 日本語 Rust 教学方言
関数 主関数() {
    宣言 可変 数 = 10;
    数 = 数 + 1;
    表示行!("数は：{}", 数);
}
```

```bash
$ rzc run src/main.ja
数は：11
```

## 📦 インストール（ソースからビルド）

rzc はオンラインのプリビルド版を提供していません。ご自身のマシンでビルドしてください（初回 1〜3 分）。

### 前提条件

| ツール | 用途 | 必須？ |
|---|---|---|
| **Rust ツールチェーン**（rustc + cargo） | rzc 本体のビルド | ✅ 必須 |
| **git** | ソースの取得 | ✅ 必須（ソース ZIP でも可） |
| **ネットワーク** | 初回ビルド時の依存取得 | ✅ 必須（初回のみ） |
| **Node.js 18+ と npm** | VS Code 拡張のビルド | 任意（IDE のみ） |
| **rust-analyzer** | IDE の補完・診断バックエンド | 任意（IDE のみ） |

### 1. Rust ツールチェーンのインストール

公式推奨の [rustup](https://rustup.rs) でインストールします（rustc / cargo / rustup がまとめて入ります）。

- **Linux / macOS**（ターミナルで実行）:

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

  インストール後、`source "$HOME/.cargo/env"` を実行するか、ターミナルを開き直して有効化します。

- **Windows**: [rustup-init.exe](https://rustup.rs) をダウンロードしてウィザードに従います（PowerShell で `winget install --id Rustlang.Rustup` でも可）。

確認（ターミナルを開き直して）:

```bash
rustc --version   # 例: rustc 1.98.0
cargo --version
```

### 2. ソースを取得してビルド

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace
./target/release/rzc --version
```

初回ビルドは依存の取得と全コンポーネントのコンパイルで 1〜3 分かかります。生成物:

- `target/release/rzc` — コマンドラインツール
- `target/release/i18n-rust-lsp` — 言語サーバー（VS Code 拡張のバックエンド）

### 3.（任意）rzc をグローバルで使えるようにする

```bash
cargo install --path crates/cli   # ローカルビルドして ~/.cargo/bin にインストール
```

### 4.（任意）IDE 機能用に rust-analyzer をインストール

```bash
rzc install toolchain --ra-only --force
```

### 5.（任意）VS Code 拡張のビルド

Node.js 18+ と npm が必要です（[nodejs.org](https://nodejs.org)）:

```bash
cd tools/vscode-extension
npm ci
npm run package    # i18n-rust-<バージョン>.vsix を生成
```

生成された `.vsix` を VS Code の「Install from VSIX...」でインストールします。言語サーバーは `rzc install lsp` が提供します。

## 🚀 クイックスタート

```bash
rzc init マイプロジェクト
cd マイプロジェクト
rzc run src/main.ja
```

`rzc init` は実行可能なプロジェクト骨格（`Cargo.toml` + `src/main.ja`）を生成します。すぐに実行できます。

## 🛠️ 主なコマンド

| コマンド | 説明 |
|----------|------|
| `rzc init <プロジェクト名>` | 新規プロジェクトを作成 |
| `rzc run <ファイル>` | 方言ソースを翻訳して実行 |
| `rzc check <ファイル>` | 型チェック、母語の教学診断を出力 |
| `rzc eject <ファイル>` | 標準 Rust コードにエクスポート |
| `rzc lang list` | インストール済み言語パック一覧 |
| `rzc mapping auto <crate名>` | サードパーティ crate マッピングを自動生成 |

## ✨ 機能

- **母語プログラミング**：母語のキーワードで完全な Rust プログラムを記述
- **多言語設計**：10 言語パック内蔵（ja/zh/de/ru/es/fr/pt/ko/ar/hi）、拡張子で自動判定
- **母語の診断**：`rzc check` が rustc エラーを翻訳し、💡 教学ヒントを表示
- **所有権の可視化**：VS Code 拡張機能（`i18n-rust` を検索）で変数の移動・再利用を色分け表示
- **完全な LSP サポート**：補完、ホバー、定義ジャンプ、参照検索、リネーム
- **段階的移行**：`rzc eject` で標準 Rust コードをワンステップでエクスポート

## 📖 チュートリアル

初心者向けの完全な中国語チュートリアル（26 章 + 用語集 + 5 付録）は [tutorials/](tutorials/) を参照してください。

## 📄 ライセンス

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
