#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""飞书知识库发布工具：把 tutorials/ 中文教程批量导入飞书知识库

「用母语学习和编写真实的 Rust」——教程需要一个国内可稳定访问的公开阅读
入口。飞书知识库：免费、可「发布到互联网」把链接分享到组织外、国内访问稳定
（语雀公网可见需会员；Gitee Pages 已停服下线）。注意「互联网获得链接的人
可阅读」不等于免登录：2026-10-29 复核时无 cookie 请求走的是飞书账号引导链、
只能拿到 SPA 空壳，是否免登录仍需无痕窗口人工确认（README 里的「免登录」
说法沿用 9-26 结论）；若确认要登录，改走「发布为网站」（点火，支持自定义域名）。

发布流水线（每篇，全部走飞书开放平台官方 API，无需第三方依赖）：
  ① 上传素材       md 以 ccm_import_open 通道上传（临时文件，导入后自动删除）
  ② 创建导入任务   md → docx 新版文档（挂到云空间中转文件夹）
  ③ 轮询导入结果   获得文档 token / url
  ④ 移入知识库     move_docs_to_wiki → 知识库节点（新章节）
     或原地替换     读中转文档块树 → 清空旧文档正文 → 「创建嵌套块」按树重建
                    （更新章节：节点与访问链接不变，需 docx:document 权限）

同步状态（docs/strategy/feishu-publish-state.json，随仓库提交）：每篇记录
title / sha256（正文指纹）/ node_token / doc_token / url / published_at /
source_commit。重跑时按指纹区分「未变更 / 已变更 / 未登记 / 未发布」，
`make feishu` 因此只会补新章节，已变更章节须用 --update 显式更新。

飞书侧一次性配置（约 15 分钟，详见 docs/strategy/发布准备清单.md「飞书知识库」节）：
  1. open.feishu.cn/app → 创建企业自建应用，记录 App ID / App Secret；
  2. 应用「添加应用能力」→ 机器人；
  3. 「权限管理」开通：drive:drive、docs:document:import、wiki:wiki；
     需要 --update 原地更新时再加 docx:document；创建版本并发布
     （个人版自己审批，立即生效）；
  4. 飞书建一个群（可只有自己）→ 群设置 → 群机器人 → 添加该应用；
     知识库 → 设置 → 成员设置 → 添加该群为「可编辑的成员」
     （应用由此获得知识库权限）；
  5. 云空间新建中转文件夹「rzc 飞书导入」→ 文件夹右上角「…」→「更多」→
     「添加文档应用」→ 选该应用（编辑权限）；从文件夹 URL 取 folder_token
     （https://xxx.feishu.cn/drive/folder/<folder_token>）；
  6. 知识库链接 https://xxx.feishu.cn/wiki/<wiki_token> 末段即 --wiki-token
     （脚本会自动换取 space_id）。

用法：
  export FEISHU_APP_ID=cli_xxx FEISHU_APP_SECRET=xxx
  export FEISHU_FOLDER_TOKEN=fldcnxxx
  export FEISHU_WIKI_TOKEN=LubgwEuXYi9yqJkUANlckY5Gnne
  python3 tools/publish-feishu.py --check        # 离线比对指纹，列出落后章节
  python3 tools/publish-feishu.py --adopt        # 认领线上已有章节（首次接入状态文件）
  python3 tools/publish-feishu.py                # 补新章节（已变更章节只提示）
  python3 tools/publish-feishu.py --update       # 原地更新已变更章节（链接不变）
  python3 tools/publish-feishu.py --only 第一章   # 只处理匹配章节（子串匹配）
  python3 tools/publish-feishu.py --force        # 忽略状态全部重导（会新建文档）
  python3 tools/publish-feishu.py --restore-backup <备份.json> --only 第十八章
                                                # 按备份把正文恢复原状（补救通道）

发布完成后：知识库 → 分享 → 打开「将知识库发布到互联网」，链接即可分享给
组织外；免登录能否成立要拿无痕窗口实测（见上）。日常改动流程：改完教程 →
`--check` 看落后清单 → `--update` 同步。

