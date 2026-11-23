import unittest

from tests import test_named_namespace as namespace


class UnnamedRegistryTests(unittest.TestCase):
    generate = namespace.NamedNamespaceTests.generate
    export = namespace.NamedNamespaceTests.export

    def row(self):
        return {"metadata": {"tkey": "fixture_uncaptioned", "record_id": "unnamed:13",
                             "name": "", "ID": [13, 16, 3], "named": True,
                             "rarity": "Heroic", "tier": "SS",
                             "fromGame": ["ID", "rarity", "tier", "droprate"]},
                "droprate": {"base": 120, "meaning": "denominator"}}

    def test_verified_unnamed_facts_survive_without_a_fabricated_name(self):
        result, output = self.generate([self.row()])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.export(output, "UNNAMED_BY_ID"), {"3:13:16": ["Heroic", 6, 120]})
        for table in ("ITEMS", "BY_ID", "RARITY_BY_NAME", "TIER_BY_NAME", "DROP_RATE"):
            self.assertEqual(self.export(output, table), {}, table)
        rust = output.with_suffix(".rs").read_text("utf-8")
        self.assertIn("pub fn unnamed_by_identity", rust)
        self.assertNotIn('"fixture_uncaptioned"', rust)

    def test_ordinary_or_unproven_namespace_cannot_admit_unnamed_facts(self):
        for named in (False, None):
            with self.subTest(named=named):
                row = self.row()
                row["metadata"]["named"] = named
                result, output = self.generate([row])
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(self.export(output, "UNNAMED_BY_ID"), {})


if __name__ == "__main__":
    unittest.main()
