# 附录D：rzc 命令速查

忘记命令时来这里查。详细讲解见第一章（rzc 基础）和第二章（VS Code 扩展）。

---

## D.1 rzc 命令行

| 命令 | 作用 | 例子 |
|---|---|---|
| `rzc init 名字` | 新建中文项目（生成 Cargo.toml + 主函数.zh） | `rzc init 我的项目` |
| `rzc run 文件.zh` | 转译 + 编译 + 运行 | `rzc run src/主函数.zh` |
| `rzc check 文件.zh` | 只检查，不生成可执行文件（更快） | `rzc check src/主函数.zh` |
| `rzc eject 文件.zh` | 导出为标准 Rust（生成 .rs） | `rzc eject src/主函数.zh` |
| `rzc lang list` | 查看可用语言包 | |
| `rzc lang install 语言` | 安装语言包 | `rzc lang install ja` |
| `rzc mapping check` | 校验第三方库映射文件 | |
| `rzc mapping scaffold` | 为新语言生成映射脚手架 | |
| `rzc cargo add 库名` | 添加依赖（转发给 cargo） | `rzc cargo add rand` |
| `rzc install lsp` | 安装语言服务器（VS Code 智能提示） | |
| `rzc install toolchain` | 一键安装内置官方工具链（standalone rustc/cargo/rust-analyzer，脱离 rustup） | |
| `rzc doctor` | 诊断工具链环境（内置 / PATH / 版本对比） | |
| `rzc --version` | 查看版本 | |

### 日常流水线

```bash
rzc init 练习簿 && cd 练习簿    # 造项目
rzc run src/主函数.zh              # 写一点，跑一点
rzc check src/主函数.zh            # 改完快速检查
rzc eject src/主函数.zh && cargo test   # 跑测试（第二十一章）
cargo build --release            # 发布优化版（第十八章）
```

---

## D.2 常用 cargo 命令

| 命令 | 作用 |
|---|---|
| `cargo build` | 构建（debug 版） |
| `cargo build --release` | 构建（优化版，给用户） |
| `cargo test` | 运行所有测试 |
| `cargo add 库名` | 添加第三方依赖 |
| `cargo clean` | 清空 target 目录 |

---

## D.3 VS Code 扩展命令

命令面板（`Ctrl+Shift+P`）输入 "i18n" 或 "rzc" 即可找到：

| 命令 | 作用 | 快捷键 |
|---|---|---|
| 运行当前文件 (run) | 编译并运行 | `Ctrl+Shift+R` |
| 检查当前文件 (check) | 只检查 | `Ctrl+Shift+C` |
| 导出标准 Rust (eject) | 转成 .rs | |
| 选择语言包 (selectLanguagePack) | 切换 10 种语言 | |
| 重启语言服务器 (restartServer) | LSP 卡住时用 | |
| AI 对话 (aiChat) | AI 辅助（需配置） | |
| 校验映射 (mappingCheck) | 检查映射文件 | |
| 生成映射脚手架 (mappingScaffold) | 新语言模板 | |
| 安装语言包 (langInstall) | 装社区语言包 | |
| 添加依赖 (cargoAdd) | 图形化加库 | |

编辑器右键 `.zh` 文件也能看到运行/检查/导出菜单。扩展的 20 个代码片段清单见第二章 2.5 节。

---

## D.4 故障急救三招

1. **报错看不懂** → 查附录 C《常见错误信息字典》；
2. **怀疑是映射词撞车** → `rzc eject` 看转译后的英文代码；
3. **语言服务器抽风** → 命令面板执行"重启语言服务器"，或关掉 VS Code 重开。
