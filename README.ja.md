<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

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

## 📦 インストール

1 コマンドでインストール完了、すぐにグローバルで使用可能：

```bash
cargo install rzc
```

> [Rust ツールチェーン](https://www.rust-lang.org/tools/install)（rustup の stable）が必要です。言語パックは内蔵済みで、追加設定は不要です。

ソースからビルドすることもできます：

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace                # バイナリ: target/release/rzc
```

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
- **多言語設計**：11 言語パック内蔵（ja/zh/en/de/ru/es/fr/pt/ko/ar/hi）、拡張子で自動判定
- **母語の診断**：`rzc check` が rustc エラーを翻訳し、💡 教学ヒントを表示
- **所有権の可視化**：VS Code 拡張機能（`i18n-rust` を検索）で変数の移動・再利用を色分け表示
- **完全な LSP サポート**：補完、ホバー、定義ジャンプ、参照検索、リネーム
- **段階的移行**：`rzc eject` で標準 Rust コードをワンステップでエクスポート

## 📖 チュートリアル

初心者向けの完全な中国語チュートリアル（24 章 + 4 付録）は [tutorials/](tutorials/) を参照してください。

## 📄 ライセンス

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
