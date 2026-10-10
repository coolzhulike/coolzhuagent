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