安全：App Secret 仅从命令行/环境变量读取，不落盘、不写入任何文件。
"""
import argparse
import copy
import hashlib
import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

# ---------- 常量 ----------
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
API = "https://open.feishu.cn/open-apis"
# 同步状态文件（随仓库提交，供 `--check` 与实际发布命令共用）
DEFAULT_STATE = os.path.join(ROOT, "docs", "strategy", "feishu-publish-state.json")
STATE_VERSION = 1
# 飞书「删除块」单次批量：batch_delete 按索引区间删，取 50 稳妥
BLOCK_BATCH = 50
# 「创建嵌套块」单次请求的块数上限官方未公开，按顶层块切批，保守取 300
DESCENDANT_BATCH = 300

# 限频错误码：HTTP 429 或 body 内错误码（退回重试）
RATE_LIMIT_CODES = {1069923, 99991400}
# 常见错误的排查提示（附加在异常消息后）
HINTS = {
    "99991663": "App ID / App Secret 可能不正确",
    "99991672": "应用缺少 API 权限：开发者后台「权限管理」开通 drive:drive、"
                "docs:document:import、wiki:wiki 后创建新版本并发布",
    "1069902": "应用对该文档无权限：确认知识库与中转文件夹均已授权给应用",
    "1069908": "中转文件夹未授权给应用：文件夹「… → 更多 → 添加文档应用」",
    "131006": "知识库权限不足：确认含机器人的群已被添加为知识库可编辑成员",
    "131005": "未找到资源：检查 wiki-token 是否正确、应用是否为知识库成员",
    "1770001": "建块参数不合法：多半是 payload 里带了只读字段（表格 cells/merge_info 等）",
    "1770004": "文档块数超上限：该章过长，需拆分或改用导入重导",
    "1770005": "文档块层级过深：表格/列表嵌套超出上限",
    "1770032": "应用无文档编辑权限：后台「权限管理」开通 docx:document 后创建新版本并发布",
}
# SUMMARY.md 条目：`- [标题](目标)` 或 `- [标题](<目标>)`（书名号包裹）
SUMMARY_LINK_RE = re.compile(r"^- \[(?P<title>.+?)\]\((?:<(?P<angle>[^>]+)>|(?P<plain>[^)]+))\)\s*$")


class FeishuError(Exception):
    """飞书 API 错误（附排查提示）。"""

    def __init__(self, message, code=None):
        if code is not None and str(code) in HINTS:
            message = f"{message}（排查：{HINTS[str(code)]}）"
        super().__init__(message)
        self.code = code


def load_chapter_plan(summary_path, tut_dir):
    """解析发布清单：按 SUMMARY.md 顺序返回 [(标题, 绝对路径, 文件名)]。

    SUMMARY 中不存在于教程目录的条目（首页 index.md、参考/ 文档）自动跳过；
    tutorials/ 下未列入 SUMMARY 的 .md 文件按文件名兜底追加到末尾。
    """
    entries = []
    seen = set()
    if os.path.exists(summary_path):
        with open(summary_path, encoding="utf-8") as f:
            for line in f:
                m = SUMMARY_LINK_RE.match(line.strip())
                if not m:
                    continue
                title = m.group("title")
                target = m.group("angle") or m.group("plain")
                if target == "index.md" or "/" in target:
                    continue
                path = os.path.join(tut_dir, target)
                if os.path.exists(path):
                    entries.append((title, path, target))
                    seen.add(target)
    for name in sorted(os.listdir(tut_dir)):
        if name.endswith(".md") and name != "README.md" and name not in seen:
            entries.append((name[:-3], os.path.join(tut_dir, name), name))
    return entries


def strip_first_h1(text):
    """去掉正文首行 H1 与其后的一个空行（文档标题已由知识库节点承担）。"""
    lines = text.splitlines(keepends=True)
    if lines and lines[0].startswith("# "):
        lines = lines[1:]
        if lines and not lines[0].strip():
            lines = lines[1:]
    return "".join(lines)


def chapter_body(path, keep_h1=False):
    """读章节正文（默认剥掉首行 H1）——与上传给飞书的内容逐字一致。"""
    with open(path, encoding="utf-8") as f:
        text = f.read()
    return text if keep_h1 else strip_first_h1(text)


def chapter_digest(path, keep_h1=False):
    """正文指纹：内容变则指纹变，用来判定飞书侧是否需要重发。"""
    blob = chapter_body(path, keep_h1).encode("utf-8")
    return hashlib.sha256(blob).hexdigest()


def git_commit():
    """当前 HEAD 短 sha（写进状态文件，便于追溯发布时的源码版本）。"""
    try:
        done = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=ROOT, capture_output=True, text=True, check=True,
        )
        return done.stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return ""


def load_state(path):
    """读同步状态；文件不存在或格式不对时返回空状态。"""
    if not os.path.exists(path):
        return {"version": STATE_VERSION, "space_id": "", "chapters": {}}
    try:
        with open(path, encoding="utf-8") as f:
            state = json.load(f)
    except (OSError, ValueError) as e:
        sys.exit(f"状态文件 {path} 无法解析：{e}")
    if not isinstance(state, dict) or not isinstance(state.get("chapters"), dict):
        sys.exit(f"状态文件 {path} 结构异常：缺 chapters 对象")
    state.setdefault("version", STATE_VERSION)
    state.setdefault("space_id", "")
    return state


def save_state(path, state):
    """写同步状态（末行换行 + 键排序，便于 git diff 稳定）。"""
    directory = os.path.dirname(path)
    if directory:
        os.makedirs(directory, exist_ok=True)
    # 原地排序而不是重新绑定：调用方可能持有 state["chapters"] 的引用
    chapters = state["chapters"]
    ordered = {k: chapters[k] for k in sorted(chapters)}
    chapters.clear()
    chapters.update(ordered)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(state, f, ensure_ascii=False, indent=2)
        f.write("\n")


class FeishuClient:
    """飞书开放平台最小客户端：tenant_access_token + 节流 + 重试。"""

    def __init__(self, app_id, app_secret, sleep=0.7):
        self.app_id = app_id
        self.app_secret = app_secret
        self.sleep = sleep
        self._token = None
        self._last = 0.0

    def _pace(self):
        wait = self.sleep - (time.monotonic() - self._last)
        if wait > 0:
            time.sleep(wait)
        self._last = time.monotonic()

    def _request(self, method, path, body=None, *, form=False, retries=4):
        """发送请求并解析响应；限频/网络抖动自动退回重试。"""
        url = API + path
        content_type = form or "application/json; charset=utf-8"
        for attempt in range(retries):
            self._pace()
            headers = {
                "Authorization": "Bearer " + self.tenant_token(),
                "Content-Type": content_type,
            }
            req = urllib.request.Request(url, data=body, method=method, headers=headers)
            try:
                with urllib.request.urlopen(req, timeout=120) as resp:
                    parsed = json.loads(resp.read().decode("utf-8"))
            except urllib.error.HTTPError as e:
                payload = e.read().decode("utf-8", "replace")
                if e.code == 429 and attempt + 1 < retries:
                    time.sleep(2 ** attempt)
                    continue
                raise FeishuError(f"HTTP {e.code} {path}：{payload[:300]}") from e
            except (urllib.error.URLError, OSError) as e:
                if attempt + 1 < retries:
                    time.sleep(2 ** attempt)
                    continue
                raise FeishuError(f"网络异常 {path}：{e}") from e
            code = parsed.get("code", 0)
            if code == 0:
                return parsed.get("data") or {}
            if code in RATE_LIMIT_CODES and attempt + 1 < retries:
                time.sleep(2 ** attempt)
                continue
            raise FeishuError(
                f"飞书错误码 {code}（{path}）：{parsed.get('msg', '')}", code=code
            )
        raise FeishuError(f"请求重试超限：{path}")

    def tenant_token(self):
        """获取（并缓存）tenant_access_token。"""
        if self._token is None:
            self._pace()
            body = json.dumps(
                {"app_id": self.app_id, "app_secret": self.app_secret}
            ).encode("utf-8")
            req = urllib.request.Request(
                API + "/auth/v3/tenant_access_token/internal",
                data=body,
                method="POST",
                headers={"Content-Type": "application/json; charset=utf-8"},
            )
            try:
                with urllib.request.urlopen(req, timeout=60) as resp:
                    parsed = json.loads(resp.read().decode("utf-8"))
            except (urllib.error.HTTPError, urllib.error.URLError, OSError) as e:
                raise FeishuError(f"获取 tenant_access_token 失败：{e}") from e
            if parsed.get("code", 0) != 0:
                raise FeishuError(
                    f"获取 tenant_access_token 失败：{parsed.get('msg', '')}",
                    code=parsed.get("code"),
                )
            self._token = parsed["tenant_access_token"]
            self._last = time.monotonic()
        return self._token

    def get(self, path, query=None):
        if query:
            path += "?" + urllib.parse.urlencode(query)
        return self._request("GET", path)

    def post(self, path, payload, query=None):
        if query:
            path += "?" + urllib.parse.urlencode(query)
        return self._request(
            "POST", path, json.dumps(payload, ensure_ascii=False).encode("utf-8")
        )

    def delete(self, path, payload=None, query=None):
        """DELETE（可带 JSON body）：块删除、云空间文件删除都走这里。"""
        if query:
            path += "?" + urllib.parse.urlencode(query)
        body = (
            json.dumps(payload, ensure_ascii=False).encode("utf-8")
            if payload is not None
            else None
        )
        return self._request("DELETE", path, body)

    def post_multipart(self, path, fields, file_name, blob):
        """multipart/form-data 上传（手工构造，无第三方依赖）。"""
        boundary = "----rzc-import-" + uuid.uuid4().hex
        parts = []
        for key, value in fields.items():
            parts.append(
                (
                    f"--{boundary}\r\n"
                    f'Content-Disposition: form-data; name="{key}"\r\n\r\n'
                    f"{value}\r\n"
                ).encode("utf-8")
            )
        parts.append(
            (
                f"--{boundary}\r\n"
                f'Content-Disposition: form-data; name="file"; filename="{file_name}"\r\n'
                f"Content-Type: application/octet-stream\r\n\r\n"
            ).encode("utf-8")
        )
        parts.append(blob)
        parts.append(f"\r\n--{boundary}--\r\n".encode("utf-8"))
        body = b"".join(parts)
        return self._request(
            "POST", path, body, form=f"multipart/form-data; boundary={boundary}"
        )


# ---------- 发布流水线四步 ----------
def upload_media(client, path, keep_h1=False):
    """① 上传素材：md 经 ccm_import_open 临时通道上传，返回 file_token。"""
    with open(path, encoding="utf-8") as f:
        text = f.read()
    if not keep_h1:
        text = strip_first_h1(text)
    blob = text.encode("utf-8")
    if len(blob) > 20 * 1024 * 1024:
        raise FeishuError("文件超过飞书导入 20MB 限制")
    fields = {
        "file_name": os.path.basename(path),
        "parent_type": "ccm_import_open",
        "size": str(len(blob)),
        "extra": json.dumps(
            {"obj_type": "docx", "file_extension": "md"}, ensure_ascii=False
        ),
    }
    data = client.post_multipart(
        "/drive/v1/medias/upload_all", fields, os.path.basename(path), blob
    )
    return data["file_token"]


def create_import_task(client, file_token, title, folder_token):
    """② 创建导入任务：md → docx，挂到中转文件夹，返回 ticket。"""
    data = client.post(
        "/drive/v1/import_tasks",
        {
            "file_extension": "md",
            "file_token": file_token,
            "type": "docx",
            "file_name": title,
            "point": {"mount_type": 1, "mount_key": folder_token},
        },
    )
    return data["ticket"]


def wait_import_task(client, ticket):
    """③ 轮询导入结果，返回 {"token": 文档token, "url": 文档链接}。"""
    for _ in range(45):  # 2s 间隔，上限 ~90s
        data = client.get(f"/drive/v1/import_tasks/{ticket}")
        result = data.get("result") or {}
        status = result.get("job_status")
        if status == 0:
            token = result.get("token")
            if not token:
                raise FeishuError("导入成功但未返回文档 token")
            return {"token": token, "url": result.get("url", "")}
        if result.get("job_error_msg") not in (None, "", "success"):
            raise FeishuError(f"导入失败：{result.get('job_error_msg')}")
        time.sleep(2)
    raise FeishuError("导入任务超时（>90s）")


def move_to_wiki(client, space_id, obj_token):
    """④ 移入知识库：返回知识库节点 token（异步时轮询任务）。"""
    data = client.post(
        f"/wiki/v2/spaces/{space_id}/nodes/move_docs_to_wiki",
        {"obj_type": "docx", "obj_token": obj_token},
    )
    if data.get("wiki_token"):
        return data["wiki_token"]
    task_id = data.get("task_id")
    if not task_id:
        raise FeishuError("移动接口未返回 wiki_token 或 task_id")
    for _ in range(30):  # 2s 间隔，上限 ~60s
        data = client.get(f"/wiki/v2/tasks/{task_id}", {"task_type": "move"})
        task = data.get("task") or {}
        results = task.get("move_result") or []
        if results:
            first = results[0]
            node = first.get("node") or {}
            if first.get("status") in (0, None) and node.get("node_token"):
                return node["node_token"]
            if first.get("status_msg"):
                raise FeishuError(f"移入知识库失败：{first.get('status_msg')}")
        time.sleep(2)
    raise FeishuError("移入知识库任务超时（>60s）")


def resolve_space_id(client, wiki_token):
    """用知识库链接末段 token 换取 space_id。"""
    data = client.get("/wiki/v2/spaces/get_node", {"token": wiki_token})
    node = data.get("node") or {}
    space_id = node.get("space_id")
    if not space_id:
        raise FeishuError("无法从 wiki-token 解析 space_id")
    return space_id


def list_existing_nodes(client, space_id):
    """列出现有知识库一级节点：{标题: {node_token, obj_token, obj_type}}。"""
    nodes = {}
    page_token = ""
    while True:
        query = {"page_size": 50}
        if page_token:
            query["page_token"] = page_token
        data = client.get(f"/wiki/v2/spaces/{space_id}/nodes", query)
        for item in data.get("items") or []:
            title = (item.get("title") or "").strip()
            if title:
                nodes[title] = {
                    "node_token": item.get("node_token", ""),
                    "obj_token": item.get("obj_token", ""),
                    "obj_type": item.get("obj_type", ""),
                }
        if not data.get("has_more") or not data.get("page_token"):
            break
        page_token = data["page_token"]
    return nodes


# ---------- 块树操作（原地更新正文）----------
# 新建块时不能带这些只读字段：block_id/parent_id 会被当成跨文档引用而报错
READONLY_BLOCK_FIELDS = ("block_id", "parent_id", "children", "comment_ids")
# payload 内还有只读子字段：表格的 cells 是单元格 id 列表（单元格改由 children 表达）、
# merge_info 由系统按行列自动生成、grid 的 columns 同理。回传它们建块会报 1770001。
READONLY_PAYLOAD_PATHS = (
    ("table", "cells"),
    ("table", "property", "merge_info"),
    ("grid", "columns"),
)


def list_blocks(client, doc_token):
    """分页取文档全部块（含各级嵌套块，按文档顺序）。"""
    items, page_token = [], ""
    while True:
        query = {"page_size": 500, "document_revision_id": -1}
        if page_token:
            query["page_token"] = page_token
        data = client.get(f"/docx/v1/documents/{doc_token}/blocks", query)
        items.extend(data.get("items") or [])
        page_token = data.get("page_token") or ""
        if not data.get("has_more") or not page_token:
            break
    return items


def block_payload(block):
    """剥掉只读字段，得到可回传给建块接口的 payload（子块改由 children 表达）。"""
    payload = copy.deepcopy(
        {k: v for k, v in block.items() if k not in READONLY_BLOCK_FIELDS}
    )
    for path in READONLY_PAYLOAD_PATHS:
        node = payload
        for key in path[:-1]:
            node = node.get(key) if isinstance(node, dict) else None
        if isinstance(node, dict):
            node.pop(path[-1], None)
    return payload


def build_block_tree(blocks, root_id):
    """把扁平块列表还原成 [(payload, 子节点列表)]，顺序沿用 children。"""
    by_id = {b.get("block_id"): b for b in blocks}
    root = by_id.get(root_id)
    if root is None:
        raise FeishuError(f"文档 {root_id} 的块列表里找不到根块")
    seen = {root_id}

    def kids(node):
        out = []
        for cid in node.get("children") or []:
            child = by_id.get(cid)
            if child is None or cid in seen:
                continue
            seen.add(cid)
            out.append((block_payload(child), kids(child)))
        return out

    return kids(root)


def tree_size(node):
    """子树块数（含自身）。"""
    _payload, kids = node
    return 1 + sum(tree_size(kid) for kid in kids)


def chunk_block_tree(nodes, limit):
    """按顶层块切批：单棵子树不拆开，每批块数不超过 limit。"""
    batches, current, size = [], [], 0
    for node in nodes:
        need = tree_size(node)
        if current and size + need > limit:
            batches.append(current)
            current, size = [], 0
        current.append(node)
        size += need
    if current:
        batches.append(current)
    return batches


def flatten_block_tree(nodes):
    """把嵌套树摊平成「创建嵌套块」要的 (children_id, descendants)。

    临时 block_id 只需在单次请求内唯一，接口按 children 里的 id 还原子树；
    表格的单元格也走这条路（不再依赖只读的 table.cells）。
    """
    descendants = []
    counter = [0]

    def walk(node):
        payload, kids = node
        counter[0] += 1
        tmp_id = f"rzc{counter[0]:04d}"
        entry = {"block_id": tmp_id}
        entry.update(payload)
        entry["children"] = []
        descendants.append(entry)
        entry["children"] = [walk(kid) for kid in kids]
        return tmp_id

    children_id = [walk(node) for node in nodes]
    return children_id, descendants


def insert_block_tree(client, doc_token, parent_id, nodes):
    """用「创建嵌套块」接口按树重建，返回实际创建的块数。

    逐层 children 接口建不了表格（cells 只读，行列另有 9 的上限）；
    descendant 接口一次收下整棵子树，表格与嵌套列表都按原样复现。
    """
    created = 0
    for batch in chunk_block_tree(nodes, DESCENDANT_BATCH):
        children_id, descendants = flatten_block_tree(batch)
        client.post(
            f"/docx/v1/documents/{doc_token}/blocks/{parent_id}/descendant",
            {"children_id": children_id, "descendants": descendants},
            query={"document_revision_id": -1},
        )
        created += len(descendants)
    return created


def clear_document_body(client, doc_token):
    """清空文档正文（保留根块），返回被删的顶层块数。"""
    blocks = list_blocks(client, doc_token)
    root = next((b for b in blocks if b.get("block_id") == doc_token), None)
    if root is None:
        raise FeishuError(f"文档 {doc_token} 缺少根块，无法清空正文")
    total = remaining = len(root.get("children") or [])
    while remaining > 0:
        start = max(0, remaining - BLOCK_BATCH)
        client.delete(
            f"/docx/v1/documents/{doc_token}/blocks/{doc_token}/children/batch_delete",
            {"start_index": start, "end_index": remaining},
            query={"document_revision_id": -1},
        )
        remaining = start
    return total


def backup_blocks(doc_token, blocks):
    """重建前把旧正文存一份到 .rz/feishu-backup/（已 gitignore），供出错回填。"""
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    directory = os.path.join(ROOT, ".rz", "feishu-backup")
    os.makedirs(directory, exist_ok=True)
    path = os.path.join(directory, f"{doc_token}-{stamp}.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(blocks, f, ensure_ascii=False, indent=1)
        f.write("\n")
    return path


def replace_document_body(client, target_token, source_token):
    """用中转文档的块树整篇替换目标文档正文（节点与访问链接不变）。

    旧正文先落盘备份；重建中途失败时尝试把旧正文填回去，避免留下半空文档。
    """
    tree = build_block_tree(list_blocks(client, source_token), source_token)
    old_blocks = list_blocks(client, target_token)
    # 回填用的树在清空之前就构好：此后旧块表可能被删块请求影响
    old_tree = build_block_tree(old_blocks, target_token)
    backup = backup_blocks(target_token, old_blocks)
    removed = clear_document_body(client, target_token)
    try:
        insert_block_tree(client, target_token, target_token, tree)
    except FeishuError as e:
        rel = os.path.relpath(backup, ROOT)
        try:
            clear_document_body(client, target_token)
            insert_block_tree(client, target_token, target_token, old_tree)
            note = "已自动回填旧正文"
        except FeishuError as again:
            note = f"自动回填也失败（{again}）"
        raise FeishuError(
            f"正文重建中断（已删 {removed} 个顶层块，备份 {rel}，{note}）：{e}"
        ) from e
    return backup


def restore_from_backup(client, doc_token, backup_path):
    """按备份文件把正文恢复原状（重建失败后的人工补救通道）。"""
    with open(backup_path, encoding="utf-8") as f:
        blocks = json.load(f)
    tree = build_block_tree(blocks, doc_token)
    removed = clear_document_body(client, doc_token)
    created = insert_block_tree(client, doc_token, doc_token, tree)
    return removed, created, len(blocks)


def delete_drive_file(client, token):
    """删掉中转文件夹里的临时文档（失败只告警，不影响已完成的正文替换）。"""
    try:
        client.delete(f"/drive/v1/files/{token}", query={"type": "docx"})
        return True
    except FeishuError as e:
        print(f"          （中转文档 {token} 未删除：{e}）")
        return False


def plan_actions(plan, state, digests):
    """离线判定每篇动作。

    same：指纹与标题均未变；stale：已变更；unverified：已认领但未比对过正文
    （--update 重刷一次即建立基准）；unregistered：状态里无记录。
    """
    chapters = state.get("chapters", {})
    actions = []
    for title, path, name in plan:
        entry = chapters.get(name)
        if entry is None:
            kind = "unregistered"
        elif entry.get("sha256") != digests[name] or (entry.get("title") or "") != title:
            kind = "stale"
        elif entry.get("adopted"):
            kind = "unverified"
        else:
            kind = "same"
        actions.append((kind, title, path, name))
    return actions


def print_status(actions):
    """打印状态对比结果与需处理的章节清单。"""
    buckets = {"same": [], "stale": [], "unverified": [], "unregistered": []}
    for kind, title, _path, _name in actions:
        buckets[kind].append(title)
    print(
        f"\n状态比对：一致 {len(buckets['same'])} 篇 / 已变更 {len(buckets['stale'])} 篇"
        f" / 未验证 {len(buckets['unverified'])} 篇"
        f" / 未登记 {len(buckets['unregistered'])} 篇（共 {len(actions)} 篇）"
    )
    for label, key in (
        ("已变更（用 --update 原地更新）", "stale"),
        ("已认领但未验证（用 --update 重刷建立基准）", "unverified"),
        ("未登记（用 --adopt 认领线上章节）", "unregistered"),
    ):
        if buckets[key]:
            print(f"{label}：")
            for title in buckets[key][:10]:
                print(f"  - {title}")
            if len(buckets[key]) > 10:
                print(f"  …另 {len(buckets[key]) - 10} 篇")


def import_chapter(client, args, space_id, title, path):
    """新章节：上传 → 导入 → 移入知识库，返回节点/文档 token 与链接。"""
    file_token = upload_media(client, path, keep_h1=args.keep_h1)
    ticket = create_import_task(client, file_token, title, args.folder_token)
    result = wait_import_task(client, ticket)
    node_token = move_to_wiki(client, space_id, result["token"])
    return {
        "node_token": node_token,
        "doc_token": result["token"],
        "url": result["url"],
    }


def update_chapter(client, args, title, path, entry):
    """已变更章节：先导入到中转文件夹做模板，再把它的块树写回原文档。"""
    if not entry.get("doc_token"):
        raise FeishuError(
            "状态文件缺 doc_token，无法原地更新；请先在飞书删除该旧文档后重跑"
        )
    file_token = upload_media(client, path, keep_h1=args.keep_h1)
    ticket = create_import_task(client, file_token, title, args.folder_token)
    result = wait_import_task(client, ticket)
    try:
        return replace_document_body(client, entry["doc_token"], result["token"])
    finally:
        delete_drive_file(client, result["token"])


def run_restore(args, plan, state):
    """--restore-backup：按备份把一章正文恢复原状（不动状态文件）。"""
    if len(plan) != 1:
        sys.exit("--restore-backup 需配合 --only 指定唯一章节")
    _title, _path, name = plan[0]
    entry = state.get("chapters", {}).get(name)
    if not entry or not entry.get("doc_token"):
        sys.exit(f"状态文件里没有 {name} 的 doc_token，无法定位文档")
    for opt, value, env in (
        ("--app-id", args.app_id, "FEISHU_APP_ID"),
        ("--app-secret", args.app_secret, "FEISHU_APP_SECRET"),
    ):
        if not value:
            sys.exit(f"缺少 {opt}（或环境变量 {env}）")
    client = FeishuClient(args.app_id, args.app_secret, sleep=args.sleep)
    removed, created, total = restore_from_backup(
        client, entry["doc_token"], args.restore_backup
    )
    print(
        f"恢复完成：文档 {entry['doc_token']} 清空 {removed} 个顶层块，"
        f"按备份重建 {created}/{total} 块"
    )


def main():
    parser = argparse.ArgumentParser(
        description="把 tutorials/ 中文教程同步到飞书知识库（按正文指纹增量发布）",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__.split("用法：", 1)[-1],
    )
    parser.add_argument("--app-id", default=os.environ.get("FEISHU_APP_ID"))
    parser.add_argument("--app-secret", default=os.environ.get("FEISHU_APP_SECRET"))
    parser.add_argument("--wiki-token", default=os.environ.get("FEISHU_WIKI_TOKEN"),
                        help="知识库链接末段 token（自动换取 space_id）")
    parser.add_argument("--folder-token", default=os.environ.get("FEISHU_FOLDER_TOKEN"),
                        help="云空间中转文件夹 token（已授权给应用）")
    parser.add_argument("--dir", default=os.path.join(ROOT, "tutorials"),
                        help="教程目录（默认 tutorials/，中文）")
    parser.add_argument("--summary", default=os.path.join(ROOT, "book", "zh", "SUMMARY.md"),
                        help="章节顺序来源（默认 book/zh/SUMMARY.md）")
    parser.add_argument("--state", default=DEFAULT_STATE,
                        help="同步状态文件（默认 docs/strategy/feishu-publish-state.json）")
    parser.add_argument("--sleep", type=float, default=0.7,
                        help="请求间隔秒数（默认 0.7，限频 100 次/分钟）")
    parser.add_argument("--only", default=None, help="仅处理标题/文件名含该子串的章节")
    parser.add_argument("--check", action="store_true",
                        help="离线比对正文指纹，列出落后章节（不需要凭证）")
    parser.add_argument("--adopt", action="store_true",
                        help="把线上已有同名节点登记进状态文件（不重导、不覆盖）")
    parser.add_argument("--update", action="store_true",
                        help="原地更新已变更章节（节点与链接不变，需 docx:document）")
    parser.add_argument("--force", action="store_true",
                        help="忽略状态全部重导（会新建文档，需自行清理旧文档）")
    parser.add_argument("--restore-backup", default=None,
                        help="按备份 json 把某章正文恢复原状（配合 --only）")
    parser.add_argument("--keep-h1", action="store_true",
                        help="保留正文首行 H1（默认移除，标题由知识库节点承担）")
    parser.add_argument("--dry-run", action="store_true", help="仅打印清单，不发请求")
    args = parser.parse_args()

    plan = load_chapter_plan(args.summary, args.dir)
    if args.only:
        plan = [e for e in plan if args.only in e[0] or args.only in e[2]]
        if not plan:
            sys.exit(f"没有匹配「{args.only}」的章节")
    digests = {name: chapter_digest(path, args.keep_h1) for _t, path, name in plan}
    state = load_state(args.state)
    if args.restore_backup:
        run_restore(args, plan, state)
        return
    actions = plan_actions(plan, state, digests)

    print(f"待处理章节：{len(plan)} 篇（顺序与文档站 SUMMARY 一致）")
    for i, (title, _path, _name) in enumerate(plan, 1):
        print(f"  {i:>2}. {title}")
    print_status(actions)
    if args.check or args.dry_run:
        flag = "--check" if args.check else "--dry-run"
        print(f"\n（{flag}：未发送任何请求）")
        if any(kind != "same" for kind, *_ in actions):
            print("已变更章节用 --update 原地同步；未登记章节先 --adopt 认领。")
        return

    needed = [
        ("--app-id", args.app_id, "FEISHU_APP_ID"),
        ("--app-secret", args.app_secret, "FEISHU_APP_SECRET"),
        ("--wiki-token", args.wiki_token, "FEISHU_WIKI_TOKEN"),
    ]
    if not args.adopt:
        needed.append(("--folder-token", args.folder_token, "FEISHU_FOLDER_TOKEN"))
    for opt, value, env in needed:
        if not value:
            sys.exit(f"缺少 {opt}（或环境变量 {env}）")

    client = FeishuClient(args.app_id, args.app_secret, sleep=args.sleep)
    space_id = resolve_space_id(client, args.wiki_token)
    print(f"\n知识库 space_id：{space_id}")
    if state.get("space_id") and state["space_id"] != space_id:
        print(f"注意：状态文件记录的 space_id 是 {state['space_id']}，与当前知识库不同")
    state["space_id"] = space_id

    online = {} if args.force else list_existing_nodes(client, space_id)
    if online:
        print(f"知识库现有 {len(online)} 个节点")
    if args.force and online:
        print("注意：--force 不查重，已存在章节会再建一份同名文档")

    chapters = state["chapters"]
    commit = git_commit()
    stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    created, updated, adopted, unchanged = 0, 0, 0, 0
    pending, failed = [], []

    for i, (kind, title, path, name) in enumerate(actions, 1):
        prefix = f"[{i:>2}/{len(actions)}] {title}"
        entry = chapters.get(name)
        node = online.get(title)
        try:
            if args.force or (node is None and not args.adopt):
                info = import_chapter(client, args, space_id, title, path)
                chapters[name] = {
                    "title": title, "sha256": digests[name], **info,
                    "published_at": stamp, "source_commit": commit,
                }
                save_state(args.state, state)
                print(f"{prefix}  上传 → 导入 → 移入知识库 ✓")
                created += 1
            elif args.adopt and entry is None:
                if node is None:
                    print(f"{prefix}  尚未发布（去掉 --adopt 后再跑即导入）")
                    pending.append(title)
                else:
                    chapters[name] = {
                        "title": title, "sha256": digests[name],
                        "node_token": node["node_token"],
                        "doc_token": node["obj_token"],
                        "url": "", "published_at": "", "source_commit": "",
                        "adopted": True,
                    }
                    save_state(args.state, state)
                    print(f"{prefix}  已认领（按当前内容登记，未重写正文）")
                    adopted += 1
            elif kind == "stale" or kind == "unverified":
                if not args.update:
                    why = "已变更" if kind == "stale" else "已认领未验证"
                    print(f"{prefix}  {why}，跳过（--update 重刷）")
                    pending.append(title)
                else:
                    backup = update_chapter(client, args, title, path, entry)
                    chapters[name] = {
                        **entry, "title": title, "sha256": digests[name],
                        "published_at": stamp, "source_commit": commit,
                    }
                    chapters[name].pop("adopted", None)
                    save_state(args.state, state)
                    rel = os.path.relpath(backup, ROOT)
                    print(f"{prefix}  正文原地替换 ✓（旧文备份 {rel}）")
                    if (entry.get("title") or "") != title:
                        print(f"          标题已变，请在飞书节点上手动重命名为「{title}」")
                    updated += 1
            elif entry is None:
                print(f"{prefix}  未登记：先 --adopt 认领（否则无法判断版本）")
                pending.append(title)
            else:
                print(f"{prefix}  未变更，跳过")
                unchanged += 1
        except FeishuError as e:
            print(f"{prefix}  ✗ {e}")
            failed.append(title)

    print(
        f"\n完成：新建 {created} 篇，更新 {updated} 篇，认领 {adopted} 篇，"
        f"未变更 {unchanged} 篇，待处理 {len(pending)} 篇，失败 {len(failed)} 篇"
    )
    if pending:
        print("待处理清单（--adopt 认领 / --update 更新）：")
        for t in pending:
            print(f"  - {t}")
    if failed:
        print("失败清单：")
        for t in failed:
            print(f"  - {t}")
    print("下一步：知识库 → 分享 → 打开「将知识库发布到互联网」，链接即可分享到组织外；")
    print("免登录是否成立请用无痕窗口实测（自动请求只能拿到账号引导链）。")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
