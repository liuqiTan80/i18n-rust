<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

[![CI](https://github.com/liuqiTan80/i18n-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/liuqiTan80/i18n-rust/actions)
[![crates.io](https://img.shields.io/crates/v/rzc.svg)](https://crates.io/crates/rzc)
[![Docs](https://img.shields.io/badge/Docs-Online%20documentation-blue)](https://liuqiTan80.github.io/i18n-rust/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

> 🪞 **リポジトリミラー**：本プロジェクトは GitHub（[liuqiTan80/i18n-rust](https://github.com/liuqiTan80/i18n-rust)）と GitCode（[tan80/i18n-rust](https://gitcode.com/tan80/i18n-rust)）の 2 プラットフォームで同期メンテナンスしています。`rzc lang install` はデフォルトで GitCode ソースを優先し（中国国内からのアクセスが高速）、失敗時は自動で GitHub にフォールバックします。

> ⚠️ 本翻訳は[中国語版](README.md)をもとに作成されています。原文の更新に伴い、翻訳が遅れる場合があります。

# rzc：多言語 Rust 教学方言コンパイラ

> 🌍 **母語で Rust を書こう。** · 10 言語対応 · 本物の Rust、本物のツールチェーン · `rzc eject` でいつでも卒業

**rzc は多言語 Rust 方言コンパイラです**：あなたが母語でコードを書くと、rzc がリアルタイムで標準 Rust に翻訳し、公式ツールチェーンがコンパイル・実行します——プログラミングを学ぶのに英語は不要。返ってくるすべての診断メッセージは母語に翻訳され、学習ヒント付きで表示されます。

- 🌍 **擬似コードではない**：母語コードは標準 Rust と完全に同構——コンパイル・実行・依存関係・エコシステムは 100% 本物
- 🎓 **初心者のために**：エラーメッセージは英単語の壁ではなく「次に何をすべきか」のガイドに変わる
- 🚪 **いつでも卒業**：`rzc eject` が標準 Rust をワンステップで書き出す——ロックインなし、主流エコシステムと完全に互換

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

ミスしても大丈夫——エラーも母語です：

```
エラー[E0384]: 不変変数 `数` に2回代入できません
  --> src/main.ja:3:5
💡 変数の値を変更したい場合は、`宣言 可変` で宣言してください。
```

同じプログラムは内蔵の 10 方言すべてで動きます——中国語（`函数 主函数()`、`打印行!`）やロシア語（`функция главная()`、`печатай_строку!`）など。標準 Rust（`fn main()`）は常にそのまま受け付けられます。

**ここから始める**：🚀 [クイックスタート](#クイックスタート) · 🌐 [オンラインドキュメント](https://liuqiTan80.github.io/i18n-rust/)（4 言語） · 📚 [Feishu KB](https://my.feishu.cn/wiki/space/7689728327082314704)（中国語チュートリアル、ログイン不要） · 📖 [体系的に学ぶ](#チュートリアル) · 🤝 [コントリビューション](#コントリビューション)

---

## 📦 インストール

二つの方法から選べます：**方法一 crates.io からインストール**（推奨、ほとんどのユーザー向け）；**方法二 ソースからビルド**（最新の開発版が必要な場合、または rzc 本体を改造する開発者向け）。

**方法一：crates.io からインストール（推奨）**——crates.io に公開済み。Rust ツールチェーンがあればワンコマンドで完了します：

```bash
cargo install rzc        # crates.io から取得しローカルでビルド（初回 1〜3 分）
rzc --version            # バージョン番号が出ればインストール成功
rzc init マイプロジェクト && cd マイプロジェクト && rzc run src/main.ja
```

> 言語パックは内蔵で追加設定は不要。crate ページ：<https://crates.io/crates/rzc>；アップグレード：`cargo install rzc --force`。Rust ツールチェーン未導入なら下の「1.」を先に実行してください。

**方法二：ソースからビルド（開発者向け）**——最新の開発版を入手する場合、または rzc を改造してローカルビルドする場合。**最短経路**（Rust ツールチェーン導入済みなら 4 コマンドで最初のプログラムが動きます）：

```bash
git clone https://github.com/liuqiTan80/i18n-rust && cd i18n-rust
cargo build --release --workspace   # 約 1〜3 分
cargo install --path crates/cli     # rzc をグローバルに使えるように（./target/release/rzc の直接使用も可）
rzc init マイプロジェクト && cd マイプロジェクト && rzc run src/main.ja
```

以下は方法二の完全な手順です（前提条件 → ビルド → 検証）；「1.」は両方に共通です。

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

公式 standalone rust-analyzer が `~/.rz/toolchain` に入り、rzc と言語サーバーは自動的にそれを優先使用します。

### 5.（任意）VS Code 拡張のビルド

Node.js 18+ と npm が必要です（[nodejs.org](https://nodejs.org)）:

```bash
cd tools/vscode-extension
npm ci
npm run package    # i18n-rust-<バージョン>.vsix を生成
```

生成された `.vsix` を VS Code の「Install from VSIX...」でインストールすると、シンタックスハイライト・補完・診断・ホバー・所有権の可視化がすべて使えます。言語サーバーは `rzc install lsp` が提供します（内蔵ツールチェーンを自動的に見つけます）。

### 完全機能セットアップ（コマンドひとつずつ）

```bash
rzc install lsp          # 言語サーバー（VS Code の補完・診断・ホバーのバックエンド）
rzc install toolchain    # 内蔵公式ツールチェーン（standalone rustc/cargo/rust-analyzer を ~/.rz/toolchain へ）
rzc doctor               # ツールチェーン環境の状態を確認（内蔵 / PATH / バージョン比較）
```

インストール後、rzc と言語サーバーは自動的に内蔵ツールチェーンを優先使用します。単一ファイルプロジェクトは cargo インデックス不要で rustc を直接呼び出します。

### 環境変数（任意、通常は設定不要）

| 変数 | 用途 |
|---|---|
| `RZ_LANG_DIR` | 言語パックディレクトリを指定（デフォルトは内蔵言語パック） |
| `RUST_ANALYZER_PATH` | rust-analyzer のパスを指定（自動検出に失敗した場合） |

VS Code の設定 `i18n-rust.serverPath` で LSP バイナリのパスを明示的に指定できます（自動検出に失敗した場合に使用）。

### ツールチェーン内のソフトウェアをアップグレード

| シナリオ | コマンド |
|---|---|
| rustc/cargo をアップグレード（例：1.98 → 1.99） | `rzc install toolchain --version 1.99.0 --force` |
| rust-analyzer のみアップグレード（300MB の再ダウンロードを回避） | `rzc install toolchain --ra-tag <日付タグ> --ra-only --force` |
| 現在のバージョンと状態を確認 | `rzc doctor` |

### エディタの変更（VSCodium / Cursor など VS Code 系へ）

i18n-rust 拡張（.vsix）はすべての VS Code 系エディタと互換です。rzc・言語サーバー・ツールチェーンはいずれもエディタ非依存です：

1. 新しいエディタ → 拡張機能 →「Install from VSIX」→ `i18n-rust-<バージョン>.vsix` を選択；
2. コンポーネントをシステム標準の場所へ接続（ビルドした rzc で実行）：
   ```bash
   rzc install lsp        # 言語サーバー → ~/.cargo/bin
   rzc install toolchain  # 内蔵ツールチェーン → ~/.rz/toolchain（オンラインインストール；配布者のオフラインパッケージからのコピーでも可）
   ```
3. `.ja` ファイルを開けばすぐ使えます（拡張が言語サーバーとツールチェーンを自動的に見つけます；環境変数 `RUST_ANALYZER_PATH` や設定 `i18n-rust.serverPath` で明示指定も可能）。

### 配布者向け：オフラインリリースパッケージの作成（教学施設などでのオフライン配布）

リポジトリにはワンショットのパッケージングスクリプトがあります（ローカルビルドで現在のプラットフォーム向けリリースパッケージを生成、Release / ファイル共有での配布用）：

| プラットフォーム | コマンド | 成果物 |
|---|---|---|
| Linux / macOS | `./release-offline.sh` | `release/rzc-<バージョン>-linux/macos-<アーキテクチャ>.tar.gz` |
| Windows（PowerShell） | `.\release-offline.ps1` | `release/rzc-<バージョン>-windows-x86_64.zip` |

前提条件：上の手順で release ビルド済みであること；一度オンラインで `rzc install toolchain --ra-only --force` を実行し、現在のプラットフォームの rust-analyzer をダウンロードしておくこと（パッケージスクリプトがそれをコピーします）。

スクリプトはプラットフォーム（Linux / Darwin / Windows）を自動判別し、パッケージには rzc、i18n-rust-lsp、rust-analyzer、11 個の内蔵言語パック（10 の自然言語 + `en` 恒等パック）とチュートリアルが含まれ、解凍するだけですぐ使えます（詳細はチュートリアル付録 D.5）。

## 🚀クイックスタート

```bash
rzc init マイプロジェクト        # 実行可能なプロジェクト骨格を生成（Cargo.toml + src/main.ja）
cd マイプロジェクト
rzc run src/main.ja              # 翻訳 → コンパイル → 実行
```

3 ステップで、最初の母語 Rust プログラムが動き出します。

---

## 🛠️ 主なコマンド

| コマンド | 説明 |
|------------------------------------|------------------------------------------------------|
| `rzc init <プロジェクト名>` | 新規プロジェクトを作成（ツールチェーンのバージョンをローカル版に固定、IDE ですぐ使える） |
| `rzc run <ファイル>` | 翻訳して実行；警告・エラー・ビルド進行状況まですべて母語化 |
| `rzc check <ファイル>` | 型チェック、母語の教学診断を出力 |
| `rzc eject <ファイル>` | 標準 Rust コードとしてエクスポート（段階的移行） |
| `rzc transpile <ファイル>` | 翻訳のみ——標準 Rust を標準出力へ |
| `rzc cheat <言語>` | 母語 ↔ Rust 対応早見表（`--markdown` でドキュメント埋め込み可） |
| `rzc add <crate名>[@バージョン]` | 依存クレートを追加（`cargo add` のラッパー、母語マッピングのヒント付き） |
| `rzc doctor` | ツールチェーン環境を診断（内蔵 / PATH / バージョン比較） |
| `rzc lang list` | インストール済み言語パック一覧 |
| `rzc lang install <コード/ディレクトリ>` | 言語パックをインストール（リモートリポジトリまたはローカルディレクトリ） |
| `rzc lang search [キーワード]` | インストール可能なリモート言語パックを検索 |
| `rzc lang remove <コード>` | ユーザーがインストールした言語パックを削除 |
| `rzc mapping auto <crate名> [--target-version バージョン]` | サードパーティクレートの母語マッピングを自動生成（AI/ルール）；生成基準バージョンを固定可能 |
| `rzc mapping check [ターゲット]` | マッピング品質を検証（重複キー / キーワード衝突 / ファイル間競合 / 必須セクション） |
| `rzc mapping coverage [--lang 言語]` | 実ソースで言語パックのカバレッジを検証し、欠落マッピングを一覧（リポジトリルートで実行） |
| `rzc mapping scaffold <ソース> <ターゲット>` | 新言語の翻訳スケルトンを生成；`--provider deepseek` で AI 自動翻訳 |
| `rzc crate search [キーワード]` | コミュニティ共有のサードパーティマッピングを検索（レジストリ） |
| `rzc crate install <crate名> --lang <言語>` | レジストリから単一のサードパーティマッピングをグローバル言語パックへインストール |
| `rzc crate list` | インストール済みコミュニティマッピング一覧 |
| `rzc crate remove <crate名> --lang <言語>` | インストール済みコミュニティマッピングを削除 |
| `rzc crate update` | マニフェストに従い全インストール済みマッピングを再取得（更新の取得） |
| `rzc crate publish <crate名> --lang <言語>` | ローカル翻訳マッピングをレジストリへ公開（先に品質ゲートを通過） |
| `rzc install <lsp\|toolchain>` | 付属コンポーネントをインストール（言語サーバー / 内蔵公式ツールチェーン） |

完全なリファレンスは[付録D：rzc コマンド早見表（中国語）](tutorials/附录D：rzc命令速查.md)を参照。

---

## ✨ 機能

### 母語プログラミング

母語キーワード（`関数`、`宣言`、`もし`、`マッチ`…）と母語標準ライブラリ（`文字列型`、`ベクタ::新規()`、`使用 標準ライブラリ::コレクション::ハッシュマップ`）で完全なプログラムを書けます。マクロ、ライフタイム、ジェネリクス、トレイトもすべてサポート。

### 10 言語を内蔵

| 言語    | 拡張子 | 言語      | 拡張子 |
|---------|-------|-----------|--------|
| 中文    | `.zh` | Español   | `.es`  |
| Deutsch | `.de` | Français  | `.fr`  |
| 日本語  | `.ja` | Português | `.pt`  |
| 한국어  | `.ko` | العربية   | `.ar`  |
| Русский | `.ru` | हिन्दी    | `.hi`  |

Rust 自体は英語で書かれているため、英語は教学方言に含まれません（恒等マッピングには教学価値がないため）。言語パックディレクトリには `en` 恒等パック（拡張子 `.en`）が別途あり、恒等マッピングが必要な場面や英語 UI テキストに使用します。上表の 10 の自然言語はファイル拡張子で自動判別され、同一プロジェクト内で混在できます。

### 教学グレードの診断

- **エラーコード＋メッセージの二重翻訳**：rustc エラーコード、コードなし lint 警告、help フレーズをカバー
- **型名のローカライズ**：`std::fmt::Display` → `標準ライブラリ::フォーマット::表示可能`
- **💡 学習ヒント**：すべてのエラーに次の一手の提案；所有権エラーには 📌 移動・借用のナラティブ付き
- **依存関係ガイダンス**：未宣言のサードパーティクレートを検出し `rzc add <crate>` を提案

### 完全な IDE 体験（VS Code / Qoder 拡張）

シンタックスハイライト、スマート補完、ホバードキュメント、定義ジャンプ、参照検索、名前変更、コードフォーマット、ワンクリック実行・チェック、全角句読点の自動半角変換、AI 翻訳支援。

### サードパーティクレートの母語化

`rzc mapping auto` はインストール済みクレートから公開 API を抽出し、AI が母語名を生成；本番用マッピングは `--target-version` で生成基準バージョンを固定（ファイル先頭に記録）；コミュニティのマッピングは `rzc mapping check` の品質ゲートを通過します。

### なぜ「おもちゃ言語」ではないのか

| | よくある「母語プログラミング」おもちゃ | rzc |
|---|---|---|
| コードの形 | 独自構文や擬似コード | 標準 Rust と完全に同構、識別子のみ母語化 |
| コンパイルと実行 | 自前インタプリタ / 翻訳のみ | 公式 rustc/cargo による本物のコンパイル実行 |
| 依存エコシステム | 閉鎖的または切り詰め | crates.io 全体（`rzc add` + 母語マッピング） |
| エラー体験 | 英語そのまままたは独自ヒント | 母語翻訳 + エラーコード + 学習ヒント |
| 撤退コスト | 途中でやめると書き直し | `rzc eject` で標準 Rust をワンステップ書き出し |

---

## 📖チュートリアル

初心者向けの完全な中国語チュートリアル：**26 章 + 用語集 + 5 付録**。[tutorials/](tutorials/) を参照。『こんにちは世界』から所有権、クロージャ、非同期、マクロを経て総合実戦まで——すべての例は中国語 Rust で書かれています。

🌐 **オンライン読書**：[オンラインドキュメント](https://liuqiTan80.github.io/i18n-rust/)（mdBook 4 言語 + トップバー切り替え；ローカルプレビューは `make site-serve`）。

**多言語訳が進行中**：日本語（第 1〜11 章、11/33）、英語（序章・第 1〜13 章・付録 D、15/33）、ロシア語（第 1〜15 章、15/33）——進捗と次のバッチは [translation-status.md](docs/translation-status.md) を参照。

> チュートリアルの品質は CI が自動で門番します（[tools/verify-tutorials.py](tools/verify-tutorials.py)）：
> すべてのコードブロックはコンパイル可能であること、エラー例は注記された期待エラーコード（`// 预期错误: EXXXX`）を出すこと。
> チュートリアル修正後のローカル検証：`make tutorials`（中国語）または `make tutorials-all`（英日露）。

> よくある質問と学習ロードマップは[付録E（中国語）](tutorials/附录E：常见问题、迁移指南与学习路线.md)を参照。チュートリアルとマッピング表の他言語への翻訳も歓迎します——下の「コントリビューション」をご覧ください。

---

## 🏗️ プロジェクト構造と動作原理

```text
母語ソース (.ja)
   │  字句翻訳 → モジュールパス置換 → 別名置換        ← engine（言語非依存）
   ▼
標準 Rust ソース
   │  cargo build / run（公式ツールチェーン）
   ▼
JSON 診断 → エラーコード・メッセージ表翻訳 + 型ローカライズ + 学習ヒント → 母語出力
```

| ディレクトリ               | 責務                                                                   |
|----------------------------|------------------------------------------------------------------------|
| `crates/engine`            | 言語非依存の中核エンジン：翻訳パイプライン、マッピング管理、診断翻訳、増分キャッシュ、Unicode 安全チェック |
| `crates/cli`               | `rzc` コマンドラインツール |
| `crates/lsp`               | `i18n-rust-lsp`：公式言語サーバー（rust-analyzer）をプロキシし、位置と診断を双方向翻訳 |
| `crates/engine/lang-packs` | 11 個の内蔵言語パック（10 自然言語 + `en` 恒等パック：キーワード / 標準ライブラリ / モジュールパス / エラー翻訳 / UI テキスト） |
| `tools/vscode-extension`   | VS Code / Qoder 拡張 |
| `tools`                    | ゲートとビルドスクリプト：チュートリアル検証、ベンチマーク回帰、ドキュメントサイト組み立てなど（[tools/README.md](tools/README.md)） |
| `tutorials`                | 26 章の中国語チュートリアルと付録（en/ja/ru 訳進行中） |
| `book`                     | ドキュメントサイト組み立てソース（mdBook、[book/README.md](book/README.md)） |
| `docs`                     | リファレンス・開発ドキュメントと[プロジェクトマップ](docs/project-map.md)；運営とロードマップは [docs/strategy/](docs/strategy/README.md) |

**設計原則**：エンジンはいかなる特定言語もハードコードしません——自然言語を一つ追加する = 言語パックディレクトリを一つ追加するだけ（ビルド時に自動埋め込み）。このパラダイムを他のプログラミング言語（例：中国語 Python）へ移植する方法は[方言プログラミングフレームワーク生成ブループリント](docs/dialect-framework-blueprint.md)を参照。

---

## 🤝コントリビューション

- **初めての方は**[プロジェクトマップ（メンテナハンドブック）](docs/project-map.md)から——「X を変えたいときはどこへ、変更後の検証はどうやるか」のワンページ早見；開発環境とコミット規約は [CONTRIBUTING.md](CONTRIBUTING.md)
- **アプリ開発者向け欠落語彙ガイド**：[docs/missing-mapping-guide.md](docs/missing-mapping-guide.md)（アプリを書きながら語彙を補う / マッピングをカスタム、初心者向け）
- **言語パックの追加**：[docs/contributing-lang-pack.md](docs/contributing-lang-pack.md)（`rzc mapping scaffold` の AI 翻訳フロー込み）
- **サードパーティクレートマッピング**：[docs/third-party-mapping.md](docs/third-party-mapping.md)
- **コミュニティ共有レジストリ**：[docs/third-party-registry.md](docs/third-party-registry.md)（自作マッピングのアップロード・ダウンロード）
- **チュートリアルの翻訳**：`tutorials/` を源に、章構成を揃えて；進捗パネルは [translation-status.md](docs/translation-status.md)
- 提出前に `make gate` がオールグリーンであることを確認（CI テストゲート相当）

---

## ⭐ プロジェクトを支援

rzc が役に立った、あるいは刺激になったなら、Star をいただけると嬉しいです。チュートリアルをあなたの母語に翻訳するのも、プロジェクトへの何よりの支援になります。

---

## 📄 ライセンス

[MIT](LICENSE) © tan80
