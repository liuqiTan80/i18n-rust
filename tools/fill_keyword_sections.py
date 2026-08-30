#!/usr/bin/env python3
"""为 9 个非 zh 语言包补齐 keywords.toml 的 [派生特征] 与 [标准库成员] 节（幂等）。

用法：python3 tools/fill_keyword_sections.py
策略：节已存在则跳过（不覆盖既有翻译），否则在文件末尾追加。
"""
import os
import sys
import tomllib

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACKS = os.path.join(REPO, "crates", "engine", "lang-packs")
LANGS = ["ja", "de", "es", "fr", "hi", "ko", "pt", "ru", "ar"]

# [派生特征]：母语词 → Clone/Copy/Debug/PartialEq/Eq/PartialOrd/Ord/Hash/Default
DERIVE = {
    "ja": ["クローン=Clone", "コピー=Copy", "デバッグ=Debug", "部分等価=PartialEq",
           "等価=Eq", "部分順序=PartialOrd", "順序=Ord", "ハッシュ=Hash", "デフォルト=Default"],
    "de": ["Klonen=Clone", "Kopieren=Copy", "Debuggen=Debug", "Teilgleich=PartialEq",
           "Gleich=Eq", "Teilordnung=PartialOrd", "Ordnung=Ord", "Hash=Hash", "Standard=Default"],
    "es": ["Clonar=Clone", "Copiar=Copy", "Depurar=Debug", "IgualdadParcial=PartialEq",
           "Igualdad=Eq", "OrdenParcial=PartialOrd", "Orden=Ord", "Hash=Hash", "Predeterminado=Default"],
    "fr": ["Cloner=Clone", "Copier=Copy", "Déboguer=Debug", "ÉgalitéPartielle=PartialEq",
           "Égalité=Eq", "OrdrePartiel=PartialOrd", "Ordre=Ord", "Hachage=Hash", "Défaut=Default"],
    "hi": ["क्लोन=Clone", "कॉपी=Copy", "डीबग=Debug", "आंशिकसमानता=PartialEq",
           "समानता=Eq", "आंशिकक्रम=PartialOrd", "क्रम=Ord", "हैश=Hash", "डिफ़ॉल्ट=Default"],
    "ko": ["클론=Clone", "복사=Copy", "디버그=Debug", "부분동등=PartialEq",
           "동등=Eq", "부분순서=PartialOrd", "순서=Ord", "해시=Hash", "기본값=Default"],
    "pt": ["Clonar=Clone", "Copiar=Copy", "Depurar=Debug", "IgualdadeParcial=PartialEq",
           "Igualdade=Eq", "OrdenaçãoParcial=PartialOrd", "Ordenação=Ord", "Hash=Hash", "Padrão=Default"],
    "ru": ["Клонировать=Clone", "Копировать=Copy", "Отлаживать=Debug", "ЧастичноеРавно=PartialEq",
           "Равно=Eq", "ЧастичныйПорядок=PartialOrd", "Порядок=Ord", "Хэш=Hash", "Поумолчанию=Default"],
    "ar": ["استنساخ=Clone", "نسخ=Copy", "تصحيح=Debug", "مساواةجزئية=PartialEq",
           "مساواة=Eq", "ترتيبجزئي=PartialOrd", "ترتيب=Ord", "تجزئة=Hash", "افتراضي=Default"],
}

