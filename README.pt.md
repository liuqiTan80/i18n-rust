<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

[![CI](https://github.com/liuqiTan80/i18n-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/liuqiTan80/i18n-rust/actions)
[![crates.io](https://img.shields.io/crates/v/rzc.svg)](https://crates.io/crates/rzc)
[![Docs](https://img.shields.io/badge/Docs-Online%20documentation-blue)](https://liuqiTan80.github.io/i18n-rust/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

> 🪞 **Espelho do repositório**: o projeto é mantido em sincronia no GitHub ([liuqiTan80/i18n-rust](https://github.com/liuqiTan80/i18n-rust)) e no GitCode ([tan80/i18n-rust](https://gitcode.com/tan80/i18n-rust)). O `rzc lang install` prefere por padrão a fonte GitCode (mais rápida da China) e recorre automaticamente ao GitHub em caso de falha.

> ⚠️ Esta tradução é baseada na [versão em chinês](README.md) e pode estar desatualizada em relação a ela.

# rzc: Compilador multilíngue do dialeto educacional de Rust

> 🌍 **Escreva Rust no seu idioma nativo.** · 10 idiomas · Rust de verdade, toolchain de verdade · forme-se quando quiser com `rzc eject`

**O rzc é um compilador multilíngue de dialetos de Rust**: você escreve código no seu idioma nativo, o rzc o traduz em tempo real para Rust padrão, a toolchain oficial compila e executa, e cada diagnóstico volta traduzido para o seu idioma com dicas educacionais. Aprenda programação, não inglês.

- 🌍 **Não é pseudocódigo**: o código no seu idioma é totalmente isomorfo ao Rust padrão — compilação, execução, dependências e ecossistema são 100% reais
- 🎓 **Feito para iniciantes**: as mensagens de erro viram orientações de «o que fazer em seguida» em vez de uma parede de inglês
- 🚪 **Forme-se quando quiser**: `rzc eject` exporta Rust padrão em uma etapa — sem aprisionamento, totalmente compatível com o ecossistema

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

Errar não tem problema — as mensagens também estão no seu idioma:

```
Erro[E0384]: Nao se pode atribuir duas vezes a variavel imutavel `numero`
  --> src/main.pt:3:5
💡 Se voce precisa alterar o valor de uma variavel, declare-a com `deixar mutavel`.
```

O mesmo programa funciona nos 10 dialetos integrados — por exemplo, em chinês (`函数 主函数()`, `打印行!`) ou em japonês (`関数 主関数()`, `表示行!`). O Rust padrão (`fn main()`) é sempre aceito como está.

**Comece por aqui**: 🚀 [Início rápido](#início-rápido) · 🌐 [Documentação online](https://liuqiTan80.github.io/i18n-rust/) (4 idiomas) · 📚 [Feishu KB](https://my.feishu.cn/wiki/space/7689728327082314704) (tutorial em chinês, sem login) · 📖 [Aprender passo a passo](#tutorial) · 🤝 [Contribuir](#contribuir)

---

## 📦 Instalação

Duas opções: **Opção 1 — instalar via crates.io** (recomendada, para a maioria); **Opção 2 — compilar a partir do código-fonte** (versão de desenvolvimento mais recente ou compilação local após modificar o rzc).

**Opção 1: instalar via crates.io (recomendada)** — publicado no crates.io; com a toolchain Rust instalada basta um comando:

```bash
cargo install rzc        # baixa do crates.io e compila localmente (1–3 minutos)
rzc --version            # número de versão = sucesso
rzc init meu-projeto && cd meu-projeto && rzc run src/main.pt
```

> Os pacotes de idioma são embutidos — sem configuração extra. Página do crate: <https://crates.io/crates/rzc>; atualização: `cargo install rzc --force`. Ainda sem toolchain Rust? Veja o «1.» abaixo.

**Opção 2: compilar a partir do código-fonte (desenvolvedores)** — para a versão de desenvolvimento mais recente ou compilar após modificar o rzc. **Caminho mais curto** (com a toolchain Rust instalada — 4 comandos até o primeiro programa):

```bash
git clone https://github.com/liuqiTan80/i18n-rust && cd i18n-rust
cargo build --release --workspace   # cerca de 1–3 minutos
cargo install --path crates/cli     # deixa o rzc global (ou use ./target/release/rzc diretamente)
rzc init meu-projeto && cd meu-projeto && rzc run src/main.pt
```

O passo a passo completo da Opção 2 está abaixo (pré-requisitos → compilação → verificação); o «1.» é comum às duas opções.

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

O rust-analyzer standalone oficial é instalado em `~/.rz/toolchain`; o rzc e o servidor de linguagem o preferem automaticamente.

### 5. (Opcional) Compilar a extensão do VS Code

Requer Node.js 18+ e npm (do [nodejs.org](https://nodejs.org)):

```bash
cd tools/vscode-extension
npm ci
npm run package    # gera i18n-rust-<versão>.vsix
```

Instale o `.vsix` via «Install from VSIX...» no VS Code — você ganha realce de sintaxe, autocompletar, diagnósticos, hover e visualização de propriedade; o servidor de linguagem é fornecido pelo `rzc install lsp` (localiza a toolchain embutida automaticamente).

### Configuração completa (um comando por componente)

```bash
rzc install lsp          # servidor de linguagem (backend de autocompletar/diagnóstico/hover do VS Code)
rzc install toolchain    # toolchain oficial embutida (rustc/cargo/rust-analyzer standalone em ~/.rz/toolchain)
rzc doctor               # estado do ambiente da toolchain (embutida / PATH / comparação de versões)
```

Após a instalação, o rzc e o servidor de linguagem preferem automaticamente a toolchain embutida; projetos de arquivo único chamam o rustc diretamente, sem índice do cargo.

### Variáveis de ambiente (opcionais, normalmente desnecessárias)

| Variável | Finalidade |
|---|---|
| `RZ_LANG_DIR` | Diretório de pacotes de idioma (padrão: os embutidos) |
| `RUST_ANALYZER_PATH` | Caminho do rust-analyzer (quando a detecção automática falha) |

A configuração do VS Code `i18n-rust.serverPath` especifica explicitamente o caminho do binário LSP (usada quando a detecção automática falha).

### Atualizar um componente específico da toolchain

| Cenário | Comando |
|---|---|
| Atualizar rustc/cargo (ex.: 1.98 → 1.99) | `rzc install toolchain --version 1.99.0 --force` |
| Atualizar apenas o rust-analyzer (sem baixar 300 MB de novo) | `rzc install toolchain --ra-tag <tag-de-data> --ra-only --force` |
| Ver versões e estado atuais | `rzc doctor` |

### Trocar de editor (VSCodium / Cursor e outros da família VS Code)

A extensão i18n-rust (.vsix) é compatível com todos os editores baseados no VS Code; o rzc, o servidor de linguagem e a toolchain independem do editor:

1. No novo editor: Extensões → «Install from VSIX» → escolha `i18n-rust-<versão>.vsix`;
2. Conecte os componentes nos locais padrão (com o rzc compilado):
   ```bash
   rzc install lsp        # servidor de linguagem → ~/.cargo/bin
   rzc install toolchain  # toolchain embutida → ~/.rz/toolchain (online; ou copie do pacote offline de um distribuidor)
   ```
3. Abra um arquivo `.pt` e pronto (a extensão localiza o servidor e a toolchain automaticamente; `RUST_ANALYZER_PATH` ou a configuração `i18n-rust.serverPath` podem substituir).

### Para distribuidores: pacote de release offline (distribuição em sala de aula)

O repositório inclui um script de empacotamento de um comando (compilação local, pacote para a plataforma atual, para Releases):

| Plataforma | Comando | Artefato |
|---|---|---|
| Linux / macOS | `./release-offline.sh` | `release/rzc-<versão>-linux/macos-<arquitetura>.tar.gz` |
| Windows (PowerShell) | `.\release-offline.ps1` | `release/rzc-<versão>-windows-x86_64.zip` |

Pré-requisito: compilação release (passo 2 acima) e uma execução online de `rzc install toolchain --ra-only --force` (o script copia o rust-analyzer da plataforma para dentro do pacote).

O script detecta a plataforma automaticamente (Linux / Darwin / Windows); o pacote contém rzc, i18n-rust-lsp, rust-analyzer, os 11 pacotes de idioma embutidos (10 idiomas naturais + o pacote identidade `en`) e o tutorial — descompacte e use (detalhes no apêndice D.5 do tutorial).

## 🚀Início rápido

```bash
rzc init meu-projeto        # cria um esqueleto de projeto executável (Cargo.toml + src/main.pt)
cd meu-projeto
rzc run src/main.pt         # traduzir → compilar → executar
```

Em três passos, seu primeiro programa Rust no seu idioma já roda.

---

## 🛠️ Comandos

| Comando | Descrição |
|------------------------------------|------------------------------------------------------|
| `rzc init <nome>` | Criar um projeto (versão da toolchain fixada na local; pronto para IDE) |
| `rzc run <arquivo>` | Traduzir e executar; avisos/erros/progresso de compilação no seu idioma |
| `rzc check <arquivo>` | Verificação de tipos com diagnóstico educacional localizado |
| `rzc eject <arquivo>` | Exportar para código Rust padrão (transição gradual) |
| `rzc transpile <arquivo>` | Apenas traduzir — Rust padrão na saída padrão |
| `rzc cheat <idioma>` | Folha de consulta idioma ↔ Rust (`--markdown` para incorporar em documentos) |
| `rzc add <crate>[@versão]` | Adicionar uma dependência (envolve `cargo add`, com dicas de mapeamento nativo) |
| `rzc doctor` | Diagnosticar o ambiente da toolchain (embutida / PATH / comparação de versões) |
| `rzc lang list` | Listar pacotes de idioma instalados |
| `rzc lang install <código/diretório>` | Instalar um pacote de idioma (registro remoto ou diretório local) |
| `rzc lang search [palavra]` | Pesquisar pacotes de idioma remotos instaláveis |
| `rzc lang remove <código>` | Remover um pacote de idioma instalado pelo usuário |
| `rzc mapping auto <crate> [--target-version <versão>]` | Gerar automaticamente mapeamentos nativos de um crate de terceiros (IA/regras); fixar a versão-base da geração |
| `rzc mapping check [alvo]` | Validar a qualidade dos mapeamentos (chaves duplicadas / colisões de palavras-chave / conflitos entre arquivos / seções obrigatórias) |
| `rzc mapping coverage [--lang <idioma>]` | Medir a cobertura do pacote de idioma com código real; listar mapeamentos ausentes (executar na raiz do repositório) |
| `rzc mapping scaffold <origem> <destino>` | Gerar um esqueleto de tradução para um novo idioma; `--provider deepseek` para tradução com IA |
| `rzc crate search [palavra]` | Pesquisar mapeamentos de terceiros compartilhados pela comunidade (registro) |
| `rzc crate install <crate> --lang <idioma>` | Instalar um mapeamento de terceiros do registro no pacote de idioma global |
| `rzc crate list` | Listar os mapeamentos da comunidade instalados |
| `rzc crate remove <crate> --lang <idioma>` | Remover um mapeamento da comunidade instalado |
| `rzc crate update` | Baixar novamente todos os mapeamentos instalados conforme o manifesto (obter atualizações) |
| `rzc crate publish <crate> --lang <idioma>` | Publicar seus mapeamentos locais no registro (passa antes pelo controle de qualidade) |
| `rzc install <lsp\|toolchain>` | Instalar componentes complementares (servidor de linguagem / toolchain oficial embutida) |

Referência completa: [apêndice D: folha de consulta de comandos rzc (chinês)](tutorials/附录D：rzc命令速查.md).

---

## ✨ Recursos

### Programação no seu idioma

Escreva programas completos com palavras-chave em português (`funcao`, `deixar`, `se`, `responder`…) e a biblioteca padrão no seu idioma (`cadeia`, `vetor::novo()`, `usar padrao::colecoes::mapa`). Macros, tempos de vida, genéricos e traits: tudo suportado.

### 10 idiomas integrados

| Idioma  | Ext.  | Idioma    | Ext.   |
|---------|-------|-----------|--------|
| 中文    | `.zh` | Español   | `.es`  |
| Deutsch | `.de` | Français  | `.fr`  |
| 日本語  | `.ja` | Português | `.pt`  |
| 한국어  | `.ko` | العربية   | `.ar`  |
| Русский | `.ru` | हिन्दी    | `.hi`  |

O próprio Rust é escrito em inglês, portanto o inglês não é um dialeto educacional (um mapeamento identidade não tem valor educacional); no diretório de pacotes de idioma há um pacote identidade `en` separado (extensão `.en`) para cenários de mapeamento identidade e textos de interface em inglês. Os 10 idiomas naturais acima são detectados automaticamente pela extensão do arquivo e podem conviver em um mesmo projeto.

### Diagnóstico de nível educacional

- **Tradução dupla: códigos + mensagens**: cobre códigos de erro do rustc, avisos lint sem código e frases de ajuda
- **Nomes de tipos localizados**: `std::fmt::Display` → `padrao::formato::exibivel`
- **💡 Dicas educacionais**: cada erro inclui uma sugestão de «o que fazer em seguida»; erros de propriedade incluem narrativa de 📌 movimentação/empréstimo
- **Orientação de dependências**: detecta crates de terceiros não declarados e sugere `rzc add <crate>`

### Experiência IDE completa (extensão VS Code / Qoder)

Realce de sintaxe, autocompletar inteligente, documentação ao passar o mouse, ir para definição, pesquisar referências, renomear, formatação de código, executar/verificar com um clique, conversão automática de pontuação de largura total, tradução assistida por IA.

### Crates de terceiros no seu idioma

O `rzc mapping auto` extrai as APIs públicas dos crates instalados e gera nomes no seu idioma (IA); mapeamentos de produção fixam sua versão-base com `--target-version` (registrada no cabeçalho do arquivo); mapeamentos da comunidade passam pelo controle de qualidade `rzc mapping check`.

### Por que não é uma «linguagem de brinquedo»

| | «Linguagens nativas» de brinquedo típicas | rzc |
|---|---|---|
| Forma do código | Sintaxe inventada ou pseudocódigo | Totalmente isomorfo ao Rust padrão — apenas os identificadores são localizados |
| Compilação e execução | Interpretador próprio / apenas transpilar | rustc/cargo oficiais — compilação e execução reais |
| Ecossistema | Fechado ou cortado | Todo o crates.io (`rzc add` + mapeamentos nativos) |
| Experiência de erros | Inglês puro ou dicas caseiras | Mensagens traduzidas + códigos de erro + dicas educacionais |
| Custo de saída | Reescrever do zero | `rzc eject` exporta Rust padrão em uma etapa |

---

## 📖Tutorial

Um tutorial completo em chinês para iniciantes: **26 capítulos + glossário + 5 apêndices** — veja [tutorials/](tutorials/). De «Olá, mundo» a ownership, closures, async e macros, até o projeto final integrador; todos os exemplos são escritos em Rust chinês.

🌐 **Leitura online**: [documentação online](https://liuqiTan80.github.io/i18n-rust/) (mdBook, 4 idiomas com seletor na barra superior; pré-visualização local com `make site-serve`).

**Traduções em andamento**: inglês (prefácio, capítulos 1–13, apêndice D, 15/33), japonês (capítulos 1–11, 11/33), russo (capítulos 1–15, 15/33) — progresso e próxima leva em [translation-status.md](docs/translation-status.md). Traduções para o português são bem-vindas — veja «Contribuir».

> A qualidade do tutorial é protegida pela CI ([tools/verify-tutorials.py](tools/verify-tutorials.py)): cada bloco de código deve compilar e os exemplos de erro devem emitir o código de erro esperado anotado (`// 预期错误: EXXXX`). Verificação local após editar: `make tutorials` (chinês) ou `make tutorials-all` (inglês/japonês/russo).

> Perguntas frequentes e roteiro de aprendizagem: [apêndice E (chinês)](tutorials/附录E：常见问题、迁移指南与学习路线.md).

---

## 🏗️ Estrutura do projeto e como funciona

```text
Código-fonte no seu idioma (.pt)
   │  tradução lexical → substituição de caminhos de módulo → substituição de aliases   ← engine (agnóstico de idioma)
   ▼
Código Rust padrão
   │  cargo build / run (toolchain oficial)
   ▼
Diagnósticos JSON → tradução de códigos/mensagens + localização de tipos + dicas educacionais → saída no seu idioma
```

| Diretório                  | Responsabilidade                                                                |
|----------------------------|---------------------------------------------------------------------------------|
| `crates/engine`            | Motor central agnóstico de idioma: pipeline de tradução, gestão de mapeamentos, tradução de diagnósticos, cache incremental, verificações de segurança Unicode |
| `crates/cli`               | a ferramenta de linha de comando `rzc` |
| `crates/lsp`               | `i18n-rust-lsp`: faz proxy do servidor de linguagem oficial (rust-analyzer), traduzindo posições e diagnósticos nos dois sentidos |
| `crates/engine/lang-packs` | 11 pacotes de idioma embutidos (10 idiomas naturais + o pacote identidade `en`: palavras-chave / biblioteca padrão / caminhos de módulo / traduções de erros / textos de interface) |
| `tools/vscode-extension`   | extensão VS Code / Qoder |
| `tools`                    | portões e scripts de build: verificação do tutorial, regressão de benchmarks, montagem do site de documentação ([tools/README.md](tools/README.md)) |
| `tutorials`                | tutorial chinês de 26 capítulos com apêndices (traduções en/ja/ru em andamento) |
| `book`                     | fonte de montagem do site de documentação (mdBook, [book/README.md](book/README.md)) |
| `docs`                     | documentação de referência e de desenvolvimento, [mapa do projeto](docs/project-map.md); operações e roteiro em [docs/strategy/](docs/strategy/README.md) |

**Princípio de design**: o motor não fixa nenhum idioma específico — adicionar um idioma natural = adicionar um diretório de pacote de idioma (embutido automaticamente na compilação). Para portar esse paradigma a outras linguagens de programação (ex.: Python em chinês), veja [a planta do framework de dialetos](docs/dialect-framework-blueprint.md).

---

## 🤝Contribuir

- **Primeira vez aqui?** Comece pelo [mapa do projeto (manual do mantenedor)](docs/project-map.md) — «onde mudar X e como verificar»; ambiente de desenvolvimento e convenções de commit em [CONTRIBUTING.md](CONTRIBUTING.md)
- **Guia de palavras faltantes (desenvolvedores de aplicativos)**: [docs/missing-mapping-guide.md](docs/missing-mapping-guide.md) (adicionar palavras / personalizar mapeamentos enquanto escreve seu aplicativo — para iniciantes)
- **Adicionar um pacote de idioma**: [docs/contributing-lang-pack.md](docs/contributing-lang-pack.md) (inclui o fluxo de tradução com IA do `rzc mapping scaffold`)
- **Mapeamentos de crates de terceiros**: [docs/third-party-mapping.md](docs/third-party-mapping.md)
- **Registro da comunidade**: [docs/third-party-registry.md](docs/third-party-registry.md) (enviar/baixar mapeamentos da comunidade)
- **Traduzir o tutorial**: parta de `tutorials/`, mantenha a estrutura dos capítulos; painel de progresso em [translation-status.md](docs/translation-status.md)
- Antes de enviar, garanta que `make gate` esteja totalmente verde (equivalente ao portão de testes da CI)

---

## ⭐ Apoiar o projeto

Se o rzc ajuda ou inspira você, deixe uma Star; e traduzir o tutorial para o seu idioma é a melhor forma de apoiar o projeto.

---

## 📄 Licença

[MIT](LICENSE) © tan80
