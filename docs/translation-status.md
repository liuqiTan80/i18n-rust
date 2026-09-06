# 翻译状态跟踪（Translation Status）

> 教程与文档多语言化的进度看板。翻译原则与管线说明见
> [tutorials/en/README.md](../../tutorials/en/README.md)。

## 教程（tutorials/，33 个文件）

| 目标语言 | 目录 | 进度 | 说明 |
|---|---|---|---|
| 英文 en | `tutorials/en/` | 3/33 | ✅ 开篇、第二章（16/16 代码块验证通过——翻译管线实测完成）、附录D；下一批：第三章《变量与类型》 |
| 日文 ja / 韩文 ko / 德文 de / 西文 es / 法文 fr / 俄文 ru / 葡文 pt / 印地文 hi / 阿文 ar | `tutorials/<lang>/` | 0/33 | 待英文版稳定后按同一管线复制推进 |

**翻译管线**（每个文件四步）：
1. AI 转创翻译正文（不逐字直译，按目标语言技术写作习惯重写）；
2. 代码块保持与中文原版逐字节一致（方言代码不翻译）；
3. `python3 tools/verify-tutorials.py --dir tutorials/<lang> --allowlist tools/expected-failures.json` 验证全部代码块可编译；
4. 人工抽校后提交。

## 文档（README ×11 / docs/）

| 文档 | 状态 |
|---|---|
| README.md / README.en.md | ✅ 最新（含英文 tagline） |
| 9 份翻译 README | ✅ 已补母语 tagline；正文为早期快照，随教程英文化后按需刷新 |
| docs/contributing-lang-pack.md 等 | ⬜ 待英文化（第 11 种语言贡献路径） |

## 近期完成

- 2026-09-06（第二批）：第二章英文转创版完成——翻译管线首次实测：16 个代码块
  （12 完整程序 + 4 错误示例）全部验证通过；中文原版同步修正 12 处中文化
  误替换缺陷（关键词表英文列、幕后揭秘图示、eject 示例、章节编号等）。

- 2026-09-06：教程全面核查（387 代码块 0 回归、术语表/链接全通过）；修正附录 D
  缺失命令（cheat/crate/transpile/mapping auto）、第十八章旧命令名；新增英文
  开篇与附录 D；9 份翻译 README 补母语 tagline。
