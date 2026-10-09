use unicode_normalization::UnicodeNormalization;

pub fn normalize(text: &str) -> String {
    text.nfkc()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' => '\'',
            _ => c,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Consumed by both this crate and scripts/test_prepare.py, so any divergence
    // between the builder's and the runtime's normalization fails on both sides.
    #[test]
    fn shared_cross_language_cases() {
        let cases: Vec<(String, String)> =
            serde_json::from_str(include_str!("../tests/fixtures/normalize.json")).unwrap();
        for (input, expected) in cases {
            assert_eq!(normalize(&input), expected, "input {input:?}");
        }
    }
}
