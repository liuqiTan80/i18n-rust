# 第三方库共享平台（社区映射注册中心）规范

本文档定义 zrRust 第三方库母语映射的**共享注册中心**规范，与
[third-party-mapping.md](./third-party-mapping.md) 的单机映射格式互补：
后者描述「一个 crate 的映射文件长什么样」，本文描述「这些文件如何在社区间
下载与上传、如何被发现与版本化」。

目标：让任何用户都能

- `rzc crate search <关键词>` 发现社区已翻译的第三方库；
- `rzc crate install <crate> --lang zh` 只拉取单个 crate 的映射，免去安装整包语言；
- `rzc crate publish <crate> --lang zh` 把自译映射上传到注册中心，供他人复用。

## 1. 设计原则

- **复用现有远程获取机制**：注册中心即一个 Git 仓库（GitCode/GitHub 或任意
  git 可达地址），复用 `rzc lang install` 已有的 `git clone` / `curl` 下载回退能力，
  零额外后端依赖。注册中心已合并进 zrRust 仓库，位于 `third-party/` 子目录；
  也可通过 `RZ_CRATE_REPO` 指向任意独立 Git 仓库或本地路径。
- **单 crate 粒度**：共享的最小单元是 `(语言, crate)` 映射文件，而非整包语言。
- **索引与数据分离**：注册中心根放一个 `index.json` 作为可发现性索引，映射文件按
  `<语言>/crates/<crate>.toml` 存放（与语言包内 `crates/` 子目录结构一致）。
- **质量门禁在客户端**：发布前 `rzc` 校验 TOML 可解析、节名合法（重复键 TOML 解析
  即失败）；跨文件冲突建议发布前对项目内整包跑一次 `rzc mapping check`。

## 2. 仓库布局

```
zrRust 仓库（合并后）/
└── third-party/               # 注册中心根
    ├── index.json            # 可发现性索引（必选）
    └── <语言>/               # 与语言包目录同名，如 zh / ru / ja
        └── crates/
            ├── serde.toml     # (zh, serde) 映射
            ├── tokio.toml     # (zh, tokio) 映射
            └── ...
```

> 注册中心作为**独立仓库**时，上述结构直接位于该仓库根（`index.json` 与 `<语言>/`
> 同级）；作为 zrRust 子目录时则统一嵌套在 `third-party/` 下。两种布局 `rzc crate`
> 均自动识别。

`index.json` 格式：

```json
{
  "registry": "zrRust-crate-mappings",
  "updated": "2026-09-04",
  "mappings": [
    {
      "crate": "serde",
      "lang": "zh",
      "version": "1.0",
      "author": "tan80",
      "license": "MIT",
      "quality": 1.0,
      "downloads": 128,
      "file": "zh/crates/serde.toml",
      "updated": "2026-09-04"
    }
  ]
}
```

字段说明：

| 字段 | 含义 |
|---|---|
| `crate` | crate 名（连字符归一为下划线，与代码中 `use` 路径一致） |
| `lang` | 语言代码（zh/ru/ja/…） |
| `version` | 该映射的版本（语义自定，默认 `1.0`） |
| `author` | 译者署名（发布时取 `git config user.name`，可 `--author` 覆盖） |
| `license` | 映射文件许可证（默认 `MIT`） |
| `quality` | 质量分（0–1，发布校验通过即 `1.0`，保留扩展空间） |
| `downloads` | 下载计数（注册中心维护；本地 Git 仓库实现为提交计数近似，Web 后端可精确统计） |
| `file` | 映射文件在仓库内的相对路径 |
| `updated` | 最后更新日期（YYYY-MM-DD） |

## 3. 子命令

| 命令 | 说明 |
|---|---|
| `rzc crate search [关键词]` | 拉取注册中心 `index.json`，按 crate 名/语言模糊匹配并打印（空关键词列出全部，按下载量排序） |
| `rzc crate install <crate> --lang <语言> [--force]` | 从注册中心复制 `<语言>/crates/<crate>.toml` 到 `~/.rz/lang-packs/<语言>/crates/`，并记录到已安装清单 |
| `rzc crate list` | 列出已安装的社区映射（来自 `~/.rz/crate-registry.json` 清单 + 全局语言包目录扫描） |
| `rzc crate remove <crate> --lang <语言>` | 从全局语言包删除该映射，并从清单移除 |
| `rzc crate update` | 重新拉取注册中心，按清单对每个映射强制重装（获取他人更新） |
| `rzc crate publish <crate> --lang <语言> [--file <路径>] [--author <名>]` | 校验映射质量后写入注册中心仓库（本地路径直写；远程地址克隆后提交推送），并更新 `index.json` |

## 4. 注册中心地址

- 默认地址：`https://gitcode.com/tan80/zrRust`（即 zrRust 主仓库，注册中心位于其
  `third-party/` 子目录；可用环境变量覆盖）。在 zrRust 源码树内直接运行 `rzc crate`
  会优先使用本地 `./third-party/`，无需网络即可 search/install。
- 覆盖方式：设置 `RZ_CRATE_REPO` 指向你的注册中心仓库（Git URL 或本地路径）。
  本地路径模式便于离线测试与自建私有注册中心：`RZ_CRATE_REPO=/path/to/my-registry rzc crate publish ...`。
- 一键初始化空仓库骨架见 [`tools/init-registry.sh`](../tools/init-registry.sh)。

## 5. 与现有机制的关系

- **整包语言**（`rzc lang install <repo>`）：仍是最完整的分发方式，适合「整套母语
  环境」一次性安装。本平台聚焦其覆盖不到的「单 crate 增量补充」场景。
- **单机映射生成**（`rzc mapping auto <crate>`）：本平台的「供应端」——先本地生成
  与打磨映射，再用 `rzc crate publish` 共享出去。
- **质量校验**（`rzc mapping check`）：本平台发布前的质量门禁基础；跨文件冲突检测
  建议在对整包语言包执行，单文件发布仅做 TOML 解析与节名合法性校验。

## 6. 治理建议（供后续迭代）

- **重复 crate 多版本**：`index.json` 允许同一 `(crate, lang)` 存在多条记录
  （不同 `author`/`version`），`search` 默认展示下载量最高者，`install` 支持
  `--version` 精装（后续扩展）。
- **冲突解决**：社区可对同一映射提不同版本，由下载量/评分自然排序；恶意或低质映射
  由注册中心维护者通过 `git` 历史回滚。
- **许可证**：映射文件建议随 crate 译者声明许可证（默认 MIT），与源码翻译的「事实性
  映射」性质一致。
