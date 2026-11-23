import bz2
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from item_bundle import current_item_rows, load_bundle

def source_inventory():
    files = [{"path": name, "sha256": hashlib.sha256(name.encode()).hexdigest(), "size": len(name)} for name in ("Hero_Siege.exe", "data.win")]
    return {"files": files, "fingerprint": hashlib.sha256(json.dumps(files, separators=(",", ":")).encode()).hexdigest()}


class BundleTests(unittest.TestCase):
    def test_unverified_historical_values_do_not_enter_lookup_tables(self):
        rows = [{"metadata": {"tkey": "test", "record_id": "test:0", "fromGame": ["ID", "name"],
                              "ID": [3, 0, 0], "name": "Current", "rarity": "Angelic", "tier": "SS",
                              "drop": [{"locations": ["1-1"], "chase": 2}]},
                 "droprate": {"base": 10}, "chaseDropRate": .2, "dropPlaces": ["Old place"]}]
        current = current_item_rows(rows)[0]
        self.assertEqual(current["metadata"]["ID"], [3, 0, 0])
        self.assertNotIn("rarity", current["metadata"])
        self.assertNotIn("tier", current["metadata"])
        self.assertNotIn("drop", current["metadata"])
        self.assertNotIn("droprate", current)
        self.assertNotIn("chaseDropRate", current)
        self.assertNotIn("dropPlaces", current)
        self.assertEqual(rows[0]["metadata"]["rarity"], "Angelic", "raw evidence must be preserved")

    def test_verified_zero_and_engine_default_are_preserved(self):
        rows = [{"metadata": {"tkey": "test", "fromGame": ["ID", "tier", "droprate", "chaseDropRate"],
                              "ID": [0, 0, 0], "tier": "D"},
                 "droprate": {"base": 50_000_000}, "chaseDropRate": 1}]
        current = current_item_rows(rows)[0]
        self.assertEqual(current["metadata"]["ID"], [0, 0, 0])
        self.assertEqual(current["droprate"]["base"], 50_000_000)
        self.assertEqual(current["chaseDropRate"], 1)

    def test_nested_bundle_files_are_verified_before_reading(self):
        with tempfile.TemporaryDirectory(prefix="hs-generator-bundle-") as directory:
            root = Path(directory)
            data = {"items": [{"metadata": {"tkey": "test", "fromGame": []}}], "talents": [{"id": 2, "key": "test"}],
                    "translationsItem": "[Items]|en\ntest|Test\n"}
            datasets = {}
            for name, rows in data.items():
                name_file = f"generations/build/source/{name}.csv" if name == "translationsItem" else f"generations/build/{name}.json"
                path = root / name_file
                path.parent.mkdir(parents=True, exist_ok=True)
                content = (rows if isinstance(rows, str) else json.dumps(rows)).encode()
                path.write_bytes(content)
                datasets[name] = {"file": name_file, "sha256": hashlib.sha256(content).hexdigest(), "records": 1, "status": "complete"}
            (root / "manifest.json").write_text(json.dumps({"schema_version": 1, "game": source_inventory(), "datasets": datasets}))
            _, files = load_bundle(root)
            self.assertEqual(files["items"], root / datasets["items"]["file"])
            manifest = json.loads((root / "manifest.json").read_text())
            manifest["game"]["files"][0]["size"] += 1
            (root / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, "fingerprint does not match"):
                load_bundle(root)
            manifest["game"] = source_inventory()
            manifest["game"]["files"].append(manifest["game"]["files"][0])
            (root / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, "Duplicate game source"):
                load_bundle(root)
            manifest["game"] = source_inventory()
            (root / "manifest.json").write_text(json.dumps(manifest))
            files["talents"].write_text("[]")
            with self.assertRaisesRegex(ValueError, "hash does not match"):
                load_bundle(root)

    def test_known_identity_with_unresolved_effective_rarity_keeps_an_unknown_guard(self):
        with tempfile.TemporaryDirectory(prefix="hs-generator-effective-rarity-") as directory:
            root = Path(directory)
            bundle = root / "bundle"
            bundle.mkdir()
            rows = [
                {"metadata": {"tkey": "socketable_orb_angel", "record_id": "orb:0", "ID": [1, 0, 15],
                              "name": "Angel", "named": True, "fromGame": ["ID", "name"]}},
                {"metadata": {"tkey": "w_ranged_angel", "record_id": "weapon:0", "ID": [20, 14, 3],
                              "name": "Angel", "named": True, "rarity": "Satanic Set", "tier": "S",
                              "fromGame": ["ID", "name", "rarity", "tier"]}},
                {"metadata": {"tkey": "helmet_exact_50m", "record_id": "helmet:0", "ID": [31, 0, 0],
                              "name": "Exact Fifty Million", "named": True, "rarity": "Satanic", "tier": "S",
                              "fromGame": ["ID", "name", "rarity", "tier", "droprate"]},
                 "droprate": {"base": 50_000_000, "meaning": "denominator"}},
            ]
            sources = {"items": json.dumps(rows), "talents": '[{"id":2,"key":"test"}]',
                       "translationsItem": "[Items]|en\nsocketable_orb_angel|Angel\nw_ranged_angel|Angel\nhelmet_exact_50m|Exact Fifty Million\n",
                       "translationsZone": "[Zones]|en\nAct_01_01|Test Zone\n"}
            datasets = {}
            for name, body in sources.items():
                relative = f"source/{name}.csv" if name.startswith("translations") else f"{name}.json"
                path = bundle / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(body, encoding="utf-8")
                datasets[name] = {"file": relative, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                                  "records": len(rows) if name == "items" else 1, "status": "partial" if name == "items" else "complete"}
            (bundle / "manifest.json").write_text(json.dumps({"schema_version": 1, "game": source_inventory(), "datasets": datasets}), encoding="utf-8")
            js = root / "generated/items.js"
            result = subprocess.run([sys.executable, str(Path(__file__).with_name("gen_items.py"))],
                                    env={**os.environ, "HS_DATA_BUNDLE": str(bundle), "HS_ITEMS_INPUT": str(bundle / "items.json"),
                                         "HS_ITEMS_OUT_JS": str(js), "HS_ITEMS_OUT_RS": str(root / "generated/items.rs"),
                                         "HS_ITEMS_LANG_OUT": str(root / "generated/lang")},
                                    capture_output=True, text=True, encoding="utf-8", timeout=30)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            script = js.read_text("utf-8")
            export = lambda name: json.loads(re.search(rf"export const {name} = (.+);", script).group(1))
            self.assertNotIn("angel", export("RARITY_BY_NAME"), "an unresolved namesake cannot be dismissed as Common")
            self.assertEqual(export("BY_ID")["15:1:0"][:3], ["Angel", "Unknown", 0])
            self.assertEqual(export("BY_ID")["3:20:14"][:3], ["Angel", "Set", 5])
            self.assertEqual(export("DROP_RATE")["exact fifty million"], 50_000_000)
            self.assertNotIn("15:999:0", export("ITEMS"), "unseen identities must keep the parser's existing fallback")


