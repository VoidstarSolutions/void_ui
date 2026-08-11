//! Currency input — a numeric field that groups digits with a thousands
//! separator and shows a currency symbol, formatted live as the user types.
//!
//! Internationalization is **consumer-injected** via [`CurrencyFormat`] (symbol,
//! symbol position, group/decimal separators, decimal places). void-ui bundles
//! no locale data — the consuming app owns locale, keeping this component
//! presentation-only and product-agnostic.
//!
//! Like the rest of the input family the value is host-controlled and carried as
//! a `String`: the field emits the regrouped string on every edit (the host
//! stores it and passes it back). The host strips the group separator it
//! supplied to recover a parseable number.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State { amount: String }
//! # let state = State { amount: String::from("1234.56") };
//! use void_ui::components::input::{currency_input, CurrencyFormat};
//! currency_input(state.amount.clone(), |s: &mut State, text| s.amount = text)
//!     .format(CurrencyFormat::default()) // $1,234.56
//!     .render(&theme)
//! # ;
//! ```

use masonry::core::ArcStr;
use xilem::WidgetView;

use super::numeric::{NumberParts, scan_number};
use super::view::{InputView, affix_label, affixed_row, field_chrome};
use crate::Theme;

/// How a [`currency_input`] formats its value. Defaults to US dollars
/// (`$1,234.56`); supply your own for other locales.
#[derive(Debug, Clone)]
pub struct CurrencyFormat {
    /// Currency symbol, e.g. `$`, `€`, `£`.
    pub symbol: ArcStr,
    /// Place the symbol after the amount (e.g. `1.234,56 €`) instead of before.
    pub symbol_suffix: bool,
    /// Thousands-group separator, e.g. `,` (US) or `.` (EU).
    pub group_separator: char,
    /// Decimal separator, e.g. `.` (US) or `,` (EU).
    pub decimal_separator: char,
    /// Maximum digits kept after the decimal separator. `0` disables decimals:
    /// a typed separator and everything after it is dropped (¥-style, so
    /// `12.34` -> `12`) rather than folded into the integer part.
    pub decimal_places: usize,
}

impl Default for CurrencyFormat {
    fn default() -> Self {
        Self {
            symbol: ArcStr::from("$"),
            symbol_suffix: false,
            group_separator: ',',
            decimal_separator: '.',
            decimal_places: 2,
        }
    }
}

/// Builder for a currency text field. Created with [`currency_input`].
///
/// Unlike [`Input`](super::Input) and [`NumberInput`](super::NumberInput), this
/// exposes no `prefix`/`suffix`: the affix slot is reserved for the currency
/// symbol, whose side is chosen by [`CurrencyFormat::symbol_suffix`].
#[must_use = "CurrencyInput does nothing until rendered with .render(&theme)"]
pub struct CurrencyInput<F> {
    value: String,
    format: CurrencyFormat,
    placeholder: ArcStr,
    disabled: bool,
    on_changed: F,
}

/// Create a currency input with the given value and change callback.
///
/// `value` is host-controlled; it is regrouped for display. `on_change` is
/// invoked on every edit with the value re-filtered to numerals and regrouped
/// per the [`CurrencyFormat`]. Defaults to US-dollar formatting.
pub fn currency_input<F>(value: impl Into<String>, on_change: F) -> CurrencyInput<F> {
    CurrencyInput {
        value: value.into(),
        format: CurrencyFormat::default(),
        placeholder: ArcStr::default(),
        disabled: false,
        on_changed: on_change,
    }
}

