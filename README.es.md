<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

> ⚠️ Esta traducción puede estar desactualizada. Consulte la [versión en chino](README.md) o la [versión en inglés](README.en.md) para la información más reciente.

# rzc: Compilador multilingüe del dialecto educativo de Rust

Escribe programas Rust en tu idioma nativo: rzc los traduce automáticamente a Rust estándar y los compila. Aprende programación, no inglés.

```rust
// src/main.es — dialecto educativo de Rust en español
funcion principal() {
    dejar mutable numero = 10;
    numero = numero + 1;
    imprimir_linea!("Número: {}", numero);
}
```

```bash
$ rzc run src/main.es
Número: 11
```

## 📦 Instalación (compilar desde el código fuente)

rzc no ofrece instaladores precompilados en línea — compílalo en tu propia máquina (1–3 minutos).

### Requisitos previos

| Herramienta | Propósito | ¿Necesaria? |
|---|---|---|
| **Toolchain de Rust** (rustc + cargo) | Compilar el propio rzc | ✅ Sí |
| **git** | Obtener el código fuente | ✅ Sí (o descargar el ZIP) |
| **Internet** | Descargar dependencias en la primera compilación | ✅ Sí (primera vez) |
| **Node.js 18+ y npm** | Compilar la extensión de VS Code | Opcional (solo IDE) |
| **rust-analyzer** | Backend de autocompletado/diagnóstico del IDE | Opcional (solo IDE) |

### 1. Instalar el toolchain de Rust

La forma recomendada es [rustup](https://rustup.rs) (instala rustc, cargo y rustup de una vez).

- **Linux / macOS** (en una terminal):

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

  Después ejecuta `source "$HOME/.cargo/env"` (o reabre la terminal).

- **Windows**: descarga [rustup-init.exe](https://rustup.rs) y sigue el asistente (o ejecuta `winget install --id Rustlang.Rustup` en PowerShell).

Verifica (tras reabrir la terminal):

```bash
rustc --version   # p. ej. rustc 1.98.0
cargo --version
```

### 2. Obtener el código y compilar

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace
./target/release/rzc --version
```

La primera compilación descarga dependencias y compila todos los componentes (1–3 minutos). Binarios:

- `target/release/rzc` — la herramienta CLI
- `target/release/i18n-rust-lsp` — el servidor de lenguaje (backend de la extensión de VS Code)

### 3. (Opcional) Hacer rzc disponible globalmente

```bash
cargo install --path crates/cli   # compila localmente e instala en ~/.cargo/bin
```

### 4. (Opcional) Instalar rust-analyzer para funciones del IDE

```bash
rzc install toolchain --ra-only --force
```

### 5. (Opcional) Compilar la extensión de VS Code

Requiere Node.js 18+ y npm (de [nodejs.org](https://nodejs.org)):

```bash
cd tools/vscode-extension
npm ci
npm run package    # genera i18n-rust-<versión>.vsix
```

Instala el `.vsix` mediante «Install from VSIX...» en VS Code; el servidor de lenguaje lo proporciona `rzc install lsp`.

## 🚀 Inicio rápido

```bash
rzc init mi-proyecto
cd mi-proyecto
rzc run src/main.es
```

`rzc init` crea un proyecto listo para ejecutar (`Cargo.toml` + `src/main.es`) — solo tienes que ejecutarlo.

## 🛠️ Comandos

| Comando | Descripción |
|---------|-------------|
| `rzc init <nombre>` | Crear un nuevo proyecto |
| `rzc run <archivo>` | Traducir y ejecutar el código fuente del dialecto |
| `rzc check <archivo>` | Comprobación de tipos con diagnóstico educativo localizado |
| `rzc eject <archivo>` | Exportar a código Rust estándar |
| `rzc lang list` | Listar paquetes de idioma instalados |
| `rzc mapping auto <crate>` | Generar automáticamente mapeos de terceros |

## ✨ Características

- **Programación en tu idioma**: escribe programas Rust completos con las palabras clave de tu lengua
- **Multilingüe por diseño**: 10 paquetes de idioma integrados (es/zh/de/ja/ru/fr/pt/ko/ar/hi), detección automática por extensión
- **Diagnósticos localizados**: `rzc check` traduce los errores de rustc al idioma del archivo, con 💡 pistas educativas
- **Visualización de propiedad**: la extensión de VS Code (busca `i18n-rust`) resalta movimientos y reutilización de variables
- **Soporte LSP completo**: autocompletado, hover, ir a definición, buscar referencias, renombrar
- **Transición gradual**: `rzc eject` exporta código Rust estándar en un solo paso

## 📖 Tutorial

Un tutorial completo en chino para principiantes (26 capítulos + glosario + 5 apéndices) — ver [tutorials/](tutorials/).

## 📄 Licencia

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
