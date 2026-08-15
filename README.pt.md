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

## 📦 Instalação

Um único comando — disponível globalmente logo após a instalação:

```bash
cargo install rzc
```

> Requer o [toolchain Rust](https://www.rust-lang.org/tools/install) (stable via rustup). Os pacotes de idioma já estão integrados — nenhuma configuração adicional é necessária.

Você também pode compilar a partir do código-fonte:

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace                # binário em target/release/rzc
```

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
- **Multilíngue por design**: 11 pacotes de idioma integrados (pt/zh/en/de/ja/ru/es/fr/ko/ar/hi), detecção automática por extensão
- **Diagnósticos localizados**: `rzc check` traduz erros do rustc para o idioma do arquivo, com 💡 dicas educacionais
- **Visualização de propriedade**: a extensão VS Code (pesquise `i18n-rust`) realça movimentações e reutilizações de variáveis
- **Suporte LSP completo**: autocompletar, passar o mouse, ir para definição, referências, renomear
- **Transição gradual**: `rzc eject` exporta código Rust padrão em uma etapa

## 📖 Tutorial

Um tutorial completo em chinês para iniciantes (24 capítulos + 4 apêndices) — veja [tutorials/](tutorials/).

## 📄 Licença

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
