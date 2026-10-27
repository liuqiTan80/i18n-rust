/* eslint-disable */
/**
 * 自动生成——请勿手改。
 * 数据源：crates/engine/lang-packs/<lang>/ui.toml ["界面消息"] 的 ext_* 键（11 语言）。
 * 取用：extension.ts 按 vscode.env.language 前缀匹配语言码，无匹配回退 en。
 * 刷新：npm run gen:all（scripts/gen-ui-strings.mjs）。
 */

export const 界面文案表: Readonly<Record<string, Readonly<Record<string, string>>>> = {
    ar: {
        ext_need_dialect_file: "يرجى فتح ملف مصدر اللهجة (.zh/.ja …)",
        ext_lsp_restarted: "تمت إعادة تشغيل خادم اللغة",
        ext_explain_need_file: "يرجى فتح ملف أولاً قبل استخدام شرح التشخيصات",
        ext_explain_no_diag: "لا توجد أخطاء أو تحذيرات في الملف الحالي؛ لا شيء للشرح",
        ext_explain_no_diag_nearby: "لم يتم العثور على تشخيصات بالقرب من المؤشر",
        ext_add_dep_no_workspace: "لا توجد مساحة عمل مفتوحة؛ تعذّر إضافة اعتمادية",
    },
    de: {
        ext_need_dialect_file: "Bitte öffnen Sie eine Dialekt-Quelldatei (.zh/.ja …)",
        ext_lsp_restarted: "Sprachserver neu gestartet",
        ext_explain_need_file: "Bitte öffnen Sie zuerst eine Datei, bevor Sie die Diagnoseerklärung verwenden",
        ext_explain_no_diag: "In der aktuellen Datei gibt es keine Fehler oder Warnungen; nichts zu erklären",
        ext_explain_no_diag_nearby: "Keine Diagnose in der Nähe des Cursors gefunden",
        ext_add_dep_no_workspace: "Kein Arbeitsbereich geöffnet; Abhängigkeit kann nicht hinzugefügt werden",
    },
    en: {
        ext_need_dialect_file: "Please open a dialect source file (.zh/.ja …)",
        ext_lsp_restarted: "Language server restarted",
        ext_explain_need_file: "Open a file first, then use diagnostic explanation",
        ext_explain_no_diag: "The current file has no errors or warnings; nothing to explain",
        ext_explain_no_diag_nearby: "No diagnostic found near the cursor",
        ext_add_dep_no_workspace: "No workspace open; cannot add dependency",
    },
    es: {
        ext_need_dialect_file: "Abra un archivo fuente de dialecto (.zh/.ja …)",
        ext_lsp_restarted: "Servidor de lenguaje reiniciado",
        ext_explain_need_file: "Abra primero un archivo antes de usar la explicación de diagnósticos",
        ext_explain_no_diag: "El archivo actual no tiene errores ni advertencias; no hay nada que explicar",
        ext_explain_no_diag_nearby: "No se encontraron diagnósticos cerca del cursor",
        ext_add_dep_no_workspace: "No hay espacio de trabajo abierto; no se puede agregar una dependencia",
    },
    fr: {
        ext_need_dialect_file: "Veuillez ouvrir un fichier source de dialecte (.zh/.ja …)",
        ext_lsp_restarted: "Serveur de langage redémarré",
        ext_explain_need_file: "Veuillez d'abord ouvrir un fichier avant d'utiliser l'explication des diagnostics",
        ext_explain_no_diag: "Le fichier actuel n'a aucune erreur ni avertissement ; rien à expliquer",
        ext_explain_no_diag_nearby: "Aucun diagnostic trouvé près du curseur",
        ext_add_dep_no_workspace: "Aucun espace de travail ouvert ; impossible d'ajouter une dépendance",
    },
    hi: {
        ext_need_dialect_file: "कृपया एक बोली स्रोत फ़ाइल (.zh/.ja …) खोलें",
        ext_lsp_restarted: "भाषा सर्वर पुनः आरंभ किया गया",
        ext_explain_need_file: "निदान व्याख्या उपयोग करने से पहले कृपया एक फ़ाइल खोलें",
        ext_explain_no_diag: "वर्तमान फ़ाइल में कोई त्रुटि या चेतावनी नहीं; व्याख्या की ज़रूरत नहीं",
        ext_explain_no_diag_nearby: "कर्सर के पास कोई निदान नहीं मिला",
        ext_add_dep_no_workspace: "कार्यक्षेत्र खुला नहीं है; निर्भरता जोड़ने में असमर्थ",
    },
    ja: {
        ext_need_dialect_file: "方言ソースファイル（.zh/.ja など）を開いてください",
        ext_lsp_restarted: "言語サーバーを再起動しました",
        ext_explain_need_file: "診断の説明を使う前にファイルを開いてください",
        ext_explain_no_diag: "現在のファイルにエラーや警告はなく、説明は不要です",
        ext_explain_no_diag_nearby: "カーソル付近の診断が見つかりません",
        ext_add_dep_no_workspace: "ワークスペースが開かれていないため、依存追加できません",
    },
    ko: {
        ext_need_dialect_file: "방언 소스 파일(.zh/.ja 등)을 열어 주세요",
        ext_lsp_restarted: "언어 서버가 재시작되었습니다",
        ext_explain_need_file: "진단 설명을 사용하려면 먼저 파일을 열어 주세요",
        ext_explain_no_diag: "현재 파일에 오류나 경고가 없어 설명이 필요 없습니다",
        ext_explain_no_diag_nearby: "커서 근처의 진단을 찾을 수 없습니다",
        ext_add_dep_no_workspace: "작업 공간이 열려 있지 않아 의존성을 추가할 수 없습니다",
    },
    pt: {
        ext_need_dialect_file: "Abra um arquivo-fonte de dialeto (.zh/.ja …)",
        ext_lsp_restarted: "Servidor de linguagem reiniciado",
        ext_explain_need_file: "Abra um arquivo primeiro antes de usar a explicação de diagnósticos",
        ext_explain_no_diag: "O arquivo atual não tem erros nem avisos; nada a explicar",
        ext_explain_no_diag_nearby: "Nenhum diagnóstico encontrado perto do cursor",
        ext_add_dep_no_workspace: "Nenhum espaço de trabalho aberto; não é possível adicionar dependência",
    },
    ru: {
        ext_need_dialect_file: "Откройте файл исходного кода диалекта (.zh/.ja …)",
        ext_lsp_restarted: "Языковой сервер перезапущен",
        ext_explain_need_file: "Сначала откройте файл, затем используйте объяснение диагностик",
        ext_explain_no_diag: "В текущем файле нет ошибок или предупреждений; объяснение не требуется",
        ext_explain_no_diag_nearby: "Диагностики возле курсора не найдены",
        ext_add_dep_no_workspace: "Рабочая область не открыта; невозможно добавить зависимость",
    },
    zh: {
        ext_need_dialect_file: "请打开一个方言源码文件（.zh/.ja 等）",
        ext_lsp_restarted: "语言服务器已重启",
        ext_explain_need_file: "请先打开一个文件，再使用诊断讲解",
        ext_explain_no_diag: "当前文件没有错误或警告，无需讲解",
        ext_explain_no_diag_nearby: "未找到光标附近的诊断",
        ext_add_dep_no_workspace: "未打开工作区，无法添加依赖",
    },
};

export const 回退语言 = "en";
