#!/usr/bin/env python3
"""zh 自举：engine 自定义 ABI 标识符全面中文化改名器。

用法：
  rename-abi.py verify        # 校验所有中文目标名不被任何词表整词劫持
  rename-abi.py zh            # 应用到 crates/engine/src/*.zh（源真相）
  rename-abi.py rs <路径...>  # 应用到指定 .rs 文件（cli/lsp/tests/benches/examples）

规则分层：PATH 模块路径规则 → 文件级特例 → 点/冒号字段模式 → 全词映射。
字符串字面量（含 r#""#、字符、生命周期）感知跳过；注释参与替换（文档链接同步改名）。
"""
import re, sys, tomllib, pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]

# ── 1. 模块路径根（.zh 与 cli/lsp .rs 通用：`cache::` 等） ──────────────
MOD_PATH = {
    'alias': '别名替换', 'cache': '缓存', 'lexer': '词法', 'lint': '教学检查',
    'mapping_manager': '映射管理', 'mapping_source': '映射源',
    'module_path': '模块路径', 'diagnostic': '诊断',
}

# ── 2. 全词改名表（类型/函数/方法/长字段名/枚举变体/常量） ──────────────
GLOBAL_WORD = {
    # 类型
    'MappingManager': '映射管理器', 'TranslationCache': '转译缓存',
    'TranspileOutput': '转译产出', 'SourceMapEntry': '源映射条目',
    'ProjectContext': '项目上下文', 'DeclaredNames': '声明收集',
    'ReplaceResult': '替换结果', 'TranspileResult': '词法产出',
    'LintKind': '检查种类', 'LintWarning': '检查警告',
    'MappingCategory': '映射类别', 'MappingLoader': '映射加载器',
    'SectionMap': '节表', 'DECL_KEYWORDS': '声明关键字表',
    'LanguageGuard': '语言守卫', 'LangTestGuard': '语言测试守卫',
    # lib 入口函数
    'transpile_source_with_map': '源码转译并映射',
    'transpile_source_with_project': '源码转译并项目',
    'transpile_pipeline_with_map_and_project': '转译管线并映射并项目',
    'transpile_pipeline_quiet_with_project': '转译管线静默并项目',
    'transpile_pipeline_with_project': '转译管线并项目',
    'transpile_pipeline_with_map': '转译管线并映射',
    'transpile_pipeline_quiet': '转译管线静默',
    'transpile_pipeline': '转译管线',
    'transpile_source': '源码转译',
    # 词法
    'transpile_source_with_macros': '词法转译并宏',
    'transpile_source_with_macro_map': '词法转译并宏映射',
    'transpile_with_map': '词法转译并映射',
    'reverse_transpile': '逆向转译',
    'added_crate_token_indices': '补包标记下标',
    # 别名替换
    'replace_aliases_with_context_and_module_paths': '替换别名并上下文并模块路径',
    'replace_aliases_with_context': '替换别名并上下文',
    'replace_aliases_with_map': '替换别名并映射',
    'replace_aliases': '替换别名',
    'collect_source_declarations': '收集源声明',
    'collect_declared_names': '收集声明名',
    'from_sources': '自源文件新建',
    'from_declarations': '自声明新建',
    # 模块路径
    'replace_module_paths_with_map': '模块路径替换并映射',
    'replace_module_paths': '模块路径替换',
    'qualify_module_paths_with_map': '模块路径限定并映射',
    'strip_file_module_decls': '剥离文件模块声明',
    'annotate_non_ascii_mods_with_lines': '标注非西文模块并行号',
    'annotate_non_ascii_mods': '标注非西文模块',
    'annotate_nested_mods_with_lines': '标注嵌套模块并行号',
    'annotate_mod_paths_with_lines': '标注模块路径并行号',
    # 教学检查
    'set_teaching_lint_enabled': '设定教学检查开关',
    'teaching_lint_enabled': '教学检查开关',
    'lint_teaching_with_words': '执行教学检查并词表',
    'lint_teaching': '执行教学检查',
    # 映射管理
    'load_from_builtin': '自内置加载', 'load_from_dir': '自目录加载',
    'load_from_file': '自文件加载', 'from_flat_maps': '自扁平映射新建',
    'ambiguous_constructor_words': '歧义构造词集',
    'context_fingerprint': '语境指纹', 'find_mapping_cycles': '查找映射环',
    'get_keyword_map': '取关键词映射表', 'get_section_mapping': '取节映射',
    'get_module_path_map': '取模块路径映射表', 'get_alias_map': '取别名映射表',
    'get_use_defer_words': '取使用延迟词', 'get_method_defer_words': '取方法延迟词',
    'get_lint_words': '取教学检查词', 'get_macro_names': '取宏名集',
    'get_macro_map': '取宏映射表', 'get_derive_map': '取派生映射表',
    # 缓存
    'compute_content_hash': '计算内容哈希',
    'generate_context_fingerprint': '生成语境指纹',
    'combine_fingerprint': '合并语境指纹',
    'get_or_transpile': '检索或转译', 'persistent_default': '持久默认项',
    'current_count': '条目数目', 'capacity_value': '容量数值',
    'hit_count': '命中数目', 'miss_count': '缺失数目', 'hit_rate': '命中率数值',
    # 映射源
    'default_filename': '默认文件名', 'display_name': '显示名称',
    'reverse_query': '逆向检索', 'get_sub_categories': '取子分类',
    'get_sub_mapping': '取子映射', 'get_mapping': '取分类映射',
    'entry_count': '条目数量', 'load_keyword_mapping': '加载关键词映射',
    'load_stdlib_mapping': '加载标准库映射', 'load_all_mappings': '加载全部映射',
    'create_builtin_keyword_mapping': '创建内置关键词映射',
    'merge_module_and_ident_sections': '合并模块路径与标识符节',
    'flatten_sections': '摊平节表', 'parse_toml_sections': '解析配置节',
    'load_all': '加载全部',
    # 语言
    'set_language': '设定语言', 'with_language': '附带语言',
    'current_language': '当前语言', 'builtin_file': '内置文件',
    'builtin_lang_files': '内置语言文件',
    'builtin_language_codes': '内置语言代码',
    'uses_cyrillic_script': '使用西里尔文字', 'has_builtin_language': '拥有所属语言',
    'parse_ui': '解析界面表',
    # 枚举变体
    'UntypedLet': '未标注类型', 'MagicNumber': '魔法数字',
    'DeepIndent': '嵌套过深', 'ConfusableMethod': '易混方法名',
    'StdLib': '标准库表项', 'ThirdParty': '三方库表项',
    # 字段（长专名可全词）
    'source_offset': '源偏移', 'source_map': '源映射',
    'pipeline_map': '管线映射', 'final_edits': '最终编辑表',
    'module_path_map': '模块路径映射表', 'alias_map': '别名映射表',
    'keyword_map': '关键词映射表', 'library_fns': '库方法名集',
}

