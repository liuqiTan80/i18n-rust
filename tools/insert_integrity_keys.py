#!/usr/bin/env python3
"""向 10 个语言包 ui.toml 插入跨语言完整性检查的 8 个界面键（幂等）。

用法：python3 tools/insert_integrity_keys.py
策略：已有键行一律删除，统一在锚点行 "mc_cross_lang_header" 后插入。
"""
import os
import sys
import tomllib

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACKS = os.path.join(REPO, "crates", "engine", "lang-packs")
LANGS = ["zh", "ja", "de", "es", "fr", "hi", "ko", "pt", "ru", "ar"]

# 键 → 各语言模板（{} 为参数占位符）
KEYS = {
    "mc_cross_integrity_header": {
        "zh": "🔬 跨语言完整性检查（基准：zh）:",
        "ja": "🔬 言語横断の完全性チェック（基準：zh）:",
        "de": "🔬 Sprachübergreifende Vollständigkeitsprüfung (Basis: zh):",
        "es": "🔬 comprobación de integridad entre idiomas (base: zh):",
        "fr": "🔬 vérification d'intégrité inter-langues (base : zh) :",
        "hi": "🔬 भाषा-सापेक्ष पूर्णता जाँच (आधार: zh):",
        "ko": "🔬 언어 간 완전성 검사 (기준: zh):",
        "pt": "🔬 verificação de integridade entre idiomas (base: zh):",
        "ru": "🔬 проверка полноты языковых пакетов (база: zh):",
        "ar": "🔬 فحص الاكتمال عبر اللغات (الأساس: zh):",
    },
    "mc_cross_integrity_ok": {
        "zh": "✅ 关键字/派生特征/错误码全覆盖",
        "ja": "✅ キーワード・派生トレイト・エラーコードすべて網羅",
        "de": "✅ Schlüsselwörter/abgeleitete Merkmale/Fehlercodes vollständig abgedeckt",
        "es": "✅ palabras clave/rasgos derivados/códigos de error totalmente cubiertos",
        "fr": "✅ mots-clés/traits dérivés/codes d'erreur entièrement couverts",
        "hi": "✅ कीवर्ड/व्युत्पन्न लक्षण/त्रुटि कोड पूर्ण रूप से शामिल",
        "ko": "✅ 키워드/파생 특성/오류 코드 완전 커버",
        "pt": "✅ palavras-chave/atributos derivados/códigos de erro totalmente cobertos",
        "ru": "✅ ключевые слова/производные признаки/коды ошибок полностью покрыты",
        "ar": "✅ الكلمات المفتاحية/السمات المشتقة/أكواد الخطأ مغطاة بالكامل",
    },
    "mc_cross_integrity_warn": {
        "zh": "✅ 覆盖完整（{} 条消息翻译缺失警告）",
        "ja": "✅ 網羅は完全（メッセージ翻訳欠落の警告 {} 件）",
        "de": "✅ Abdeckung vollständig ({} Warnungen zu fehlenden Meldungsübersetzungen)",
        "es": "✅ cobertura completa ({} advertencias de traducciones de mensajes faltantes)",
        "fr": "✅ couverture complète ({} avertissements de traductions de messages manquantes)",
        "hi": "✅ कवरेज पूर्ण ({} संदेश अनुवाद लुप्त चेतावनी)",
        "ko": "✅ 커버리지 완전 (메시지 번역 누락 경고 {}개)",
        "pt": "✅ cobertura completa ({} avisos de traduções de mensagens ausentes)",
        "ru": "✅ покрытие полное ({} предупреждений об отсутствующих переводах сообщений)",
        "ar": "✅ التغطية كاملة ({} تحذيرات من ترجمات رسائل مفقودة)",
    },
    "mc_cross_integrity_none": {
        "zh": "（无可比较的语言包）",
        "ja": "（比較対象の言語パックなし）",
        "de": "(keine vergleichbaren Sprachpakete)",
        "es": "(no hay paquetes de idiomas comparables)",
        "fr": "(aucun pack de langue comparable)",
        "hi": "(कोई तुलनीय भाषा पैक नहीं)",
        "ko": "(비교할 언어 팩 없음)",
        "pt": "(nenhum pacote de idiomas comparável)",
        "ru": "(нет сопоставимых языковых пакетов)",
        "ar": "(لا توجد حزم لغات قابلة للمقارنة)",
    },
    "mc_cross_kw_missing": {
        "zh": "  ❌ {} 缺少 {} 个关键字值（方言词不可用）: {}",
        "ja": "  ❌ {} にキーワード値が {} 個不足（方言語彙が使えない）: {}",
        "de": "  ❌ {} fehlen {} Schlüsselwortwerte (Dialektwörter nicht verfügbar): {}",
        "es": "  ❌ a {} faltan {} valores de palabras clave (palabras dialectales no disponibles): {}",
        "fr": "  ❌ {} manque de {} valeurs de mots-clés (mots dialectaux indisponibles) : {}",
        "hi": "  ❌ {} में {} कीवर्ड मान लुप्त (बोली शब्द उपलब्ध नहीं): {}",
        "ko": "  ❌ {}에 키워드 값 {}개 누락(방언 단어 사용 불가): {}",
        "pt": "  ❌ {} faltam {} valores de palavras-chave (palavras do dialeto indisponíveis): {}",
        "ru": "  ❌ в {} отсутствует {} значений ключевых слов (слова диалекта недоступны): {}",
        "ar": "  ❌ {} ينقصها {} قيمة كلمات مفتاحية (كلمات اللهجة غير متاحة): {}",
    },
    "mc_cross_derive_missing": {
        "zh": "  ❌ {} 缺少 {} 个派生特征（派生属性参数不可用）: {}",
        "ja": "  ❌ {} に派生トレイトが {} 個不足（派生属性の引数が使えない）: {}",
        "de": "  ❌ {} fehlen {} abgeleitete Merkmale (Parameter des Ableitungsattributs nicht verfügbar): {}",
        "es": "  ❌ a {} faltan {} rasgos derivados (parámetros del atributo de derivación no disponibles): {}",
        "fr": "  ❌ {} manque de {} traits dérivés (paramètres de l'attribut de dérivation indisponibles) : {}",
        "hi": "  ❌ {} में {} व्युत्पन्न लक्षण लुप्त (व्युत्पन्न विशेषता पैरामीटर उपलब्ध नहीं): {}",
        "ko": "  ❌ {}에 파생 특성 {}개 누락(파생 속성 매개변수 사용 불가): {}",
        "pt": "  ❌ {} faltam {} atributos derivados (parâmetros do atributo de derivação indisponíveis): {}",
        "ru": "  ❌ в {} отсутствует {} производных признаков (параметры атрибута выведения недоступны): {}",
        "ar": "  ❌ {} ينقصها {} سمات مشتقة (معاملات سمة الاشتقاق غير متاحة): {}",
    },
    "mc_cross_err_missing": {
        "zh": "  ❌ {} 缺少 {} 个错误码节（无母语教学提示）: {}",
        "ja": "  ❌ {} にエラーコード節が {} 個不足（母語の学習ヒントなし）: {}",
        "de": "  ❌ {} fehlen {} Fehlercode-Abschnitte (keine Hinweise in der Muttersprache): {}",
        "es": "  ❌ a {} faltan {} secciones de códigos de error (sin sugerencias en el idioma nativo): {}",
        "fr": "  ❌ {} manque de {} sections de codes d'erreur (aucun conseil en langue maternelle) : {}",
        "hi": "  ❌ {} में {} त्रुटि कोड अनुभाग लुप्त (मातृभाषा में शिक्षण संकेत नहीं): {}",
        "ko": "  ❌ {}에 오류 코드 섹션 {}개 누락(모국어 학습 힌트 없음): {}",
        "pt": "  ❌ {} faltam {} seções de códigos de erro (sem dicas no idioma nativo): {}",
        "ru": "  ❌ в {} отсутствует {} разделов кодов ошибок (нет подсказок на родном языке): {}",
        "ar": "  ❌ {} ينقصها {} أقسام أكواد خطأ (لا تلميحات باللغة الأم): {}",
    },
    "mc_cross_msg_missing": {
        "zh": "  ⚠️ {} 缺少 {} 个消息翻译键（回退英文原文）: {} 等",
        "ja": "  ⚠️ {} にメッセージ翻訳キーが {} 個不足（英語原文にフォールバック）: {} など",
        "de": "  ⚠️ {} fehlen {} Übersetzungsschlüssel für Meldungen (Fallback auf englischen Originaltext): {} usw.",
        "es": "  ⚠️ a {} faltan {} claves de traducción de mensajes (se vuelve al texto original en inglés): {} etc.",
        "fr": "  ⚠️ {} manque de {} clés de traduction de messages (repli sur le texte anglais d'origine) : {} etc.",
        "hi": "  ⚠️ {} में {} संदेश अनुवाद कुंजियाँ लुप्त (अंग्रेज़ी मूल पर वापसी): {} आदि",
        "ko": "  ⚠️ {}에 메시지 번역 키 {}개 누락(영어 원문으로 대체): {} 등",
        "pt": "  ⚠️ {} faltam {} chaves de tradução de mensagens (retorno ao texto original em inglês): {} etc.",
        "ru": "  ⚠️ в {} отсутствует {} ключей перевода сообщений (возврат к английскому оригиналу): {} и др.",
        "ar": "  ⚠️ {} ينقصها {} مفاتيح ترجمة رسائل (الرجوع إلى النص الإنجليزي الأصلي): {} إلخ.",
    },
}

