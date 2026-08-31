<div align="center">

**[中文](README.md)** · **[English](README.en.md)** · **[日本語](README.ja.md)** · **[Русский](README.ru.md)** · **[Español](README.es.md)** · **[Français](README.fr.md)** · **[Deutsch](README.de.md)** · **[한국어](README.ko.md)** · **[العربية](README.ar.md)** · **[Português](README.pt.md)** · **[हिन्दी](README.hi.md)**

</div>

# rzc: مترجم لهجة رست التعليمية متعدد اللغات

اكتب برامج رست بلغتك الأم — يقوم rzc بترجمتها تلقائيًا إلى رست القياسية وتجميعها. تعلّم البرمجة، لا الإنجليزية.

```rust
// src/main.ar — لهجة رست التعليمية بالعربية
دالة رئيسي() {
    دع متغير عدد = 10;
    عدد = عدد + 1;
    اطبع_سطرا!("العدد: {}", عدد);
}
```

```bash
$ rzc run src/main.ar
العدد: 11
```

## 📦 التثبيت (البناء من المصدر)

لا يوفر rzc مثبّتًا جاهزًا عبر الإنترنت — قم ببنائه على جهازك (1–3 دقائق).

### المتطلبات الأساسية

| الأداة | الغرض | مطلوبة؟ |
|---|---|---|
| **أدوات رست** (rustc + cargo) | بناء rzc نفسه | ✅ نعم |
| **git** | الحصول على الكود المصدري | ✅ نعم (أو تحميل ZIP) |
| **الإنترنت** | تنزيل التبعيات عند البناء الأول | ✅ نعم (المرة الأولى) |
| **Node.js 18+ و npm** | بناء إضافة VS Code | اختياري (لبيئة IDE فقط) |
| **rust-analyzer** | خلفية الإكمال/التشخيص في IDE | اختياري (لبيئة IDE فقط) |

### 1. تثبيت أدوات رست

الطريقة الموصى بها هي [rustup](https://rustup.rs) (يثبّت rustc و cargo و rustup دفعة واحدة).

- **Linux / macOS** (في الطرفية):

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

  ثم نفّذ `source "$HOME/.cargo/env"` (أو أعد فتح الطرفية).

- **Windows**: حمّل [rustup-init.exe](https://rustup.rs) واتبع المعالج (أو نفّذ `winget install --id Rustlang.Rustup` في PowerShell).

تحقق (بعد إعادة فتح الطرفية):

```bash
rustc --version   # مثل: rustc 1.98.0
cargo --version
```

### 2. الحصول على المصدر والبناء

```bash
git clone https://github.com/liuqiTan80/i18n-rust.git
cd i18n-rust
cargo build --release --workspace
./target/release/rzc --version
```

البناء الأول ينزّل التبعيات ويجمّع كل المكونات (1–3 دقائق). النواتج:

- `target/release/rzc` — أداة سطر الأوامر
- `target/release/i18n-rust-lsp` — خادم اللغة (خلفية إضافة VS Code)

### 3. (اختياري) جعل rzc متاحًا عالميًا

```bash
cargo install --path crates/cli   # بناء محلي وتثبيت إلى ~/.cargo/bin
```

### 4. (اختياري) تثبيت rust-analyzer لميزات IDE

```bash
rzc install toolchain --ra-only --force
```

### 5. (اختياري) بناء إضافة VS Code

يتطلب Node.js 18+ و npm (من [nodejs.org](https://nodejs.org)):

```bash
cd tools/vscode-extension
npm ci
npm run package    # ينتج i18n-rust-<الإصدار>.vsix
```

ثبّت ملف `.vsix` عبر «Install from VSIX...» في VS Code؛ خادم اللغة يوفره `rzc install lsp`.

## 🚀 بدء سريع

```bash
rzc init مشروعي
cd مشروعي
rzc run src/main.ar
```

`rzc init` ينشئ هيكل مشروع جاهزًا للتشغيل (`Cargo.toml` + `src/main.ar`) — شغّله مباشرة.

## 🛠️ الأوامر

| الأمر | الوصف |
|-------|-------|
| `rzc init <الاسم>` | إنشاء مشروع جديد |
| `rzc run <ملف>` | ترجمة وتشغيل كود اللهجة |
| `rzc check <ملف>` | فحص الأنواع مع تشخيص تعليمي بلغتك |
| `rzc eject <ملف>` | تصدير إلى كود رست القياسي |
| `rzc lang list` | قائمة حزم اللغات المثبتة |
| `rzc mapping auto <crate>` | توليد تلقائي لخرائط المكتبات الخارجية |

## ✨ الميزات

- **البرمجة بلغتك الأم**: اكتب برامج رست كاملة بكلمات مفتاحية من لغتك
- **متعدد اللغات**: 10 حزم لغة مدمجة (ar/zh/de/ja/ru/es/fr/pt/ko/hi)، اكتشاف تلقائي بالامتداد
- **تشخيصات مترجمة**: `rzc check` يترجم أخطاء rustc إلى لغة الملف مع 💡 تلميحات تعليمية
- **تصور الملكية**: إضافة VS Code (ابحث عن `i18n-rust`) تبرز نقل المتغيرات وإعادة استخدامها بالألوان
- **دعم LSP كامل**: إكمال تلقائي، تمرير، الانتقال إلى التعريف، البحث عن المراجع، إعادة التسمية
- **انتقال تدريجي**: `rzc eject` يصدر كود رست القياسي في خطوة واحدة

## 📖 البرنامج التعليمي

برنامج تعليمي كامل بالصينية للمبتدئين (26 فصلًا + مسرد مصطلحات + 5 ملاحق) — انظر [tutorials/](tutorials/).

## 📄 الترخيص

[MIT](https://github.com/liuqiTan80/i18n-rust/blob/main/LICENSE)
