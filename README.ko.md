<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

[![CI](https://github.com/liuqiTan80/i18n-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/liuqiTan80/i18n-rust/actions)
[![crates.io](https://img.shields.io/crates/v/rzc.svg)](https://crates.io/crates/rzc)
[![Docs](https://img.shields.io/badge/Docs-Online%20documentation-blue)](https://liuqiTan80.github.io/i18n-rust/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

> 🪞 **저장소 미러**: 이 프로젝트는 GitHub([liuqiTan80/i18n-rust](https://github.com/liuqiTan80/i18n-rust))와 GitCode([tan80/i18n-rust](https://gitcode.com/tan80/i18n-rust))에 동기화되어 유지됩니다. `rzc lang install`은 기본적으로 GitCode 소스(중국에서 더 빠름)를 우선 사용하고, 실패하면 자동으로 GitHub로 전환합니다.

> ⚠️ 이 번역은 [중국어판](README.md)을 기반으로 하며, 원문에 비해 늦을 수 있습니다.

# rzc: 다국어 Rust 교육 방언 컴파일러

> 🌍 **모국어로 Rust를 쓰세요.** · 10개 언어 · 진짜 Rust, 진짜 툴체인 · `rzc eject`로 언제든 졸업

**rzc는 다국어 Rust 방언 컴파일러**입니다: 모국어로 코드를 작성하면 rzc가 실시간으로 표준 Rust로 번역하고, 공식 툴체인이 컴파일·실행하며, 모든 진단이 교육 힌트와 함께 모국어로 돌아옵니다. 영어가 아니라 프로그래밍을 배우세요.

- 🌍 **의사코드가 아닙니다**: 모국어 코드는 표준 Rust와 완전히 동형입니다 — 컴파일, 실행, 의존성, 생태계가 100% 실제입니다
- 🎓 **초보자를 위해 설계**: 오류 메시지가 영어의 벽 대신 「다음에 무엇을 할까」를 안내합니다
- 🚪 **원할 때 졸업**: `rzc eject`가 표준 Rust를 한 단계로 내보냅니다 — 종속 없음, 생태계와 완전 호환

```rust
// src/main.ko — 한국어 Rust 교육 방언
함수 메인() {
    선언 가변 수 = 10;
    수 = 수 + 1;
    출력_줄!("수: {}", 수);
}
```

```bash
$ rzc run src/main.ko
수: 11
```

실수해도 괜찮습니다 — 오류 메시지도 모국어로 나옵니다:

```
오류[E0384]: 불변 변수 `수`에 두 번 값을 할당할 수 없습니다
  --> src/main.ko:3:5
💡 변수의 값을 변경해야 한다면 `선언 가변`으로 선언하세요.
```

같은 프로그램이 10개 내장 방언에서 모두 동작합니다 — 예를 들어 중국어(`函数 主函数()`, `打印行!`)나 일본어(`関数 主関数()`, `表示行!`)로도. 표준 Rust(`fn main()`)는 언제나 그대로 사용할 수 있습니다.

**여기서 시작하세요**: 🚀 [빠른 시작](#빠른-시작) · 🌐 [온라인 문서](https://liuqiTan80.github.io/i18n-rust/) (4개 언어) · 📚 [Feishu KB](https://my.feishu.cn/wiki/space/7689728327082314704) (중국어 튜토리얼, 로그인 불필요) · 📖 [튜토리얼](#튜토리얼) · 🤝 [기여하기](#기여하기)

---

## 📦 설치

두 가지 방법 중 선택하세요: **방법 1 — crates.io에서 설치** (권장, 대부분의 사용자용); **방법 2 — 소스에서 빌드** (최신 개발 버전 또는 rzc를 수정한 뒤의 로컬 빌드).

**방법 1: crates.io에서 설치 (권장)** — crates.io에 공개되어 있어 Rust 툴체인만 있으면 명령 하나로 끝납니다:

```bash
cargo install rzc        # crates.io에서 받아 로컬에서 빌드 (1~3분)
rzc --version            # 버전 번호가 나오면 성공
rzc init 내-프로젝트 && cd 내-프로젝트 && rzc run src/main.ko
```

> 언어 팩은 내장되어 별도 설정이 필요 없습니다. crate 페이지: <https://crates.io/crates/rzc>; 업그레이드: `cargo install rzc --force`. Rust 툴체인이 없다면 아래 「1.」을 먼저 실행하세요.

**방법 2: 소스에서 빌드 (개발자용)** — 최신 개발 버전을 쓰거나 rzc를 수정한 뒤 컴파일할 때 사용합니다. **가장 짧은 경로** (Rust 툴체인이 설치되어 있다면 첫 프로그램까지 명령 4개):

```bash
git clone https://github.com/liuqiTan80/i18n-rust && cd i18n-rust
cargo build --release --workspace   # 약 1~3분
cargo install --path crates/cli     # rzc를 전역으로 설치 (또는 ./target/release/rzc 직접 사용)
rzc init 내-프로젝트 && cd 내-프로젝트 && rzc run src/main.ko
```

방법 2의 전체 단계는 아래에 있습니다 (사전 요구 사항 → 빌드 → 확인); 「1.」은 두 방법 모두 공통입니다.

### 사전 요구 사항

| 도구 | 용도 | 필수? |
|---|---|---|
| **Rust 툴체인** (rustc + cargo) | rzc 자체 빌드 | ✅ 필수 |
| **git** | 소스 코드 받기 | ✅ 필수 (소스 ZIP도 가능) |
| **네트워크** | 첫 빌드 시 의존성 다운로드 | ✅ 필수 (첫 번째만) |
| **Node.js 18+ 및 npm** | VS Code 확장 빌드 | 선택 (IDE만) |
| **rust-analyzer** | IDE 자동 완성/진단 백엔드 | 선택 (IDE만) |

### 1. Rust 툴체인 설치

공식 권장 방법은 [rustup](https://rustup.rs)입니다 (rustc, cargo, rustup을 한 번에 설치).

- **Linux / macOS** (터미널에서):

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

  설치 후 `source "$HOME/.cargo/env"`를 실행하거나 터미널을 다시 엽니다.

- **Windows**: [rustup-init.exe](https://rustup.rs)를 다운로드해 마법사를 따릅니다 (PowerShell에서 `winget install --id Rustlang.Rustup`도 가능).

확인 (터미널을 다시 연 후):

```bash
rustc --version   # 예: rustc 1.98.0
cargo --version
```

### 2. 소스 코드 받기 및 빌드

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace
./target/release/rzc --version
```

첫 빌드는 의존성 다운로드와 전체 컴포넌트 컴파일로 1~3분이 걸립니다. 산출물:

- `target/release/rzc` — CLI 도구
- `target/release/i18n-rust-lsp` — 언어 서버 (VS Code 확장 백엔드)

### 3. (선택) rzc를 전역으로 사용

```bash
cargo install --path crates/cli   # 로컬 빌드 후 ~/.cargo/bin에 설치
```

### 4. (선택) IDE 기능용 rust-analyzer 설치

```bash
rzc install toolchain --ra-only --force
```

공식 standalone rust-analyzer가 `~/.rz/toolchain`에 설치되며, rzc와 언어 서버가 자동으로 우선 사용합니다.

### 5. (선택) VS Code 확장 빌드

Node.js 18+와 npm이 필요합니다 ([nodejs.org](https://nodejs.org)):

```bash
cd tools/vscode-extension
npm ci
npm run package    # i18n-rust-<버전>.vsix 생성
```

생성된 `.vsix`를 VS Code의 「Install from VSIX...」로 설치합니다 — 구문 강조, 스마트 자동 완성, 진단, 호버, 소유권 시각화를 얻을 수 있으며, 언어 서버는 `rzc install lsp`가 제공합니다 (내장 툴체인을 자동으로 찾습니다).

### 전체 기능 구성 (컴포넌트당 명령 하나)

```bash
rzc install lsp          # 언어 서버 (VS Code 자동 완성/진단/호버 백엔드)
rzc install toolchain    # 내장 공식 툴체인 (~/.rz/toolchain에 standalone rustc/cargo/rust-analyzer)
rzc doctor               # 툴체인 환경 상태 (내장/PATH/버전 비교)
```

설치 후 rzc와 언어 서버는 자동으로 내장 툴체인을 우선 사용합니다; 단일 파일 프로젝트는 cargo 인덱스 없이 rustc를 직접 호출합니다.

### 환경 변수 (선택 사항, 보통 불필요)

| 변수 | 용도 |
|---|---|
| `RZ_LANG_DIR` | 언어 팩 디렉터리 (기본값: 내장 팩) |
| `RUST_ANALYZER_PATH` | rust-analyzer 경로 (자동 감지 실패 시) |

VS Code 설정 `i18n-rust.serverPath`는 LSP 바이너리 경로를 명시적으로 지정합니다 (자동 감지 실패 시 사용).

### 툴체인 특정 컴포넌트 업그레이드

| 상황 | 명령 |
|---|---|
| rustc/cargo 업그레이드 (예: 1.98 → 1.99) | `rzc install toolchain --version 1.99.0 --force` |
| rust-analyzer만 업그레이드 (300MB 재다운로드 없이) | `rzc install toolchain --ra-tag <데이터 태그> --ra-only --force` |
| 현재 버전과 상태 확인 | `rzc doctor` |

### 에디터 변경 (VSCodium / Cursor 등 VS Code 계열)

i18n-rust 확장(.vsix)은 모든 VS Code 기반 에디터와 호환됩니다; rzc, 언어 서버, 툴체인은 에디터에 종속되지 않습니다:

1. 새 에디터에서: 확장 → 「Install from VSIX」 → `i18n-rust-<버전>.vsix` 선택;
2. 표준 위치에 컴포넌트 연결 (rzc가 컴파일되어 있는 경우):
   ```bash
   rzc install lsp        # 언어 서버 → ~/.cargo/bin
   rzc install toolchain  # 내장 툴체인 → ~/.rz/toolchain (온라인; 또는 배포자의 오프라인 패키지에서 복사)
   ```
3. `.ko` 파일을 열면 끝입니다 (확장이 서버와 툴체인을 자동으로 찾습니다; `RUST_ANALYZER_PATH` 또는 `i18n-rust.serverPath` 설정으로 재정의 가능).

### 배포자용: 오프라인 릴리스 패키지 (강의실 일괄 배포)

저장소에는 원클릭 패키징 스크립트가 포함되어 있습니다 (로컬 빌드 후 현재 플랫폼용 패키지를 만들어 Releases/파일 공유에 사용):

| 플랫폼 | 명령 | 산출물 |
|---|---|---|
| Linux / macOS | `./release-offline.sh` | `release/rzc-<버전>-linux/macos-<아키텍처>.tar.gz` |
| Windows (PowerShell) | `.\release-offline.ps1` | `release/rzc-<버전>-windows-x86_64.zip` |

사전 조건: release 빌드(위 2단계)와 온라인 상태에서 `rzc install toolchain --ra-only --force` 1회 실행 (스크립트가 해당 플랫폼의 rust-analyzer를 패키지에 복사합니다).

스크립트는 플랫폼(Linux / Darwin / Windows)을 자동 감지합니다; 패키지에는 rzc, i18n-rust-lsp, rust-analyzer, 11개 내장 언어 팩(10개 자연어 + `en` 아이덴티티 팩)과 튜토리얼이 포함됩니다 — 압축을 풀면 바로 사용할 수 있습니다 (자세한 내용은 튜토리얼 부록 D.5).

## 🚀빠른 시작

```bash
rzc init 내-프로젝트        # 실행 가능한 프로젝트 골격 생성 (Cargo.toml + src/main.ko)
cd 내-프로젝트
rzc run src/main.ko         # 번역 → 컴파일 → 실행
```

세 단계면 모국어로 쓴 첫 Rust 프로그램이 실행됩니다.

---

## 🛠️ 명령어

| 명령 | 설명 |
|------------------------------------|------------------------------------------------------|
| `rzc init <이름>` | 프로젝트 생성 (툴체인 버전을 로컬에 고정, IDE 바로 사용 가능) |
| `rzc run <파일>` | 번역 후 실행; 경고/오류/컴파일 진행 상황이 모국어로 출력 |
| `rzc check <파일>` | 현지화된 교육 진단과 함께 타입 검사 |
| `rzc eject <파일>` | 표준 Rust 코드로 내보내기 (점진적 전환) |
| `rzc transpile <파일>` | 번역만 수행 — 표준 출력으로 표준 Rust 출력 |
| `rzc cheat <언어>` | 언어 ↔ Rust 대조표 (`--markdown`으로 문서 삽입용) |
| `rzc add <crate>[@버전]` | 의존성 추가 (`cargo add` 래핑, 모국어 매핑 힌트 제공) |
| `rzc doctor` | 툴체인 환경 진단 (내장/PATH/버전 비교) |
| `rzc lang list` | 설치된 언어 팩 목록 |
| `rzc lang install <코드/디렉터리>` | 언어 팩 설치 (원격 레지스트리 또는 로컬 디렉터리) |
| `rzc lang search [키워드]` | 설치 가능한 원격 언어 팩 검색 |
| `rzc lang remove <코드>` | 사용자가 설치한 언어 팩 제거 |
| `rzc mapping auto <crate> [--target-version <버전>]` | 서드파티 crate의 모국어 매핑 자동 생성 (AI/규칙); `--target-version`으로 생성 기준 버전 고정 |
| `rzc mapping check [대상]` | 매핑 품질 검증 (중복 키/키워드 충돌/파일 간 충돌/필수 섹션) |
| `rzc mapping coverage [--lang <언어>]` | 실제 코드로 언어 팩 커버리지 측정; 누락 매핑 목록 (저장소 루트에서 실행) |
| `rzc mapping scaffold <원본> <대상>` | 새 언어의 번역 골격 생성; `--provider deepseek`으로 AI 번역 활성화 |
| `rzc crate search [키워드]` | 커뮤니티가 공유한 서드파티 매핑 검색 (레지스트리) |
| `rzc crate install <crate> --lang <언어>` | 레지스트리의 서드파티 매핑을 전역 언어 팩에 설치 |
| `rzc crate list` | 설치된 커뮤니티 매핑 목록 |
| `rzc crate remove <crate> --lang <언어>` | 설치된 커뮤니티 매핑 제거 |
| `rzc crate update` | 매니페스트에 따라 설치된 모든 매핑 재다운로드 (업데이트 받기) |
| `rzc crate publish <crate> --lang <언어>` | 로컬 매핑을 레지스트리에 게시 (게시 전 품질 검사 통과 필요) |
| `rzc install <lsp\|toolchain>` | 보조 컴포넌트 설치 (언어 서버/내장 공식 툴체인) |

전체 레퍼런스: [부록 D: rzc 명령어 대조표 (중국어)](tutorials/附录D：rzc命令速查.md).

---

## ✨ 기능

### 모국어 프로그래밍

모국어 키워드(`함수`, `선언`, `만약`, `매치`…)와 모국어 표준 라이브러리(`문자열`, `벡터::새로운()`, `사용 표준::컬렉션::해시맵`)로 완전한 프로그램을 작성하세요. 매크로, 라이프타임, 제네릭, 트레이트를 모두 지원합니다.

### 10개 언어 내장

| 언어    | 확장자 | 언어      | 확장자 |
|---------|--------|-----------|--------|
| 中文    | `.zh`  | Español   | `.es`  |
| Deutsch | `.de`  | Français  | `.fr`  |
| 日本語  | `.ja`  | Português | `.pt`  |
| 한국어  | `.ko`  | العربية   | `.ar`  |
| Русский | `.ru`  | हिन्दी    | `.hi`  |

Rust 자체가 영어로 작성되므로 영어는 교육 방언이 아닙니다(아이덴티티 매핑은 교육적 가치가 없음); 언어 팩 디렉터리에는 아이덴티티 매핑과 영어 UI 텍스트 시나리오를 위한 별도의 `en` 아이덴티티 팩(확장자 `.en`)이 있습니다. 위 10개 자연어는 파일 확장자로 자동 감지되며 한 프로젝트에서 함께 사용할 수 있습니다.

### 교육 수준의 진단

- **이중 번역: 코드 + 메시지**: rustc 오류 코드, 코드 없는 lint 경고, 도움말 문장까지 커버
- **현지화된 타입 이름**: `std::fmt::Display` → `표준::포맷::표시_가능`
- **💡 교육 힌트**: 모든 오류에 「다음에 무엇을 할지」 제안 포함; 소유권 오류에는 📌 이동/대여 서사 포함
- **의존성 안내**: 선언되지 않은 서드파티 crate를 감지하면 `rzc add <crate>` 제안

### 완전한 IDE 경험 (VS Code / Qoder 확장)

구문 강조, 스마트 자동 완성, 호버 문서, 정의로 이동, 참조 검색, 이름 바꾸기, 코드 포맷, 원클릭 실행/검사, 전각 구두점 자동 변환, AI 보조 번역.

### 모국어로 쓰는 서드파티 crate

`rzc mapping auto`는 설치된 crate의 공개 API를 스캔해 모국어 이름(AI)을 생성합니다; 프로덕션 매핑은 `--target-version`으로 기준 버전을 고정하고(파일 헤더에 기록), 커뮤니티 매핑은 `rzc mapping check` 품질 검사를 통과해야 합니다.

### 왜 「장난감 언어」가 아닌가

| | 일반적인 장난감 「모국어 언어」 | rzc |
|---|---|---|
| 코드 형태 | 독자 문법 또는 의사코드 | 표준 Rust와 완전 동형 — 식별자만 현지화 |
| 컴파일·실행 | 자체 인터프리터 / 트랜스파일만 | 공식 rustc/cargo — 실제 컴파일과 실행 |
| 생태계 | 폐쇄적이거나 단절 | crates.io 전체 (`rzc add` + 모국어 매핑) |
| 오류 경험 | 영어 그대로 또는 자체 제작 힌트 | 번역된 메시지 + 오류 코드 + 교육 힌트 |
| 탈출 비용 | 처음부터 다시 작성 | `rzc eject`가 표준 Rust를 한 단계로 내보냄 |

---

## 📖튜토리얼

초보자를 위한 완전한 중국어 튜토리얼: **26개 장 + 용어집 + 5개 부록** — [tutorials/](tutorials/)를 참조하세요. 「Hello, World」부터 소유권, 클로저, async, 매크로를 거쳐 최종 통합 프로젝트까지; 모든 예제는 중국어 Rust로 작성되었습니다.

🌐 **온라인 열람**: [온라인 문서](https://liuqiTan80.github.io/i18n-rust/) (mdBook, 상단 표시줄 언어 전환기로 4개 언어; 로컬 미리보기는 `make site-serve`).

**번역 진행 중**: 영어(서문, 1-13장, 부록 D, 15/33), 일본어(1-11장, 11/33), 러시아어(1-15장, 15/33) — 진행 상황과 다음 배치는 [translation-status.md](docs/translation-status.md). 한국어 번역도 환영합니다 — 「기여하기」를 참고하세요.

> 튜토리얼 품질은 CI([tools/verify-tutorials.py](tools/verify-tutorials.py))가 보증합니다: 모든 코드 블록은 컴파일되어야 하고, 오류 예제는 주석으로 표기된 오류 코드(`// 预期错误: EXXXX`)를 출력해야 합니다. 수정 후 로컬 검증: `make tutorials` (중국어) 또는 `make tutorials-all` (영어/일본어/러시아어).

> 자주 묻는 질문과 학습 로드맵: [부록 E (중국어)](tutorials/附录E：常见问题、迁移指南与学习路线.md).

---

## 🏗️ 프로젝트 구조와 동작 방식

```text
모국어 소스 코드 (.ko)
   │  어휘 번역 → 모듈 경로 치환 → 별칭 치환   ← engine (언어 불가지론)
   ▼
표준 Rust 코드
   │  cargo build / run (공식 툴체인)
   ▼
JSON 진단 → 코드/메시지 번역 + 타입 현지화 + 교육 힌트 → 모국어 출력
```

| 디렉터리                   | 역할                                                                |
|----------------------------|---------------------------------------------------------------------------------|
| `crates/engine`            | 언어 불가지론 핵심 엔진: 번역 파이프라인, 매핑 관리, 진단 번역, 증분 캐시, Unicode 안전 검사 |
| `crates/cli`               | `rzc` 명령줄 도구 |
| `crates/lsp`               | `i18n-rust-lsp`: 공식 언어 서버(rust-analyzer)를 프록시하며 위치와 진단을 양방향 번역 |
| `crates/engine/lang-packs` | 11개 내장 언어 팩 (10개 자연어 + `en` 아이덴티티 팩: 키워드/표준 라이브러리/모듈 경로/오류 번역/UI 텍스트) |
| `tools/vscode-extension`   | VS Code / Qoder 확장 |
| `tools`                    | 게이트와 빌드 스크립트: 튜토리얼 검증, 벤치마크 회귀, 문서 사이트 조립 ([tools/README.md](tools/README.md)) |
| `tutorials`                | 부록이 포함된 26장 중국어 튜토리얼 (en/ja/ru 번역 진행 중) |
| `book`                     | 문서 사이트 조립 소스 (mdBook, [book/README.md](book/README.md)) |
| `docs`                     | 참조 및 개발 문서, [프로젝트 지도](docs/project-map.md); 운영과 로드맵은 [docs/strategy/](docs/strategy/README.md) |

**설계 원칙**: 엔진은 특정 언어를 하드코딩하지 않습니다 — 자연어 추가 = 언어 팩 디렉터리 추가 (빌드 시 자동 포함). 이 패러다임을 다른 프로그래밍 언어(예: 중국어 Python)로 포팅하려면 [방언 프레임워크 설계도](docs/dialect-framework-blueprint.md)를 참조하세요.

---

## 🤝기여하기

- **처음이신가요?** [프로젝트 지도 (메인테이너 매뉴얼)](docs/project-map.md)부터 시작하세요 — 「무엇을 어디서 고치고 어떻게 검증하는가」; 개발 환경과 커밋 규칙은 [CONTRIBUTING.md](CONTRIBUTING.md)
- **누락 단어 가이드 (앱 개발자)**: [docs/missing-mapping-guide.md](docs/missing-mapping-guide.md) (앱을 작성하면서 단어 추가/매핑 커스터마이즈 — 초보자용)
- **언어 팩 추가**: [docs/contributing-lang-pack.md](docs/contributing-lang-pack.md) (`rzc mapping scaffold` AI 번역 흐름 포함)
- **서드파티 crate 매핑**: [docs/third-party-mapping.md](docs/third-party-mapping.md)
- **커뮤니티 레지스트리**: [docs/third-party-registry.md](docs/third-party-registry.md) (커뮤니티 매핑 제출/다운로드)
- **튜토리얼 번역**: `tutorials/`에서 시작해 장 구조를 유지하세요; 진행 상황 패널은 [translation-status.md](docs/translation-status.md)
- 제출 전에 `make gate`가 완전히 통과하는지 확인하세요 (CI 테스트 게이트와 동일)

---

## ⭐ 프로젝트 지원

rzc가 도움이 되거나 영감을 주었다면 Star를 눌러주세요; 튜토리얼을 모국어로 번역하는 것이 프로젝트를 지원하는 가장 좋은 방법입니다.

---

## 📄 라이선스

[MIT](LICENSE) © tan80
