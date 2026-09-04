<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

> ⚠️ 이 번역은 최신 내용을 반영하지 못했을 수 있습니다. 최신 정보는 [중국어판](README.md) 또는 [영어판](README.en.md)을 참고하세요.

# rzc: 다국어 Rust 교육 방언 컴파일러

모국어로 Rust 프로그램을 작성하세요. rzc가 표준 Rust로 자동 번역하여 컴파일·실행합니다. 영어 암기가 아닌 프로그래밍 학습.

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

## 📦 설치 (소스에서 빌드)

rzc는 온라인 프리빌드 설치를 제공하지 않습니다. 직접 빌드하세요 (첫 빌드 약 1~3분).

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

### 5. (선택) VS Code 확장 빌드

Node.js 18+와 npm이 필요합니다 ([nodejs.org](https://nodejs.org)):

```bash
cd tools/vscode-extension
npm ci
npm run package    # i18n-rust-<버전>.vsix 생성
```

생성된 `.vsix`를 VS Code의 「Install from VSIX...」로 설치합니다. 언어 서버는 `rzc install lsp`가 제공합니다.

## 🚀 빠른 시작

```bash
rzc init 내-프로젝트
cd 내-프로젝트
rzc run src/main.ko
```

`rzc init`은 실행 가능한 프로젝트 골격(`Cargo.toml` + `src/main.ko`)을 생성합니다. 바로 실행하면 됩니다.

## 🛠️ 주요 명령어

| 명령어 | 설명 |
|--------|------|
| `rzc init <이름>` | 새 프로젝트 생성 |
| `rzc run <파일>` | 방언 소스를 번역하여 실행 |
| `rzc check <파일>` | 모국어 교육 진단과 함께 타입 검사 |
| `rzc eject <파일>` | 표준 Rust 코드로 내보내기 |
| `rzc lang list` | 설치된 언어팩 목록 |
| `rzc mapping auto <crate>` | 서드파티 매핑 자동 생성 |

## ✨ 기능

- **모국어 프로그래밍**: 모국어 키워드로 완전한 Rust 프로그램 작성
- **다국어 설계**: 10개 언어팩 내장 (ko/zh/de/ja/ru/es/fr/pt/ar/hi), 확장자로 자동 판별
- **현지화된 진단**: `rzc check`가 rustc 오류를 파일 언어로 번역하고 💡 교육 힌트 제공
- **소유권 시각화**: VS Code 확장(`i18n-rust` 검색)으로 변수 이동·재사용을 색상으로 강조
- **완전한 LSP 지원**: 자동 완성, 호버, 정의 이동, 참조 검색, 이름 바꾸기
- **점진적 전환**: `rzc eject`로 표준 Rust 코드를 한 단계로 내보내기

## 📖 튜토리얼

초보자용 완전한 중국어 튜토리얼(26장 + 용어집 + 5부록)은 [tutorials/](tutorials/)를 참조하세요.

## 📄 라이선스

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