impl<F> CurrencyInput<F> {
    /// Replace the formatting (symbol, separators, decimal places).
    pub fn format(mut self, format: CurrencyFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the placeholder shown while the field is empty.
    pub fn placeholder(mut self, placeholder: impl Into<ArcStr>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Disable the field.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action> + use<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
    {
        let CurrencyInput {
            value,
            format,
            placeholder,
            disabled,
            on_changed,
        } = self;

        let symbol = affix_label(format.symbol.clone(), theme);

        let core = {
            let format = format.clone();
            InputView::new(
                format_currency(&value, &format),
                placeholder,
                disabled,
                theme,
                move |state: &mut State, text: String| {
                    (on_changed)(state, format_currency(&text, &format))
                },
            )
        };

        // The symbol sits inside the border, on the side the format chooses.
        let (prefix, suffix) = if format.symbol_suffix {
            (None, Some(symbol))
        } else {
            (Some(symbol), None)
        };

        field_chrome(affixed_row!(prefix, core, suffix, theme), theme)
    }
}

/// Format a raw currency `value` into its grouped display string per `fmt`:
/// strip everything that isn't a digit, a leading `-`, or the decimal separator
/// (any existing group separators included, so it re-groups cleanly), then
/// insert group separators into the integer part and cap the fraction at
/// `decimal_places`. The currency analogue of [`format_mask`](super::format_mask)
/// — call it to render a host-stored value read-only elsewhere (e.g. a summary).
///
/// `format_currency("1250000", &CurrencyFormat::default())` -> `"1,250,000"`.
#[must_use]
pub fn format_currency(value: &str, fmt: &CurrencyFormat) -> String {
    // Shared scan kernel: split into sign/integer/fraction, honoring this
    // locale's decimal separator and capping the fraction at `decimal_places`.
    let NumberParts {
        negative,
        int_digits,
        frac_digits,
        saw_decimal,
    } = scan_number(value, fmt.decimal_separator, Some(fmt.decimal_places));

    // No digits typed (a lone `-`, `.`, or `-.`): keep it minimal so the
    // placeholder still shows, rather than manufacturing `0.` for a stray
    // decimal key.
    if int_digits.is_empty() && frac_digits.is_empty() {
        return if negative {
            "-".to_owned()
        } else {
            String::new()
        };
    }

    // Currency has no insignificant leading zeros (no `$007.00`). Collapse them,
    // keeping a single `0` so a zero amount (`000`) and a bare fraction
    // (`.50` -> `0.50`) still read naturally.
    let significant = int_digits.trim_start_matches('0');
    let int_part = if significant.is_empty() {
        "0"
    } else {
        significant
    };

    let mut out = String::new();
    // A leading `-` is only meaningful with a non-zero magnitude — no `-0`.
    let is_zero = int_part == "0" && frac_digits.chars().all(|c| c == '0');
    if negative && !is_zero {
        out.push('-');
    }
    out.push_str(&group_integer(int_part, fmt.group_separator));
    if saw_decimal && fmt.decimal_places > 0 {
        out.push(fmt.decimal_separator);
        out.push_str(&frac_digits);
    }
    out
}

/// Insert `separator` between every group of three digits, counting from the
/// right. `"1234567"` -> `"1,234,567"`.
fn group_integer(digits: &str, separator: char) -> String {
    // Build right-to-left so groups of three fall out of a simple counter
    // (no modulo), then reverse back.
    let mut reversed = String::with_capacity(digits.len() + digits.len() / 3);
    let mut in_group = 0;
    for c in digits.chars().rev() {
        if in_group == 3 {
            reversed.push(separator);
            in_group = 0;
        }
        reversed.push(c);
        in_group += 1;
    }
    reversed.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::{CurrencyFormat, format_currency};
    use masonry::core::ArcStr;

    fn us() -> CurrencyFormat {
        CurrencyFormat::default()
    }

    fn eu() -> CurrencyFormat {
        CurrencyFormat {
            symbol: ArcStr::from("\u{20ac}"),
            symbol_suffix: true,
            group_separator: '.',
            decimal_separator: ',',
            decimal_places: 2,
        }
    }

    #[test]
    fn groups_thousands() {
        assert_eq!(format_currency("1234567", &us()), "1,234,567");
        assert_eq!(format_currency("1234.5", &us()), "1,234.5");
        assert_eq!(format_currency("999", &us()), "999");
    }

    #[test]
    fn strips_existing_separators_and_regroups() {
        assert_eq!(format_currency("1,234,567", &us()), "1,234,567");
        assert_eq!(format_currency("12,34", &us()), "1,234");
    }

    #[test]
    fn filters_non_numeric() {
        assert_eq!(format_currency("a1b2c3", &us()), "123");
        assert_eq!(format_currency("", &us()), "");
    }

    #[test]
    fn caps_fraction_at_decimal_places() {
        assert_eq!(format_currency("1.239", &us()), "1.23");
    }

    #[test]
    fn handles_leading_minus_only() {
        assert_eq!(format_currency("-1234.5", &us()), "-1,234.5");
        assert_eq!(format_currency("12-34", &us()), "1,234");
    }

    #[test]
    fn honors_european_separators() {
        assert_eq!(format_currency("1234567,89", &eu()), "1.234.567,89");
    }

    #[test]
    fn strips_insignificant_leading_zeros() {
        assert_eq!(format_currency("007", &us()), "7");
        assert_eq!(format_currency("00012345", &us()), "12,345");
        assert_eq!(format_currency("012.50", &us()), "12.50");
        // Internal and trailing zeros are significant and kept.
        assert_eq!(format_currency("100", &us()), "100");
        assert_eq!(format_currency("1020", &us()), "1,020");
    }

    #[test]
    fn keeps_a_single_zero_for_zero_amounts() {
        assert_eq!(format_currency("000", &us()), "0");
        assert_eq!(format_currency("0.50", &us()), "0.50");
    }

    #[test]
    fn normalizes_bare_fraction_with_leading_zero() {
        assert_eq!(format_currency(".50", &us()), "0.50");
        assert_eq!(format_currency("-.50", &us()), "-0.50");
    }

    #[test]
    fn empty_input_stays_empty() {
        // Must NOT become "0" — the field needs a true empty state for its
        // placeholder.
        assert_eq!(format_currency("", &us()), "");
        assert_eq!(format_currency("-", &us()), "-");
    }

    #[test]
    fn lone_decimal_point_stays_minimal() {
        // A stray "." (or "-.") carries no digits, so it must not manufacture
        // "0." and suppress the placeholder.
        assert_eq!(format_currency(".", &us()), "");
        assert_eq!(format_currency("-.", &us()), "-");
    }

    #[test]
    fn drops_meaningless_negative_zero() {
        // "negative zero" is meaningless — drop the sign when the magnitude is 0.
        assert_eq!(format_currency("-0", &us()), "0");
        assert_eq!(format_currency("-0.00", &us()), "0.00");
        assert_eq!(format_currency("-000", &us()), "0");
        // A genuinely non-zero negative keeps its sign.
        assert_eq!(format_currency("-0.50", &us()), "-0.50");
    }

    #[test]
    fn format_currency_is_idempotent() {
        // The grouped display is a fixed point: re-formatting an already
        // formatted value must reproduce it exactly, so re-rendering host state
        // never drifts and the shared scan kernel keeps round-tripping its own
        // separators (group vs decimal) correctly across locales.
        for fmt in [us(), eu()] {
            for input in [
                "1234567",
                "1234.5",
                "-0.50",
                ".50",
                "007",
                "1,234,567",
                "0.00",
                "-0",
                "12.",
            ] {
                let once = format_currency(input, &fmt);
                assert_eq!(
                    format_currency(&once, &fmt),
                    once,
                    "not idempotent for {input:?}"
                );
            }
        }
    }

    #[test]
    fn zero_decimal_places_drops_fraction() {
        let mut fmt = us();
        fmt.decimal_places = 0;
        assert_eq!(format_currency("1234.56", &fmt), "1,234");
        // A typed separator and the digits after it are dropped, not folded into
        // the integer (so "12.34" is 12, never 1234).
        assert_eq!(format_currency("12.34", &fmt), "12");
        assert_eq!(format_currency("12.", &fmt), "12");
        // A lone fractional entry has no integer part to keep, so it's empty.
        assert_eq!(format_currency(".5", &fmt), "");
    }
}
