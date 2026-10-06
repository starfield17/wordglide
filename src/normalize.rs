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
    #[test]
    fn same_normalization_as_data_builder() {
        assert_eq!(normalize("  ＴＡＫＥ\tOff  "), "take off");
        assert_eq!(normalize("DON’T"), "don't");
        assert_ne!(normalize("résumé"), normalize("resume"));
    }
}
