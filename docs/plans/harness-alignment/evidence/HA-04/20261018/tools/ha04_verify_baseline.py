import hashlib
import json
import shutil
import subprocess
from pathlib import Path

ROOT = Path(r"D:\coolzhuagent")
BASE = ROOT / "docs" / "plans" / "harness-alignment" / "baseline" / "HA-00-20261018"
TMP = ROOT / "tmp" / "ha04-20261018-baseline-verify"
manifest = json.loads((BASE / "manifest.json").read_text(encoding="utf-8"))

if TMP.exists():
    raise RuntimeError("恢复验证目录已存在，禁止覆盖")
assert TMP.resolve().is_relative_to((ROOT / "tmp").resolve())
TMP.mkdir(parents=True)


def git_show(rev_path: str):
    result = subprocess.run(
        ["git", "-C", str(ROOT), "show", rev_path],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    return result.stdout if result.returncode == 0 else None


def git_blob(path: Path, rel: str):
    return subprocess.run(
        ["git", "-C", str(ROOT), "hash-object", "--path", rel, "--", str(path)],
        check=True, stdout=subprocess.PIPE,
    ).stdout.decode().strip()


def sha256(path: Path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()

for entry in manifest["included_changed_files"]:
    rel = entry["path"]
    data = git_show(f"{manifest['git_head']}:{rel}")
    if data is None:
        continue
    dst = TMP / rel
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(data)

subprocess.run(["git", "-C", str(TMP), "init", "-q"], check=True)
apply = subprocess.run(
    ["git", "-C", str(TMP), "apply", "--binary", str(BASE / "source.patch")],
    stdout=subprocess.PIPE, stderr=subprocess.PIPE,
)
print("git_apply_exit", apply.returncode)
if apply.returncode != 0:
    print(apply.stderr.decode("utf-8", "replace"))
    raise SystemExit(1)

for entry in manifest["untracked_source_files"]:
    rel = entry["path"]
    src = BASE / "untracked" / rel
    dst = TMP / rel
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)

errors = []
for entry in manifest["included_changed_files"]:
    rel = entry["path"]
    restored = TMP / rel
    if entry.get("deleted"):
        if restored.exists():
            errors.append(f"expected deleted but exists: {rel}")
        continue
    if not restored.exists():
        errors.append(f"missing after restore: {rel}")
        continue
    actual = git_blob(restored, rel)
    expected = entry.get("git_blob_sha1")
    if actual != expected:
        errors.append(f"blob mismatch: {rel} expected={expected} actual={actual}")

for entry in manifest["untracked_source_files"]:
    rel = entry["path"]
    restored = TMP / rel
    if not restored.exists():
        errors.append(f"missing untracked: {rel}")
        continue
    if sha256(restored) != entry["sha256"]:
        errors.append(f"untracked hash mismatch: {rel}")

print("restored_files", len(manifest["included_changed_files"]) + len(manifest["untracked_source_files"]))
print("errors", len(errors))
for error in errors[:20]:
    print(error)
raise SystemExit(1 if errors else 0)
