# rzc 演示 GIF 制作指南

> 15 秒展示 rzc 的三段式卖点：**母语源码 → 母语教学报错 → 一键 eject 标准 Rust**。
> 录屏脚本与演示样本已就绪（本目录），只差一次运行录屏。

## 内容脚本（三幕，约 15 秒）

| 幕 | 命令 | 展示点 | 停留 |
|---|---|---|---|
| ① | `cat src/main.<lang>` | 母语关键字源码（4 行） | 2s |
| ② | `rzc check src/main.<lang>` | 母语教学报错：错误码 + 💡 提示 | 3.5s |
| ③ | `rzc eject src/main.<lang> && cat src/main.rs` | 导出标准 Rust，零锁定 | 3s |

## 文件清单

| 文件 | 说明 |
|---|---|
| `demo-zh.tape` / `demo-ja.tape` / `demo-ru.tape` | 三语言 [vhs](https://github.com/charmbracelet/vhs) 录屏脚本 |
| `samples/main.zh` / `main.ja` / `main.ru` | 演示源码（故意触发 E0384 教学报错） |

样本已实机验证：三语言 `rzc check` 均命中 `E0384` 教学报错，`rzc eject` 均成功导出。

## 方案 A：vhs 自动录屏（推荐）

vhs 读取 `.tape` 脚本自动输出 GIF（依赖 `ttyd` 与 `ffmpeg`）。

### 1. 安装 vhs

```bash
brew install vhs                                     # macOS
go install github.com/charmbracelet/vhs@latest       # Linux（Go 环境）
# 或无 Go：从 https://github.com/charmbracelet/vhs/releases 下载二进制
```

### 2. 准备演示项目（以中文版为例）

```bash
rzc init rzc-demo && cd rzc-demo
cp <仓库>/docs/demo/samples/main.zh src/main.zh
```

### 3. 运行 tape（在演示项目目录内）

```bash
vhs <仓库>/docs/demo/demo-zh.tape
```

输出的 `demo-zh.gif` 位于当前目录（`Output` 为相对路径，可改 tape 中路径）。

日语 / 俄语同理：`rzc init rzc-demo-ja --lang ja`、复制对应 sample、运行对应 tape。

### 4. 收纳产物

把 GIF 放回本目录，供 README 引用：

```
docs/demo/demo-zh.gif
docs/demo/demo-ja.gif
docs/demo/demo-ru.gif
```

## 方案 B：手动录屏（无 vhs 时）

1. 终端窗口调到约 1080×620，等宽字体 ≥16px；
2. 按上面「内容脚本」在演示项目中依次执行三条命令（借助已生成的 `src/main.rs` 展示 ejected 结果）；
3. 录屏为 mp4 后转 GIF 并压缩：

```bash
ffmpeg -i demo.mp4 -vf "fps=12,scale=1080:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse" -loop 0 demo-zh.gif
```

## 规格要求

| 指标 | 建议值 |
|---|---|
| 时长 | 12–15 秒（README 首屏；超过 20 秒劝退） |
| 宽度 | ≤ 1100px（GitHub 正文显示宽度约 900px） |
| 体积 | < 3 MB（超出则降 fps 或缩短尾部 `Sleep`） |
| 内容 | 三幕缺一不可：母语源码 / 教学报错 / eject |

## 挂载到 README（录屏完成后）

在 [README.md](../../README.md) 顶部（镜像说明之后、主标题之前）插入：

```markdown
![rzc 演示：用母语编写真正的 Rust](docs/demo/demo-zh.gif)
```

README.en.md 可复用同一份（GIF 中命令与报错结构自明），或按同法录一版以英文为主的镜头。

## 常见问题

- **中日文显示为方块**：vhs 默认字体不含 CJK，在 tape 末尾追加 `Set FontFamily "Noto Sans Mono CJK SC"`（俄语用 `Noto Sans Mono`），并确保系统已装该字体。
- **GIF 超过 3 MB**：调低 `Set Width`（如 900）、提高 `Set TypingSpeed`（如 80ms）或缩短 `Sleep`。
- **`rzc: command not found`**：确保 rzc 已安装并在 PATH（参见主 README 安装节），或把 tape 中命令临时改为 `target/release/rzc` 全路径。
