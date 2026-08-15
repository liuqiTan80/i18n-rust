<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

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

## 📦 Instalación

Un solo comando — disponible globalmente al instante:

```bash
cargo install rzc
```

> Requiere el [toolchain de Rust](https://www.rust-lang.org/tools/install) (stable vía rustup). Los paquetes de idioma están integrados — no se necesita configuración adicional.

También puedes compilar desde el código fuente:

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace                # binario en target/release/rzc
```

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
- **Multilingüe por diseño**: 11 paquetes de idioma integrados (es/zh/en/de/ja/ru/fr/pt/ko/ar/hi), detección automática por extensión
- **Diagnósticos localizados**: `rzc check` traduce los errores de rustc al idioma del archivo, con 💡 pistas educativas
- **Visualización de propiedad**: la extensión de VS Code (busca `i18n-rust`) resalta movimientos y reutilización de variables
- **Soporte LSP completo**: autocompletado, hover, ir a definición, buscar referencias, renombrar
- **Transición gradual**: `rzc eject` exporta código Rust estándar en un solo paso

## 📖 Tutorial

Un tutorial completo en chino para principiantes (24 capítulos + 4 apéndices) — ver [tutorials/](tutorials/).

## 📄 Licencia

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
