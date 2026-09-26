#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""飞书知识库发布工具：把 tutorials/ 中文教程批量导入飞书知识库

「用母语学习和编写真实的 Rust」——教程需要一个国内可稳定访问的公开阅读
入口。飞书知识库：免费、可「发布到互联网」供访客免登录阅读、国内访问稳定
（语雀公网可见需会员；Gitee Pages 已停服下线）。

发布流水线（每篇，全部走飞书开放平台官方 API，无需第三方依赖）：
  ① 上传素材       md 以 ccm_import_open 通道上传（临时文件，导入后自动删除）
  ② 创建导入任务   md → docx 新版文档（挂到云空间中转文件夹）
  ③ 轮询导入结果   获得文档 token / url
  ④ 移入知识库     move_docs_to_wiki → 知识库节点

飞书侧一次性配置（约 15 分钟，详见 docs/strategy/发布准备清单.md「飞书知识库」节）：
  1. open.feishu.cn/app → 创建企业自建应用，记录 App ID / App Secret；
  2. 应用「添加应用能力」→ 机器人；
  3. 「权限管理」开通：drive:drive、docs:document:import、wiki:wiki，
     创建版本并发布（个人版自己审批，立即生效）；
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
  python3 tools/publish-feishu.py --dry-run       # 预览清单（不发请求）
  python3 tools/publish-feishu.py                 # 幂等发布（同名跳过）
  python3 tools/publish-feishu.py --only 第一章    # 只发单篇（子串匹配）
  python3 tools/publish-feishu.py --force         # 全部重导（会新建文档）

发布完成后：知识库 → 分享 → 打开「将知识库发布到互联网」，访客即可免登录
阅读；教程更新后重跑本脚本（同名文档默认跳过，先在飞书删除旧文档再重跑，
或用 --only 定向重导）。

安全：App Secret 仅从命令行/环境变量读取，不落盘、不写入任何文件。
"""
import argparse
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

# ---------- 常量 ----------
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
API = "https://open.feishu.cn/open-apis"

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

    def post(self, path, payload):
        return self._request(
            "POST", path, json.dumps(payload, ensure_ascii=False).encode("utf-8")
        )

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


def list_existing_titles(client, space_id):
    """列出现有知识库一级节点标题（供幂等跳过），返回 {标题: node_token}。"""
    titles = {}
    page_token = ""
    while True:
        query = {"page_size": 50}
        if page_token:
            query["page_token"] = page_token
        data = client.get(f"/wiki/v2/spaces/{space_id}/nodes", query)
        for item in data.get("items") or []:
            title = (item.get("title") or "").strip()
            if title:
                titles[title] = item.get("node_token", "")
        if not data.get("has_more") or not data.get("page_token"):
            break
        page_token = data["page_token"]
    return titles


def main():
    parser = argparse.ArgumentParser(
        description="把 tutorials/ 中文教程批量导入飞书知识库（幂等，可重复执行）",
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
    parser.add_argument("--sleep", type=float, default=0.7,
                        help="请求间隔秒数（默认 0.7，限频 100 次/分钟）")
    parser.add_argument("--only", default=None, help="仅处理标题/文件名含该子串的章节")
    parser.add_argument("--force", action="store_true",
                        help="不查重全部重导（会新建文档，需自行清理旧文档）")
    parser.add_argument("--keep-h1", action="store_true",
                        help="保留正文首行 H1（默认移除，标题由知识库节点承担）")
    parser.add_argument("--dry-run", action="store_true", help="仅打印清单，不发请求")
    args = parser.parse_args()

    plan = load_chapter_plan(args.summary, args.dir)
    if args.only:
        plan = [e for e in plan if args.only in e[0] or args.only in e[2]]
        if not plan:
            sys.exit(f"没有匹配「{args.only}」的章节")
    print(f"待发布章节：{len(plan)} 篇（顺序与文档站 SUMMARY 一致）")
    for i, (title, _path, _name) in enumerate(plan, 1):
        print(f"  {i:>2}. {title}")
    if args.dry_run:
        print("\n（--dry-run：未发送任何请求；去掉该参数即执行发布）")
        return

    for opt, value, env in (
        ("--app-id", args.app_id, "FEISHU_APP_ID"),
        ("--app-secret", args.app_secret, "FEISHU_APP_SECRET"),
        ("--wiki-token", args.wiki_token, "FEISHU_WIKI_TOKEN"),
        ("--folder-token", args.folder_token, "FEISHU_FOLDER_TOKEN"),
    ):
        if not value:
            sys.exit(f"缺少 {opt}（或环境变量 {env}）")

    client = FeishuClient(args.app_id, args.app_secret, sleep=args.sleep)
    space_id = resolve_space_id(client, args.wiki_token)
    print(f"\n知识库 space_id：{space_id}")
    existing = {} if args.force else list_existing_titles(client, space_id)
    if existing:
        print(f"知识库现有 {len(existing)} 个节点（同名文档将跳过）\n")

    created, skipped, failed = 0, 0, []
    for i, (title, path, name) in enumerate(plan, 1):
        prefix = f"[{i:>2}/{len(plan)}] {title}"
        if title in existing:
            print(f"{prefix}  已存在，跳过")
            skipped += 1
            continue
        url = ""
        try:
            file_token = upload_media(client, path, keep_h1=args.keep_h1)
            ticket = create_import_task(client, file_token, title, args.folder_token)
            result = wait_import_task(client, ticket)
            url = result["url"]
            move_to_wiki(client, space_id, result["token"])
            print(f"{prefix}  上传 → 导入 → 移入知识库 ✓")
            created += 1
        except FeishuError as e:
            print(f"{prefix}  ✗ {e}")
            if url:
                print(f"          （文档已导入到中转文件夹：{url}，可手动移入知识库）")
            failed.append(title)

    print(f"\n完成：新导入 {created} 篇，跳过 {skipped} 篇，失败 {len(failed)} 篇")
    if failed:
        print("失败清单：")
        for t in failed:
            print(f"  - {t}")
    print("下一步：知识库 → 分享 → 打开「将知识库发布到互联网」，访客即可免登录阅读。")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
