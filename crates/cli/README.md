# rzc — 多语言 Rust 教学方言编译器

用你的母语编写 Rust 程序，rzc 自动翻译为标准 Rust 并编译运行——学编程，不背英语。

```rust
// src/main.zh —— 中文 Rust 教学方言
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

## 安装

一条命令，装完即可全局使用：

```bash
cargo install rzc
```

> 需要已安装 Rust 工具链（rustup 的 stable 即可）。语言包已内置，无需任何额外配置。

## 快速开始

```bash
rzc init 我的项目
cd 我的项目
rzc run src/main.zh
```

`rzc init` 生成完整可运行的项目骨架（`Cargo.toml` + `src/main.zh`），直接运行即可。

## 常用命令

| 命令 | 说明 |
|------|------|
| `rzc init <项目名>` | 创建新项目 |
| `rzc run <文件>` | 翻译并运行方言源码 |
| `rzc check <文件>` | 类型检查，输出中文教学诊断 |
| `rzc eject <文件>` | 导出为标准 Rust 代码 |
| `rzc lang` | 语言包管理（list / install / remove） |
| `rzc mapping auto <crate>` | 自动生成第三方库映射 |

## 功能特性

- **母语编程**：用中文关键字（`函数`、`让`、`如果`、`返回`…）编写完整 Rust 程序
- **多语言原生**：内置 11 种语言包（中/英/德/日/俄/西/法/葡/韩/阿/印地），扩展名自动匹配
- **本地化诊断**：`rzc check` 把 rustc 错误翻译为对应语言，附带 💡 教学提示
- **所有权可视化**：配合 VS Code 扩展，颜色高亮变量的移动与再使用
- **完整 LSP 支持**：补全、悬停、定义跳转、引用查找、重命名
- **渐进过渡**：`rzc eject` 一键导出标准 Rust 代码

## 许可证

MIT
