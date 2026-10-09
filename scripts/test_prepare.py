import unittest
import json
import tempfile
from pathlib import Path
from types import SimpleNamespace
from prepare import extract, normalize, hyphen_penalty, ranking_score, rerank_prepared, OLD_POLICY, POLICY


class RankingTests(unittest.TestCase):
    def test_fixed_hyphen_penalty_and_normalization(self):
        self.assertEqual(hyphen_penalty("mother-in-law"), 300)
        self.assertEqual(hyphen_penalty(normalize("house\u2011like")), 150)
        self.assertEqual(hyphen_penalty("well\u2010known"), 150)
        self.assertEqual(hyphen_penalty("take off"), 0)
        self.assertEqual(hyphen_penalty("don't"), 0)
        self.assertEqual(hyphen_penalty("x–y"), 0)
        self.assertGreater(ranking_score("home", 5.81), ranking_score("how-to", 6.21))
        self.assertGreater(ranking_score("household", 4.47), ranking_score("house-like", 5.63))
        self.assertEqual(ranking_score("house-like", 5.63), 393)

    def test_reranking_preserves_source_fields_and_is_not_applied_twice(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "original"
            original.mkdir()
            entry = {"key":"house-like", "headword":"house-like", "score":543,
                     "groups":[{"pos":"adj", "senses":[{"glosses":["source text"]}]}]}
            (original / "entries.jsonl").write_text(json.dumps(entry) + "\n")
            (original / "source.json").write_text(json.dumps({"ranking":OLD_POLICY, "snapshot":"test",
                                                             "quality_report":{"entries":1}}))
            rerank_prepared(SimpleNamespace(prepared=original, output=root / "new"))
            new = json.loads((root / "new/entries.jsonl").read_text())
            self.assertEqual(new.pop("score"), 393)
            entry.pop("score")
            self.assertEqual(new, entry)
            source = json.loads((root / "new/source.json").read_text())
            self.assertEqual(source["ranking"], POLICY)
            self.assertEqual(source["snapshot"], "test")
            rerank_prepared(SimpleNamespace(prepared=root / "new", output=root / "again"))
            self.assertEqual((root / "again/entries.jsonl").read_bytes(), (root / "new/entries.jsonl").read_bytes())
            source["ranking"] = "unknown"
            (root / "new/source.json").write_text(json.dumps(source))
            with self.assertRaises(ValueError):
                rerank_prepared(SimpleNamespace(prepared=root / "new", output=root / "bad"))


class ExtractTests(unittest.TestCase):
    def test_normalization_matches_shared_cross_language_cases(self):
        # Shared with the runtime's normalize() unit test, so the builder and the
        # lookup path cannot drift apart silently.
        fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/normalize.json"
        for raw, expected in json.loads(fixture.read_text()):
            self.assertEqual(normalize(raw), expected, raw)

    def test_preserves_content_and_orders_historical_senses(self):
        raw = {"word": "Test", "lang_code": "en", "pos": "noun", "sounds": [{"ipa": "/test/"}],
               "senses": [{"glosses": ["old fixture"], "tags": ["archaic"]},
                          {"glosses": ["modern fixture"], "examples": [
                              {"text": "citation fixture", "type": "quotation", "ref": "source"},
                              {"text": "example fixture", "type": "example"}]}],
               "forms": [{"form": "Tests", "tags": ["plural"]}]}
        key, group, forms = extract(raw)
        self.assertEqual(key, "test")
        self.assertEqual(forms, ["tests"])
        self.assertEqual(group["senses"][0]["glosses"], ["modern fixture"])
        self.assertEqual(group["senses"][1]["glosses"], ["old fixture"])
        self.assertEqual(group["senses"][0]["examples"][0]["text"], "example fixture")
        self.assertEqual(group["senses"][0]["examples"][1]["reference"], "source")

    def test_english_only_and_malformed_data(self):
        self.assertIsNone(extract({"lang_code": "fr", "word": "test"}))
        with self.assertRaises(ValueError):
            extract({"lang_code": "en", "word": ""})

    def test_does_not_split_compound_morphology_target(self):
        result = extract({"word": "better", "lang_code": "en", "pos": "adj",
                          "senses": [{"glosses": ["fixture"], "form_of": [{"word": "good and well"}]}]})
        self.assertEqual(result[1]["senses"][0]["targets"], ["good and well"])

    def test_real_source_numeric_etymology_strings(self):
        raw = {"word": "air", "lang_code": "en", "pos": "noun", "etymology_number": "1",
               "senses": [{"glosses": ["fixture"]}]}
        self.assertEqual(extract(raw)[1]["etymology_number"], 1)
        raw["etymology_number"] = "unknown"
        with self.assertRaises(ValueError):
            extract(raw)

    def test_inverse_forms_exclude_aliases_and_historical_tables(self):
        raw = {"word": "good", "lang_code": "en", "pos": "adj",
               "senses": [{"glosses": ["fixture"]}], "forms": [
                   {"form": "better", "tags": ["comparative"]},
                   {"form": "alias", "tags": ["alternative"]},
                   {"form": "old", "tags": ["past", "archaic"]},
                   {"form": "word", "tags": ["canonical"]}]}
        self.assertEqual(extract(raw)[2], ["better"])

    def test_historical_only_groups_do_not_supply_inverse_forms(self):
        raw = {"word": "product", "lang_code": "en", "pos": "verb",
               "senses": [{"glosses": ["fixture"], "tags": ["obsolete"]}],
               "forms": [{"form": "producted", "tags": ["past"]}]}
        self.assertEqual(extract(raw)[2], [])
        raw["senses"][0]["tags"] = []
        raw["tags"] = ["archaic"]
        self.assertEqual(extract(raw)[2], [])
        raw["tags"] = []
        self.assertEqual(extract(raw)[2], ["producted"])


if __name__ == "__main__":
    unittest.main()
