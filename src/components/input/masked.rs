//! Masked input — a fixed template/pattern field, e.g. a phone number
//! `(###)-###-####`.
//!
//! Each `#` in the mask is a digit slot; every other character is a literal
//! inserted automatically. The user types digits and sees them formatted into
//! the template, the same filter-and-format approach as [`currency_input`] —
//! just a fixed template instead of grouping. There is no editor-level
//! obscuring, so unlike a password field this needs no upstream support.
//!
//! Per the host-controlled model the field's value is the **raw** digit string
//! (the "unmask value"): the host stores `"1234430989"`, the field formats it to
//! `"(123)-443-0989"` for display, and the change callback emits the raw digits.
//! Use [`format_mask`] to render the masked string anywhere else (e.g. a
//! read-only summary).
//!
//! ```ignore
//! use void_ui::components::input::masked_input;
//! masked_input(state.phone.clone(), |s: &mut State, raw| s.phone = raw)
//!     .mask("(###)-###-####")
//!     .render(&theme)
//! ```

use masonry::core::ArcStr;
use xilem::WidgetView;

use super::view::{InputView, affixed_row, field_chrome};
use crate::Theme;

/// The digit-slot token in a mask; every other character is a literal.
const SLOT: char = '#';

/// Builder for a masked (template) text field. Created with [`masked_input`].
///
/// Unlike [`Input`](super::Input) and [`NumberInput`](super::NumberInput), this
/// exposes no `prefix`/`suffix`: the mask template defines the field's full
/// visible structure, so the affix slot is intentionally unused.
#[must_use = "MaskedInput does nothing until rendered with .render(&theme)"]
pub struct MaskedInput<F> {
    raw: String,
    mask: String,
    placeholder: ArcStr,
    disabled: bool,
    on_changed: F,
}

/// Create a masked input over the raw digit string `raw`.
///
/// `raw` is host-controlled and holds only the significant digits; the field
/// formats it through the [`mask`](MaskedInput::mask) for display. `on_changed`
/// receives the raw digits (mask literals stripped) on every edit.
pub fn masked_input<F>(raw: impl Into<String>, on_changed: F) -> MaskedInput<F> {
    MaskedInput {
        raw: raw.into(),
        mask: String::new(),
        placeholder: ArcStr::default(),
        disabled: false,
        on_changed,
    }
}

impl<F> MaskedInput<F> {
    /// Set the template, e.g. `"(###)-###-####"`. `#` marks a digit slot; other
    /// characters are literals. An empty mask passes digits through unformatted.
    pub fn mask(mut self, mask: impl Into<String>) -> Self {
        self.mask = mask.into();
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
        let MaskedInput {
            raw,
            mask,
            placeholder,
            disabled,
            on_changed,
        } = self;

        let core = {
            let mask = mask.clone();
            InputView::new(
                format_mask(&raw, &mask),
                placeholder,
                disabled,
                theme,
                move |state: &mut State, text: String| {
                    (on_changed)(state, extract_digits(&text, &mask))
                },
            )
        };

        // No affixes — the mask template defines the structure; `()` fills the
        // affix slots so masked shares the one baseline-aligned row helper.
        field_chrome(affixed_row!((), core, (), theme), theme)
    }
}

/// Format raw digits into `mask`: fill each `#` with the next digit and emit
/// literals between filled slots. Excess digits and non-digits in `raw` are
/// dropped. A mask without a `#` slot returns the digits unformatted.
///
/// `format_mask("1234430989", "(###)-###-####")` -> `"(123)-443-0989"`.
#[must_use]
pub fn format_mask(raw: &str, mask: &str) -> String {
    let digits: Vec<char> = raw.chars().filter(char::is_ascii_digit).collect();
    if !mask.contains(SLOT) {
        return digits.into_iter().collect();
    }

    let mut out = String::new();
    let mut next = 0;
    for m in mask.chars() {
        if m == SLOT {
            if next < digits.len() {
                out.push(digits[next]);
                next += 1;
            } else {
                break;
            }
        } else if next < digits.len() {
            // A literal is emitted only while there are still digits to place,
            // so a separator appears as you type into the slot after it rather
            // than dangling at the end.
            out.push(m);
        } else {
            break;
        }
    }
    out
}

/// Recover the raw digits from field text, capped at the number of `#` slots in
/// `mask`. A mask without slots imposes no cap.
fn extract_digits(text: &str, mask: &str) -> String {
    let digits = text.chars().filter(char::is_ascii_digit);
    let capacity = mask.chars().filter(|&c| c == SLOT).count();
    if capacity == 0 {
        digits.collect()
    } else {
        digits.take(capacity).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_digits, format_mask};

    const PHONE: &str = "(###)-###-####";

    #[test]
    fn formats_full_value() {
        assert_eq!(format_mask("1234430989", PHONE), "(123)-443-0989");
    }

    #[test]
    fn formats_partial_value() {
        assert_eq!(format_mask("123", PHONE), "(123");
        assert_eq!(format_mask("1234", PHONE), "(123)-4");
    }

    #[test]
    fn drops_excess_digits_and_non_digits() {
        assert_eq!(format_mask("12344309890000", PHONE), "(123)-443-0989");
        assert_eq!(format_mask("abc123", PHONE), "(123");
    }

    #[test]
    fn empty_mask_is_identity() {
        assert_eq!(format_mask("123", ""), "123");
        assert_eq!(format_mask("12a3", ""), "123");
    }

    #[test]
    fn extracts_raw_digits() {
        assert_eq!(extract_digits("(123)-443-0989", PHONE), "1234430989");
        assert_eq!(extract_digits("(123)-4", PHONE), "1234");
    }

    #[test]
    fn extract_caps_at_capacity() {
        assert_eq!(extract_digits("12344309890000", PHONE), "1234430989");
        assert_eq!(extract_digits("12a3", ""), "123");
    }
}
