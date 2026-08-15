<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc: Compilador dialeto Rust multilíngue para ensino

Escreva programas Rust em seu idioma nativo. O rzc traduz automaticamente para Rust padrão, compila e executa — a educação em programação retorna ao pensamento lógico, não à memorização do inglês.

```rust
函数 主函数() {
    让 可变 数量 = 10;
    数量 = 数量 + 1;
    打印行!("数量是：{}", 数量);
}
```

```bash
$ rzc run src/main.zh
数量是：11
```

## ✨ Recursos

- **Programação no idioma nativo**: escreva programas Rust completos com palavras-chave em chinês
- **Multilíngue nativo**: arquitetura que suporta qualquer idioma natural; pacote chinês integrado, outros instaláveis remotamente
- **Detecção automática por extensão**: `.zh`, `.ja`, `.ru` são automaticamente associados ao pacote de idioma
- **Diagnósticos localizados**: erros do rustc traduzidos com 💡 dicas de ensino; erros de propriedade visualizados via JSON
- **Visualização de propriedade**: extensão VS Code destaca movimentos (amarelo), reuso (vermelho) e tempos de vida (verde)
- **Suporte LSP completo**: autocompletar, hover, ir para definição, buscar referências, renomear
- **Autocompletar macros**: o `!` pode ser omitido; adicionado automaticamente na transpilação
- **Transição gradual**: `eject` exporta código Rust padrão em um passo
- **Tutorial completo**: 24 capítulos + 4 apêndices, do iniciante absoluto ao projeto integral

## 📦 Instalação

### Via crates.io (recomendado)

```bash
cargo install rzc
```

### A partir do código-fonte

```bash
# Espelho China
git clone https://gitcode.com/tan80/zrRust.git
# Internacional
git clone https://github.com/liuqiTan80/i18n-rust.git
cd zrRust ou i18n-rust
cargo build --release --workspace
```

## 🚀 Início rápido

```bash
rzc init meu-projeto
cd meu-projeto
rzc run src/main.zh
```

## 🛠️ Comandos

| Comando | Descrição |
|---------|-----------|
| `rzc init <nome>` | Criar novo projeto |
| `rzc run <arquivo>` | Traduzir e executar código `.zh` |
| `rzc check <arquivo>` | Verificação de tipos com diagnósticos |
| `rzc eject <arquivo>` | Exportar para código `.rs` padrão |
| `rzc lang list` | Listar pacotes de idioma instalados |
| `rzc lang install <fonte>` | Instalar pacote de idioma |
| `rzc lang remove <código>` | Remover pacote de idioma |
| `rzc mapping auto <crate>` | Gerar mapeamentos de crates de terceiros |

## 📖 Tutorial

Tutorial completo para iniciantes: 24 capítulos + 4 apêndices

| Etapa | Capítulos |
|-------|-----------|
| **Básico** | Cap.1 Olá mundo · Cap.2 Variáveis e tipos · Cap.3 Tipos compostos · Cap.4 Fluxo de controle · Cap.5 Funções e métodos |
| **Núcleo** | Cap.6 Propriedade · Cap.7 Referências e empréstimo · Cap.8 Strings · Cap.9 Estruturas · Cap.10 Enumerações e padrões |
| **Genéricos** | Cap.11 Genéricos · Cap.12 Traits · Cap.13 Tempos de vida · Cap.14 Coleções |
| **Erros e módulos** | Cap.15 Tratamento de erros · Cap.16 Sistema de módulos · Cap.17 Gestão de pacotes |
| **Avançado** | Cap.18 Ponteiros inteligentes · Cap.19 Concorrência · Cap.20 Testes |
| **Expert** | Cap.21 Closures e iteradores · Cap.22 Macros · Cap.23 Programação assíncrona |
| **Projeto** | Cap.24 Calculadora de linha de comando |
| **Apêndices** | A Referência de mapeamentos · B Glossário · C Guia de migração · D FAQ e caminho de aprendizado |

## ❓ Perguntas frequentes

**P: Por que as macros podem omitir o ponto de exclamação?**
Para reduzir a memorização. O transpilador adiciona `!` automaticamente.

**P: Posso usar nomes de variáveis em chinês?**
Sim. Rust suporta identificadores Unicode.

**P: Como instalo outros pacotes de idioma?**
`rzc lang install 日本語` (remoto) ou `rzc lang install ./diretório` (local).

## 🤝 Contribuir

Feedback via [GitHub Issues](https://github.com/liuqiTan80/i18n-rust/issues), PRs bem-vindos.

## 📄 Licença

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