# 词法.zh 内部：transpile_source 是词法自己的函数（lib 根的同名入口不在此文件）
FILE_WORD = {
    '词法.zh': {'transpile_source': '词法转译'},
}

# ── 3. 字段/短方法模式（点前缀与冒号定义，顺序执行） ────────────────────
PATTERNS = [
    # 模块声明（.zh 中文关键字位与 .rs 英文位）
    (r'(?<![A-Za-z0-9_])(?:模块|mod) (alias|cache|diagnostic|lexer|lint|mapping_manager|mapping_source|module_path)(?![A-Za-z0-9_])',
     lambda m: m.group(1).replace(m.group(1), '模块 ' if m.group(0).startswith('模块') else 'mod ') + MOD_PATH[m.group(1)]),
    (r'(?<![A-Za-z0-9_])(alias|cache|diagnostic|lexer|lint|mapping_manager|mapping_source|module_path)::',
     lambda m: MOD_PATH[m.group(1)] + '::'),
    # 构造器按接收类型区分
    (r'TranslationCache::new\b', '转译缓存::新建缓存'),
    (r'TranspileOutput::new\b', '转译产出::新建产出'),
    (r'SourceMapEntry::new\b', '源映射条目::新建条目'),
    (r'MappingLoader::new\b', '映射加载器::新建加载器'),
    # 短字段名：访问位与定义位
    (r'\.output(?![A-Za-z0-9_])', '.产出'),
    (r'(?<![A-Za-z0-9_])output(?=[:=])', '产出'),
    (r'\.edits(?![A-Za-z0-9_])', '.编辑表'),
    (r'(?<![A-Za-z0-9_])edits(?=[:=])', '编辑表'),
    (r'\.kind(?![A-Za-z0-9_])', '.种类'),
    (r'(?<![A-Za-z0-9_])kind(?=[:=])', '种类'),
    (r'\.line(?![A-Za-z0-9_])', '.行号'),
    (r'(?<![A-Za-z0-9_])line(?=[:=])', '行号'),
    (r'\.column(?![A-Za-z0-9_])', '.列号'),
    (r'(?<![A-Za-z0-9_])column(?=[:=])', '列号'),
    (r'\.text(?![A-Za-z0-9_])', '.相关文本'),
    (r'(?<![A-Za-z0-9_])text(?=[:=])', '相关文本'),
    (r'\.extra(?![A-Za-z0-9_])', '.补充文本'),
    (r'(?<![A-Za-z0-9_])extra(?=[:=])', '补充文本'),
    (r'\.items(?![A-Za-z0-9_])', '.项名集'),
    (r'(?<![A-Za-z0-9_])items(?=[:=])', '项名集'),
    (r'\.names(?![A-Za-z0-9_])', '.声明名'),
    (r'(?<![A-Za-z0-9_])names(?=[:=])', '声明名'),
    (r'\.modules(?![A-Za-z0-9_])', '.模块名集'),
    (r'(?<![A-Za-z0-9_])modules(?=[:=])', '模块名集'),
    (r'\.variables(?![A-Za-z0-9_])', '.变量名集'),
    (r'(?<![A-Za-z0-9_])variables(?=[:=])', '变量名集'),
    (r'\.members(?![A-Za-z0-9_])', '.成员名集'),
    (r'(?<![A-Za-z0-9_])members(?=[:=])', '成员名集'),
    (r'\.original(?![A-Za-z0-9_])', '.原文'),
    (r'(?<![A-Za-z0-9_])original(?=[:=])', '原文'),
    (r'\.replacement(?![A-Za-z0-9_])', '.替换文本'),
    (r'(?<![A-Za-z0-9_])replacement(?=[:=])', '替换文本'),
    (r'\.length(?![A-Za-z0-9_])', '.字节长度'),
    (r'(?<![A-Za-z0-9_])length(?=[:=])', '字节长度'),
    # 短方法名（限定调用位；std 同名调用不在此模式内）
    (r'(?<![A-Za-z0-9_.])query\(', '检索('),
    (r'\.query\(', '.检索('),
    (r'(?<![A-Za-z0-9_.])save\(', '存盘('),
    (r'(?<![A-Za-z0-9_.])flush\(', '刷盘('),
    (r'\.flush\(', '.刷盘('),
    (r'(?<![A-Za-z0-9_.])clear\(', '清空记录('),
    (r'(?<![A-Za-z0-9_.])persistent\(', '启用持久化('),
    (r'::persistent\(', '::启用持久化('),
    (r'(?<![A-Za-z0-9_.])format\(', '格式化提示('),
    (r'\.format\(', '.格式化提示('),
    (r'(?<![A-Za-z0-9_.])enter\(', '进入守卫('),
]

