//! Character classes shared by the parsers.
//!
//! These mirror the Foundation character sets the original Swift implementation
//! used, so imported files keep parsing exactly as they did there.

/// `CharacterSet.newlines`: every character that ends a line in a lyrics file.
pub(crate) fn is_newline(c: char) -> bool {
    matches!(
        c,
        '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// `CharacterSet.whitespaces`: horizontal tab plus Unicode space separators.
pub(crate) fn is_inline_space(c: char) -> bool {
    matches!(
        c,
        '\t' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

/// Inline whitespace plus the byte-order mark some editors put at the start of a file.
pub(crate) fn is_line_padding(c: char) -> bool {
    is_inline_space(c) || c == '\u{FEFF}'
}

pub(crate) fn trim_padding(value: &str) -> &str {
    value.trim_matches(is_line_padding)
}

pub(crate) fn trim_inline(value: &str) -> &str {
    value.trim_matches(is_inline_space)
}

pub(crate) fn split_newlines(value: &str) -> impl Iterator<Item = &str> {
    value.split(is_newline)
}

/// Length of `value` in UTF-16 code units, the unit text fields count carets in.
pub(crate) fn utf16_len(value: &str) -> usize {
    value.chars().map(char::len_utf16).sum()
}

/// Byte offset of the UTF-16 position `index`, or `None` when it lies outside the
/// string or between the two halves of a surrogate pair.
pub(crate) fn utf16_to_byte(value: &str, index: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, c) in value.char_indices() {
        if units == index {
            return Some(byte);
        }
        if units > index {
            return None;
        }
        units += c.len_utf16();
    }
    (units == index).then_some(value.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_follow_surrogate_pairs() {
        let value = "a🌙b";
        assert_eq!(utf16_len(value), 4);
        assert_eq!(utf16_to_byte(value, 0), Some(0));
        assert_eq!(utf16_to_byte(value, 1), Some(1));
        assert_eq!(utf16_to_byte(value, 2), None, "inside the surrogate pair");
        assert_eq!(utf16_to_byte(value, 3), Some(5));
        assert_eq!(utf16_to_byte(value, 4), Some(6));
        assert_eq!(utf16_to_byte(value, 5), None);
    }

    #[test]
    fn newlines_split_like_foundation() {
        let parts: Vec<_> = split_newlines("a\r\nb\u{2028}c").collect();
        assert_eq!(parts, ["a", "", "b", "c"]);
    }
}
