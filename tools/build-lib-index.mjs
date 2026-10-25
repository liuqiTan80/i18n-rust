#!/usr/bin/env node
/**
 * 已翻译库索引可视化生成器（Node，无需本机 Python）
 *
 * 读取两类数据，产出一个自包含、可交互、随语言切换整页本地化的单文件 HTML：
 *   1) 标准库：crates/engine/lang-packs/<code>/stdlib.toml
 *      （[模块路径] + [标识符]；条目用 # 注释分节作分类；若存在可选 [解释] 节，
 *       则按方言名附大白话说明）
 *   2) 第三方库：third-party/index.json + third-party/<lang>/crates/*.toml
 *      （[标识符] + [解释] 大白话描述 + updated 翻译日期 + 作者/版本/许可）
 *
 * 用法：node tools/build-lib-index.mjs [--out docs/library-index.html]
 */
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const LANG_PACKS = path.join(ROOT, 'crates', 'engine', 'lang-packs');
const THIRD_PARTY = path.join(ROOT, 'third-party');

// 语言注册表（code → 显示名 / 文字方向），与扩展 languages.ts 单一事实源一致
const LANGS = [
    { code: 'zh', name: '中文', dir: 'ltr' },
    { code: 'en', name: 'English', dir: 'ltr' },
    { code: 'ja', name: '日本語', dir: 'ltr' },
    { code: 'ko', name: '한국어', dir: 'ltr' },
    { code: 'ru', name: 'Русский', dir: 'ltr' },
    { code: 'de', name: 'Deutsch', dir: 'ltr' },
    { code: 'es', name: 'Español', dir: 'ltr' },
    { code: 'fr', name: 'Français', dir: 'ltr' },
    { code: 'pt', name: 'Português', dir: 'ltr' },
    { code: 'hi', name: 'हिन्दी', dir: 'ltr' },
    { code: 'ar', name: 'العربية', dir: 'rtl' },
];

/**
 * 轻量 TOML 行解析：支持 `["表名"]` 表头、`"键" = "值"`、`# 注释`（短标题作分类）。
 * 返回 Map<表名, Array<{k,v,cat}>>。
 */