# [标准库成员]：特征方法/关联类型/属性名（母语词 → 英文原名）
STD_MEMBERS = {
    "ja": ["次=next", "から=from", "書式化メソッド=fmt", "加算メソッド=add", "要素型=Item",
           "出力型=Output", "レイアウト=repr", "式=expr", "識別子=ident", "リテラル=literal",
           "型指定=ty", "パニックすべき=should_panic"],
    "de": ["Nächstes=next", "Von=from", "Formatmethode=fmt", "Addmethode=add", "Elementtyp=Item",
           "Ausgabetyp=Output", "Layout=repr", "Ausdruck=expr", "Bezeichner=ident", "Literal=literal",
           "Typspezifikation=ty", "SolltePaniken=should_panic"],
    "es": ["Siguiente=next", "Desde=from", "MétodoFormato=fmt", "MétodoSuma=add", "TipoElemento=Item",
           "TipoSalida=Output", "Diseño=repr", "Expresión=expr", "Identificador=ident", "Literal=literal",
           "EspecificaciónTipo=ty", "DeberíaPanico=should_panic"],
    "fr": ["Suivant=next", "Depuis=from", "MéthodeFormat=fmt", "MéthodeAddition=add", "TypeÉlément=Item",
           "TypeSortie=Output", "Disposition=repr", "Expression=expr", "Identifiant=ident", "Littéral=literal",
           "SpécificationType=ty", "DevraitPaniquer=should_panic"],
    "hi": ["अगला=next", "से=from", "प्रारूपविधि=fmt", "जोड़विधि=add", "वस्तुप्रकार=Item",
           "आउटपुटप्रकार=Output", "लेआउट=repr", "अभिव्यक्ति=expr", "पहचानकर्ता=ident", "शाब्दिक=literal",
           "प्रकारनिर्देश=ty", "पैनिकहोना=should_panic"],
    "ko": ["다음=next", "로부터=from", "포맷메서드=fmt", "덧셈메서드=add", "항목유형=Item",
           "출력유형=Output", "레이아웃=repr", "표현식=expr", "식별자=ident", "리터럴=literal",
           "유형지정=ty", "패닉해야=should_panic"],
    "pt": ["Próximo=next", "De=from", "MétodoFormato=fmt", "MétodoAdição=add", "TipoItem=Item",
           "TipoSaída=Output", "Layout=repr", "Expressão=expr", "Identificador=ident", "Literal=literal",
           "EspecificaçãoTipo=ty", "DeveEntrarEmPânico=should_panic"],
    "ru": ["Следующий=next", "Из=from", "МетодФормата=fmt", "МетодСложения=add", "ТипЭлемента=Item",
           "ТипВывода=Output", "Расположение=repr", "Выражение=expr", "Идентификатор=ident", "Литерал=literal",
           "СпецификацияТипа=ty", "ДолжнаПаниковать=should_panic"],
    "ar": ["التالي=next", "من=from", "طريقةالتنسيق=fmt", "طريقةالجمع=add", "نوعالعنصر=Item",
           "نوعالمخرجات=Output", "تخطيط=repr", "تعبير=expr", "معرف=ident", "حرفي=literal",
           "مواصفةالنوع=ty", "يجبالذعر=should_panic"],
}


def build_section(title: str, comment: str, pairs: list[str]) -> str:
    lines = [f"\n# ============================================\n# {comment}\n# ============================================\n[\"{title}\"]\n"]
    for pair in pairs:
        key, value = pair.split("=", 1)
        lines.append(f'"{key}" = "{value}"\n')
    return "".join(lines)


def main() -> int:
    failed = 0
    for lang in LANGS:
        path = os.path.join(PACKS, lang, "keywords.toml")
        with open(path, encoding="utf-8") as f:
            content = f.read()
        # 幂等：节已存在则跳过
        try:
            data = tomllib.loads(content)
        except tomllib.TOMLDecodeError as e:
            print(f"❌ {lang}: 解析失败: {e}")
            failed += 1
            continue
        additions = []
        if "派生特征" not in data:
            additions.append(build_section(
                "派生特征", "派生属性（#[派生(...)]）内专用的特征名：母语词 → 英文特征名", DERIVE[lang]))
        if "标准库成员" not in data:
            additions.append(build_section(
                "标准库成员", "标准库成员名（特征方法 / 关联类型 / 属性名）：母语词 → 英文原名", STD_MEMBERS[lang]))
        if not additions:
            print(f"✅ {lang}: 两节已存在，跳过")
            continue
        new_content = content.rstrip("\n") + "\n" + "".join(additions)
        # 验证
        try:
            tomllib.loads(new_content)
        except tomllib.TOMLDecodeError as e:
            print(f"❌ {lang}: 写后验证失败: {e}")
            failed += 1
            continue
        with open(path, "w", encoding="utf-8") as f:
            f.write(new_content)
        added = sum(1 for a in additions if a)
        print(f"✅ {lang}: 追加 {added} 节")
    return failed


if __name__ == "__main__":
    sys.exit(main())
