#!/usr/bin/env node
/**
 * 已翻译库索引可视化生成器（Node，无需本机 Python）
 *
 * 读取两类数据，产出一个自包含、可交互的单文件 HTML：
 *   1) 标准库：crates/engine/lang-packs/<code>/stdlib.toml
 *      （[模块路径] + [标识符]；条目用 # 注释分节作分类；无独立描述字段，
 *       故以「方言名本身」作见名知意说明 + 附英文原名）
 *   2) 第三方库：third-party/index.json + third-party/<lang>/crates/*.toml
 *      （[标识符] + [解释] 大白话描述 + updated 翻译日期 + 作者/版本/许可）
 *
 * 翻译日期来源：
 *   - 第三方库：index.json 的 updated 字段（逐 crate+语言）。
 *   - 标准库：该语言 stdlib.toml 的 git 最近提交日期（诚实反映"上次更新"）。
 *
 * 展示：
 *   - 第三方库：每张卡片默认收起，点击展开查看该 crate 的 API 明细（模块路径/标识符 + 大白话解释 + 元数据）。
 *   - 标准库：按分类聚合为「卡片网格」，每张卡片显示分类名 + 条目数，展开可见「方言名 = 英文原名」。
 *   - 覆盖核对：附「覆盖情况」面板，如实标注各 crate/语言的数据覆盖差异（不编造缺失库）。
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
 * 轻量 TOML 行解析（与 prompt-builder.ts 同源约定，并额外捕获 # 注释作分类）。
 * 支持：`["表名"]` 表头、`"键" = "值"`（非 ASCII 键必带引号）、`# 注释`。
 * 返回 Map<表名, Array<{k,v,cat}>>，cat 为最近一条"非分隔线"注释文本。
 */
