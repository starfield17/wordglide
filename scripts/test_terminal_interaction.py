import unittest

from terminal_interaction_smoke import screen_text


class TerminalScreenTests(unittest.TestCase):
    def test_differential_output_preserves_unchanged_cells(self):
        output = b"\x1b[1;1HSearch actions\x1b[1;1HMouse c\x1b[1;9Hp\x1b[1;11Hure"
        self.assertIn("Mouse capture", screen_text(output))

    def test_style_and_clear_do_not_become_content(self):
        output = b"old\x1b[2J\x1b[2;1H\x1b[31mReading layout: focus\x1b[0m"
        text = screen_text(output)
        self.assertNotIn("old", text)
        self.assertIn("Reading layout: focus", text)

    def test_wide_and_combining_characters_keep_terminal_columns(self):
        output = "\x1b[1;1H界é xyz\x1b[1;5Hok".encode()
        self.assertIn("界é okz", screen_text(output))


if __name__ == "__main__":
    unittest.main()
