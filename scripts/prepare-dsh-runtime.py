"""准备固定 Windows DSH 运行时；不运行 npm、包脚本或第三方入口。"""
import argparse
import base64
import concurrent.futures
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import stat
import tarfile
import tempfile
import urllib.request
import zipfile

REPO = Path(__file__).resolve().parent.parent
HOST = REPO / "modules/tooling/packages/dsh-plugin-host"
LOCK = REPO / "config/dsh-runtime-lock.json"
NODE_URL = "https://nodejs.org/download/release/v24.15.0/node-v24.15.0-win-x64.zip"
NODE_SHA256 = "cc5149eabd53779ce1e7bdc5401643622d0c7e6800ade18928a767e940bb0e62"
NODE_EXE_SHA256 = "3331e1ffe19874215472217c5e94f5a0c6d8e18c4ac7111d3937aa0ad5e9b4a5"
HOST_FILES = ("package.json", "package-lock.json", "src/host.mjs", "src/process.mjs", "src/source_imports.mjs")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise ValueError("固定运行时来源不能重定向")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_url(url, limit):
    # URL只由本脚本固定Node地址或已核验npm锁提供；不使用Cookie/用户认证。
    request = urllib.request.Request(url, headers={"User-Agent": "Coolzhu-fixed-runtime", "Accept-Encoding": "identity"})
    with urllib.request.build_opener(NoRedirect()).open(request, timeout=30) as response:
        if response.getheader("Content-Encoding", "identity").lower() != "identity":
            raise ValueError("固定运行时响应编码不受支持")
        data = response.read(limit + 1)
    if len(data) > limit:
        raise ValueError("固定运行时来源超出大小限制")
    return data


def safe_parts(path):
    parts = path.split("/")
    if not parts or len(parts) > 20 or len(path) > 220:
        raise ValueError("运行时相对路径长度无效")
    for part in parts:
        base = part.split(".", 1)[0].upper()
        if (not part or part in (".", "..") or part.endswith((".", " "))
                or re.search(r'[\\:<>"|?*\x00-\x1f]', part)
                or base in ("CON", "PRN", "AUX", "NUL")
                or re.fullmatch(r"(?:COM|LPT)[1-9¹²³]", base)):
            raise ValueError("运行时路径包含逃逸或Windows别名")
    return parts


def assert_plain_root(root):
    if not root.is_absolute():
        raise ValueError("运行时目录必须为绝对路径")
    for part in (root, *root.parents):
        if part.exists():
            info = part.lstat()
            if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
                raise ValueError("运行时目录不能经过链接或reparse点")


def inventory(root):
    assert_plain_root(root)
    rows = []
    seen = set()
    for parent, dirs, files in os.walk(root, followlinks=False):
        for name in dirs + files:
            path = Path(parent) / name
            assert_plain_root(path)
            relative = path.relative_to(root).as_posix()
            safe_parts(relative)
            folded = relative.lower()
            if folded in seen:
                raise ValueError("运行时存在重复路径")
            seen.add(folded)
            if path.is_file():
                data = path.read_bytes()
                if len(data) > 160 * 1024 * 1024:
                    raise ValueError("运行时文件过大")
                rows.append({"path": relative, "size": len(data), "sha256": digest(data)})
            elif not path.is_dir():
                raise ValueError("运行时包含非普通文件")
            if len(seen) > 5000:
                raise ValueError("运行时文件数量超限")
    return sorted(rows, key=lambda row: row["path"])


def dependency_sources():
    package_lock = json.loads((HOST / "package-lock.json").read_bytes())
    sources = []
    for key, value in package_lock["packages"].items():
        if not key:
            continue
        if not key.startswith("node_modules/") or "node_modules/" in key[13:]:
            raise ValueError("首批固定SDK不支持嵌套依赖")
        safe_parts(key)
        url = value["resolved"]
        if (not url.startswith("https://registry.npmjs.org/") or "?" in url or "#" in url
                or not value["integrity"].startswith("sha512-") or value.get("hasInstallScript")):
            raise ValueError("SDK锁来源或脚本不受支持")
        sources.append({"path": key, "version": value["version"], "url": url, "integrity": value["integrity"]})
    if len(sources) != 17:
        raise ValueError("固定SDK集合已变化，需重新审查锁文件")
    return sources


def external_identity():
    return {"schema": 1, "platform": "win32-x64", "node_version": "24.15.0",
            "node_url": NODE_URL, "node_archive_sha256": NODE_SHA256,
            "sdk_lock_sha256": digest((HOST / "package-lock.json").read_bytes()),
            "dependencies": dependency_sources()}


def expected_files(lock):
    # 宿主脚本属于第一方源码，跟随本次冻结构建；第三方完整文件表来自受审查锁。
    rows = list(lock["files"])
    for relative in HOST_FILES:
        data = (HOST / relative).read_bytes()
        rows.append({"path": "host/" + relative, "size": len(data), "sha256": digest(data)})
    return sorted(rows, key=lambda row: row["path"])