# 二轮中文对齐模式（在英文全词替换之后跑）：
# 调用方以词条别名 `::新建(` 写构造（eject 成 new），改名后对齐专名
PATTERNS_CN = [
    (r'源映射条目::新建\(', '源映射条目::新建条目('),
    (r'转译产出::新建\(', '转译产出::新建产出('),
    (r'转译缓存::新建\(', '转译缓存::新建缓存('),
    (r'映射加载器::新建\(', '映射加载器::新建加载器('),
    (r'语言::f\(', '语言::查译('),
    (r'语言::t\(', '语言::查句('),
]

# ── 词表加载与劫持校验 ────────────────────────────────────────────────
def load_wordmap():
    d = ROOT / 'crates/engine/lang-packs/zh'
    zh2en = {}
    def walk(x):
        for k, v in x.items():
            if isinstance(v, str): zh2en.setdefault(k, v)
            elif isinstance(v, dict): walk(v)
    for f in ['keywords.toml', 'stdlib.toml', 'module_paths.toml']:
        walk(tomllib.loads((d / f).read_text()))
    for f in (d / 'crates').glob('*.toml'):
        walk(tomllib.loads(f.read_text()))
    return zh2en

def all_targets():
    ts = set(GLOBAL_WORD.values()) | set(MOD_PATH.values())
    for v in FILE_WORD.values(): ts |= set(v.values())
    for _, r in PATTERNS:
        if isinstance(r, str) and '::' in r: ts.add(r.split('::')[-1])
    ts |= {'新建缓存', '新建产出', '新建条目', '新建加载器', '检索', '写入缓存',
           '存盘', '刷盘', '清空记录', '启用持久化', '加载类别', '格式化提示',
           '进入守卫', '产出', '编辑表', '种类', '行号', '列号',
           '相关文本', '补充文本', '项名集', '声明名', '模块名集', '变量名集',
           '成员名集', '原文', '替换文本', '字节长度', '词法转译'}
    return {t for t in ts if re.search(r'[\u4e00-\u9fff]', t)}