function parseToml(content) {
    const tables = new Map();
    let current = null;
    let category = '';
    let prevSep = false;
    for (const raw of content.split(/\r?\n/)) {
        const line = raw.trim();
        if (!line) continue;
        if (line.startsWith('#')) {
            const text = line.replace(/^#+\s?/, '').trim();
            if (/^[-=]{2,}$/.test(text)) { prevSep = true; continue; } // 纯分隔线
            if (!text) continue;
            // 仅“短标题”作分节分类：长度适中、不含反引号/箭头/句读标点；
            // 长篇说明注释一律忽略，维持上一分类（避免把注释长句当标题）
            const cleanHeading = text.length <= 26 && !/[\x60\u2192\uFF1A\u3002\uFF1B\u300C\u300D]/.test(text);
            if (cleanHeading) category = text;
            prevSep = false;
            continue;
        }
        prevSep = false;
        const header = /^\[\s*(?:"(.+?)"|([A-Za-z0-9_.-]+))\s*\]$/.exec(line);
        if (header) {
            current = header[1] ?? header[2];
            category = current; // 进入表时基线分类=表名，后续子注释再细化
            if (!tables.has(current)) tables.set(current, []);
            continue;
        }
        const kv = /^"(.+?)"\s*=\s*"(.+?)"\s*$/.exec(line);
        if (kv && current) {
            tables.get(current).push({ k: kv[1], v: kv[2], cat: category });
        }
    }
    return tables;
}

/** 读取文件文本，缺失返回 '' */
function readIf(file) {
    try {
        return fs.readFileSync(file, 'utf8');
    } catch {
        return '';
    }
}

/** stdlib.toml 的 git 最近提交日期（YYYY-MM-DD），失败返回 '' */
function gitDate(relPath) {
    try {
        return execFileSync('git', ['-C', ROOT, 'log', '-1', '--format=%ad', '--date=short', '--', relPath], { encoding: 'utf8' }).trim();
    } catch {
        return '';
    }
}

/**
 * 解析标准库：泛化读取 stdlib.toml 的【全部映射表】（当前为 模块路径 + 标识符；
 * 若将来新增其它表也自动纳入，避免漏读）。每条带 cat（分节分类）与 table（来源表名）。
 */
function loadStdlib(code) {
    const file = path.join(LANG_PACKS, code, 'stdlib.toml');
    const tables = parseToml(readIf(file));
    const entries = [];
    for (const [tname, rows] of tables) {
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

/** 读取 index.json 并按语言归组第三方库 */
function loadThirdParty() {
    const byLang = new Map();
    const indexPath = path.join(THIRD_PARTY, 'index.json');
    if (!fs.existsSync(indexPath)) return byLang;
    const index = JSON.parse(fs.readFileSync(indexPath, 'utf8'));
    for (const m of index.mappings ?? []) {
        const abs = path.join(THIRD_PARTY, m.file);
        if (!fs.existsSync(abs)) continue;
        const crate = {
            name: m.crate_name,
            version: m.version ?? '',
            license: m.license ?? '',
            author: m.author ?? '',
            updated: m.updated ?? '',
            quality: m.quality ?? 0,
            items: loadCrate(abs),
        };
        if (!byLang.has(m.lang)) byLang.set(m.lang, []);
        byLang.get(m.lang).push(crate);
    }
    return byLang;
}

/** 组装全部语言数据 */
function buildData() {
    const tp = loadThirdParty();
    return LANGS.map((lang) => {
        const std = loadStdlib(lang.code);
        return {
            ...lang,
            stdlibDate: std.date,
            stdlib: std.entries,
            crates: (tp.get(lang.code) ?? []).sort((a, b) => a.name.localeCompare(b.name)),
        };
    });
}

/**
 * 数据覆盖核对：从实际读取结果派生（严禁编造），供页面「覆盖情况」面板使用。
 * - crates：每个 crate → 已提供其映射的语言 code 列表（跨语言覆盖差异，如 rand 仅 zh）。
 * - langsNoCrate：完全没有第三方库映射的语言。
 * - stdlibCounts：各语言标准库条目数（暴露条目多少差异）。
 * - tableNames：stdlib.toml 实际读取到的映射表名（核对是否读全）。
 */
function buildCoverage(data) {
    const crateLangs = new Map();
    for (const l of data) {
        for (const c of l.crates) {
            if (!crateLangs.has(c.name)) crateLangs.set(c.name, new Set());
            crateLangs.get(c.name).add(l.code);
        }
    }
    const tableNames = new Set();
    for (const l of data) for (const e of l.stdlib) if (e.table) tableNames.add(e.table);
    return {
        totalLangs: data.length,
        crates: [...crateLangs.entries()]
            .map(([name, set]) => ({ name, langs: [...set] }))
            .sort((a, b) => a.name.localeCompare(b.name)),
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
  /* 覆盖面板 */
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
  /* 网格卡片 */
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
<h1>rzc 已翻译库索引</h1>
<p class="sub">用母语写 Rust · 浏览各语言包已翻译的标准库与第三方库 API</p>
<div class="legend">
  <b>说明</b>：标准库条目无独立文字描述，故以「方言名本身」作见名知意的说明，右侧灰色徽章为其对应的<b>英文原名</b>；
  第三方库条目带作者撰写的<b>大白话解释</b>（若该条目未写解释，则同样以方言名 + 英文原名呈现）。
  <br><b>交互</b>：卡片默认收起，<b>点击标题</b>即可展开查看明细；<b>搜索</b>时命中分组会自动展开。
  <br><b>日期</b>：第三方库取 <code>index.json</code> 的翻译日期；标准库取该语言包 <code>stdlib.toml</code> 的最近 Git 提交日期。
</div>
<div class="tabs" id="tabs"></div>
<input class="search" id="search" type="search" placeholder="搜索方言名 / 英文名 / 描述 / 分类…" autocomplete="off">
<div id="coverage"></div>
<div id="content"></div>
<footer>
  由 <code>tools/build-lib-index.mjs</code> 生成 · 生成时间 <span id="gen"></span> ·
  数据源：<code>crates/engine/lang-packs/</code> 与 <code>third-party/</code>
</footer>
<script>
const DATA = __DATA__;
const COVERAGE = __COVERAGE__;
let cur = "zh";
const $ = (s) => document.querySelector(s);
const esc = (s) => String(s).replace(/[&<>]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;"}[c]));
const NAME = {}; DATA.forEach(l => NAME[l.code] = l.name);
// 两张 TOML 表名（模块路径/标识符）是引擎规定的跨语言统一数据键，绝不可改；
// 但在非中文语言的展示层给它们一个母语标签，避免页面出现未翻译的中文分类名。
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

function renderTabs(){
  $("#tabs").innerHTML = DATA.map(l =>
    '<button class="tab'+(l.code===cur?" on":"")+'" data-c="'+l.code+'">'+esc(l.name)+'</button>'
  ).join("");
  document.querySelectorAll(".tab").forEach(b => b.onclick = () => { cur=b.dataset.c; renderTabs(); render(); });
}
function match(e, q){
  if(!q) return true;
  return [e.d, e.e, e.desc, e.cat, e.table].some(x => x && String(x).toLowerCase().includes(q));
}
function rowHtml(e){
  return '<div class="row"><span class="d">'+esc(e.d)+'</span><span class="e">'+esc(e.e)+'</span>'+
    (e.desc ? '<span class="desc">'+esc(e.desc)+'</span>' : '')+'</div>';
}
// 标准库：按分类聚合为卡片网格；每张卡片标题=分类名+条目数，展开见 方言名=英文原名
function stdlibGrid(lang, q){
  const groups = new Map();
  for(const e of lang.stdlib){
    const g = e.cat || e.table || "其它";
    if(!match(e,q) && !dispCat(lang.code, g).toLowerCase().includes(q)) continue;
    if(!groups.has(g)) groups.set(g, []);
    groups.get(g).push(e);
  }
  if(!groups.size) return '<p class="empty">无匹配的标准库条目</p>';
  const open = q ? " open" : "";
  const cards = [];
  for(const [g, rows] of groups){
    cards.push(
      '<details class="gcard"'+open+'><summary><span class="gt">'+esc(dispCat(lang.code, g))+'</span>'+
      '<span class="gc">'+rows.length+'</span></summary>'+
      '<div class="body">'+rows.map(rowHtml).join("")+'</div></details>'
    );
  }
  return '<div class="grid">'+cards.join("")+'</div>';
}
// 第三方库：每张卡片默认收起，展开显示 API 明细（模块路径 / 标识符 + 解释）与元数据
function crateCards(lang, q){
  if(!lang.crates || !lang.crates.length)
    return '<p class="empty">该语言暂无已登记的第三方库映射（详见上方“覆盖情况”）</p>';
  const cards = [];
  for(const c of lang.crates){
    const items = c.items.filter(e => match(e,q));
    if(!items.length && q) continue;
    const meta = ['<span>📅 '+(esc(c.updated)||"—")+'</span>'];
    if(c.version) meta.push('<span>v'+esc(c.version)+'</span>');
    if(c.license) meta.push('<span>'+esc(c.license)+'</span>');
    if(c.author) meta.push('<span>作者 '+esc(c.author)+'</span>');
    const mods = items.filter(e => e.kind==="module");
    const its  = items.filter(e => e.kind==="item");
    let body = "";
    if(mods.length) body += '<div class="sub-h">'+esc(dispCat(lang.code, "模块路径"))+' · '+mods.length+'</div>'+mods.map(rowHtml).join("");
    if(its.length)  body += '<div class="sub-h">'+esc(dispCat(lang.code, "标识符"))+' · '+its.length+'</div>'+its.map(rowHtml).join("");
    if(!body) body = '<div class="empty">（暂无条目）</div>';
    const open = q ? " open" : "";
    cards.push(
      '<details class="ccard"'+open+'><summary><span class="cr">'+esc(c.name)+'</span>'+
      '<span class="gc">'+items.length+' API</span></summary>'+
      '<div class="body"><div class="meta">'+meta.join("")+'</div>'+body+'</div></details>'
    );
  }
  return cards.length ? '<div class="cards">'+cards.join("")+'</div>'
                      : '<p class="empty">无匹配的第三方库</p>';
}
// 覆盖核对面板（语言无关，静态渲染一次）
function renderCoverage(){
  let h = '<details class="cover"><summary>📊 覆盖情况与数据源核对</summary><div class="coverbody">';
  h += '<p>标准库映射来自 <code>crates/engine/lang-packs/&lt;code&gt;/stdlib.toml</code>，';
  h += '本页读取到映射表：<b>'+COVERAGE.tableNames.map(esc).join(' + ')+'</b>（'+COVERAGE.totalLangs+' 种语言全部纳入）。</p>';
  h += '<p>第三方库映射来自 <code>third-party/index.json</code> 与 <code>third-party/&lt;lang&gt;/crates/*.toml</code>；';
  h += '各 crate 的跨语言覆盖如下（差异如实标注，未提供者绝不臆造）：</p><ul>';
  for(const c of COVERAGE.crates){
    const names = c.langs.map(x => esc(NAME[x]||x)).join('、');
    h += '<li><b>'+esc(c.name)+'</b>：'+names+' <span class="frac">'+c.langs.length+'/'+COVERAGE.totalLangs+'</span></li>';
  }
  h += '</ul>';
  if(COVERAGE.langsNoCrate.length){
    h += '<p class="warn">尚无第三方库映射的语言：'+COVERAGE.langsNoCrate.map(x => esc(NAME[x]||x)).join('、')+
         '（共 '+COVERAGE.langsNoCrate.length+' 种）。</p>';
  }
  const cnts = COVERAGE.stdlibCounts.map(x => x.n);
  const min = Math.min.apply(null, cnts), max = Math.max.apply(null, cnts);
  if(min !== max){
    h += '<p class="warn">各语言标准库条目数存在差异（最少 '+min+'、最多 '+max+'），';
    h += '反映各语言包翻译进度不同，非页面漏读。</p>';
  }
  h += '<table class="stdtab"><tbody>';
  for(const s of COVERAGE.stdlibCounts) h += '<tr><td>'+esc(s.name)+'</td><td>'+s.n+' 条</td></tr>';
  h += '</tbody></table>';
  h += '</div></details>';
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
    '<h2>第三方库 <span class="cnt">'+(lang.crates?lang.crates.length:0)+' 个 · 点击卡片展开 API</span></h2>' + crateCards(lang, q) +
    '<h2>标准库 <span class="cnt">更新于 '+esc(date)+'，共 '+lang.stdlib.length+' 条 · 点击分类展开 方言名 = 英文原名</span></h2>' + stdlibGrid(lang, q);
}
$("#gen").textContent = "__GEN__";
$("#search").addEventListener("input", render);
renderTabs();
renderCoverage();
render();
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
    console.log(`已生成 ${path.relative(ROOT, outPath)}：`);
    console.log(`  标准库映射表：${coverage.tableNames.join(' + ')}`);
    for (const c of coverage.crates) {
        console.log(`  crate ${c.name.padEnd(8)} 覆盖语言 ${c.langs.length}/${coverage.totalLangs}：${c.langs.join(', ')}`);
    }
    if (coverage.langsNoCrate.length) {
        console.log(`  无第三方库映射的语言：${coverage.langsNoCrate.join(', ')}`);
    }
    for (const l of data) {
        console.log(`  ${l.code.padEnd(3)} ${l.name.padEnd(10)} 标准库 ${String(l.stdlib.length).padStart(4)} 条 · 第三方 ${l.crates.length} 库  (stdlib 更新 ${l.stdlibDate || '—'})`);
    }
    console.log(`合计：标准库条目 ${totalStd}，第三方库映射 ${totalCr}`);
}

main();
