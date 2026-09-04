<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

> ⚠️ यह अनुवाद पुराना हो सकता है। नवीनतम जानकारी के लिए [चीनी संस्करण](README.md) या [अंग्रेज़ी संस्करण](README.en.md) देखें।

# rzc: बहुभाषी Rust शिक्षण बोली कंपाइलर

अपनी मातृभाषा में Rust प्रोग्राम लिखें — rzc उन्हें स्वचालित रूप से मानक Rust में अनुवादित कर संकलित करता है। अंग्रेज़ी नहीं, प्रोग्रामिंग सीखें।

```rust
// src/main.hi — हिंदी Rust शिक्षण बोली
फंक्शन मुख्य() {
    मानो परिवर्तनीय संख्या = 10;
    संख्या = संख्या + 1;
    पंक्ति_छापो!("संख्या: {}", संख्या);
}
```

```bash
$ rzc run src/main.hi
संख्या: 11
```

## 📦 स्थापना (स्रोत से बिल्ड करें)

rzc कोई ऑनलाइन पहले से बना इंस्टॉलर नहीं देता — इसे अपनी मशीन पर बिल्ड करें (पहली बार 1–3 मिनट)।

### पूर्वापेक्षाएँ

| उपकरण | उद्देश्य | आवश्यक? |
|---|---|---|
| **Rust टूलचेन** (rustc + cargo) | rzc को ही बिल्ड करना | ✅ हाँ |
| **git** | सोर्स कोड प्राप्त करना | ✅ हाँ (या सोर्स ZIP) |
| **इंटरनेट** | पहली बिल्ड पर निर्भरताएँ डाउनलोड करना | ✅ हाँ (पहली बार) |
| **Node.js 18+ और npm** | VS Code एक्सटेंशन बिल्ड करना | वैकल्पिक (केवल IDE) |
| **rust-analyzer** | IDE पूर्णता/डायग्नोस्टिक्स बैकएंड | वैकल्पिक (केवल IDE) |

### 1. Rust टूलचेन स्थापित करें

अनुशंसित तरीका [rustup](https://rustup.rs) है (एक साथ rustc, cargo और rustup स्थापित करता है)।

- **Linux / macOS** (टर्मिनल में):

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

  फिर `source "$HOME/.cargo/env"` चलाएँ (या टर्मिनल फिर से खोलें)।

- **Windows**: [rustup-init.exe](https://rustup.rs) डाउनलोड करके विज़ार्ड का पालन करें (या PowerShell में `winget install --id Rustlang.Rustup`)।

जाँचें (टर्मिनल फिर से खोलकर):

```bash
rustc --version   # उदा.: rustc 1.98.0
cargo --version
```

### 2. सोर्स प्राप्त करें और बिल्ड करें

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace
./target/release/rzc --version
```

पहली बिल्ड निर्भरताएँ डाउनलोड करती है और सभी घटकों को संकलित करती है (1–3 मिनट)। परिणाम:

- `target/release/rzc` — CLI उपकरण
- `target/release/i18n-rust-lsp` — भाषा सर्वर (VS Code एक्सटेंशन बैकएंड)

### 3. (वैकल्पिक) rzc को वैश्विक रूप से उपलब्ध करें

```bash
cargo install --path crates/cli   # स्थानीय रूप से बिल्ड करके ~/.cargo/bin में स्थापित करें
```

### 4. (वैकल्पिक) IDE सुविधाओं के लिए rust-analyzer स्थापित करें

```bash
rzc install toolchain --ra-only --force
```

### 5. (वैकल्पिक) VS Code एक्सटेंशन बिल्ड करें

Node.js 18+ और npm चाहिए ([nodejs.org](https://nodejs.org) से):

```bash
cd tools/vscode-extension
npm ci
npm run package    # i18n-rust-<संस्करण>.vsix बनाता है
```

`.vsix` को VS Code में «Install from VSIX...» से स्थापित करें; भाषा सर्वर `rzc install lsp` प्रदान करता है।

## 🚀 त्वरित आरंभ

```bash
rzc init मेरा-प्रोजेक्ट
cd मेरा-प्रोजेक्ट
rzc run src/main.hi
```

`rzc init` एक चलने-योग्य प्रोजेक्ट ढांचा (`Cargo.toml` + `src/main.hi`) बनाता है — बस चलाएँ।

## 🛠️ कमांड

| कमांड | विवरण |
|--------|--------|
| `rzc init <नाम>` | नया प्रोजेक्ट बनाएँ |
| `rzc run <फ़ाइल>` | बोली स्रोत का अनुवाद कर चलाएँ |
| `rzc check <फ़ाइल>` | मातृभाषा शैक्षिक निदान के साथ टाइप जाँच |
| `rzc eject <फ़ाइल>` | मानक Rust कोड में निर्यात करें |
| `rzc lang list` | स्थापित भाषा पैक की सूची |
| `rzc mapping auto <crate>` | तृतीय-पक्ष मैपिंग स्वतः उत्पन्न करें |

## ✨ विशेषताएँ

- **मातृभाषा प्रोग्रामिंग**: अपनी भाषा के कीवर्ड से पूर्ण Rust प्रोग्राम लिखें
- **बहुभाषी डिज़ाइन**: 10 अंतर्निहित भाषा पैक (hi/zh/de/ja/ru/es/fr/pt/ko/ar), एक्सटेंशन से स्वतः पहचान
- **स्थानीय निदान**: `rzc check` rustc त्रुटियों का फ़ाइल की भाषा में अनुवाद करता है, 💡 शैक्षिक संकेतों के साथ
- **स्वामित्व विज़ुअलाइज़ेशन**: VS Code एक्सटेंशन (`i18n-rust` खोजें) चरों के स्थानांतरण व पुनः उपयोग को रंगों से उजागर करता है
- **पूर्ण LSP समर्थन**: ऑटो-कम्प्लीशन, होवर, परिभाषा पर जाना, संदर्भ खोज, नाम बदलना
- **क्रमिक संक्रमण**: `rzc eject` एक चरण में मानक Rust कोड निर्यात करता है

## 📖 ट्यूटोरियल

शुरुआती लोगों के लिए पूर्ण चीनी ट्यूटोरियल (26 अध्याय + शब्दावली + 5 परिशिष्ट) — देखें [tutorials/](tutorials/)।

## 📄 लाइसेंस

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
