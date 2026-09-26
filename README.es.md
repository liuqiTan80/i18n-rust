<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

[![CI](https://github.com/liuqiTan80/i18n-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/liuqiTan80/i18n-rust/actions)
[![crates.io](https://img.shields.io/crates/v/rzc.svg)](https://crates.io/crates/rzc)
[![Docs](https://img.shields.io/badge/Docs-Online%20documentation-blue)](https://liuqiTan80.github.io/i18n-rust/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

> 🪞 **Espejo del repositorio**: el proyecto se mantiene en sincronía en GitHub ([liuqiTan80/i18n-rust](https://github.com/liuqiTan80/i18n-rust)) y GitCode ([tan80/i18n-rust](https://gitcode.com/tan80/i18n-rust)). `rzc lang install` prefiere por defecto la fuente GitCode (más rápida desde China) y recurre automáticamente a GitHub si falla.

> ⚠️ Esta traducción se basa en la [versión en chino](README.md) y puede estar desactualizada respecto a ella.

# rzc: Compilador multilingüe del dialecto educativo de Rust

> 🌍 **Escribe Rust en tu lengua materna.** · 10 idiomas · Rust real, toolchain real · gradúate cuando quieras con `rzc eject`

**rzc es un compilador multilingüe de dialectos de Rust**: escribes código en tu lengua materna, rzc lo traduce en tiempo real a Rust estándar, el toolchain oficial lo compila y lo ejecuta, y cada mensaje de diagnóstico vuelve traducido a tu idioma con pistas didácticas. Aprende programación, no inglés.

- 🌍 **No es pseudocódigo**: el código en tu idioma es completamente isomorfo al Rust estándar — compilación, ejecución, dependencias y ecosistema son 100 % reales
- 🎓 **Pensado para principiantes**: los mensajes de error se convierten en guías de «qué hacer a continuación» en vez de un muro de inglés
- 🚪 **Gradúate cuando quieras**: `rzc eject` exporta Rust estándar en un paso — sin ataduras, totalmente compatible con el ecosistema

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

Equivocarse no pasa nada — los errores también están en tu idioma:

```
Error[E0384]: No se puede asignar dos veces a la variable inmutable `numero`
  --> src/main.es:3:5
💡 Si necesita cambiar el valor de una variable, declárela con `dejar mutable`.
```

El mismo programa funciona en los 10 dialectos integrados — por ejemplo, en chino (`函数 主函数()`, `打印行!`) o en japonés (`関数 主関数()`, `表示行!`). El Rust estándar (`fn main()`) siempre se acepta tal cual.

**Empieza aquí**: 🚀 [Inicio rápido](#inicio-rápido) · 🌐 [Documentación en línea](https://liuqiTan80.github.io/i18n-rust/) (4 idiomas) · 📚 [Feishu KB](https://my.feishu.cn/wiki/space/7689728327082314704) (tutorial en chino, sin inicio de sesión) · 📖 [Aprender paso a paso](#tutorial) · 🤝 [Contribuir](#contribuir)

---

## 📦 Instalación

Dos opciones: **Opción 1 — instalar desde crates.io** (recomendada, para la mayoría); **Opción 2 — compilar desde el código fuente** (versión de desarrollo más reciente o compilación local tras modificar rzc).

**Opción 1: instalar desde crates.io (recomendada)** — publicado en crates.io; con el toolchain de Rust instalado basta un comando:

```bash
cargo install rzc        # descarga desde crates.io y compila localmente (1–3 minutos)
rzc --version            # un número de versión = éxito
rzc init mi-proyecto && cd mi-proyecto && rzc run src/main.es
```

> Los paquetes de idioma vienen incluidos — sin configuración adicional. Página del crate: <https://crates.io/crates/rzc>; actualización: `cargo install rzc --force`. ¿Aún sin toolchain de Rust? Primero el «1.» de abajo.

**Opción 2: compilar desde el código fuente (desarrolladores)** — para la última versión de desarrollo o compilar tras modificar rzc. **Ruta más corta** (con el toolchain de Rust instalado — 4 comandos hasta tu primer programa):

```bash
git clone https://github.com/liuqiTan80/i18n-rust && cd i18n-rust
cargo build --release --workspace   # unos 1–3 minutos
cargo install --path crates/cli     # hace rzc global (también puedes usar ./target/release/rzc directamente)
rzc init mi-proyecto && cd mi-proyecto && rzc run src/main.es
```

Abajo está la guía completa paso a paso de la Opción 2 (requisitos → compilación → verificación); el «1.» es común a ambas opciones.

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

El rust-analyzer standalone oficial se instala en `~/.rz/toolchain`; rzc y el servidor de lenguaje lo prefieren automáticamente.

### 5. (Opcional) Compilar la extensión de VS Code

Requiere Node.js 18+ y npm (de [nodejs.org](https://nodejs.org)):

```bash
cd tools/vscode-extension
npm ci
npm run package    # genera i18n-rust-<versión>.vsix
```

Instala el `.vsix` mediante «Install from VSIX...» en VS Code — tendrás resaltado de sintaxis, autocompletado, diagnósticos, hover y visualización de propiedad; el servidor de lenguaje lo proporciona `rzc install lsp` (localiza el toolchain integrado automáticamente).

### Configuración completa (un comando por componente)

```bash
rzc install lsp          # servidor de lenguaje (backend de autocompletado/diagnóstico/hover de VS Code)
rzc install toolchain    # toolchain oficial integrado (rustc/cargo/rust-analyzer standalone en ~/.rz/toolchain)
rzc doctor               # estado del entorno del toolchain (integrado / PATH / comparación de versiones)
```

Tras instalarlo, rzc y el servidor de lenguaje prefieren automáticamente el toolchain integrado; los proyectos de un solo archivo llaman a rustc directamente, sin índice de cargo.

### Variables de entorno (opcionales, normalmente innecesarias)

| Variable | Propósito |
|---|---|
| `RZ_LANG_DIR` | Directorio de paquetes de idioma (por defecto, los integrados) |
| `RUST_ANALYZER_PATH` | Ruta a rust-analyzer (cuando falla la detección automática) |

El ajuste de VS Code `i18n-rust.serverPath` especifica explícitamente la ruta del binario LSP (se usa cuando falla la detección automática).

### Actualizar un componente concreto del toolchain

| Escenario | Comando |
|---|---|
| Actualizar rustc/cargo (p. ej. 1.98 → 1.99) | `rzc install toolchain --version 1.99.0 --force` |
| Actualizar solo rust-analyzer (sin volver a descargar 300 MB) | `rzc install toolchain --ra-tag <etiqueta-de-fecha> --ra-only --force` |
| Ver versiones y estado actuales | `rzc doctor` |

### Cambiar de editor (VSCodium / Cursor y otros de la familia VS Code)

La extensión i18n-rust (.vsix) es compatible con todos los editores basados en VS Code; rzc, el servidor de lenguaje y el toolchain son independientes del editor:

1. En el nuevo editor: Extensiones → «Install from VSIX» → elige `i18n-rust-<versión>.vsix`;
2. Conecta los componentes en las ubicaciones estándar (con el rzc compilado):
   ```bash
   rzc install lsp        # servidor de lenguaje → ~/.cargo/bin
   rzc install toolchain  # toolchain integrado → ~/.rz/toolchain (en línea; o copia desde el paquete offline de un distribuidor)
   ```
3. Abre un archivo `.es` y listo (la extensión localiza el servidor y el toolchain automáticamente; `RUST_ANALYZER_PATH` o el ajuste `i18n-rust.serverPath` pueden sobrescribir).

### Para distribuidores: paquete de lanzamiento offline (distribución en aulas)

El repositorio incluye un script de empaquetado de un solo comando (compila en local y genera un paquete para la plataforma actual, para Releases / almacenamiento de archivos):

| Plataforma | Comando | Artefacto |
|---|---|---|
| Linux / macOS | `./release-offline.sh` | `release/rzc-<versión>-linux/macos-<arquitectura>.tar.gz` |
| Windows (PowerShell) | `.\release-offline.ps1` | `release/rzc-<versión>-windows-x86_64.zip` |

Requisito previo: haber compilado en release (paso 2) y una ejecución en línea de `rzc install toolchain --ra-only --force` (el script copia el rust-analyzer de la plataforma dentro del paquete).

El script detecta la plataforma automáticamente (Linux / Darwin / Windows); el paquete incluye rzc, i18n-rust-lsp, rust-analyzer, los 11 paquetes de idioma integrados (10 lenguas naturales + el paquete identidad `en`) y el tutorial — descomprime y úsalo (detalles en el apéndice D.5 del tutorial).

## 🚀Inicio rápido

```bash
rzc init mi-proyecto        # crea un proyecto listo para ejecutar (Cargo.toml + src/main.es)
cd mi-proyecto
rzc run src/main.es         # traducir → compilar → ejecutar
```

En tres pasos, tu primer programa Rust en tu idioma ya funciona.

---

## 🛠️ Comandos

| Comando | Descripción |
|------------------------------------|------------------------------------------------------|
| `rzc init <nombre>` | Crear un proyecto (versión del toolchain fijada a la local; listo para el IDE) |
| `rzc run <archivo>` | Traducir y ejecutar; avisos/errores/progreso de compilación en tu idioma |
| `rzc check <archivo>` | Comprobación de tipos con diagnóstico educativo localizado |
| `rzc eject <archivo>` | Exportar a código Rust estándar (transición gradual) |
| `rzc transpile <archivo>` | Solo traducir — Rust estándar por stdout |
| `rzc cheat <idioma>` | Chuleta idioma ↔ Rust (`--markdown` para incrustar en documentos) |
| `rzc add <crate>[@versión]` | Añadir una dependencia (envuelve `cargo add`, con pistas de mapeo nativo) |
| `rzc doctor` | Diagnosticar el entorno del toolchain (integrado / PATH / comparación de versiones) |
| `rzc lang list` | Listar paquetes de idioma instalados |
| `rzc lang install <código/directorio>` | Instalar un paquete de idioma (registro remoto o directorio local) |
| `rzc lang search [palabra]` | Buscar paquetes de idioma remotos instalables |
| `rzc lang remove <código>` | Eliminar un paquete de idioma instalado por el usuario |
| `rzc mapping auto <crate> [--target-version <versión>]` | Generar automáticamente mapeos nativos de un crate de terceros (IA/reglas); fijar la versión base de generación |
| `rzc mapping check [objetivo]` | Validar la calidad de los mapeos (claves duplicadas / colisiones de palabras clave / conflictos entre archivos / secciones obligatorias) |
| `rzc mapping coverage [--lang <idioma>]` | Medir la cobertura del paquete de idioma con código real; listar mapeos faltantes (ejecutar en la raíz del repositorio) |
| `rzc mapping scaffold <origen> <destino>` | Generar un esqueleto de traducción para un idioma nuevo; `--provider deepseek` para traducción con IA |
| `rzc crate search [palabra]` | Buscar mapeos de terceros compartidos por la comunidad (registro) |
| `rzc crate install <crate> --lang <idioma>` | Instalar un mapeo de terceros del registro en el paquete de idioma global |
| `rzc crate list` | Listar los mapeos de la comunidad instalados |
| `rzc crate remove <crate> --lang <idioma>` | Eliminar un mapeo de la comunidad instalado |
| `rzc crate update` | Volver a descargar todos los mapeos instalados según el manifiesto (obtener actualizaciones) |
| `rzc crate publish <crate> --lang <idioma>` | Publicar tus mapeos locales en el registro (pasa antes el control de calidad) |
| `rzc install <lsp\|toolchain>` | Instalar componentes complementarios (servidor de lenguaje / toolchain oficial integrado) |

Referencia completa: [apéndice D: chuleta de comandos rzc (chino)](tutorials/附录D：rzc命令速查.md).

---

## ✨ Características

### Programación en tu idioma

Escribe programas completos con palabras clave en español (`funcion`, `dejar`, `si`, `coincidir`…) y la biblioteca estándar en tu idioma (`cadena`, `vector::nuevo()`, `usar estandar::colecciones::mapa`). Macros, tiempos de vida, genéricos y traits: todo compatible.

### 10 idiomas integrados

| Idioma  | Ext.  | Idioma    | Ext.   |
|---------|-------|-----------|--------|
| 中文    | `.zh` | Español   | `.es`  |
| Deutsch | `.de` | Français  | `.fr`  |
| 日本語  | `.ja` | Português | `.pt`  |
| 한국어  | `.ko` | العربية   | `.ar`  |
| Русский | `.ru` | हिन्दी    | `.hi`  |

Rust en sí está escrito en inglés, por lo que el inglés no es un dialecto educativo (un mapeo identidad no aporta valor didáctico); en el directorio de paquetes de idioma hay un paquete identidad `en` aparte (extensión `.en`) para escenarios de mapeo identidad y textos de interfaz en inglés. Los 10 idiomas naturales de la tabla se detectan automáticamente por la extensión del archivo y pueden convivir en un mismo proyecto.

### Diagnóstico de nivel educativo

- **Traducción doble: códigos + mensajes**: cubre códigos de error de rustc, avisos lint sin código y frases de ayuda
- **Nombres de tipos localizados**: `std::fmt::Display` → `estandar::formato::mostrable`
- **💡 Pistas didácticas**: cada error incluye una sugerencia de «qué hacer a continuación»; los errores de propiedad añaden una narrativa de 📌 movimiento/préstamo
- **Guía de dependencias**: detecta crates de terceros no declarados y sugiere `rzc add <crate>`

### Experiencia IDE completa (extensión de VS Code / Qoder)

Resaltado de sintaxis, autocompletado inteligente, documentación al pasar el cursor, ir a definición, buscar referencias, renombrar, formateo de código, ejecutar/comprobar con un clic, conversión automática de puntuación de ancho completo, traducción asistida por IA.

### Crates de terceros en tu idioma

`rzc mapping auto` extrae las APIs públicas de los crates instalados y genera nombres en tu idioma (IA); los mapeos de producción fijan su versión base con `--target-version` (registrada en la cabecera del archivo); los mapeos de la comunidad pasan el control de calidad `rzc mapping check`.

### Por qué no es un «lenguaje de juguete»

| | Típicos «lenguajes nativos» de juguete | rzc |
|---|---|---|
| Forma del código | Sintaxis inventada o pseudocódigo | Totalmente isomorfo al Rust estándar — solo se localizan los identificadores |
| Compilación y ejecución | Intérprete propio / solo traducir | rustc/cargo oficiales — compilación y ejecución reales |
| Ecosistema | Cerrado o recortado | Todo crates.io (`rzc add` + mapeos nativos) |
| Experiencia de errores | Inglés tal cual o pistas caseras | Mensajes traducidos + códigos de error + pistas didácticas |
| Coste de salida | Reescribir desde cero | `rzc eject` exporta Rust estándar en un paso |

---

## 📖Tutorial

Un tutorial completo en chino para principiantes: **26 capítulos + glosario + 5 apéndices** — ver [tutorials/](tutorials/). Desde «Hola, mundo» hasta ownership, closures, async y macros, hasta el proyecto final integrador; todos los ejemplos están escritos en Rust chino.

🌐 **Lectura en línea**: [documentación en línea](https://liuqiTan80.github.io/i18n-rust/) (mdBook, 4 idiomas con selector en la barra superior; vista previa local con `make site-serve`).

**Traducciones en curso**: inglés (prefacio, capítulos 1–13, apéndice D, 15/33), japonés (capítulos 1–11, 11/33), ruso (capítulos 1–15, 15/33) — progreso y siguiente lote en [translation-status.md](docs/translation-status.md). Las traducciones al español son bienvenidas — consulta «Contribuir».

> La calidad del tutorial está protegida por CI ([tools/verify-tutorials.py](tools/verify-tutorials.py)): cada bloque de código debe compilar y los ejemplos de error deben emitir el código de error esperado anotado (`// 预期错误: EXXXX`). Verificación local tras editar: `make tutorials` (chino) o `make tutorials-all` (inglés/japonés/ruso).

> Preguntas frecuentes y hoja de ruta de aprendizaje: [apéndice E (chino)](tutorials/附录E：常见问题、迁移指南与学习路线.md).

---

## 🏗️ Estructura del proyecto y cómo funciona

```text
Código fuente en tu idioma (.es)
   │  traducción léxica → reemplazo de rutas de módulo → reemplazo de alias   ← engine (agnóstico del idioma)
   ▼
Código Rust estándar
   │  cargo build / run (toolchain oficial)
   ▼
Diagnósticos JSON → traducción de códigos/mensajes + localización de tipos + pistas didácticas → salida en tu idioma
```

| Directorio                 | Responsabilidad                                                                 |
|----------------------------|---------------------------------------------------------------------------------|
| `crates/engine`            | Motor central agnóstico del idioma: pipeline de traducción, gestión de mapeos, traducción de diagnósticos, caché incremental, comprobaciones de seguridad Unicode |
| `crates/cli`               | la herramienta de línea de comandos `rzc` |
| `crates/lsp`               | `i18n-rust-lsp`: hace de proxy del servidor de lenguaje oficial (rust-analyzer), traduciendo posiciones y diagnósticos en ambos sentidos |
| `crates/engine/lang-packs` | 11 paquetes de idioma integrados (10 lenguas naturales + el paquete identidad `en`: palabras clave / biblioteca estándar / rutas de módulo / traducciones de errores / textos de interfaz) |
| `tools/vscode-extension`   | extensión de VS Code / Qoder |
| `tools`                    | puertas y scripts de compilación: verificación del tutorial, regresión de benchmarks, ensamblado del sitio de documentación ([tools/README.md](tools/README.md)) |
| `tutorials`                | tutorial chino de 26 capítulos con apéndices (traducciones en/ja/ru en curso) |
| `book`                     | fuente de ensamblado del sitio de documentación (mdBook, [book/README.md](book/README.md)) |
| `docs`                     | documentación de referencia y de desarrollo, [mapa del proyecto](docs/project-map.md); operaciones y hoja de ruta en [docs/strategy/](docs/strategy/README.md) |

**Principio de diseño**: el motor no fija ningún idioma concreto — añadir una lengua natural = añadir un directorio de paquete de idioma (se integra automáticamente en la compilación). Para portar este paradigma a otros lenguajes de programación (p. ej. Python en chino), consulta el [plano del framework de dialectos](docs/dialect-framework-blueprint.md).

---

## 🤝Contribuir

- **¿Primera vez?** Empieza por el [mapa del proyecto (manual del mantenedor)](docs/project-map.md) — «dónde cambiar X y cómo verificarlo»; entorno de desarrollo y convenciones de commit en [CONTRIBUTING.md](CONTRIBUTING.md)
- **Guía de palabras faltantes (para desarrolladores de apps)**: [docs/missing-mapping-guide.md](docs/missing-mapping-guide.md) (añadir palabras / personalizar mapeos mientras escribes tu app — para principiantes)
- **Añadir un paquete de idioma**: [docs/contributing-lang-pack.md](docs/contributing-lang-pack.md) (incluye el flujo de traducción con IA de `rzc mapping scaffold`)
- **Mapeos de crates de terceros**: [docs/third-party-mapping.md](docs/third-party-mapping.md)
- **Registro comunitario**: [docs/third-party-registry.md](docs/third-party-registry.md) (subir/descargar mapeos de la comunidad)
- **Traducir el tutorial**: parte de `tutorials/`, mantén la estructura de capítulos; panel de progreso en [translation-status.md](docs/translation-status.md)
- Antes de enviar, asegúrate de que `make gate` esté completamente en verde (equivalente a la puerta de CI)

---

## ⭐ Apoyar el proyecto

Si rzc te ayuda o te inspira, déjanos una Star; y traducir el tutorial a tu idioma es la mejor forma de apoyar el proyecto.

---

## 📄 Licencia

[MIT](LICENSE) © tan80
