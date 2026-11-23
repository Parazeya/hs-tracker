import copy
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest


TOOLS = Path(__file__).resolve().parents[1]
FIXTURE = Path(__file__).with_name("fixtures") / "named_namespace.json"


class NamedNamespaceTests(unittest.TestCase):
    def generate(self, rows, *, canonical=True):
        directory = tempfile.TemporaryDirectory(prefix="hs-named-namespace-")
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        inputs = root / "inputs"
        inputs.mkdir()
        sources = {
            "items": json.dumps(rows),
            "talents": '[{"id":2,"key":"fixture"}]',
            "translationsItem": "[Items]|en\n" + "".join(
                f"{row['metadata']['tkey']}|{row['metadata']['name']}\n" for row in rows
            ),
            "translationsZone": "[Zones]|en\nAct_01_01|Fixture Zone\n",
        }
        datasets = {}
        for name, content in sources.items():
            relative = f"source/{name}.csv" if name.startswith("translations") else f"{name}.json"
            path = inputs / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
            datasets[name] = {
                "file": relative, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "records": len(rows) if name == "items" else 1, "status": "partial",
            }
        if canonical:
            files = [{"path": name, "sha256": hashlib.sha256(name.encode()).hexdigest(), "size": len(name)}
                     for name in ("Hero_Siege.exe", "data.win")]
            game = {"files": files, "fingerprint": hashlib.sha256(
                json.dumps(files, separators=(",", ":")).encode()).hexdigest()}
            (inputs / "manifest.json").write_text(json.dumps({
                "schema_version": 1, "game": game, "datasets": datasets,
            }), encoding="utf-8")
        output = root / "generated/items.js"
        environment = {key: value for key, value in os.environ.items()
                       if key not in ("HS_DATA_BUNDLE", "HS_ITEMS_INPUT")}
        environment.update({
            "HS_ITEMS_INPUT": str(inputs / "items.json"),
            "HERO_SIEGE_BIN": str(inputs / "source"),
            "HS_ITEMS_OUT_JS": str(output), "HS_ITEMS_OUT_RS": str(root / "generated/items.rs"),
            "HS_ITEMS_LANG_OUT": str(root / "generated/lang"),
        })
        if canonical:
            environment["HS_DATA_BUNDLE"] = str(inputs)
        result = subprocess.run([sys.executable, str(TOOLS / "gen_items.py")], env=environment,
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        return result, output

    def fixture(self):
        return json.loads(FIXTURE.read_text("utf-8"))

    def export(self, output, name):
        script = output.read_text("utf-8")
        return json.loads(re.search(rf"export const {name} = (.+);", script).group(1))

    def test_named_common_wins_its_namespace_regardless_of_rarity_and_input_order(self):
        for reverse in (False, True):
            rows = self.fixture()
            if reverse:
                rows.reverse()
            with self.subTest(reverse=reverse):
                result, output = self.generate(rows)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(self.export(output, "ITEMS")["0:71:0"], "Fixture Named Common")
                # The ordinary base's more notable rarity cannot leak into
                # the winning named identity's own facts.
                self.assertEqual(self.export(output, "RARITY_BY_NAME")["fixture named common"], "Common")
                self.assertEqual(self.export(output, "TIER_BY_NAME")["fixture named common"], 1)
                override = self.export(output, "BY_ID").get("0:71:0")
                if override is not None:
                    self.assertEqual(override[:3], ["Fixture Named Common", "Common", 1])

    def test_two_explicit_named_records_conflict_even_when_one_is_common(self):
        rows = self.fixture()
        rows[0]["metadata"]["named"] = True
        result, output = self.generate(rows)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Conflicting named item identity 0:71:0", result.stdout + result.stderr)
        self.assertFalse(output.exists(), "a conflicting table must not be installed")

    def test_canonical_missing_named_is_not_inferred_from_common(self):
        rows = self.fixture()
        rows[0]["metadata"]["named"] = True
        del rows[1]["metadata"]["named"]
        result, output = self.generate(rows)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Conflicting named item identity", result.stdout + result.stderr)
        self.assertFalse(output.exists())

    def test_legacy_records_without_named_keep_the_rarity_fallback(self):
        rows = copy.deepcopy(self.fixture())
        for row in rows:
            del row["metadata"]["named"]
        result, output = self.generate(rows, canonical=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.export(output, "ITEMS")["0:71:0"], "Fixture Ordinary Base")


if __name__ == "__main__":
    unittest.main()
