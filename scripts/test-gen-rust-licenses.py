"""Regression controls for the MIT text carried by the PipeWire bindings."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("rust_licenses", ROOT / "scripts/gen-rust-licenses.py")
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)


class MitRecognition(unittest.TestCase):
    def test_standard_text(self):
        text = (ROOT / "licenses/rust/upstream/spdx-MIT.txt").read_text()
        self.assertIn("MIT", generator.identify(text))

    def test_pipewire_text_and_missing_condition_control(self):
        text = (ROOT / "NOTICE").read_text().split("## PipeWire receive-monitor bindings", 1)[1].split("## DeepCW model", 1)[0]
        self.assertIn("MIT", generator.identify(text))
        broken = text.replace("notice (including the next\nparagraph) shall be included", "notice CONDITION REMOVED")
        self.assertNotEqual(text, broken)
        self.assertNotIn("MIT", generator.identify(broken))


if __name__ == "__main__":
    unittest.main()
