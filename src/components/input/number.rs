//! Number input — a single-line field constrained to numeric text, with
//! `-`/`+` stepper buttons that adjust the value.
//!
//! Built on the same editor core and chrome as [`input`](super::input): the
//! editor takes a numeric-filtered change callback, and the steppers parse the
//! current value, apply `step` (clamped to the configured range), and re-emit
//! the formatted string. Everything stays host-controlled — the field never
//! mutates its own value; it emits the new string and the host stores it.
//!
//! The value is carried as a `String` (the editable source of truth), not an
//! `f64`, so partial entries like `12.` don't get reformatted mid-typing. Hosts
//! parse it when they need a number.
//!
//! ```ignore
//! use void_ui::components::input::number_input;
//! number_input(state.qty.clone(), |s: &mut State, text| s.qty = text)
//!     .step(1.0)
//!     .range(0.0, 100.0)
//!     .render(&theme)
//! ```

use std::sync::Arc;

use masonry::core::ArcStr;
use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, FlexExt as _, flex_row};

use super::numeric::scan_number;
use super::view::{InputView, affix_label, affixed_row, field_chrome};
use crate::Theme;
use crate::components::button::button;

/// Builder for a numeric text field with `-`/`+` steppers.
///
/// Created with [`number_input`]. Returns a xilem view via [`Self::render`].
#[must_use = "NumberInput does nothing until rendered with .render(&theme)"]
pub struct NumberInput<F> {
    value: String,
    step: f64,
    min: f64,
    max: f64,
    placeholder: ArcStr,
    disabled: bool,
    prefix: Option<ArcStr>,
    suffix: Option<ArcStr>,
    on_changed: F,
}

/// Create a numeric input with the given value and change callback.
///
/// `value` is host-controlled. `on_changed` is invoked — with non-numeric
/// characters stripped — on every edit, and also by the steppers with the
/// adjusted, formatted value. Defaults: `step` 1.0, unbounded range.
pub fn number_input<F>(value: impl Into<String>, on_changed: F) -> NumberInput<F> {
    NumberInput {
        value: value.into(),
        step: 1.0,
        min: f64::NEG_INFINITY,
        max: f64::INFINITY,
        placeholder: ArcStr::default(),
        disabled: false,
        prefix: None,
        suffix: None,
        on_changed,
    }
}

impl<F> NumberInput<F> {
    /// Set the amount the `-`/`+` steppers add or subtract. Defaults to `1.0`.
    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    /// Clamp **stepper** results to `[min, max]`. Typing is intentionally left
    /// unclamped: clamping each keystroke is hostile — with a `[10, 100]` range,
    /// typing "50" would catch on the leading "5" and snap to "10", so the value
    /// could never be entered. The host validates the final value at its own
    /// commit point; only the steppers enforce the range here. Defaults to
    /// unbounded.
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// Set the placeholder shown while the field is empty.
    pub fn placeholder(mut self, placeholder: impl Into<ArcStr>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Disable the field and its steppers.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set a leading affix shown inside the border (e.g. `$`).
    pub fn prefix(mut self, text: impl Into<ArcStr>) -> Self {
        self.prefix = Some(text.into());
        self
    }

    /// Set a trailing affix shown inside the border, before the steppers (e.g.
    /// a unit).
    pub fn suffix(mut self, text: impl Into<ArcStr>) -> Self {
        self.suffix = Some(text.into());
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
        let NumberInput {
            value,
            step,
            min,
            max,
            placeholder,
            disabled,
            prefix,
            suffix,
            on_changed,
        } = self;

        // One non-`Clone` host callback has to be owned by three independent
        // `'static` closures — the editor's filtered handler and both steppers —
        // so it's shared through an `Arc`. Each of the first two consumers clones
        // a handle; the last (the `+` stepper) moves the original.
        let on_changed = Arc::new(on_changed);

        let core = {
            let on_changed = on_changed.clone();
            // Typed text is only numeric-filtered, never range-clamped (see
            // `range`): clamping mid-keystroke would make many values untypeable.
            InputView::new(
                value.clone(),
                placeholder,
                disabled,
                theme,
                move |state: &mut State, text: String| (*on_changed)(state, filter_numeric(&text)),
            )
        };

        let minus = {
            let on_changed = on_changed.clone();
            let value = value.clone();
            button(move |state: &mut State| (*on_changed)(state, adjust(&value, -step, min, max)))
                .label("\u{2212}")
                .disabled(disabled)
                .render(theme)
        };
        let plus =
            button(move |state: &mut State| (*on_changed)(state, adjust(&value, step, min, max)))
                .label("+")
                .disabled(disabled)
                .render(theme);

        let prefix = prefix.map(|text| affix_label(text, theme));
        let suffix = suffix.map(|text| affix_label(text, theme));

        let steppers = flex_row((minus, plus)).gap(Length::px(4.0));

        // Text affixes + editor share a baseline (affixed_row!); the stepper
        // buttons then center against that text block (a button has no clean
        // text baseline to sit on).
        let text = affixed_row!(prefix, core, suffix, theme);
        let row = flex_row((text.flex(1.0), steppers))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(f64::from(theme.density.gap_lg)));

        field_chrome(row, theme)
    }
}

