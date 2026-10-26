"""Verify the catalog actually loaded by the audio description window."""
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]


class AudioDescriptionLocalizationTests(unittest.TestCase):
    def test_all_used_keys_exist_in_every_loaded_catalog(self):
        source = (ROOT / "src/audio_description.rs").read_text(encoding="utf-8")
        # Inspect translation calls, excluding similarly named diagnostic events.
        keys = set(re.findall(r'\btrf?\(\s*"([^"]+)"', source))
        catalogs = set(re.findall(r'include_str!\("\.\./(i18n/audio_description_[a-z]+\.json)"\)', source))
        self.assertTrue(keys)
        self.assertEqual(len(catalogs), 7)
        for path in sorted(catalogs):
            with self.subTest(catalog=path):
                data = json.loads((ROOT / path).read_text(encoding="utf-8"))
                self.assertEqual(keys - data.keys(), set())
                for key in keys:
                    self.assertTrue(data[key].strip(), (path, key))
                    self.assertNotEqual(data[key], key, (path, key))


if __name__ == "__main__":
    unittest.main()
