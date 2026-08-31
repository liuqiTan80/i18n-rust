<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc: Compilador multilíngue do dialeto educacional de Rust

Escreva programas Rust no seu idioma nativo — o rzc traduz automaticamente para Rust padrão e compila. Aprenda programação, não inglês.

```rust
// src/main.pt — dialeto educacional de Rust em português
funcao principal() {
    deixar mutavel numero = 10;
    numero = numero + 1;
    imprimir_linha!("Número: {}", numero);
}
```

```bash
$ rzc run src/main.pt
Número: 11
```

## 📦 Instalação (compilar a partir do código-fonte)

O rzc não oferece instalador pré-compilado online — compile-o na sua própria máquina (1–3 minutos).

### Pré-requisitos

| Ferramenta | Finalidade | Necessária? |
|---|---|---|
| **Toolchain Rust** (rustc + cargo) | Compilar o próprio rzc | ✅ Sim |
| **git** | Obter o código-fonte | ✅ Sim (ou baixar o ZIP) |
| **Internet** | Baixar dependências na primeira compilação | ✅ Sim (primeira vez) |
| **Node.js 18+ e npm** | Compilar a extensão do VS Code | Opcional (apenas IDE) |
| **rust-analyzer** | Backend de autocompletar/diagnóstico do IDE | Opcional (apenas IDE) |

### 1. Instalar a toolchain Rust

A forma recomendada é o [rustup](https://rustup.rs) (instala rustc, cargo e rustup de uma vez).

- **Linux / macOS** (em um terminal):

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

  Depois execute `source "$HOME/.cargo/env"` (ou reabra o terminal).

- **Windows**: baixe o [rustup-init.exe](https://rustup.rs) e siga o assistente (ou execute `winget install --id Rustlang.Rustup` no PowerShell).

Verifique (após reabrir o terminal):

```bash
rustc --version   # ex.: rustc 1.98.0
cargo --version
```

### 2. Obter o código e compilar

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace
./target/release/rzc --version
```

A primeira compilação baixa dependências e compila todos os componentes (1–3 minutos). Binários:

- `target/release/rzc` — a ferramenta CLI
- `target/release/i18n-rust-lsp` — o servidor de linguagem (backend da extensão do VS Code)

### 3. (Opcional) Deixar o rzc disponível globalmente

```bash
cargo install --path crates/cli   # compila localmente e instala em ~/.cargo/bin
```

### 4. (Opcional) Instalar rust-analyzer para recursos do IDE

```bash
rzc install toolchain --ra-only --force
```

### 5. (Opcional) Compilar a extensão do VS Code

Requer Node.js 18+ e npm (do [nodejs.org](https://nodejs.org)):

```bash
cd tools/vscode-extension
npm ci
npm run package    # gera i18n-rust-<versão>.vsix
```

Instale o `.vsix` via «Install from VSIX...» no VS Code; o servidor de linguagem é fornecido pelo `rzc install lsp`.

## 🚀 Início rápido

```bash
rzc init meu-projeto
cd meu-projeto
rzc run src/main.pt
```

O `rzc init` cria um esqueleto de projeto executável (`Cargo.toml` + `src/main.pt`) — basta executar.

## 🛠️ Comandos

| Comando | Descrição |
|---------|-----------|
| `rzc init <nome>` | Criar um novo projeto |
| `rzc run <arquivo>` | Traduzir e executar o código-fonte do dialeto |
| `rzc check <arquivo>` | Verificação de tipos com diagnóstico educacional localizado |
| `rzc eject <arquivo>` | Exportar para código Rust padrão |
| `rzc lang list` | Listar pacotes de idioma instalados |
| `rzc mapping auto <crate>` | Gerar automaticamente mapeamentos de terceiros |

## ✨ Recursos

- **Programação no seu idioma**: escreva programas Rust completos com as palavras-chave da sua língua
- **Multilíngue por design**: 10 pacotes de idioma integrados (pt/zh/de/ja/ru/es/fr/ko/ar/hi), detecção automática por extensão
- **Diagnósticos localizados**: `rzc check` traduz erros do rustc para o idioma do arquivo, com 💡 dicas educacionais
- **Visualização de propriedade**: a extensão VS Code (pesquise `i18n-rust`) realça movimentações e reutilizações de variáveis
- **Suporte LSP completo**: autocompletar, passar o mouse, ir para definição, referências, renomear
- **Transição gradual**: `rzc eject` exporta código Rust padrão em uma etapa

## 📖 Tutorial

Um tutorial completo em chinês para iniciantes (26 capítulos + glossário + 5 apêndices) — veja [tutorials/](tutorials/).

## 📄 Licença

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
