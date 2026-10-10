"""从冻结 Cargo metadata 导出本地许可证原文证据；不推断实际分发或法律结论。"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import tempfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def export(metadata_path, output_path):
    raw = Path(metadata_path).read_bytes()
    metadata = json.loads(raw.decode("utf-8-sig"))
    packages = metadata["packages"]
    components = []
    identities = set()
    for package in packages:
        identity = package["id"]
        if identity in identities:
            raise ValueError("metadata 包身份重复")
        identities.add(identity)
        root = Path(package["manifest_path"]).resolve(strict=True).parent
        candidates = set()
        for path in root.iterdir():
            name = path.name.lower()
            if name.startswith(("license", "licence", "copying", "copyright", "notice")):
                if path.is_file():
                    candidates.add(path)
                elif path.is_dir() and not path.is_symlink():
                    candidates.update(child for child in path.iterdir() if child.is_file())
        declared_file = package.get("license_file")
        if declared_file:
            candidates.add(root / declared_file)
        files, unresolved = [], []
        for candidate in sorted(candidates, key=lambda path: str(path)):
            # 拒绝逃出组件根的链接；不扫描父目录补造证据。
            try:
                relative = candidate.relative_to(root).as_posix()
                resolved = candidate.resolve(strict=True)
                resolved.relative_to(root)
                if candidate.is_symlink() or not resolved.is_file():
                    raise ValueError("非普通组件内文件")
                body = resolved.read_bytes()
                try:
                    content, encoding = body.decode("utf-8"), "utf-8"
                except UnicodeDecodeError:
                    content, encoding = base64.b64encode(body).decode("ascii"), "base64"
                files.append({"path": relative, "bytes": len(body), "sha256": digest(body),
                              "encoding": encoding, "content": content})
            except (OSError, ValueError):
                unresolved.append({"path": candidate.name, "reason": "缺失、不可读或不在组件根内"})
        components.append({"identity_sha256": digest(identity.encode("utf-8")),
                           "name": package["name"], "version": package["version"],
                           "source": package.get("source"), "declared_license": package.get("license"),
                           "files": files, "unresolved": unresolved,
                           "license_body_found": bool(files)})
    result = {"schema": 1, "kind": "cargo-local-license-evidence",
              "metadata_sha256": digest(raw), "scope": "解析候选组件及本地原文字节，包含非当前目标候选",
              "completeness": "incomplete", "compiled_component_mapping_verified": False,
              "license_obligations_verified": False, "component_count": len(components),
              "components": components}
    target = Path(output_path).absolute()
    if target.exists():
        raise FileExistsError("许可证证据输出已存在，禁止覆盖")
    target.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=".license-evidence-", dir=target.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write((json.dumps(result, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
            stream.flush()
            os.fsync(stream.fileno())
        # 独占创建最终名字，既有成果与并发胜者均不得覆盖。
        os.link(temporary, target)
    finally:
        Path(temporary).unlink(missing_ok=True)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--metadata", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    result = export(args.metadata, args.output)
    print(json.dumps({"component_count": result["component_count"],
                      "with_local_text": sum(item["license_body_found"] for item in result["components"]),
                      "completeness": result["completeness"]}, ensure_ascii=False))