/// Keep only characters that form a number: digits, a single decimal point, and
/// a single leading minus. Everything else is dropped.
///
/// Reassembles the scanned parts verbatim — no leading-zero collapsing or
/// regrouping — so a partially typed value like `12.` or `.5` survives editing
/// unchanged (unlike the currency formatter, which reformats).
fn filter_numeric(input: &str) -> String {
    // Locale-agnostic: the number field always uses `.` and never caps the
    // fraction.
    let parts = scan_number(input, '.', None);
    let mut out = String::with_capacity(input.len());
    if parts.negative {
        out.push('-');
    }
    out.push_str(&parts.int_digits);
    if parts.saw_decimal {
        out.push('.');
        out.push_str(&parts.frac_digits);
    }
    out
}

/// Parse `value` (empty/garbage reads as `0`), apply `delta`, clamp to
/// `[min, max]`, and format back to the shortest string. Float `Display` omits a
/// trailing `.0`, so whole results render without a decimal.
fn adjust(value: &str, delta: f64, min: f64, max: f64) -> String {
    let current = value.trim().parse::<f64>().unwrap_or(0.0);
    // `f64::clamp` panics if min > max, so normalize a reversed range rather
    // than trust the caller.
    let next = (current + delta).clamp(min.min(max), max.max(min));
    // Strip binary float noise (e.g. 0.1 + 0.2 -> 0.30000000000000004) before
    // display. Ten decimal places is far finer than any realistic stepper.
    let cleaned = (next * 1e10).round() / 1e10;
    format!("{cleaned}")
}

/// Support for [`tests::prefix_does_not_shift_editor_text_vertically`]:
/// builds a [`TestHarness`] from a `number_input` view and inspects where its
/// `TextArea` actually paints text.
#[cfg(test)]
mod text_area_layout {
    use std::sync::Arc;

    use masonry::core::{NewWidget, WidgetRef};
    use masonry::kurbo::Rect;
    use masonry::testing::TestHarness;
    use masonry::widgets::{Flex, TextArea};
    use xilem::core::RawProxy;
    use xilem::{ViewCtx, WidgetView};

    fn find_text_area(
        widget: WidgetRef<'_, dyn masonry::core::Widget>,
    ) -> Option<WidgetRef<'_, TextArea<true>>> {
        if let Some(area) = widget.downcast::<TextArea<true>>() {
            return Some(area);
        }
        widget.children().into_iter().find_map(find_text_area)
    }

    /// Returns `(first_ink_row, last_ink_row)` within `rect` (window
    /// coordinates), relative to `rect.y0`, where "ink" means a pixel
    /// differing from the background sampled just above the rect.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "rect coordinates are small positive layout pixels"
    )]
    fn ink_rows(image: &image::RgbaImage, rect: Rect) -> Option<(u32, u32)> {
        let x0 = rect.x0.round().max(0.0) as u32;
        let x1 = (rect.x1.round().max(0.0) as u32).min(image.width());
        let y0 = rect.y0.round().max(0.0) as u32;
        let y1 = (rect.y1.round().max(0.0) as u32).min(image.height());
        let bg = *image.get_pixel(x0 + 1, y0.saturating_sub(2));

        let mut first = None;
        let mut last = None;
        for y in y0..y1 {
            for x in x0..x1 {
                let px = image.get_pixel(x, y);
                if *px != bg {
                    first.get_or_insert(y - y0);
                    last = Some(y - y0);
                }
            }
        }
        first.zip(last)
    }

    /// Builds `view`, finds its `TextArea`, and returns the area's content
    /// box plus the rows where its text actually paints.
    pub(super) fn measure_text_area(
        proxy: &Arc<dyn RawProxy>,
        runtime: &Arc<tokio::runtime::Runtime>,
        view: &impl WidgetView<()>,
    ) -> (Rect, (u32, u32)) {
        let mut ctx = ViewCtx::new(proxy.clone(), runtime.clone());
        let mut state = ();
        let (pod, _) = view.build(&mut ctx, &mut state);
        let root = Flex::column().with_fixed(pod.new_widget);
        let mut harness =
            TestHarness::create(masonry::theme::default_property_set(), NewWidget::new(root));
        let area = find_text_area(harness.root_widget().as_dyn()).expect("TextArea");
        let rect = area.ctx().content_box();
        let origin = area.ctx().window_transform() * rect.origin();
        let window_rect = Rect::from_origin_size(origin, rect.size());
        let image = harness.render();
        let ink = ink_rows(&image, window_rect).expect("text should paint ink");
        (rect, ink)
    }
}

