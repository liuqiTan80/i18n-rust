<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

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

## 📦 설치

한 줄이면 설치 완료, 바로 전역에서 사용 가능:

```bash
cargo install rzc
```

> [Rust 툴체인](https://www.rust-lang.org/tools/install) (rustup의 stable)이 필요합니다. 언어팩은 내장되어 있어 추가 설정이 필요 없습니다.

소스에서 직접 빌드할 수도 있습니다:

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace                # 바이너리: target/release/rzc
```

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
- **다국어 설계**: 11개 언어팩 내장 (ko/zh/en/de/ja/ru/es/fr/pt/ar/hi), 확장자로 자동 판별
- **현지화된 진단**: `rzc check`가 rustc 오류를 파일 언어로 번역하고 💡 교육 힌트 제공
- **소유권 시각화**: VS Code 확장(`i18n-rust` 검색)으로 변수 이동·재사용을 색상으로 강조
- **완전한 LSP 지원**: 자동 완성, 호버, 정의 이동, 참조 검색, 이름 바꾸기
- **점진적 전환**: `rzc eject`로 표준 Rust 코드를 한 단계로 내보내기

## 📖 튜토리얼

초보자용 완전한 중국어 튜토리얼(24장 + 4부록)은 [tutorials/](tutorials/)를 참조하세요.

## 📄 라이선스

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