def verify(root):
    lock = json.loads(LOCK.read_bytes())
    identity = external_identity()
    if any(lock.get(key) != value for key, value in identity.items()):
        raise ValueError("运行时锁与固定来源或SDK锁不一致")
    rows = inventory(root)
    if rows != expected_files(lock):
        actual = {row["path"]: row for row in rows}
        expected = {row["path"]: row for row in expected_files(lock)}
        differences = sorted(path for path in actual.keys() | expected.keys() if actual.get(path) != expected.get(path))
        raise ValueError("运行时文件缺失、增加或摘要不符：" + ", ".join(differences[:8]))
    print(json.dumps({"verified": True, "platform": lock["platform"], "file_count": len(rows),
                      "total_bytes": sum(row["size"] for row in rows), "lock_sha256": digest(LOCK.read_bytes())}, ensure_ascii=False))


def put(root, path, data):
    destination = root.joinpath(*safe_parts(path))
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("xb") as file:
        file.write(data)


def prepare(root, record):
    identity = external_identity()
    if root.exists():
        if record:
            raise ValueError("记录新锁必须使用不存在的暂存目录，不能覆盖旧运行时")
        verify(root)
        return
    # 输出/清理严格限于仓库tmp，不递归删除任意用户路径。
    if not root.is_absolute() or REPO / "tmp" not in root.parents:
        raise ValueError("准备目录必须是仓库tmp的子目录")
    assert_plain_root(root)
    root.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix="dsh-runtime-stage-", dir=root.parent))
    try:
        archive = read_url(NODE_URL, 100 * 1024 * 1024)
        if digest(archive) != NODE_SHA256:
            raise ValueError("官方Node发行包摘要不符")
        with zipfile.ZipFile(io.BytesIO(archive)) as package:
            for name in ("node.exe", "LICENSE"):
                info = package.getinfo("node-v24.15.0-win-x64/" + name)
                if info.file_size > 160 * 1024 * 1024 or info.is_dir():
                    raise ValueError("Node发行文件无效")
                data = package.read(info)
                if name == "node.exe" and digest(data) != NODE_EXE_SHA256:
                    raise ValueError("官方Node可执行文件摘要不符")
                put(temporary, "node/" + name, data)
        def fetch(source):
            data = read_url(source["url"], 8 * 1024 * 1024)
            checksum = base64.b64decode(source["integrity"][7:], validate=True)
            if hashlib.sha512(data).digest() != checksum:
                raise ValueError("固定SDK归档完整性不符")
            return source, data
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            for source, data in pool.map(fetch, identity["dependencies"]):
                with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as package:
                    seen = set()
                    total = 0
                    for member in package:
                        if not member.name.startswith("package/"):
                            raise ValueError("SDK归档根目录无效")
                        relative = member.name[8:]
                        safe_parts(relative)
                        if member.isdir():
                            continue
                        if not member.isfile() or member.size > 4 * 1024 * 1024 or relative.lower() in seen:
                            raise ValueError("SDK归档包含链接、重复或超大文件")
                        seen.add(relative.lower())
                        total += member.size
                        if len(seen) > 1500 or total > 16 * 1024 * 1024:
                            raise ValueError("SDK归档超出展开限制")
                        extracted = package.extractfile(member)
                        if extracted is None:
                            raise ValueError("SDK归档文件不可读")
                        content = extracted.read(member.size + 1)
                        if len(content) != member.size:
                            raise ValueError("SDK归档文件大小不符")
                        put(temporary, "host/" + source["path"] + "/" + relative, content)
                package_json = json.loads((temporary / "host" / source["path"] / "package.json").read_bytes())
                if package_json["name"] != source["path"][13:] or package_json["version"] != source["version"]:
                    raise ValueError("SDK归档包身份与锁不符")
        external_rows = inventory(temporary)
        if record:
            if LOCK.exists():
                raise ValueError("已有运行时锁；不能在准备操作中自动改写信任表")
            LOCK.write_bytes((json.dumps({**identity, "files": external_rows}, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
        for relative in HOST_FILES:
            put(temporary, "host/" + relative, (HOST / relative).read_bytes())
        verify(temporary)
        os.rename(temporary, root)
    finally:
        if temporary.exists():
            assert_plain_root(temporary)
            if REPO / "tmp" not in temporary.parents:
                raise ValueError("暂存清理路径越界")
            shutil.rmtree(temporary)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=REPO / "tmp/dsh-runtime/windows-x64")
    parser.add_argument("--verify", type=Path)
    parser.add_argument("--record", action="store_true", help="首次审查固定官方归档时记录信任表；禁止覆盖已有锁")
    args = parser.parse_args()
    if args.verify:
        if args.record:
            parser.error("核验不能改写运行时锁")
        verify(args.verify.absolute())
    else:
        prepare(args.output.absolute(), args.record)
