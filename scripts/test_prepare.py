import unittest
from prepare import extract, normalize


class ExtractTests(unittest.TestCase):
    def test_normalization(self):
        self.assertEqual(normalize("  ＴＡＫＥ\tOff  "), "take off")
        self.assertEqual(normalize("DON’T"), "don't")
        self.assertNotEqual(normalize("résumé"), normalize("resume"))

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
