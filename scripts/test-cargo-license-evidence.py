"""许可证证据原文字节、范围与输出独占验证。"""
import base64
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("license_export", Path(__file__).with_name("export-cargo-license-evidence.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class LicenseEvidenceTests(unittest.TestCase):
    def test_workspace_license_requires_member_inheritance_and_matching_declaration(self):
        with tempfile.TemporaryDirectory(dir="tmp", prefix="license-inheritance-test-") as directory:
            root = Path(directory).resolve()
            package = root / "member"
            package.mkdir()
            (root / "Cargo.toml").write_text('[workspace.package]\nlicense="MIT"\n', encoding="utf-8")
            (root / "LICENSE").write_bytes(b"Workspace original license\r\n")
            manifest = package / "Cargo.toml"
            manifest.write_text('[package]\nlicense.workspace=true\n', encoding="utf-8")
            p = {"id": "local", "name": "member", "version": "1.0", "source": None,
                 "license": "MIT", "license_file": "../LICENSE", "manifest_path": str(manifest)}
            metadata = {"packages": [p], "workspace_root": str(root), "workspace_members": ["local"]}
            path = root / "metadata.json"
            path.write_text(json.dumps(metadata), encoding="utf-8")
            evidence = module.export(path, root / "out.json")["components"][0]
            self.assertEqual(len(evidence["files"]), 1)
            self.assertEqual(evidence["files"][0]["scope"], "workspace-inherited")
            self.assertEqual(evidence["files"][0]["content"], "Workspace original license\r\n")
            self.assertEqual(len(evidence["unresolved"]), 1)  # 显式逃根路径依旧拒绝。
            for n, override in enumerate(({"workspace_members": []}, {"packages": [{**p, "license": "Apache-2.0"}]},
                                         {"packages": [{**p, "source": "registry+example"}]})):
                path.write_text(json.dumps({**metadata, **override}), encoding="utf-8")
                self.assertFalse(module.export(path, root / f"negative-{n}.json")["components"][0]["files"])
            manifest.write_text('[package]\nlicense="MIT"\n', encoding="utf-8")
            path.write_text(json.dumps(metadata), encoding="utf-8")
            self.assertFalse(module.export(path, root / "not-inherited.json")["components"][0]["files"])

    def test_build_messages_require_complete_success_and_exact_component_scope(self):
        with tempfile.TemporaryDirectory(dir="tmp", prefix="license-build-test-") as directory:
            root = Path(directory)
            messages = root / "build.jsonl"
            artifact = {"reason": "compiler-artifact", "package_id": "known",
                        "target": {"name": "producer", "kind": ["custom-build"]}, "fresh": True}
            finish = {"reason": "build-finished", "success": True}
            messages.write_text("\n".join(json.dumps(item) for item in [artifact, finish]), encoding="utf-8")
            evidence, observed = module.read_build_observations(messages, {"known", "unused"})
            self.assertEqual(set(observed), {"known"})
            self.assertEqual(observed["known"][0]["target_kind"], ["custom-build"])
            self.assertTrue(observed["known"][0]["fresh"])
            self.assertTrue(evidence["success_message_verified"])
            for records in ([artifact], [artifact, {"reason": "build-finished", "success": False}],
                            [artifact, finish, finish], [finish, artifact], [finish],
                            [{**artifact, "package_id": "foreign"}, finish]):
                messages.write_text("\n".join(json.dumps(item) for item in records), encoding="utf-8")
                with self.assertRaises(ValueError):
                    module.read_build_observations(messages, {"known"})

    def test_bytes_identity_scope_and_no_overwrite(self):
        with tempfile.TemporaryDirectory(dir="tmp", prefix="license-evidence-test-") as directory:
            root = Path(directory).absolute()
            package = root / "package"
            package.mkdir()
            (package / "Cargo.toml").write_text("[package]\n", encoding="utf-8")
            original = b"License body\r\n\xff"
            (package / "LICENSE").write_bytes(original)
            (root / "outside-license").write_text("outside must not be copied", encoding="utf-8")
            metadata = root / "metadata.json"
            metadata.write_text(json.dumps({"packages": [{"id": "candidate", "name": "fixture",
                "version": "1.0.0", "source": None, "license": "MIT OR Apache-2.0",
                "license_file": "../outside-license", "manifest_path": str(package / "Cargo.toml")}]}), encoding="utf-8")
            output = root / "output.json"
            result = module.export(metadata, output)
            evidence = result["components"][0]
            self.assertEqual(evidence["declared_license"], "MIT OR Apache-2.0")
            self.assertEqual(len(evidence["files"]), 1)
            body = evidence["files"][0]
            self.assertEqual(base64.b64decode(body["content"]), original)
            self.assertEqual(body["sha256"], hashlib.sha256(original).hexdigest())
            self.assertEqual(len(evidence["unresolved"]), 1)
            self.assertFalse(result["compiled_component_mapping_verified"])
            self.assertEqual(result["completeness"], "incomplete")
            frozen = output.read_bytes()
            with self.assertRaises(FileExistsError):
                module.export(metadata, output)
            self.assertEqual(output.read_bytes(), frozen)

    def test_invalid_metadata_does_not_publish_output(self):
        with tempfile.TemporaryDirectory(dir="tmp", prefix="license-evidence-test-") as directory:
            root = Path(directory)
            metadata = root / "metadata.json"
            metadata.write_text("{}", encoding="utf-8")
            output = root / "output.json"
            with self.assertRaises(KeyError):
                module.export(metadata, output)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
