# =============================================================
# i18n-rust 本地门禁快捷入口
#
# 与 CI（.github/workflows/ci.yml / release.yml）命令链保持一致：
#   make gate        —— 等价于 CI test job 全链（提交前跑这一个即可）
#   make bench-check —— 等价于 CI bench job（本地阈值 30%）
# 依赖：rustup 工具链（stable + clippy/rustfmt，见 rust-toolchain.toml）、
#       python3（教程与术语表校验）；npm 仅 vsix 目标需要。
# =============================================================

CARGO ?= cargo
PYTHON ?= python3

.PHONY: help fmt fmt-check clippy test gate ui-keys readme-parity mapping-check tutorials tutorials-all site site-serve feishu glossary bench bench-check bench-update vsix clean

help: ## 显示全部可用目标
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  %-16s %s\n", $$1, $$2}'

fmt: ## 格式化全部代码（cargo fmt --all）
	$(CARGO) fmt --all

fmt-check: ## 检查代码格式（CI 同款）
	$(CARGO) fmt --all -- --check

clippy: ## Clippy 零告警（CI 同款：警告视为错误）
	$(CARGO) clippy --workspace --all-targets -- -D warnings

test: ## 全部单元测试（debug + release，锁定依赖；CI 同款）
	$(CARGO) test --workspace --locked
	$(CARGO) test --workspace --release --locked

gate: fmt-check clippy test ui-keys readme-parity mapping-check tutorials-all glossary ## 提交前完整门禁（= CI test job 全链）

ui-keys: ## ui.toml 键完备性检查（11 语言与 zh 对齐，含占位符一致性）
	$(PYTHON) tools/check-ui-keys.py

readme-parity: ## 翻译 README 结构 parity 检查（章节数/锚点有效性/围栏配对，基准 en）
	$(PYTHON) tools/check-readme-parity.py

mapping-check: ## 第三方库映射质量门禁（重复键/避让/跨文件冲突/跨语言条目数/告警基线）
	$(CARGO) build --quiet --bin rzc
	$(PYTHON) tools/check-mapping-warnings.py

tutorials: ## 教程代码块编译验证（zh 全量，携带预期失败白名单）
	$(CARGO) build --quiet --bin rzc
	$(PYTHON) tools/verify-tutorials.py --rzc target/debug/rzc --allowlist tools/expected-failures.json

tutorials-all: ## 多语言教程门禁（zh/en/ja/ru 四语依次验证）
	$(CARGO) build --quiet --bin rzc
	$(PYTHON) tools/verify-tutorials.py --allowlist tools/expected-failures.json
	$(PYTHON) tools/verify-tutorials.py --dir tutorials/en --lang en --allowlist tools/expected-failures.json
	$(PYTHON) tools/verify-tutorials.py --dir tutorials/ja --lang ja --allowlist tools/expected-failures.json
	$(PYTHON) tools/verify-tutorials.py --dir tutorials/ru --lang ru --allowlist tools/expected-failures.json

site: ## 构建文档站到 _site/（多语言 mdBook；需 mdbook，安装见 book/README.md）
	$(PYTHON) tools/build-site.py

site-serve: ## 本地预览文档站（mdbook serve，默认中文书，:3000）
	$(PYTHON) tools/build-site.py --serve zh

feishu: ## 同步中文教程到飞书知识库（凭证见 docs/strategy/发布准备清单.md 第 5 节）
	$(PYTHON) tools/publish-feishu.py

glossary: ## 术语表与语言包一致性检查
	$(PYTHON) tools/verify-glossary.py

bench: ## 运行基准（engine 转译管线 + LSP 热路径，只出结果）
	$(CARGO) bench -p i18n-rust-engine --bench transpile
	$(CARGO) bench -p i18n-rust-lsp --bench hot_paths

bench-check: ## 基准回归门禁（对比入库基线，本地阈值 30%）
	tools/bench-check.sh

bench-update: ## 刷新入库基线（engine + lsp 两套，随提交）
	tools/bench-check.sh --update

vsix: ## 本地打包 VS Code 扩展（产物 release/*.vsix）
	cd tools/vscode-extension && npm ci && npm run compile && \
		npx --no-install @vscode/vsce package --out ../../release/

clean: ## 清理构建产物（cargo clean，可再生成）
	$(CARGO) clean