class TextureSourceTests(unittest.TestCase):
    @staticmethod
    def texture(red):
        inner = b"fioq" + struct.pack("<HHI", 1, 1, 0) + bytes([0xff, red, 20, 30, 255])
        return b"2zoq" + struct.pack("<HHI", 1, 1, len(inner)) + bz2.compress(inner)

    def test_two_game_installations_do_not_share_paths_or_cached_pages(self):
        import datawin
        with tempfile.TemporaryDirectory(prefix="hs-texture-sources-") as directory:
            root = Path(directory)
            objects = []
            for name, red in (("first", 10), ("second", 90)):
                game = root / name
                game.mkdir()
                (game / "data.win").write_bytes(b"FORM" + b"\0" * 4)
                (game / "page_0.yytex").write_bytes(self.texture(red))
                with patch.object(datawin.DataWin, "_parse_tpag"), patch.object(datawin.DataWin, "_parse_sprt"), \
                     patch.object(datawin.DataWin, "_parse_tgin"), patch.object(datawin.DataWin, "_parse_objt"):
                    obj = datawin.DataWin(game / "data.win")
                obj.page_files = {0: Path("page_0")}
                objects.append(obj)
            self.assertEqual(objects[0].page_image(0).getpixel((0, 0)), (10, 20, 30, 255))
            self.assertEqual(objects[1].page_image(0).getpixel((0, 0)), (90, 20, 30, 255))

    def test_an_embedded_page_needs_no_loose_texture_file(self):
        import datawin
        obj = datawin.DataWin.__new__(datawin.DataWin)
        obj.raw = self.texture(50)
        obj._page_cache = {}
        obj._embedded_pages = {0: (0, len(obj.raw))}
        obj.page_files = {}
        self.assertEqual(obj.page_image(0).getpixel((0, 0)), (50, 20, 30, 255))

    def test_an_incomplete_texture_does_not_return_a_partial_image(self):
        import yytex
        inner = b"fioq" + struct.pack("<HHI", 2, 1, 0) + bytes([0xff, 1, 2, 3, 255])
        raw = b"2zoq" + struct.pack("<HHI", 2, 1, len(inner)) + bz2.compress(inner)
        with self.assertRaisesRegex(ValueError, "incomplete texture"):
            yytex.decode_bytes(raw)


if __name__ == "__main__":
    unittest.main()
