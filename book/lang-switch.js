// =============================================================
// rzc 文档站 · 顶栏语言切换
//
// mdBook 原生不支持多语言站点，本脚本在每种语言书的页面右上角注入切换菜单。
// 站点内各语言位于站根的同级子目录（/<lang>/，由 tools/build-site.py 生成），
// 从当前路径中定位语言段并替换即可完成切换。
//
// 新增语言时：在 LANGS 与 tools/build-site.py 的注册表中各加一行。
// =============================================================
(function () {
  "use strict";

  var LANGS = [
    { code: "zh", label: "中文" },
    { code: "en", label: "English" },
    { code: "ja", label: "日本語" },
    { code: "ru", label: "Русский" }
  ];

  // 从 URL 路径定位语言段（如 /i18n-rust/zh/chapter.html → zh）。
  // 落地页（站根）不注入：落地页自身已提供语言卡片。
  var m = window.location.pathname.match(/\/(zh|en|ja|ru)\//);
  if (!m) return;
  var current = m[1];
  // 站根前缀（含尾斜杠）：/i18n-rust/zh/... → /i18n-rust/
  var root = window.location.pathname.slice(0, m.index + 1);

  var box = document.createElement("div");
  box.id = "rzc-lang-switch";
  box.setAttribute(
    "style",
    [
      "position:fixed",
      "top:10px",
      "right:12px",
      "z-index:1000",
      "background:var(--bg,#fff)",
      "border:1px solid var(--table-border-color,#d0d0d0)",
      "border-radius:6px",
      "padding:3px 9px",
      "font-size:13px",
      "line-height:1.6",
      "box-shadow:0 1px 4px rgba(0,0,0,.08)",
      "user-select:none"
    ].join(";")
  );

  var parts = [];
  for (var i = 0; i < LANGS.length; i++) {
    var l = LANGS[i];
    if (l.code === current) {
      parts.push('<b style="color:var(--fg,#333)">' + l.label + "</b>");
    } else {
      parts.push(
        '<a href="' + root + l.code + '/" ' +
          'style="color:var(--links,#2b6cb0);text-decoration:none">' +
          l.label + "</a>"
      );
    }
  }
  box.innerHTML = parts.join(' <span style="color:#bbb">·</span> ');
  document.body.appendChild(box);

  var st = document.createElement("style");
  st.textContent = "@media print{#rzc-lang-switch{display:none}}";
  document.head.appendChild(st);
})();