ANCHOR = '"mc_cross_lang_header"'


def main() -> int:
    failed = 0
    for lang in LANGS:
        path = os.path.join(PACKS, lang, "ui.toml")
        with open(path, encoding="utf-8") as f:
            lines = f.readlines()
        # 1. 删除已有键行（幂等）
        existing = {k for k in KEYS if any(f'"{k}"' in ln for ln in lines)}
        if existing:
            lines = [ln for ln in lines if not any(f'"{k}"' in ln for k in KEYS)]
        # 2. 找锚点行号（删除后的新索引）
        anchor_idx = next(i for i, ln in enumerate(lines) if ANCHOR in ln)
        # 3. 构造新行
        new_lines = []
        for k, tpls in KEYS.items():
            tpl = tpls[lang]
            new_lines.append(f'"{k}" = "{tpl}"\n')
        lines[anchor_idx + 1 : anchor_idx + 1] = new_lines
        # 4. 验证 TOML 可解析
        try:
            tomllib.loads("".join(lines))
        except tomllib.TOMLDecodeError as e:
            print(f"❌ {lang}: TOML 验证失败: {e}")
            failed += 1
            continue
        with open(path, "w", encoding="utf-8") as f:
            f.writelines(lines)
        status = "更新" if existing else "插入"
        print(f"✅ {lang}: {status} {len(KEYS)} 个键（锚点后）")
    return failed


if __name__ == "__main__":
    sys.exit(main())