#[cfg(test)]
mod tests {
    use super::{adjust, filter_numeric};

    const UNBOUNDED: (f64, f64) = (f64::NEG_INFINITY, f64::INFINITY);

    #[test]
    fn filter_keeps_digits_one_dot_and_leading_minus() {
        assert_eq!(filter_numeric("12a3"), "123");
        assert_eq!(filter_numeric("1.2.3"), "1.23");
        assert_eq!(filter_numeric(".5"), ".5");
        assert_eq!(filter_numeric("-5"), "-5");
        assert_eq!(filter_numeric("5-3"), "53");
        assert_eq!(filter_numeric("$1,250.00"), "1250.00");
        assert_eq!(filter_numeric("abc"), "");
    }

    #[test]
    fn filter_numeric_is_idempotent() {
        // Re-filtering already-filtered text must be a no-op. This is the
        // property that keeps a mid-edit value from drifting as the host stores
        // it and passes it back, and it guards the shared scan kernel against a
        // change that would reformat (rather than merely filter) typed text.
        for input in [
            "12a3",
            "1.2.3",
            "-.5",
            "--5",
            ".-5",
            "$1,250.00",
            "12.",
            ".",
            "-",
            "abc",
            "0.000",
        ] {
            let once = filter_numeric(input);
            assert_eq!(filter_numeric(&once), once, "not idempotent for {input:?}");
        }
    }

    #[test]
    fn adjust_reads_empty_and_garbage_as_zero() {
        let (lo, hi) = UNBOUNDED;
        assert_eq!(adjust("5", 1.0, lo, hi), "6");
        assert_eq!(adjust("", 1.0, lo, hi), "1");
        assert_eq!(adjust("garbage", 1.0, lo, hi), "1");
    }

    #[test]
    fn adjust_formats_whole_results_without_decimal() {
        let (lo, hi) = UNBOUNDED;
        assert_eq!(adjust("9.5", 0.5, lo, hi), "10");
    }

    #[test]
    fn adjust_strips_float_noise() {
        let (lo, hi) = UNBOUNDED;
        // 0.2 + 0.1 is 0.30000000000000004 in f64; it must display as "0.3".
        assert_eq!(adjust("0.2", 0.1, lo, hi), "0.3");
    }

    #[test]
    fn adjust_clamps_to_range() {
        assert_eq!(adjust("100", 5.0, 0.0, 100.0), "100");
        assert_eq!(adjust("0", -5.0, 0.0, 100.0), "0");
    }

    #[test]
    fn adjust_tolerates_reversed_range() {
        // min > max must not panic.
        assert_eq!(adjust("50", 10.0, 100.0, 0.0), "60");
    }

    /// Renders a number input with no affixes and one with a `prefix`, then
    /// finds where the editor's text glyphs actually paint (the rows that
    /// differ from the field background) relative to the `TextArea`'s box.
    /// A `prefix` must not shift the editor text vertically within its
    /// field.
    #[test]
    fn prefix_does_not_shift_editor_text_vertically() {
        use crate::Theme;
        use crate::components::input::number::number_input;
        use crate::components::input::number::text_area_layout::measure_text_area;
        use crate::test_support;

        let runtime = test_support::current_thread_runtime();
        let proxy = test_support::noop_proxy();
        let theme = Theme::default();

        // Quantity-like field: no prefix/suffix.
        let qty_view = number_input("5", |(): &mut (), _text| ())
            .range(0.0, 100.0)
            .render(&theme);
        let (qty_rect, qty_ink) = measure_text_area(&proxy, &runtime, &qty_view);

        // Price-like field: "$" prefix.
        let price_view = number_input("9.5", |(): &mut (), _text| ())
            .prefix("$")
            .step(0.5)
            .render(&theme);
        let (price_rect, price_ink) = measure_text_area(&proxy, &runtime, &price_view);

        #[expect(
            clippy::float_cmp,
            reason = "both rects come from the same theme/density, so heights should be bit-identical"
        )]
        let same_height = qty_rect.height() == price_rect.height();
        assert!(
            same_height,
            "field heights differ: {qty_rect:?} vs {price_rect:?}"
        );
        assert_eq!(qty_ink, price_ink);
    }
}
