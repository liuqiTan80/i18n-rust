# rzc 文档站（mdBook）

本分支为中文专属：把仓库里的中文教程、附录与参考文档组装成静态站点（仅一本中文书），
并部署到 GitHub Pages。

- 线上地址（启用 Pages 后）：<https://liuqiTan80.github.io/i18n-rust/>
- **md 文件仍在仓库原位**（`tutorials/` 与 `docs/`），本目录只放站点装配：
  每语言的 `book.toml`（mdBook 配置）、`SUMMARY.md`（目录）、`index.md`（书内首页），
  以及所有语言共用的 `lang-switch.js`（顶栏语言切换）。

## 本地构建与预览

```bash
cargo install mdbook --locked --version 0.4.52   # 与 CI 同款版本
make site          # 组装并构建到 _site/
make site-serve    # mdbook serve 本地预览（热重载，默认 3000 端口）
```

## 构建流程（tools/build-site.py）

1. 在 `build/site-work/zh/` 组装工作区：复制中文教程 md
   （`README.md` 除外，站内导航由 SUMMARY 承担）+ `book/zh/` 下的
   `SUMMARY.md` / `index.md` / 配置 / 语言切换脚本；并将 `docs/`
   下六份参考文档复制到 `参考/`，其中 `../` 仓库相对链接改写为 GitHub
   blob 绝对链接；
2. 执行 `mdbook build`，输出到 `_site/zh/`；
3. 生成站根落地页、`404.html` 与 `.nojekyll`。

## 已知限制（骨架阶段）

- 参考文档中的仓库相对链接（源码/README）在站内不可达：脚本已将 `../`
  链接改写为 GitHub blob 链接，参考文档之间的同目录互链保持有效；
- 代码块 Playground 按钮已禁用：中文教程代码为方言源码，标准 Rust
  Playground 无法编译；
- `book.toml` 的 `site-url` 与 `tools/build-site.py` 的 404 逻辑与
  GitHub Pages 地址绑定，更换自定义域名时需同步修改。

## CI

`.github/workflows/pages.yml`：push 到 `main` 且触及教程 / 文档 / 站点装配文件
时，安装同款 mdBook 构建并部署 Pages（也可 `workflow_dispatch` 手动触发）。
首次启用需在仓库 Settings → Pages 将 Source 设为 "GitHub Actions"。
