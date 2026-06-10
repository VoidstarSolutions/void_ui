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
//! ```ignore
//! use void_ui::components::input::{currency_input, CurrencyFormat};
//! currency_input(state.amount.clone(), |s: &mut State, text| s.amount = text)
//!     .format(CurrencyFormat::default()) // $1,234.56
//!     .render(&theme)
//! ```

use masonry::core::ArcStr;
use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, FlexExt as _, flex_row};

use super::view::{InputView, field_chrome};
use crate::Theme;
use crate::label;

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
    /// Maximum digits kept after the decimal separator. `0` disables decimals.
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
/// `value` is host-controlled; it is regrouped for display. `on_changed` is
/// invoked on every edit with the value re-filtered to numerals and regrouped
/// per the [`CurrencyFormat`]. Defaults to US-dollar formatting.
pub fn currency_input<F>(value: impl Into<String>, on_changed: F) -> CurrencyInput<F> {
    CurrencyInput {
        value: value.into(),
        format: CurrencyFormat::default(),
        placeholder: ArcStr::default(),
        disabled: false,
        on_changed,
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

        let symbol = label(format.symbol.clone())
            .color(theme.palette.text_muted)
            .render(theme);

        let core = {
            let format = format.clone();
            InputView::new(
                group_currency(&value, &format),
                placeholder,
                disabled,
                theme,
                move |state: &mut State, text: String| {
                    (on_changed)(state, group_currency(&text, &format))
                },
            )
        };

        // The symbol sits inside the border, on the side the format chooses.
        let (prefix, suffix) = if format.symbol_suffix {
            (None, Some(symbol))
        } else {
            (Some(symbol), None)
        };

        // Baseline-align so the currency symbol sits on the digits' line.
        let row = flex_row((prefix, core.flex(1.0), suffix))
            .cross_axis_alignment(CrossAxisAlignment::FirstBaseline)
            .gap(Length::px(f64::from(theme.density.col)));

        field_chrome(row, theme)
    }
}

/// Re-filter `text` to numerals and regroup it per `fmt`: strip everything that
/// isn't a digit, a leading `-`, or the decimal separator (existing group
/// separators included, so the result re-groups cleanly), then insert group
/// separators into the integer part and cap the fraction at `decimal_places`.
fn group_currency(text: &str, fmt: &CurrencyFormat) -> String {
    let mut negative = false;
    let mut int_digits = String::new();
    let mut frac_digits = String::new();
    let mut seen_decimal = false;

    for c in text.chars() {
        if c.is_ascii_digit() {
            if seen_decimal {
                if frac_digits.len() < fmt.decimal_places {
                    frac_digits.push(c);
                }
            } else {
                int_digits.push(c);
            }
        } else if c == fmt.decimal_separator && !seen_decimal {
            // Stop integer accumulation even when decimals are disabled, so the
            // fractional digits are dropped rather than folded into the integer.
            seen_decimal = true;
        } else if c == '-' && int_digits.is_empty() && !seen_decimal && !negative {
            negative = true;
        }
        // Anything else (group separators, stray characters) is ignored.
    }

    // Currency has no insignificant leading zeros (no `$007.00`). Collapse
    // them, but keep a single `0` so a zero amount (`000`) and a bare fraction
    // (`.50` -> `0.50`) still read naturally. An entirely empty value stays
    // empty so the placeholder can show.
    let significant = int_digits.trim_start_matches('0');
    let int_part = if significant.is_empty() && (!int_digits.is_empty() || seen_decimal) {
        "0"
    } else {
        significant
    };

    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&group_integer(int_part, fmt.group_separator));
    if seen_decimal && fmt.decimal_places > 0 {
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
    use super::{CurrencyFormat, group_currency};
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
        assert_eq!(group_currency("1234567", &us()), "1,234,567");
        assert_eq!(group_currency("1234.5", &us()), "1,234.5");
        assert_eq!(group_currency("999", &us()), "999");
    }

    #[test]
    fn strips_existing_separators_and_regroups() {
        assert_eq!(group_currency("1,234,567", &us()), "1,234,567");
        assert_eq!(group_currency("12,34", &us()), "1,234");
    }

    #[test]
    fn filters_non_numeric() {
        assert_eq!(group_currency("a1b2c3", &us()), "123");
        assert_eq!(group_currency("", &us()), "");
    }

    #[test]
    fn caps_fraction_at_decimal_places() {
        assert_eq!(group_currency("1.239", &us()), "1.23");
    }

    #[test]
    fn handles_leading_minus_only() {
        assert_eq!(group_currency("-1234.5", &us()), "-1,234.5");
        assert_eq!(group_currency("12-34", &us()), "1,234");
    }

    #[test]
    fn honors_european_separators() {
        assert_eq!(group_currency("1234567,89", &eu()), "1.234.567,89");
    }

    #[test]
    fn strips_insignificant_leading_zeros() {
        assert_eq!(group_currency("007", &us()), "7");
        assert_eq!(group_currency("00012345", &us()), "12,345");
        assert_eq!(group_currency("012.50", &us()), "12.50");
        // Internal and trailing zeros are significant and kept.
        assert_eq!(group_currency("100", &us()), "100");
        assert_eq!(group_currency("1020", &us()), "1,020");
    }

    #[test]
    fn keeps_a_single_zero_for_zero_amounts() {
        assert_eq!(group_currency("000", &us()), "0");
        assert_eq!(group_currency("0.50", &us()), "0.50");
    }

    #[test]
    fn normalizes_bare_fraction_with_leading_zero() {
        assert_eq!(group_currency(".50", &us()), "0.50");
        assert_eq!(group_currency("-.50", &us()), "-0.50");
    }

    #[test]
    fn empty_input_stays_empty() {
        // Must NOT become "0" — the field needs a true empty state for its
        // placeholder.
        assert_eq!(group_currency("", &us()), "");
        assert_eq!(group_currency("-", &us()), "-");
    }

    #[test]
    fn zero_decimal_places_drops_fraction() {
        let mut fmt = us();
        fmt.decimal_places = 0;
        assert_eq!(group_currency("1234.56", &fmt), "1,234");
    }
}