def verify():
    zh2en = load_wordmap()
    bad = sorted(t for t in all_targets() if t in zh2en)
    if bad:
        print('!!! 劫持冲突：', *[f'  {t} -> {zh2en[t]}' for t in bad], sep='\n')
        sys.exit(1)
    print(f'OK：{len(all_targets())} 个中文目标名均非词表整词词条')

# ── 字符串字面量感知的分段替换 ────────────────────────────────────────
STR_SPAN = re.compile(
    r'r\#*"[^"]*"\#*|b?"(?:[^"\\\n]|\\.)*"'
    r"|'(?:[^'\\\n]|\\[ntbr'\\]|\\x[0-9a-fA-F]{2}|\\u\{[0-9a-fA-F]+\})'"
    r"|'[_A-Za-z][A-Za-z0-9_]*")

def apply_seg(seg, word_map):
    for pat, rep in PATTERNS:
        if callable(rep):
            seg = re.sub(pat, rep, seg)
        elif rep is not None:
            seg = re.sub(pat, rep, seg)
    for k, v in sorted(word_map.items(), key=lambda x: -len(x[0])):
        seg = re.sub(r'(?<![A-Za-z0-9_])' + re.escape(k) + r'(?![A-Za-z0-9_])', v, seg)
    for k, v in sorted(GLOBAL_WORD.items(), key=lambda x: -len(x[0])):
        if k in word_map: continue
        seg = re.sub(r'(?<![A-Za-z0-9_])' + re.escape(k) + r'(?![A-Za-z0-9_])', v, seg)
    for pat, rep in PATTERNS_CN:
        seg = re.sub(pat, rep, seg)
    return seg

def transform(text, word_map):
    out, last = [], 0
    for m in STR_SPAN.finditer(text):
        out.append(apply_seg(text[last:m.start()], word_map))
        out.append(m.group(0))
        last = m.end()
    out.append(apply_seg(text[last:], word_map))
    return ''.join(out)

def target_word_map(path):
    return dict(FILE_WORD.get(pathlib.Path(path).name, {}))

def apply_paths(paths):
    n = 0
    for p in paths:
        f = pathlib.Path(p)
        src = f.read_text()
        dst = transform(src, target_word_map(p))
        if dst != src:
            f.write_text(dst); n += 1
    print(f'替换落盘 {n} 个文件')

if __name__ == '__main__':
    mode = sys.argv[1]
    if mode == 'verify': verify()
    elif mode == 'zh':
        apply_paths(sorted(str(p) for p in (ROOT / 'crates/engine/src').glob('*.zh')))
    elif mode == 'rs': apply_paths(sys.argv[2:])
    else: sys.exit('用法: verify | zh | rs <files...>')
