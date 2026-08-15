<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc: 다국어 Rust 교육 방언 컴파일러

모국어로 Rust 프로그램을 작성하세요. rzc가 자동으로 표준 Rust로 번역하고 컴파일·실행합니다 — 프로그래밍 교육은 영어 암기가 아닌 논리적 사고로 돌아갑니다.

```rust
函数 主函数() {
    让 可变 数量 = 10;
    数量 = 数量 + 1;
    打印行!("数量是：{}", 数量);
}
```

```bash
$ rzc run src/main.zh
数量是：11
```

## ✨ 기능

- **모국어 프로그래밍**: 중국어 키워드(`函数`, `让`, `如果`, `返回`…)로 완전한 Rust 프로그램 작성
- **다국어 네이티브**: 모든 자연어를 네이티브로 지원; 중국어 팩 내장, 다른 언어는 원격 설치 가능
- **확장자 자동 감지**: `.zh`, `.ja`, `.ru` 등이 해당 언어 팩에 자동 매칭
- **지역화 오류 진단**: rustc 출력을 번역하고 💡 교육 힌트 추가; 소유권 오류를 JSON으로 시각화
- **소유권 시각화**: VS Code 확장으로 변수 이동(노란색), 재사용(빨간색), 수명(초록색) 하이라이트
- **완전한 LSP 지원**: 자동 완성, 호버, 정의 이동, 참조 검색, 이름 변경
- **매크로 자동 완성**: `!` 생략 가능; 트랜스파일 시 자동 추가
- **점진적 전환**: `eject`로 표준 Rust 코드를 한 단계로 내보내기
- **완전한 튜토리얼**: 24장 + 4개 부록, 완전한 초보자부터 종합 프로젝트까지

## 📦 설치

### crates.io経由 (권장)

```bash
cargo install rzc
```

### 소스에서 빌드

```bash
# 중국 미러
git clone https://gitcode.com/tan80/zrRust.git
# 국제판
git clone https://github.com/liuqiTan80/i18n-rust.git
cd zrRust 또는 i18n-rust
cargo build --release --workspace
```

## 🚀 빠른 시작

```bash
rzc init 내-프로젝트
cd 내-프로젝트
rzc run src/main.zh
```

## 🛠️ 명령어

| 명령어 | 설명 |
|--------|------|
| `rzc init <이름>` | 새 프로젝트 생성 |
| `rzc run <파일>` | `.zh` 소스 번역 및 실행 |
| `rzc check <파일>` | 타입 검사 + 교육 진단 출력 |
| `rzc eject <파일>` | 표준 `.rs` 코드로 내보내기 |
| `rzc lang list` | 설치된 언어 팩 목록 |
| `rzc lang install <소스>` | 언어 팩 설치 |
| `rzc lang remove <코드>` | 사용자 언어 팩 제거 |
| `rzc mapping auto <crate>` | 서드파티 crate 매핑 자동 생성 |

## 📖 튜토리얼

완전한 초보자 튜토리얼: 24장 + 4개 부록

| 단계 | 장 |
|------|-----|
| **기초** | 제1장 hello world · 제2장 변수와 타입 · 제3장 복합 타입 · 제4장 제어 흐름 · 제5장 함수와 메서드 |
| **핵심** | 제6장 소유권 · 제7장 참조와 빌림 · 제8장 문자열 · 제9장 구조체 · 제10장 열거형과 패턴 매칭 |
| **제네릭** | 제11장 제네릭 · 제12장 트레잇 · 제13장 수명 · 제14장 컬렉션 |
| **오류와 모듈** | 제15장 오류 처리 · 제16장 모듈 시스템 · 제17장 패키지 관리 |
| **고급** | 제18장 스마트 포인터 · 제19장 동시성 · 제20장 테스트 |
| **전문** | 제21장 클로저와 이터레이터 · 제22장 매크로 · 제23장 비동기 프로그래밍 |
| **프로젝트** | 제24장 커맨드라인 계산기 |
| **부록** | A 매핑 레퍼런스 · B 용어집 · C 마이그레이션 가이드 · D FAQ와 학습 경로 |

## ❓ FAQ

**Q: 매크로에서 느낌표를 생략할 수 있는 이유는?**
초보자의 암기 부담을 줄이기 위해. 트랜스파일러가 자동으로 `!`를 추가합니다.

**Q: 중국어 변수명을 사용할 수 있나요?**
네. Rust는 Unicode 식별자를 지원합니다.

**Q: 다른 언어 팩은 어떻게 설치하나요?**
`rzc lang install 日本語` (원격) 또는 `rzc lang install ./디렉토리` (로컬).

## 🤝 기여

[GitHub Issues](https://github.com/liuqiTan80/i18n-rust/issues)로 피드백, PR 환영합니다.

## 📄 라이선스

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
