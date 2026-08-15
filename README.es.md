<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc: Compilador dialecto de Rust para enseñanza multilingüe

Escribe programas en Rust en tu idioma nativo. rzc los traduce automáticamente a Rust estándar, compila y ejecuta — la educación en programación vuelve al pensamiento lógico, no a la memorización del inglés.

```rust
// src/main.es — dialecto español de Rust para enseñanza
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

## ✨ Características

- **Programación en idioma nativo**: escribe programas completos en Rust con palabras clave en español (`funcion`, `dejar`, `si`, `devolver`…)
- **Multilingüe nativo**: la arquitectura soporta cualquier idioma natural; 11 paquetes incluidos, otros instalables remotamente
- **Detección automática por extensión**: `.zh`, `.en`, `.de` se asocian automáticamente al paquete de idioma
- **Diagnósticos localizados**: errores de rustc traducidos con 💡 consejos didácticos; errores de propiedad visualizados con JSON
- **Visualización de propiedad**: la extensión VS Code resalta movimientos (amarillo), reuso (rojo) y tiempos de vida (verde)
- **Soporte LSP completo**: autocompletado, hover, ir a definición, buscar referencias, renombrar
- **Autocompletado de macros**: se puede omitir el `!`; se añade automáticamente al transpilar
- **Transición gradual**: `eject` exporta código Rust estándar en un paso
- **Tutorial completo**: 24 capítulos + 4 apéndices, desde principiante absoluto hasta proyecto integral

## 📦 Instalación

### Vía crates.io (recomendado)

```bash
cargo install rzc
```

### Desde el código fuente

```bash
# Internacional
git clone https://github.com/liuqiTan80/i18n-rust.git

cd i18n-rust
cargo build --release --workspace
```

## 🚀 Inicio rápido

```bash
rzc init mi-proyecto
cd mi-proyecto
rzc run src/main.es
```

## 🛠️ Comandos

| Comando | Descripción |
|---------|-------------|
| `rzc init <nombre>` | Crear nuevo proyecto |
| `rzc run <archivo>` | Traducir y ejecutar código `.es` |
| `rzc check <archivo>` | Verificación de tipos con diagnósticos en español |
| `rzc eject <archivo>` | Exportar a código `.rs` estándar |
| `rzc lang list` | Listar paquetes de idioma instalados |
| `rzc lang install <fuente>` | Instalar paquete de idioma |
| `rzc lang remove <código>` | Eliminar paquete de idioma de usuario |
| `rzc mapping auto <crate>` | Generar mapeos de crates de terceros |

## 🌍 Gestión de paquetes de idioma

rzc incluye 11 paquetes de idioma (español, inglés, chino, alemán, japonés, ruso, francés, portugués, coreano, árabe e hindi, cada uno con traducciones de errores y consejos didácticos).

- **Detección automática**: `main.es` usa español; `main.en` usa inglés; `main.de` usa alemán
- **Instalación remota**: descarga desde GitCode, respaldo en GitHub
- **Instalación local**: `rzc lang install ./mi-paquete`

## 📖 Tutorial

Tutorial completo para principiantes: 24 capítulos + 4 apéndices

| Etapa | Capítulos |
|-------|-----------|
| **Básico** | Cap.1 Hola mundo · Cap.2 Variables y tipos · Cap.3 Tipos compuestos · Cap.4 Flujo de control · Cap.5 Funciones y métodos |
| **Núcleo** | Cap.6 Propiedad · Cap.7 Referencias y préstamo · Cap.8 Cadenas · Cap.9 Estructuras · Cap.10 Enumeraciones y patrones |
| **Genéricos** | Cap.11 Genéricos · Cap.12 Traits · Cap.13 Tiempos de vida · Cap.14 Colecciones |
| **Errores y módulos** | Cap.15 Manejo de errores · Cap.16 Sistema de módulos · Cap.17 Gestión de paquetes |
| **Avanzado** | Cap.18 Punteros inteligentes · Cap.19 Concurrencia · Cap.20 Pruebas |
| **Experto** | Cap.21 Closures e iteradores · Cap.22 Macros · Cap.23 Programación asíncrona |
| **Proyecto** | Cap.24 Calculadora de línea de comandos |
| **Apéndices** | A Referencia de mapeos · B Glosario · C Guía de migración · D FAQ y ruta de aprendizaje |

## ❓ Preguntas frecuentes

**Q: ¿Por qué las macros pueden omitir el signo de exclamación?**
Para reducir la memorización. El transpilador añade `!` automáticamente.

**Q: ¿Puedo usar nombres de variables en español?**
Sí. `numero` y `principal` son identificadores válidos. Rust soporta identificadores Unicode.

**Q: ¿Cómo instalo otros paquetes de idioma?**
`rzc lang install ja` (remoto) o `rzc lang install ./directorio` (local).

## 🤝 Contribuir

Comentarios vía [GitHub Issues](https://github.com/liuqiTan80/i18n-rust/issues), PR bienvenidos.

## 📄 Licencia

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
