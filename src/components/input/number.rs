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

use super::view::{InputView, field_chrome};
use crate::Theme;
use crate::components::button::button;
use crate::label;

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

    /// Clamp stepper results to `[min, max]`. Typing is not clamped; only the
    /// steppers respect the range. Defaults to unbounded.
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

        // The single host callback is shared between the editor's filtered
        // change handler and both steppers.
        let on_changed = Arc::new(on_changed);

        let core = {
            let on_changed = on_changed.clone();
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
        let plus = {
            let on_changed = on_changed.clone();
            let value = value.clone();
            button(move |state: &mut State| (*on_changed)(state, adjust(&value, step, min, max)))
                .label("+")
                .disabled(disabled)
                .render(theme)
        };

        let prefix = prefix.map(|text| label(text).color(theme.palette.text_muted).render(theme));
        let suffix = suffix.map(|text| label(text).color(theme.palette.text_muted).render(theme));

        let steppers = flex_row((minus, plus)).gap(Length::px(4.0));

        // Text affixes + editor align on a shared baseline; the stepper buttons
        // then center against that text block (a button has no clean text
        // baseline to sit on).
        let text = flex_row((prefix, core.flex(1.0), suffix))
            .cross_axis_alignment(CrossAxisAlignment::FirstBaseline)
            .gap(Length::px(f64::from(theme.density.col)));
        let row = flex_row((text.flex(1.0), steppers))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(f64::from(theme.density.col)));

        field_chrome(row, theme)
    }
}

/// Keep only characters that form a number: digits, a single decimal point, and
/// a single leading minus. Everything else is dropped.
fn filter_numeric(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut seen_dot = false;
    for c in input.chars() {
        match c {
            '0'..='9' => out.push(c),
            '.' if !seen_dot => {
                seen_dot = true;
                out.push('.');
            }
            // A minus is only meaningful as the leading sign.
            '-' if out.is_empty() => out.push('-'),
            _ => {}
        }
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

    /// Diagnostic: render a number input with no affixes vs one with a
    /// `prefix`, and find where the editor's text glyphs actually paint
    /// (first non-background row) relative to the `TextArea`'s box, to
    /// compare vertical centering of the text within the field.
    #[test]
    fn dump_text_area_rects_with_and_without_prefix() {
        use std::fmt;
        use std::sync::Arc;

        use masonry::core::{NewWidget, WidgetRef};
        use masonry::kurbo::Rect;
        use masonry::testing::TestHarness;
        use masonry::widgets::{Flex, TextArea};
        use xilem::ViewCtx;
        use xilem::core::{ProxyError, RawProxy, SendMessage, View, ViewId};

        use crate::Theme;
        use crate::components::input::number::number_input;

        struct NoopProxy;
        impl fmt::Debug for NoopProxy {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "NoopProxy")
            }
        }
        impl RawProxy for NoopProxy {
            fn send_message(
                &self,
                _path: Arc<[ViewId]>,
                _message: SendMessage,
            ) -> Result<(), ProxyError> {
                Ok(())
            }
            fn dyn_debug(&self) -> &dyn fmt::Debug {
                self
            }
        }

        fn find_text_area<'a>(
            widget: WidgetRef<'a, dyn masonry::core::Widget>,
        ) -> Option<WidgetRef<'a, TextArea<true>>> {
            if let Some(area) = widget.downcast::<TextArea<true>>() {
                return Some(area);
            }
            widget.children().into_iter().find_map(find_text_area)
        }

        /// Returns `(first_ink_row, last_ink_row)` within `rect` (window
        /// coordinates), relative to `rect.y0`, where "ink" means a pixel
        /// differing from the background sampled just above the rect.
        fn ink_rows(image: &image::RgbaImage, rect: Rect) -> Option<(i64, i64)> {
            let x0 = rect.x0.round() as i64;
            let x1 = rect.x1.round() as i64;
            let y0 = rect.y0.round() as i64;
            let y1 = rect.y1.round() as i64;
            let bg = *image.get_pixel(x0.max(0) as u32 + 1, (y0 - 2).max(0) as u32);

            let mut first = None;
            let mut last = None;
            for y in y0..y1 {
                for x in x0..x1 {
                    if x < 0 || y < 0 || x >= i64::from(image.width()) || y >= i64::from(image.height())
                    {
                        continue;
                    }
                    let px = image.get_pixel(x as u32, y as u32);
                    if *px != bg {
                        first.get_or_insert(y - y0);
                        last = Some(y - y0);
                    }
                }
            }
            first.zip(last)
        }

        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap(),
        );
        let proxy: Arc<dyn RawProxy> = Arc::new(NoopProxy);
        let theme = Theme::default();

        // Quantity-like field: no prefix/suffix.
        let mut ctx = ViewCtx::new(proxy.clone(), runtime.clone());
        let mut state = ();
        let view = number_input("5", |_: &mut (), _text| ())
            .range(0.0, 100.0)
            .render(&theme);
        let (pod, _) = view.build(&mut ctx, &mut state);
        let root = Flex::column().with_fixed(pod.new_widget);
        let mut harness =
            TestHarness::create(masonry::theme::default_property_set(), NewWidget::new(root));
        let qty_area = find_text_area(harness.root_widget().as_dyn()).expect("TextArea");
        let qty_rect = qty_area.ctx().content_box();
        let qty_origin = qty_area.ctx().window_transform() * qty_rect.origin();
        let qty_window_rect = Rect::from_origin_size(qty_origin, qty_rect.size());
        let qty_image = harness.render();
        let qty_ink = ink_rows(&qty_image, qty_window_rect);

        // Price-like field: "$" prefix.
        let mut ctx2 = ViewCtx::new(proxy.clone(), runtime.clone());
        let mut state2 = ();
        let view2 = number_input("9.5", |_: &mut (), _text| ())
            .prefix("$")
            .step(0.5)
            .render(&theme);
        let (pod2, _) = view2.build(&mut ctx2, &mut state2);
        let root2 = Flex::column().with_fixed(pod2.new_widget);
        let mut harness2 =
            TestHarness::create(masonry::theme::default_property_set(), NewWidget::new(root2));
        let price_area = find_text_area(harness2.root_widget().as_dyn()).expect("TextArea");
        let price_rect = price_area.ctx().content_box();
        let price_origin = price_area.ctx().window_transform() * price_rect.origin();
        let price_window_rect = Rect::from_origin_size(price_origin, price_rect.size());
        let price_image = harness2.render();
        let price_ink = ink_rows(&price_image, price_window_rect);

        panic!(
            "qty: box_height={:.1} ink_rows={qty_ink:?}\n\
             price: box_height={:.1} ink_rows={price_ink:?}",
            qty_rect.height(),
            price_rect.height(),
        );
    }
}
