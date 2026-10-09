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

/** 解析标准库：合并 [模块路径] + [标识符] 为带 kind/cat 的条目 */
function loadStdlib(code) {
    const file = path.join(LANG_PACKS, code, 'stdlib.toml');
    const tables = parseToml(readIf(file));
    const entries = [];
    const push = (rows, kind) => {
        for (const r of rows ?? []) entries.push({ d: r.k, e: r.v, cat: r.cat, kind });
    };
    push(tables.get('模块路径'), 'module');
    push(tables.get('标识符'), 'item');
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

const HTML = `<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>rzc 已翻译库索引 · 用母语写 Rust</title>
<style>
  :root { color-scheme: light dark; --bd:#d5d5d5; --mut:#888; --acc:#3b82f6; --bg:#fff; --fg:#1a1a1a; }
  @media (prefers-color-scheme: dark){ :root{ --bd:#333; --mut:#9a9a9a; --bg:#1a1a1a; --fg:#eaeaea; } }
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
  .cards { display:grid; grid-template-columns:repeat(auto-fill,minmax(280px,1fr)); gap:14px; }
  .card { border:1px solid var(--bd); border-radius:10px; padding:.9rem 1rem; }
  .card .cr { font-weight:700; font-size:1.05rem; }
  .meta { color:var(--mut); font-size:.78rem; margin:.2rem 0 .6rem; }
  .meta span{margin-right:.6rem;}
  .cat { margin:.7rem 0 .3rem; font-size:.8rem; color:var(--acc); font-weight:600;
         border-bottom:1px dashed var(--bd); }
  .row { display:flex; flex-wrap:wrap; align-items:baseline; gap:.5rem; padding:.28rem 0;
         border-bottom:1px solid rgba(128,128,128,.12); }
  .row .d { font-weight:600; }
  .row .e { font-family:ui-monospace,Consolas,monospace; font-size:.8rem; color:var(--mut);
            background:rgba(128,128,128,.12); padding:.05rem .4rem; border-radius:5px; }
  .row .desc { flex-basis:100%; color:var(--fg); font-size:.83rem; opacity:.85; }
  .empty { color:var(--mut); font-style:italic; padding:.4rem 0; }
  footer { margin-top:2rem; color:var(--mut); font-size:.8rem; border-top:1px solid var(--bd); padding-top:.8rem; }
  [dir=rtl] .row { text-align:right; }
  [dir=rtl] .meta span{margin:0 .6rem 0 0;}
</style>
</head>
<body>
<h1>rzc 已翻译库索引</h1>
<p class="sub">用母语写 Rust · 浏览各语言包已翻译的标准库与第三方库 API</p>
<div class="legend">
  <b>说明</b>：标准库条目无独立文字描述，故以「方言名本身」作见名知意的说明，右侧灰色徽章为其对应的<b>英文原名</b>；
  第三方库条目带作者撰写的<b>大白话解释</b>（若该条目未写解释，则同样以方言名 + 英文原名呈现）。
  <br><b>日期</b>：第三方库取 <code>index.json</code> 的翻译日期；标准库取该语言包 <code>stdlib.toml</code> 的最近 Git 提交日期。
</div>
<div class="tabs" id="tabs"></div>
<input class="search" id="search" type="search" placeholder="搜索方言名 / 英文名 / 描述 / 分类…" autocomplete="off">
<div id="content"></div>
<footer>
  由 <code>tools/build-lib-index.mjs</code> 生成 · 生成时间 <span id="gen"></span> ·
  数据源：<code>crates/engine/lang-packs/</code> 与 <code>third-party/</code>
</footer>
<script>
const DATA = __DATA__;
let cur = "zh";
const $ = (s) => document.querySelector(s);
const esc = (s) => String(s).replace(/[&<>]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;"}[c]));

function renderTabs(){
  $("#tabs").innerHTML = DATA.map(l =>
    '<button class="tab'+(l.code===cur?" on":"")+'" data-c="'+l.code+'">'+esc(l.name)+'</button>'
  ).join("");
  document.querySelectorAll(".tab").forEach(b => b.onclick = () => { cur=b.dataset.c; renderTabs(); render(); });
}
function match(e, q){
  if(!q) return true;
  return [e.d, e.e, e.desc, e.cat].some(x => x && x.toLowerCase().includes(q));
}
function stdlibBlock(lang, q){
  const groups = new Map();
  for(const e of lang.stdlib){ if(!match(e,q)) continue;
    const g = e.cat || (e.kind==="module"?"模块路径":"标识符");
    if(!groups.has(g)) groups.set(g, []); groups.get(g).push(e); }
  if([...groups.values()].every(a=>!a.length)) return '<p class="empty">无匹配的标准库条目</p>';
  let h = "";
  for(const [g, rows] of groups){
    if(!rows.length) continue;
    h += '<div class="cat">'+esc(g)+' · '+rows.length+'</div>';
    h += rows.map(e => '<div class="row"><span class="d">'+esc(e.d)+'</span><span class="e">'+esc(e.e)+'</span></div>').join("");
  }
  return h;
}
function crateCards(lang, q){
  if(!lang.crates || !lang.crates.length) return '<p class="empty">该语言暂无已登记的第三方库映射</p>';
  const cards = [];
  for(const c of lang.crates){
    const items = c.items.filter(e => match(e,q));
    if(!items.length && q) continue;
    const meta = ['<span>📅 '+(esc(c.updated)||"—")+'</span>'];
    if(c.version) meta.push('<span>v'+esc(c.version)+'</span>');
    if(c.license) meta.push('<span>'+esc(c.license)+'</span>');
    if(c.author) meta.push('<span>作者 '+esc(c.author)+'</span>');
    let body = items.map(e =>
      '<div class="row"><span class="d">'+esc(e.d)+'</span><span class="e">'+esc(e.e)+'</span>'+
      (e.desc?'<span class="desc">'+esc(e.desc)+'</span>':'')+'</div>'
    ).join("") || '<div class="empty">（暂无条目）</div>';
    cards.push('<div class="card"><div class="cr">'+esc(c.name)+'</div><div class="meta">'+meta.join("")+'</div>'+body+'</div>');
  }
  return cards.length ? '<div class="cards">'+cards.join("")+'</div>' : '<p class="empty">无匹配的第三方库</p>';
}
function render(){
  const lang = DATA.find(l => l.code===cur);
  const q = ($("#search").value||"").trim().toLowerCase();
  const c = $("#content");
  c.setAttribute("dir", lang.dir);
  const date = lang.stdlibDate || "—";
  c.innerHTML =
    '<h2>第三方库 · '+(lang.crates?lang.crates.length:0)+' 个</h2>' + crateCards(lang, q) +
    '<h2>标准库（stdlib.toml，更新于 '+esc(date)+'，共 '+lang.stdlib.length+' 条）</h2>' + stdlibBlock(lang, q);
}
$("#gen").textContent = "__GEN__";
$("#search").addEventListener("input", render);
renderTabs();
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
    const gen = new Date().toISOString().slice(0, 10);
    const html = HTML.replace('__DATA__', JSON.stringify(data)).replace('__GEN__', gen);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, html, 'utf8');
    const totalStd = data.reduce((n, l) => n + l.stdlib.length, 0);
    const totalCr = data.reduce((n, l) => n + (l.crates ? l.crates.length : 0), 0);
    console.log(`已生成 ${path.relative(ROOT, outPath)}：`);
    for (const l of data) {
        console.log(`  ${l.code.padEnd(3)} ${l.name.padEnd(10)} 标准库 ${String(l.stdlib.length).padStart(4)} 条 · 第三方 ${l.crates.length} 库  (stdlib 更新 ${l.stdlibDate || '—'})`);
    }
    console.log(`合计：标准库条目 ${totalStd}，第三方库映射 ${totalCr}`);
}

main();