function parseToml(content) {
    const tables = new Map();
    let current = null;
    let category = '';
    for (const raw of content.split(/\r?\n/)) {
        const line = raw.trim();
        if (!line) continue;
        if (line.startsWith('#')) {
            const text = line.replace(/^#+\s?/, '').trim();
            if (/^[-=]{2,}$/.test(text)) continue; // 纯分隔线
            if (!text) continue;
            const cleanHeading = text.length <= 26 && !/[\x60\u2192\uFF1A\u3002\uFF1B\u300C\u300D]/.test(text);
            if (cleanHeading) category = text;
            continue;
        }
        const header = /^\[\s*(?:"(.+?)"|([A-Za-z0-9_.-]+))\s*\]$/.exec(line);
        if (header) {
            current = header[1] ?? header[2];
            category = current;
            if (!tables.has(current)) tables.set(current, []);
            continue;
        }
        const kv = /^"(.+?)"\s*=\s*"(.+?)"\s*(?:#.*)?$/.exec(line);
        if (kv && current) {
            tables.get(current).push({ k: kv[1], v: kv[2], cat: category });
        }
    }
    return tables;
}

function readIf(file) {
    try { return fs.readFileSync(file, 'utf8'); } catch { return ''; }
}

function gitDate(relPath) {
    try {
        return execFileSync('git', ['-C', ROOT, 'log', '-1', '--format=%ad', '--date=short', '--', relPath], { encoding: 'utf8' }).trim();
    } catch { return ''; }
}

/**
 * 解析标准库：仅读取映射表（[模块路径] + [标识符]）。
 * 标准库条目不带大白话说明（stdlib.toml 无解释数据），说明只用于第三方库。
 */
function loadStdlib(code) {
    const file = path.join(LANG_PACKS, code, 'stdlib.toml');
    const tables = parseToml(readIf(file));
    const entries = [];
    for (const [tname, rows] of tables) {
        // 危险等级/危险备注是元数据节，不是词条（与引擎 is_metadata_section 对齐）
        if (tname === '危险等级' || tname === '危险备注') continue;
        for (const r of rows ?? []) {
            entries.push({ d: r.k, e: r.v, cat: r.cat || tname, table: tname });
        }
    }
    return { entries, date: gitDate(path.relative(ROOT, file).replace(/\\/g, '/')) };
}

/** 解析第三方库某 crate toml：标识符 + 解释（dialect→desc） */
function loadCrate(absFile) {
    const tables = parseToml(readIf(absFile));
    const explanations = new Map();
    for (const r of tables.get('解释') ?? []) explanations.set(r.k, r.v);
    const items = [];
    for (const r of tables.get('模块路径') ?? []) items.push({ d: r.k, e: r.v, desc: explanations.get(r.k) ?? '', kind: 'module' });
    for (const r of tables.get('标识符') ?? []) items.push({ d: r.k, e: r.v, desc: explanations.get(r.k) ?? '', kind: 'item' });
    return items;
}

function loadThirdParty() {
    const byLang = new Map();
    const indexPath = path.join(THIRD_PARTY, 'index.json');
    if (!fs.existsSync(indexPath)) return byLang;
    const index = JSON.parse(fs.readFileSync(indexPath, 'utf8'));
    for (const m of index.mappings ?? []) {
        const abs = path.join(THIRD_PARTY, m.file);
        if (!fs.existsSync(abs)) continue;
        const crate = {
            name: m.crate_name, version: m.version ?? '', license: m.license ?? '',
            author: m.author ?? '', updated: m.updated ?? '', quality: m.quality ?? 0,
            items: loadCrate(abs),
        };
        if (!byLang.has(m.lang)) byLang.set(m.lang, []);
        byLang.get(m.lang).push(crate);
    }
    return byLang;
}

function buildData() {
    const tp = loadThirdParty();
    return LANGS.map((lang) => {
        const std = loadStdlib(lang.code);
        return {
            ...lang, stdlibDate: std.date, stdlib: std.entries,
            crates: (tp.get(lang.code) ?? []).sort((a, b) => a.name.localeCompare(b.name)),
        };
    });
}

function buildCoverage(data) {
    const crateLangs = new Map();
    for (const l of data) for (const c of l.crates) {
        if (!crateLangs.has(c.name)) crateLangs.set(c.name, new Set());
        crateLangs.get(c.name).add(l.code);
    }
    const tableNames = new Set();
    for (const l of data) for (const e of l.stdlib) if (e.table) tableNames.add(e.table);
    return {
        totalLangs: data.length,
        crates: [...crateLangs.entries()].map(([name, set]) => ({ name, langs: [...set] })).sort((a, b) => a.name.localeCompare(b.name)),
        langsNoCrate: data.filter((l) => !l.crates.length).map((l) => l.code),
        stdlibCounts: data.map((l) => ({ code: l.code, name: l.name, n: l.stdlib.length })),
        tableNames: [...tableNames],
    };
}

const HTML = `<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>rzc 已翻译库索引 · 用母语写 Rust</title>
<style>
  :root { color-scheme: light dark; --bd:#d5d5d5; --mut:#888; --acc:#3b82f6; --bg:#fff; --fg:#1a1a1a; --chip:rgba(128,128,128,.12); }
  @media (prefers-color-scheme: dark){ :root{ --bd:#333; --mut:#9a9a9a; --bg:#1a1a1a; --fg:#eaeaea; --chip:rgba(160,160,160,.16); } }
  * { box-sizing: border-box; }
  body { font-family: system-ui,"Segoe UI","Noto Sans CJK SC","Noto Sans Arabic",sans-serif;
         max-width: 1000px; margin: 4vh auto 8vh; padding: 0 1rem; background:var(--bg); color:var(--fg); }
  h1 { font-size:1.5rem; margin:0 0 .2rem; }
  .sub { color:var(--mut); font-size:.9rem; margin:0 0 1rem; }
  .legend { background:rgba(59,130,246,.08); border:1px solid var(--bd); border-radius:8px;
            padding:.6rem .8rem; font-size:.85rem; color:var(--fg); margin-bottom:1rem; }
  .legend b{color:var(--acc);}
  .tabs { display:flex; flex-wrap:wrap; gap:6px; margin-bottom:.8rem; }
  .tab { border:1px solid var(--bd); border-radius:20px; padding:.25rem .7rem; cursor:pointer;
         font-size:.85rem; background:transparent; color:var(--fg); }
  .tab.on { background:var(--acc); color:#fff; border-color:var(--acc); }
  .search { width:100%; padding:.5rem .7rem; border:1px solid var(--bd); border-radius:8px;
            font-size:.95rem; margin-bottom:1rem; background:var(--bg); color:var(--fg); }
  h2 { font-size:1.1rem; margin:1.4rem 0 .6rem; border-bottom:2px solid var(--bd); padding-bottom:.2rem; }
  h2 .cnt { color:var(--mut); font-weight:400; font-size:.85rem; }
  .cover { border:1px solid var(--bd); border-radius:10px; margin-bottom:1rem; background:rgba(59,130,246,.05); }
  .cover > summary { padding:.6rem .9rem; cursor:pointer; font-weight:600; font-size:.9rem; }
  .coverbody { padding:.2rem .9rem .8rem; font-size:.85rem; }
  .coverbody ul { margin:.3rem 0; padding-inline-start:1.2rem; }
  .coverbody li { margin:.15rem 0; }
  .frac { color:var(--mut); font-size:.8rem; }
  .warn { color:#b45309; }
  @media (prefers-color-scheme: dark){ .warn { color:#f0b429; } }
  .stdtab { width:100%; border-collapse:collapse; margin-top:.4rem; font-size:.82rem; }
  .stdtab td { border-bottom:1px solid var(--chip); padding:.2rem .4rem; }
  .grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(280px,1fr)); gap:12px; }
  .cards { display:grid; grid-template-columns:repeat(auto-fill,minmax(300px,1fr)); gap:14px; }
  details { border:1px solid var(--bd); border-radius:10px; background:var(--bg); }
  details > summary { list-style:none; cursor:pointer; padding:.7rem .9rem; display:flex;
                      align-items:baseline; gap:.5rem; flex-wrap:wrap; }
  details > summary::-webkit-details-marker { display:none; }
  details > summary::before { content:"\\25B8"; color:var(--acc); font-size:.8rem; flex:0 0 auto; }
  details[open] > summary::before { content:"\\25BE"; }
  details[open] > summary { border-bottom:1px solid var(--bd); }
  .gt { font-weight:600; font-size:.98rem; }
  .gc { margin-inline-start:auto; color:var(--mut); font-size:.78rem; background:var(--chip);
        border-radius:12px; padding:.05rem .55rem; }
  .body { padding:.5rem .9rem .8rem; }
  .cr { font-weight:700; font-size:1.05rem; }
  .meta { color:var(--mut); font-size:.76rem; }
  .meta span{margin-inline-end:.6rem;}
  .sub-h { margin:.6rem 0 .3rem; font-size:.78rem; color:var(--acc); font-weight:600;
           border-bottom:1px dashed var(--bd); }
  .row { display:flex; flex-wrap:wrap; align-items:baseline; gap:.5rem; padding:.28rem 0;
         border-bottom:1px solid var(--chip); }
  .row .d { font-weight:600; }
  .row .e { font-family:ui-monospace,Consolas,monospace; font-size:.8rem; color:var(--mut);
            background:var(--chip); padding:.05rem .4rem; border-radius:5px; }
  .row .desc { flex-basis:100%; color:var(--fg); font-size:.83rem; opacity:.85; }
  .empty { color:var(--mut); font-style:italic; padding:.4rem 0; }
  footer { margin-top:2rem; color:var(--mut); font-size:.8rem; border-top:1px solid var(--bd); padding-top:.8rem; }
  [dir=rtl] .row { text-align:right; }
  [dir=rtl] .meta span{margin:.05rem 0 .05rem .6rem;}
</style>
</head>
<body>
<h1 id="ttl"></h1>
<p class="sub" id="subt"></p>
<div class="legend" id="legend"></div>
<div class="tabs" id="tabs"></div>
<input class="search" id="search" type="search" autocomplete="off">
<div id="coverage"></div>
<div id="content"></div>
<footer>
  <span id="ftGen"></span>
</footer>
<script>
const DATA = __DATA__;
const COVERAGE = __COVERAGE__;
let cur = "zh";
const $ = (s) => document.querySelector(s);
const esc = (s) => String(s).replace(/[&<>]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;"}[c]));
const NAME = {}; DATA.forEach(l => NAME[l.code] = l.name);

// 两张 TOML 表名是引擎统一数据键，不可改；非中文语言给母语标签
const TABLE_LABEL = {
  en: { "模块路径": "Module paths", "标识符": "Identifiers" },
  ja: { "模块路径": "モジュールパス", "标识符": "識別子" },
  ko: { "模块路径": "모듈 경로", "标识符": "식별자" },
  ru: { "模块路径": "Пути модулей", "标识符": "Идентификаторы" },
  de: { "模块路径": "Modulpfade", "标识符": "Bezeichner" },
  es: { "模块路径": "Rutas de módulos", "标识符": "Identificadores" },
  fr: { "模块路径": "Chemins de modules", "标识符": "Identifiants" },
  pt: { "模块路径": "Caminhos de módulos", "标识符": "Identificadores" },
  hi: { "模块路径": "मॉड्यूल पथ", "标识符": "पहचानकर्ता" },
  ar: { "模块路径": "مسارات الوحدات", "标识符": "المعرفات" },
};
const dispCat = (code, cat) =>
  (code !== "zh" && TABLE_LABEL[code] && TABLE_LABEL[code][cat]) ? TABLE_LABEL[code][cat] : cat;

// 分类标题清洗：去括注（含英文技术词 Vec/Option/Result 等）+ 去尾部“补充/追加”类冗余后缀
const SUPP = "补充|補足|補充|附加|添补|追加|보충|추가|adicional|adicionais|adicionales|дополнительно|дополнительн\\p{L}*|дополнени\\p{L}*|zusätzlic\\p{L}*|Ergänzung\\p{L}*|suppl[ée]mentai\\p{L}*|अतिरिक्त|إضاف[يى]\\p{L}*|supplements?";
function cleanCat(code, t) {
  const base = dispCat(code, String(t));
  let s = base.replace(/[（(][^）)]*[）)]/g, '');                                  // 去括注（含英文技术词）
  s = s.replace(new RegExp("[\\\\s/／-]*(?:" + SUPP + ")[\\\\s/／-]*$", "iu"), ''); // 去尾部“补充”类冗余
  s = s.replace(/\\s{2,}/g, ' ').trim();
  return s || base;
}

// 整页 UI 文案（随语言切换同步）
const UI = {
  zh:{title:'rzc 已翻译库索引',sub:'用母语写 Rust · 浏览各语言包已翻译的标准库与第三方库 API',legend:'<b>说明</b>：卡片默认收起，<b>点击标题</b>展开明细；<b>搜索</b>命中会自动展开。标准库条目右侧灰徽章为<b>英文原名</b>；第三方条目带<b>大白话解释</b>。<br><b>日期</b>：第三方取 index.json 翻译日期，标准库取该语言包 stdlib.toml 最近 Git 提交日期。',search:'搜索方言名 / 英文名 / 描述 / 分类…',third:'第三方库',thirdCnt:'{n} 个 · 点击卡片展开 API',std:'标准库',stdCnt:'更新于 {d}，共 {n} 条 · 点击分类展开 方言名 = 英文原名',noCrate:'该语言暂无已登记的第三方库映射（详见上方“覆盖情况”）',noStd:'无匹配的标准库条目',noCov:'无匹配的第三方库',covTitle:'📊 覆盖情况与数据源核对',covStdSrc:'标准库映射来自 <code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code>，本页读取到映射表：',covTables:'（{n} 种语言全部纳入）。',covTpSrc:'第三方库映射来自 <code>third-party/index.json</code> 与 <code>third-party/&lt;lang&gt;/crates/*.toml</code>，各 crate 跨语言覆盖如下（差异如实标注）：',covNone:'尚无第三方库映射的语言：',covDiff:'各语言标准库条目数存在差异（最少 {min}、最多 {max}），反映翻译进度不同，非漏读。',covTables2:'映射表',gen:'由 tools/build-lib-index.mjs 生成 · 生成时间',src:'数据源'},
  en:{title:'rzc Translated Library Index',sub:'Write Rust in your native language · browse the translated stdlib and third-party APIs',legend:'<b>Note</b>: cards are collapsed by default — <b>click a title</b> to expand; <b>search</b> auto-expands matches. Stdlib rows show the English original as the grey badge; third-party rows carry <b>plain-language explanations</b>.<br><b>Dates</b>: third-party uses the index.json translation date; stdlib uses the latest Git commit of that pack\\'s stdlib.toml.',search:'Search dialect name / English name / description / category…',third:'Third-party crates',thirdCnt:'{n} · click a card to expand APIs',std:'Standard library',stdCnt:'Updated {d} · {n} entries · click a category to expand  dialect = English',noCrate:'No registered third-party mappings for this language (see "Coverage" above).',noStd:'No matching stdlib entries',noCov:'No matching crates',covTitle:'📊 Coverage & data-source check',covStdSrc:'Stdlib mappings come from <code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code>; tables read on this page:',covTables:'({n} languages all included).',covTpSrc:'Third-party mappings come from <code>third-party/index.json</code> and <code>third-party/&lt;lang&gt;/crates/*.toml</code>; per-crate coverage:',covNone:'Languages without third-party mappings:',covDiff:'Stdlib entry counts differ across languages (min {min}, max {max}), reflecting translation progress, not missing reads.',covTables2:'Tables',gen:'Generated by tools/build-lib-index.mjs · generated',src:'Sources'},
  ja:{title:'rzc 翻訳済みライブラリ一覧',sub:'母語で Rust を書く · 各言語パック翻訳済みの標準ライブラリと外部クレートを閲覧',legend:'<b>説明</b>：カードは既定で折りたたみ。<b>タイトル</b>で展開、<b>検索</b>は該当を自動展開。標準は英語原名をグレーバッジで表示、外部は<b>平易な説明</b>付き。<br><b>日付</b>：外部は index.json の翻訳日、標準は stdlib.toml の最終 Git 提交日。',search:'方言名 / 英語名 / 説明 / 分類 を検索…',third:'外部クレート',thirdCnt:'{n} 件 · クリックで API 展開',std:'標準ライブラリ',stdCnt:'更新 {d} · 全 {n} 件 · クリックで分類展開（方言名 = 英語原名）',noCrate:'この言語に登録済みの外部クレートはありません（上の「カバレッジ」参照）。',noStd:'一致する標準ライブラリ項目なし',noCov:'一致するクレートなし',covTitle:'📊 カバレッジとデータソース照合',covStdSrc:'標準マッピングの出所：<code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code>。読み込んだ表：',covTables:'（{n} 言語すべて収録）。',covTpSrc:'外部マッピングの出所：<code>third-party/index.json</code> と <code>third-party/&lt;lang&gt;/crates/*.toml</code>。クレート別カバレッジ：',covNone:'外部クレート未登録の言語：',covDiff:'言語ごとに標準項目数が異なります（最小 {min}、最大 {max}）。翻訳進行度の差で、読み落としではありません。',covTables2:'表',gen:'tools/build-lib-index.mjs で生成 · 生成日',src:'データソース'},
  ko:{title:'rzc 번역된 라이브러리 인덱스',sub:'모국어로 Rust 작성 · 각 언어 팩의 번역된 표준 라이브러리와 서드파티 API 탐색',legend:'<b>설명</b>: 카드는 기본 접힘, <b>제목</b> 클릭으로 펼침, <b>검색</b> 시 자동 펼침. 표준은 영어 원명을 회색 배지로, 서드파티는 <b>쉬운 설명</b>과 함께.<br><b>날짜</b>: 서드파티는 index.json 번역일, 표준은 stdlib.toml 최근 Git 커밋일.',search:'방언명 / 영어명 / 설명 / 분류 검색…',third:'서드파티 크레이트',thirdCnt:'{n} 개 · 클릭으로 API 펼침',std:'표준 라이브러리',stdCnt:'갱신 {d} · 총 {n} 항목 · 클릭으로 분류 펼침 (방언명 = 영어 원명)',noCrate:'이 언어에 등록된 서드파티 매핑이 없습니다 (위 "커버리지" 참조).',noStd:'일치하는 표준 라이브러리 항목 없음',noCov:'일치하는 크레이트 없음',covTitle:'📊 커버리지 및 데이터 소스 확인',covStdSrc:'표준 매핑 소스: <code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code>. 읽은 표:',covTables:'({n}개 언어 모두 포함).',covTpSrc:'서드파티 매핑 소스: <code>third-party/index.json</code> 및 <code>third-party/&lt;lang&gt;/crates/*.toml</code>. 크레이트별 커버리지:',covNone:'서드파티 매핑이 없는 언어:',covDiff:'언어별 표준 항목 수 차이(최소 {min}, 최대 {max})는 번역 진행도 차이이며 누락이 아닙니다.',covTables2:'표',gen:'tools/build-lib-index.mjs 로 생성 · 생성일',src:'데이터 소스'},
  ru:{title:'rzc указатель переведённых библиотек',sub:'Пишите Rust на родном языке · просмотрите переведённые API стандартной и сторонних библиотек',legend:'<b>Примечание</b>: карточки свёрнуты — <b>кликните заголовок</b>; <b>поиск</b> разворачивает совпадения. В stdlib оригинал показан серым значком; сторонние API снабжены <b>простым пояснением</b>.<br><b>Даты</b>: у сторонних — дата из index.json; у stdlib — последний коммит stdlib.toml.',search:'Поиск: диалект / английское имя / описание / категория…',third:'Сторонние крейты',thirdCnt:'{n} · кликните, чтобы развернуть API',std:'Стандартная библиотека',stdCnt:'Обновлено {d} · всего {n} · кликните категорию (диалект = оригинал)',noCrate:'Для этого языка нет зарегистрированных сторонних маппингов (см. «Покрытие» выше).',noStd:'Нет совпадений в stdlib',noCov:'Нет совпадающих крейтов',covTitle:'📊 Покрытие и проверка источников',covStdSrc:'Маппинги stdlib: <code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code>; прочитанные таблицы:',covTables:'({n} языков включены).',covTpSrc:'Сторонние маппинги: <code>third-party/index.json</code> и <code>third-party/&lt;lang&gt;/crates/*.toml</code>; покрытие по крейтам:',covNone:'Языки без сторонних маппингов:',covDiff:'Число записей stdlib различается (мин {min}, макс {max}) — это разная готовность перевода, а не пропуск.',covTables2:'Таблицы',gen:'Сгенерировано tools/build-lib-index.mjs · дата',src:'Источники'},
  de:{title:'rzc-Index übersetzter Bibliotheken',sub:'Rust in der Muttersprache · übersetzte APIs von Standard- und Drittanbieter-Bibliotheken',legend:'<b>Hinweis</b>: Karten sind eingeklapst — <b>Titel anklicken</b>; <b>Suche</b> klappt Treffer automatisch auf. Bei stdlib zeigt das graue Abzeichen den englischen Originalnamen; Drittanbieter-APIs enthalten eine <b>Laienerklärung</b>.<br><b>Daten</b>: Drittanbieter aus index.json; stdlib aus dem letzten Git-Commit der stdlib.toml.',search:'Dialektname / englischer Name / Beschreibung / Kategorie suchen…',third:'Fremdbibliotheken',thirdCnt:'{n} · Karte zum Aufklappen der APIs anklicken',std:'Standardbibliothek',stdCnt:'Aktualisiert {d} · {n} Einträge · Kategorie anklicken (Dialekt = Original)',noCrate:'Für diese Sprache sind keine Fremd-Bibliotheken registriert (siehe „Abdeckung“ oben).',noStd:'Keine passenden Stdlib-Einträge',noCov:'Keine passenden Crate',covTitle:'📊 Abdeckung & Datenquellen-Prüfung',covStdSrc:'Stdlib-Mappings aus <code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code>; gelesene Tabellen:',covTables:'({n} Sprachen enthalten).',covTpSrc:'Fremd-Mappings aus <code>third-party/index.json</code> und <code>third-party/&lt;lang&gt;/crates/*.toml</code>; Abdeckung je Crate:',covNone:'Sprachen ohne Fremd-Bibliotheken:',covDiff:'Stdlib-Eintragszahlen unterscheiden sich (min {min}, max {max}) — Übersetzungsstand, kein Lesefehler.',covTables2:'Tabellen',gen:'Generiert von tools/build-lib-index.mjs · Datum',src:'Quellen'},
  es:{title:'Índice de bibliotecas traducidas de rzc',sub:'Escribe Rust en tu idioma · explora las APIs traducidas de la estándar y de terceros',legend:'<b>Nota</b>: tarjetas plegadas — <b>clic en el título</b>; la <b>búsqueda</b> despliega coincidencias. En stdlib el nombre inglés es la insignia gris; los de terceros traen <b>explicación sencilla</b>.<br><b>Fechas</b>: terceros desde index.json; stdlib desde el último commit de stdlib.toml.',search:'Buscar nombre dialectal / inglés / descripción / categoría…',third:'Crateos de terceros',thirdCnt:'{n} · clic para desplegar APIs',std:'Biblioteca estándar',stdCnt:'Actualizada {d} · {n} entradas · clic en categoría (dialecto = original)',noCrate:'Este idioma no tiene mappings de terceros registrados (ver "Cobertura").',noStd:'Sin entradas coincidentes en la stdlib',noCov:'Sin crateos coincidentes',covTitle:'📊 Cobertura y fuentes de datos',covStdSrc:'Mappings de stdlib desde <code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code>; tablas leídas:',covTables:'({n} idiomas incluidos).',covTpSrc:'Mappings de terceros desde <code>third-party/index.json</code> y <code>third-party/&lt;lang&gt;/crates/*.toml</code>; cobertura por crateo:',covNone:'Idiomas sin mappings de terceros:',covDiff:'El número de entradas de stdlib varía (mín {min}, máx {max}) según el avance de traducción, no por omisión.',covTables2:'Tablas',gen:'Generado por tools/build-lib-index.mjs · fecha',src:'Fuentes'},
  fr:{title:'Index des bibliothèques traduites de rzc',sub:'Écrivez Rust dans votre langue · parcourez les API traduites de la std et des bibliothèques tierces',legend:'<b>Note</b> : cartes repliées — <b>clic sur le titre</b> ; la <b>recherche</b> déplie les correspondances. Pour la std, le nom anglais est le badge gris ; les tierces portent une <b>explication simple</b>.<br><b>Dates</b> : tierces depuis index.json ; std depuis le dernier commit de stdlib.toml.',search:'Rechercher nom dialectal / anglais / description / catégorie…',third:'Crates tierces',thirdCnt:'{n} · clic pour déplier les API',std:'Bibliothèque standard',stdCnt:'Mise à jour {d} · {n} entrées · clic sur catégorie (dialecte = original)',noCrate:'Aucune correspondance tierce enregistrée pour cette langue (voir « Couverture »).',noStd:'Aucune entrée stdlib correspondante',noCov:'Aucune crate correspondante',covTitle:'📊 Couverture et sources de données',covStdSrc:'Mappings stdlib depuis <code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code> ; tables lues :',covTables:'({n} langues incluses).',covTpSrc:'Mappings tierces depuis <code>third-party/index.json</code> et <code>third-party/&lt;lang&gt;/crates/*.toml</code> ; couverture par crate :',covNone:'Langue sans mapping tiers :',covDiff:'Le nombre d\\'entrées stdlib varie (min {min}, max {max}) selon l\\'avancement de traduction, pas par omission.',covTables2:'Tables',gen:'Généré par tools/build-lib-index.mjs · date',src:'Sources'},
  pt:{title:'Índice de bibliotecas traduzidas do rzc',sub:'Escreva Rust no seu idioma · explore as APIs traduzidas da padrão e de terceiros',legend:'<b>Nota</b>: cartões recolhidos — <b>clique no título</b>; a <b>busca</b> expande correspondências. Na stdlib o nome em inglês é o selo cinza; os de terceiros trazem <b>explicação simples</b>.<br><b>Datas</b>: terceiros do index.json; stdlib do último commit do stdlib.toml.',search:'Buscar nome dialetal / inglês / descrição / categoria…',third:'Crates de terceiros',thirdCnt:'{n} · clique para expandir APIs',std:'Biblioteca padrão',stdCnt:'Atualizada {d} · {n} itens · clique na categoria (dialeto = original)',noCrate:'Este idioma não tem mapeamentos de terceiros registrados (veja "Cobertura").',noStd:'Nenhuma entrada de stdlib correspondente',noCov:'Nenhum crate correspondente',covTitle:'📊 Cobertura e fontes de dados',covStdSrc:'Mapeamentos stdlib de <code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code>; tabelas lidas:',covTables:'({n} idiomas incluídos).',covTpSrc:'Mapeamentos de terceiros de <code>third-party/index.json</code> e <code>third-party/&lt;lang&gt;/crates/*.toml</code>; cobertura por crate:',covNone:'Idiomas sem mapeamentos de terceiros:',covDiff:'A contagem de entradas stdlib varia (mín {min}, máx {max}) conforme o progresso da tradução, não por omissão.',covTables2:'Tabelas',gen:'Gerado por tools/build-lib-index.mjs · data',src:'Fontes'},
  hi:{title:'rzc अनुवादित लाइब्रेरी अनुक्रमणिका',sub:'अपनी मातृभाषा में Rust लिखें · मानक और थर्ड-पार्टी लाइब्रेरी के अनुवादित API देखें',legend:'<b>नोट</b>: कार्ड संकुचित — <b>शीर्षक क्लिक</b>; <b>खोज</b> मिलान स्वतः खोलती है। stdlib में अंग्रेज़ी मूल ग्रे बैज; थर्ड-पार्टी में <b>सरल व्याख्या</b>।<br><b>तिथि</b>: थर्ड-पार्टी index.json से; stdlib के अंतिम Git कमिट से।',search:'बोली नाम / अंग्रेज़ी / विवरण / श्रेणी खोजें…',third:'थर्ड-पार्टी क्रेट',thirdCnt:'{n} · API खोलने हेतु क्लिक',std:'मानक लाइब्रेरी',stdCnt:'अपडेट {d} · कुल {n} · श्रेणी क्लिक करें (बोली = मूल)',noCrate:'इस भाषा हेतु कोई थर्ड-पार्टी मैपिंग पंजीबद्ध नहीं (ऊपर "कवरेज" देखें)।',noStd:'मिलते-जुलते stdlib प्रविष्टि नहीं',noCov:'मिलते-जुलते क्रेट नहीं',covTitle:'📊 कवरेज एवं स्रोत जाँच',covStdSrc:'stdlib मैपिंग <code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code> से; पढ़ी गई तालिकाएँ:',covTables:'({n} भाषाएँ शामिल)।',covTpSrc:'थर्ड-पार्टी मैपिंग <code>third-party/index.json</code> व <code>third-party/&lt;lang&gt;/crates/*.toml</code> से; क्रेट-वार कवरेज:',covNone:'बिना थर्ड-पार्टी मैपिंग की भाषाएँ:',covDiff:'भाषाओं में stdlib प्रविष्टि संख्या भिन्न (न्यून {min}, अधिक {max}) — अनुवाद प्रगति, चूक नहीं।',covTables2:'तालिकाएँ',gen:'tools/build-lib-index.mjs द्वारा निर्मित · तिथि',src:'स्रोत'},
  ar:{title:'فهرس مكتبات rzc المترجمة',sub:'اكتب Rust بلغتك · تصفّح واجهات المكتبة القياسية والمكتبات الخارجية المترجمة',legend:'<b>ملاحظة</b>: البطاقات مطوية — <b>انقر العنوان</b> للفتح؛ والبحث يفتح المطابقات. في القياسية يظهر الاسم الإنجليزي بالشارة الرمادية؛ والخارجية لها <b>شرح مبسّط</b>.<br><b>التاريخ</b>: الخارجي من index.json؛ والقياسي من آخر إيداع لـ stdlib.toml.',search:'ابحث عن الاسم باللهجة / الإنجليزي / الوصف / الفئة…',third:'الحزم الخارجية',thirdCnt:'{n} · انقر لفتح الواجهات',std:'المكتبة القياسية',stdCnt:'آخر تحديث {d} · {n} بندًا · انقر الفئة (الاسم = الأصل)',noCrate:'لا توجد خريطة حزم مسجلة لهذه اللغة (انظر "التغطية" بالأعلى).',noStd:'لا بنود مطابقة في المكتبة القياسية',noCov:'لا حزم مطابقة',covTitle:'📊 التغطية ومصدر البيانات',covStdSrc:'خرائط المكتبة القياسية من <code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code>؛ الجداول المقروءة:',covTables:'({n} لغة مشمولة).',covTpSrc:'خرائط الخارج من <code>third-party/index.json</code> و<code>third-party/&lt;lang&gt;/crates/*.toml</code>؛ التغطية حسب الحزمة:',covNone:'لغات بلا خرائط خارجية:',covDiff:'عدد بنود القياسية يختلف بين اللغات (أدنى {min}, أعلى {max}) حسب تقدّم الترجمة لا سهوًا.',covTables2:'الجداول',gen:'أُنشئ بواسطة tools/build-lib-index.mjs · تاريخ الإنشاء',src:'المصادر'},
};
const fill = (t, o) => String(t).replace(/\\{(\\w+)\\}/g, (_, k) => (o[k] != null ? o[k] : ''));
const ui = (k) => (UI[cur] && UI[cur][k] != null) ? UI[cur][k] : (UI.zh[k] || k);

function applyUi(){
  const dir = (DATA.find(l => l.code===cur) || {}).dir || 'ltr';
  document.documentElement.lang = cur;
  document.documentElement.dir = dir;
  document.body.setAttribute('dir', dir);
  $('#ttl').textContent = ui('title');
  $('#subt').textContent = ui('sub');
  $('#legend').innerHTML = ui('legend');
  $('#search').placeholder = ui('search');
  $('#ftGen').textContent = ui('gen');
}
function renderTabs(){
  $('#tabs').innerHTML = DATA.map(l =>
    '<button class="tab'+(l.code===cur?" on":"")+'" data-c="'+l.code+'">'+esc(l.name)+'</button'
  ).join("");
  document.querySelectorAll(".tab").forEach(b => b.onclick = () => { cur=b.dataset.c; renderAll(); });
}
function match(e, q){
  if(!q) return true;
  return [e.d, e.e, e.desc, e.cat, e.table].some(x => x && String(x).toLowerCase().includes(q));
}
function rowHtml(e){
  return '<div class="row"><span class="d">'+esc(e.d)+'</span><span class="e">'+esc(e.e)+'</span>'+
    (e.desc ? '<span class="desc">'+esc(e.desc)+'</span>' : '')+'</div>';
}
function stdlibGrid(lang, q){
  const groups = new Map();
  for(const e of lang.stdlib){
    const g = cleanCat(lang.code, e.cat || e.table || "其它");
    if(!match(e,q) && !g.toLowerCase().includes(q)) continue;
    if(!groups.has(g)) groups.set(g, []);
    groups.get(g).push(e);
  }
  if(!groups.size) return '<p class="empty">'+esc(ui('noStd'))+'</p>';
  const open = q ? " open" : "";
  const cards = [];
  for(const [g, rows] of groups){
    cards.push('<details class="gcard"'+open+'><summary><span class="gt">'+esc(g)+'</span>'+
      '<span class="gc">'+rows.length+'</span></summary><div class="body">'+rows.map(rowHtml).join("")+'</div></details>');
  }
  return '<div class="grid">'+cards.join("")+'</div>';
}
function crateCards(lang, q){
  if(!lang.crates || !lang.crates.length)
    return '<p class="empty">'+esc(ui('noCrate'))+'</p>';
  const cards = [];
  for(const c of lang.crates){
    const items = c.items.filter(e => match(e,q));
    if(!items.length && q) continue;
    const meta = ['<span>📅 '+(esc(c.updated)||"—")+'</span>'];
    if(c.version) meta.push('<span>v'+esc(c.version)+'</span>');
    if(c.license) meta.push('<span>'+esc(c.license)+'</span>');
    const mods = items.filter(e => e.kind==="module");
    const its  = items.filter(e => e.kind==="item");
    let body = "";
    if(mods.length) body += '<div class="sub-h">'+esc(cleanCat(lang.code,"模块路径"))+' · '+mods.length+'</div>'+mods.map(rowHtml).join("");
    if(its.length)  body += '<div class="sub-h">'+esc(cleanCat(lang.code,"标识符"))+' · '+its.length+'</div>'+its.map(rowHtml).join("");
    if(!body) body = '<div class="empty">（—）</div>';
    const open = q ? " open" : "";
    cards.push('<details class="ccard"'+open+'><summary><span class="cr">'+esc(c.name)+'</span>'+
      '<span class="gc">'+items.length+' API</span></summary><div class="body"><div class="meta">'+meta.join("")+'</div>'+body+'</div></details>');
  }
  return cards.length ? '<div class="cards">'+cards.join("")+'</div>'
                      : '<p class="empty">'+esc(ui('noCov'))+'</p>';
}
function renderCoverage(){
  const cov = COVERAGE;
  let h = '<details class="cover"><summary>'+esc(ui('covTitle'))+'</summary><div class="coverbody">';
  h += '<p>'+ui('covStdSrc')+' <b>'+cov.tableNames.map(esc).join(' + ')+'</b>'+esc(fill(ui('covTables'),{n:cov.totalLangs}))+'</p>';
  h += '<p>'+ui('covTpSrc')+'</p><ul>';
  for(const c of cov.crates){
    const names = c.langs.map(x => esc(NAME[x]||x)).join('、');
    h += '<li><b>'+esc(c.name)+'</b>：'+names+' <span class="frac">'+c.langs.length+'/'+cov.totalLangs+'</span></li>';
  }
  h += '</ul>';
  if(cov.langsNoCrate.length){
    h += '<p class="warn">'+ui('covNone')+cov.langsNoCrate.map(x => esc(NAME[x]||x)).join('、')+'（'+cov.langsNoCrate.length+'）</p>';
  }
  const cnts = cov.stdlibCounts.map(x => x.n);
  const min = Math.min.apply(null, cnts), max = Math.max.apply(null, cnts);
  if(min !== max) h += '<p class="warn">'+esc(fill(ui('covDiff'),{min:Math.min.apply(null,cnts),max:max}))+'</p>';
  h += '<table class="stdtab"><tbody>';
  for(const s of cov.stdlibCounts) h += '<tr><td>'+esc(s.name)+'</td><td>'+s.n+'</td></tr>';
  h += '</tbody></table></div></details>';
  $("#coverage").innerHTML = h;
}
function render(){
  const lang = DATA.find(l => l.code===cur);
  const q = ($("#search").value||"").trim().toLowerCase();
  const c = $("#content");
  c.setAttribute("dir", lang.dir);
  c.setAttribute("lang", cur);
  const date = lang.stdlibDate || "—";
  c.innerHTML =
    '<h2>'+esc(ui('third'))+' <span class="cnt">'+esc(fill(ui('thirdCnt'),{n:(lang.crates?lang.crates.length:0)}))+'</span></h2>' + crateCards(lang, q) +
    '<h2>'+esc(ui('std'))+' <span class="cnt">'+esc(fill(ui('stdCnt'),{d:date,n:lang.stdlib.length}))+'</span></h2>' + stdlibGrid(lang, q);
}
function renderAll(){ applyUi(); renderTabs(); renderCoverage(); render(); }
$("#search").addEventListener("input", render);
$("#ftGen").textContent += " " + "__GEN__";
renderAll();
</script>
</body>
</html>
`;

function main() {
    const args = process.argv.slice(2);
    const outIdx = args.indexOf('--out');
    const outRel = outIdx >= 0 ? args[outIdx + 1] : path.join('docs', 'library-index.html');
    const outPath = path.resolve(ROOT, outRel);
    const data = buildData();
    const coverage = buildCoverage(data);
    const gen = new Date().toISOString().slice(0, 10);
    const html = HTML
        .replace('__DATA__', JSON.stringify(data))
        .replace('__COVERAGE__', JSON.stringify(coverage))
        .replace('__GEN__', gen);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, html, 'utf8');
    const totalStd = data.reduce((n, l) => n + l.stdlib.length, 0);
    const totalCr = data.reduce((n, l) => n + (l.crates ? l.crates.length : 0), 0);
    console.log(`已生成 ${path.relative(ROOT, outPath)}：合计标准库 ${totalStd} 条，第三方 ${totalCr} 库`);
}

main();
