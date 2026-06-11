//! The shared numeric-scan kernel behind the input family's number-aware fields.
//!
//! Both the [`number`](super::number) field's edit filter and the
//! [`currency`](super::currency) formatter need to read a free-form string into
//! the same shape — a sign, the integer digits, and the fraction digits —
//! tolerating group separators, stray characters, and a misplaced sign. They
//! diverge only in what they do afterward: the number field reassembles a raw
//! editable string (deliberately *not* reformatting mid-typing), while currency
//! collapses leading zeros, suppresses negative-zero, and groups thousands.
//! [`scan_number`] is that common first step.

/// The digit/sign decomposition of a scanned numeric string.
pub(super) struct NumberParts {
    /// A leading `-` was seen before any digit or decimal separator.
    pub negative: bool,
    /// Integer-side digits, in order, with group separators and noise removed.
    pub int_digits: String,
    /// Fraction-side digits (after the decimal separator), capped per the scan.
    pub frac_digits: String,
    /// The decimal separator was seen — distinguishes `"12"` from `"12."`, which
    /// matters for both the raw editable form and whether currency emits a
    /// decimal at all.
    pub saw_decimal: bool,
}

/// Scan `input` into sign / integer-digit / fraction-digit [`NumberParts`],
/// treating `decimal_sep` as the single decimal separator and capping the
/// fraction at `frac_cap` digits (`None` = unbounded).
///
/// Everything that isn't a digit, the first `decimal_sep`, or a leading `-` is
/// ignored — so group separators (`,`/`.`), currency symbols, and a sign that
/// arrives after digits all fall away. The first `decimal_sep` ends integer
/// accumulation even when `frac_cap` is `Some(0)`, so trailing digits are
/// dropped rather than folded into the integer part.
pub(super) fn scan_number(input: &str, decimal_sep: char, frac_cap: Option<usize>) -> NumberParts {
    let mut negative = false;
    let mut int_digits = String::new();
    let mut frac_digits = String::new();
    let mut saw_decimal = false;

    for c in input.chars() {
        if c.is_ascii_digit() {
            if saw_decimal {
                if frac_cap.is_none_or(|cap| frac_digits.len() < cap) {
                    frac_digits.push(c);
                }
            } else {
                int_digits.push(c);
            }
        } else if c == decimal_sep && !saw_decimal {
            saw_decimal = true;
        } else if c == '-' && int_digits.is_empty() && !saw_decimal && !negative {
            // A minus is only meaningful as the leading sign.
            negative = true;
        }
        // Anything else (group separators, stray characters) is ignored.
    }

    NumberParts {
        negative,
        int_digits,
        frac_digits,
        saw_decimal,
    }
}

#[cfg(test)]
mod tests {
    use super::scan_number;

    #[test]
    fn splits_sign_integer_and_fraction() {
        let p = scan_number("-1,234.56", '.', None);
        assert!(p.negative);
        assert_eq!(p.int_digits, "1234");
        assert_eq!(p.frac_digits, "56");
        assert!(p.saw_decimal);
    }

    #[test]
    fn ignores_non_leading_sign_and_stray_chars() {
        let p = scan_number("$1a2-3", '.', None);
        assert!(!p.negative);
        assert_eq!(p.int_digits, "123");
        assert!(!p.saw_decimal);
    }

    #[test]
    fn only_the_first_separator_counts() {
        let p = scan_number("1.2.3", '.', None);
        assert_eq!(p.int_digits, "1");
        assert_eq!(p.frac_digits, "23");
    }

    #[test]
    fn caps_the_fraction() {
        let p = scan_number("1.239", '.', Some(2));
        assert_eq!(p.frac_digits, "23");
    }

    #[test]
    fn zero_cap_stops_integer_but_keeps_no_fraction() {
        // The separator still ends the integer, so digits after it vanish rather
        // than joining the integer part.
        let p = scan_number("12.34", '.', Some(0));
        assert_eq!(p.int_digits, "12");
        assert_eq!(p.frac_digits, "");
        assert!(p.saw_decimal);
    }

    #[test]
    fn honors_a_custom_separator() {
        let p = scan_number("1.234,56", ',', Some(2));
        assert_eq!(p.int_digits, "1234");
        assert_eq!(p.frac_digits, "56");
    }

    #[test]
    fn lone_separator_and_lone_sign() {
        let dot = scan_number(".", '.', None);
        assert!(dot.saw_decimal && dot.int_digits.is_empty() && dot.frac_digits.is_empty());
        let minus = scan_number("-", '.', None);
        assert!(minus.negative && !minus.saw_decimal && minus.int_digits.is_empty());
    }
}
