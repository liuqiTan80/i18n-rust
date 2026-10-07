# rzc 演示 GIF 制作指南

> 15 秒展示 rzc 的三段式卖点：**母语源码 → 母语教学报错 → 一键 eject 标准 Rust**。
> 录屏脚本与演示样本已就绪（本目录），只差一次运行录屏。

## 内容脚本（三幕，约 15 秒）

| 幕 | 命令 | 展示点 | 停留 |
|---|---|---|---|
| ① | `cat src/main.zh` | 中文关键字源码（4 行） | 2s |
| ② | `rzc check src/main.zh` | 中文教学报错：错误码 + 💡 提示 | 3.5s |
| ③ | `rzc eject src/main.zh && cat src/main.rs` | 导出标准 Rust，零锁定 | 3s |

## 文件清单

| 文件 | 说明 |
|---|---|
| `demo-zh.tape` | 中文 [vhs](https://github.com/charmbracelet/vhs) 录屏脚本 |
| `samples/main.zh` | 演示源码（故意触发 E0384 教学报错） |

样本已实机验证：`rzc check` 命中 `E0384` 教学报错，`rzc eject` 成功导出。

## 方案 A：vhs 自动录屏（推荐）

vhs 读取 `.tape` 脚本自动输出 GIF（依赖 `ttyd` 与 `ffmpeg`）。

### 1. 安装 vhs

```bash
brew install vhs                                     # macOS
go install github.com/charmbracelet/vhs@latest       # Linux（Go 环境）
# 无 Go：从 https://github.com/charmbracelet/vhs/releases 下载二进制（Linux x86_64 为例）
curl -fL -o vhs.tar.gz \
  https://github.com/charmbracelet/vhs/releases/download/v0.12.1/vhs_0.12.1_Linux_x86_64.tar.gz
tar xzf vhs.tar.gz && install -m755 vhs_*/vhs ~/.local/bin/vhs
```

依赖 `ttyd` 与 `ffmpeg`（ffmpeg 多数发行版已装）。ttyd 同样从 release 取静态二进制，
**必须放进 PATH**（仅放 `~/.vhs/bin/` 不生效，vhs 仍会报 `ttyd is not installed`）：

```bash
curl -fL -o ~/.local/bin/ttyd \
  https://github.com/tsl0922/ttyd/releases/download/1.7.7/ttyd.x86_64
chmod +x ~/.local/bin/ttyd
```

> 🌐 国内环境若 GitHub 443 不通（SSH 推送正常但下载失败），可在下载 URL 前加
> GitHub 加速代理，例如 `https://gh-proxy.com/<原始 URL>`；代理仅用于本机取工具，
> 不写进仓库与文档产物。

### 2. 准备演示项目（以中文版为例）

```bash
rzc init rzc-demo && cd rzc-demo
cp <仓库>/docs/demo/samples/main.zh src/main.zh
```

### 3. 运行 tape（在演示项目目录内）

```bash
rm -f ~/.rz/cache/transpile-v1.json   # 清转译缓存，使 `rzc check` 段完整展示教学 lint
vhs <仓库>/docs/demo/demo-zh.tape
```

输出的 `demo-zh.gif` 位于当前目录（`Output` 为相对路径，可改 tape 中路径）。
渲染前建议先手动跑一次 `rzc check` 确认环境就绪，再清缓存正式录。

### 4. 收纳产物

把 GIF 放回本目录，供 README 引用：

```
docs/demo/demo-zh.gif
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

## 挂载到 README（已完成）

`docs/demo/demo-zh.gif` 已渲染入库（1080×620、约 12.8 秒、262 KB），并作为首屏
展示挂在 README 主标题之前（镜像说明之后）：

```markdown
![rzc 演示：母语源码 → 母语教学报错 → 一键 eject 标准 Rust](docs/demo/demo-zh.gif)
```

## 常见问题

- **中日文显示为方块**：vhs 默认主题字体不含 CJK，在 tape 中显式 `Set FontFamily "Noto Sans Mono CJK SC"`
  （`demo-zh.tape` 已内置），并确保系统已装该字体；
- **报错行前的 `[时间戳] [警告] [lint]` 噪声**：来自教学 lint 日志（每次转译都会重放，
  包括 `eject`），属预期画面；若想只保留 E0384 教学报错，需先让样本不触发 lint
  （如给 `让` 补上类型标注、把 `10` 提为常量），但会同时丢掉「未标注类型」这类教学提示展示。
- **教学 lint 出现在第三幕而不是第二幕**：转译磁盘缓存（`~/.rz/cache/transpile-v1.json`）
  命中时不重放教学告警，`check` 段会变“干净”而 `eject` 段反而弹出告警；
  录前 `rm -f ~/.rz/cache/transpile-v1.json` 即可（见上文步骤 3）。
- **修复了语言包但 GIF / 终端输出不变**：检查是否被陈旧全局语言包遮蔽——
  解析优先级为 `--lang-pack` 目录 > 项目内 `lang-packs/<语言码>/` >
  `~/.rz/lang-packs/<语言码>/` > 内置表，只要前三者存在旧副本，重新构建的内置新词条
  永远不生效（排障可用 `rzc check --lang-pack <仓库>/crates/engine/lang-packs/zh` 强制验证）。
- **GIF 超过 3 MB**：调低 `Set Width`（如 900）、提高 `Set TypingSpeed`（如 80ms）或缩短 `Sleep`。
- **`rzc: command not found`**：确保 rzc 已安装并在 PATH（参见主 README 安装节），或把 tape 中命令临时改为 `target/release/rzc` 全路径。
