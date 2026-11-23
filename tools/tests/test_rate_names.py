import unittest

from tests import test_named_namespace as namespace


class RateNameTests(unittest.TestCase):
    generate = namespace.NamedNamespaceTests.generate
    export = namespace.NamedNamespaceTests.export

    def row(self, identity, rarity, rate=None):
        fields = ["ID", "name", "tier"]
        meta = {"tkey": f"fixture_rate_{identity}", "record_id": f"rate:{identity}",
                "ID": [identity, 0, 10], "name": "Shared rate name", "named": True,
                "tier": "S", "fromGame": fields}
        if rarity is not None:
            meta["rarity"] = rarity
            fields.append("rarity")
        row = {"metadata": meta}
        if rate is not None:
            row["droprate"] = {"base": rate, "meaning": "denominator"}
            fields.append("droprate")
        return row

    def test_missing_rate_on_another_common_identity_blocks_name_but_preserves_exact_rate(self):
        result, output = self.generate([self.row(1, "Common", 27), self.row(2, "Common")])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("shared rate name", self.export(output, "DROP_RATE"))
        self.assertEqual(self.export(output, "BY_ID")["10:1:0"][3], 27)
        self.assertNotIn('engine\'s "does not drop"', result.stdout)

    def test_missing_competing_notable_rate_or_quality_cannot_inherit_name_rate(self):
        for rarity, rate in (("Set", None), (None, None), (None, 120)):
            with self.subTest(rarity=rarity, rate=rate):
                result, output = self.generate([self.row(1, "Heroic", 120), self.row(2, rarity, rate)])
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertNotIn("shared rate name", self.export(output, "DROP_RATE"))
                self.assertEqual(self.export(output, "BY_ID")["10:1:0"][3], 120)

    def test_verified_ordinary_namesake_does_not_block_the_sole_notable_rate(self):
        result, output = self.generate([self.row(1, "Heroic", 120), self.row(2, "Common")])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.export(output, "DROP_RATE")["shared rate name"], 120)


if __name__ == "__main__":
    unittest.main()
