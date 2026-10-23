"""从冻结 Cargo metadata 导出本地许可证原文证据；不推断实际分发或法律结论。"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import tempfile
import tomllib


def digest(data):
    return hashlib.sha256(data).hexdigest()


def inherited_license_root(metadata, package, component_root):
    """只接受 Cargo 明示的本工作区许可证继承，不推测第三方父目录。"""
    if (package["id"] not in metadata.get("workspace_members", [])
            or package.get("source") is not None or not metadata.get("workspace_root")):
        return None
    try:
        root = Path(metadata["workspace_root"]).resolve(strict=True)
        component_root.relative_to(root)
        local = tomllib.loads((component_root / "Cargo.toml").read_text(encoding="utf-8-sig"))
        workspace = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8-sig"))
        declaration = local.get("package", {}).get("license")
        declared = workspace.get("workspace", {}).get("package", {}).get("license")
        if (root != component_root and isinstance(declaration, dict)
                and declaration.get("workspace") is True and declared
                and declared == package.get("license")):
            return root
    except (OSError, ValueError):
        pass
    return None


def read_build_observations(path, package_ids):
    raw = Path(path).read_bytes()
    records = [json.loads(line) for line in raw.decode("utf-8-sig").splitlines() if line.strip()]
    finished = [record for record in records if record.get("reason") == "build-finished"]
    if len(finished) != 1 or finished[0].get("success") is not True or records[-1] is not finished[0]:
        raise ValueError("构建消息必须唯一以成功 build-finished 结束")
    observations = {}
    for record in records:
        if record.get("reason") not in ("compiler-artifact", "build-script-executed"):
            continue
        identity = record["package_id"]
        if identity not in package_ids:
            raise ValueError("实际构建组件无法关联到本次 metadata")
        target = record.get("target") or {}
        observations.setdefault(identity, []).append({
            "reason": record["reason"], "target_name": target.get("name"),
            "target_kind": target.get("kind", []), "crate_types": target.get("crate_types", []),
            "fresh": record.get("fresh"),
        })
    if not observations:
        raise ValueError("构建成功消息不含可关联的实际组件记录")
    return {"sha256": digest(raw), "success_message_verified": True,
            "scope": "构建观察，包含 build/proc-macro/缓存；不等于运行时分发"}, observations


def export(metadata_path, output_path, build_messages=None):
    raw = Path(metadata_path).read_bytes()
    metadata = json.loads(raw.decode("utf-8-sig"))
    packages = metadata["packages"]
    package_ids = [package["id"] for package in packages]
    if len(set(package_ids)) != len(package_ids):
        raise ValueError("metadata 包身份重复")
    build_observation, observations = (None, {})
    if build_messages:
        build_observation, observations = read_build_observations(build_messages, set(package_ids))
        packages = [package for package in packages if package["id"] in observations]
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
        scoped_candidates = [(path, root, "component") for path in candidates]
        inherited_root = inherited_license_root(metadata, package, root)
        if inherited_root:
            scoped_candidates.extend((path, inherited_root, "workspace-inherited")
                for path in inherited_root.iterdir() if path.is_file()
                and path.name.lower().startswith(("license", "licence", "copying", "copyright", "notice")))
        for candidate, allowed_root, scope in sorted(scoped_candidates, key=lambda row: str(row[0])):
            # 声明的组件文件仍严格限于组件根；继承文件另有明确的工作区依据。
            try:
                relative = candidate.relative_to(allowed_root).as_posix()
                resolved = candidate.resolve(strict=True)
                resolved.relative_to(allowed_root)
                if candidate.is_symlink() or not resolved.is_file():
                    raise ValueError("非普通组件内文件")
                body = resolved.read_bytes()
                try:
                    content, encoding = body.decode("utf-8"), "utf-8"
                except UnicodeDecodeError:
                    content, encoding = base64.b64encode(body).decode("ascii"), "base64"
                files.append({"path": relative, "scope": scope, "bytes": len(body), "sha256": digest(body),
                              "encoding": encoding, "content": content})
            except (OSError, ValueError):
                unresolved.append({"path": candidate.name, "reason": "缺失、不可读或不在组件根内"})
        components.append({"identity_sha256": digest(identity.encode("utf-8")),
                           "name": package["name"], "version": package["version"],
                           "source": package.get("source"), "declared_license": package.get("license"),
                           "build_observations": observations.get(identity, []),
                           "files": files, "unresolved": unresolved,
                           "license_body_found": bool(files)})
    result = {"schema": 1, "kind": "cargo-local-license-evidence",
              "metadata_sha256": digest(raw), "scope": "解析候选组件及本地原文字节，包含非当前目标候选",
              "completeness": "incomplete", "compiled_component_mapping_verified": False,
              "build_observation": build_observation,
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
    parser.add_argument("--build-messages", help="本次实际 Cargo JSON 构建记录；不是安装载荷证明")
    args = parser.parse_args()
    result = export(args.metadata, args.output, args.build_messages)
    print(json.dumps({"component_count": result["component_count"],
                      "with_local_text": sum(item["license_body_found"] for item in result["components"]),
                      "completeness": result["completeness"]}, ensure_ascii=False))
