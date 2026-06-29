//! Masonry widgets for the autocomplete component.
//!
//! Two widgets live here:
//!
//! - [`SuggestionList`] — item-list overlay, closely mirrors `MenuContent` from
//!   the dropdown button. Handles hover, keyboard highlight painting, and fires
//!   [`SuggestionSelected`] on click.
//! - [`AutocompleteWidget`] — composite host: an [`AnchoredOverlay`] whose
//!   primary is a chromed `InputFrame(TextInput(TextArea))` and whose overlay is
//!   the `SuggestionList`. Intercepts `TextAction`, `InputCleared`, and
//!   `SuggestionSelected` actions from its descendants and re-emits the single
//!   public [`AutocompleteAction::TextChanged`] that the view layer consumes.

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ActionCtx, ArcStr, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx,
    NewWidget, PaintCtx, PropertySet, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty,
    TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::properties::{
    Background, BorderColor, BorderWidth, CaretColor, ContentColor, CornerRadius, Padding,
    PlaceholderColor, SelectionColor,
};
use masonry::widgets::{self, Label, SizedBox, TextAction};

use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::components::input::widget::{InputCleared, InputFrame};
use crate::components::popover::PopoverAnchor;
use crate::focus_ring::{FOCUS_RING_INSET, paint_focus_ring};

/// Fully transparent fill — strips the `TextInput`'s default masonry chrome.
const TRANSPARENT: Color = Color::from_rgba8(0, 0, 0, 0);
/// Vertical padding above/below the suggestion list content.
const LIST_PAD_V: f64 = 4.0;
/// Suggestion list chrome corner radius.
const LIST_CORNER: f64 = 5.0;
/// Suggestion list border stroke width.
const LIST_BORDER: f64 = 1.0;
/// Inset of the keyboard-highlight ring from the item bounds.
const HIGHLIGHT_RING_INSET: f64 = FOCUS_RING_INSET;
/// Minimum suggestion list width in logical pixels.
const MIN_LIST_WIDTH: f64 = 80.0;
/// Max visible suggestions (prevents enormous lists on large datasets).
const MAX_SUGGESTIONS: usize = 20;
/// Gap between the input field and the suggestion list overlay.
const OVERLAY_GAP: Length = Length::const_px(2.0);

// ─────────────────────────────────────────────────────────────────────────────
// SuggestionSelected action
// ─────────────────────────────────────────────────────────────────────────────

/// Fired when the user selects item at `0` (index into the current filtered
/// list). Handled by [`AutocompleteWidget::on_action`].
#[derive(Debug)]
pub(crate) struct SuggestionSelected(pub usize);

// ─────────────────────────────────────────────────────────────────────────────
// AutocompleteAction
// ─────────────────────────────────────────────────────────────────────────────

/// Public action type emitted by [`AutocompleteWidget`] to its view layer.
/// Carries the new text string (either typed or selected from the list).
#[derive(Debug)]
pub(crate) enum AutocompleteAction {
    TextChanged(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// SuggestionList
// ─────────────────────────────────────────────────────────────────────────────

/// Filtered suggestion list overlay for the autocomplete.
///
/// Paints its own rounded-rect chrome (background + border), tracks hover,
/// draws a focus-ring for keyboard navigation, and fires [`SuggestionSelected`]
/// on click. Closely mirrors `MenuContent` from the dropdown button.
pub(crate) struct SuggestionList {
    labels: Vec<WidgetPod<dyn Widget>>,
    item_rects: Vec<Rect>,
    hover_index: Option<usize>,
    /// Keyboard-highlighted row index, driven by [`AutocompleteWidget`].
    highlighted: Option<usize>,
    theme: Theme,
}

impl SuggestionList {
    pub(crate) fn new(items: impl IntoIterator<Item = ArcStr>, theme: &Theme) -> Self {
        let labels = items.into_iter().map(|t| Self::make_label(&t, theme)).collect();
        Self {
            labels,
            item_rects: Vec::new(),
            hover_index: None,
            highlighted: None,
            theme: *theme,
        }
    }

    fn make_label(text: &ArcStr, theme: &Theme) -> WidgetPod<dyn Widget> {
        let mut lbl = Label::new(text.clone())
            .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
            .prepare();
        lbl.properties.insert(ContentColor::new(theme.palette.text));
        lbl.erased().to_pod()
    }

    fn item_height(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size) + 2.0 * f64::from(self.theme.density.button_pad_v)
    }

    fn pad_h(&self) -> f64 {
        f64::from(self.theme.density.button_pad_h)
    }

    fn hit_item(&self, local: Point) -> Option<usize> {
        self.item_rects.iter().position(|r| r.contains(local))
    }

    fn to_local(ctx: &EventCtx<'_>, window_pos: Point) -> Point {
        let origin = ctx.to_window(Point::ZERO);
        window_pos - origin.to_vec2()
    }
}

// --- MARK: WIDGETMUT SETTERS
impl SuggestionList {
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        for label in &mut this.widget.labels {
            let mut lbl = this.ctx.get_mut(label);
            lbl.insert_prop(ContentColor::new(theme.palette.text));
            let mut lbl = lbl.downcast::<Label>();
            Label::insert_style(&mut lbl, StyleProperty::FontSize(theme.density.ui_font_size));
        }
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    pub(crate) fn set_items(
        this: &mut WidgetMut<'_, Self>,
        items: impl IntoIterator<Item = ArcStr>,
    ) {
        for label in this.widget.labels.drain(..) {
            this.ctx.remove_child(label);
        }
        let theme = this.widget.theme;
        this.widget.labels = items
            .into_iter()
            .map(|t| Self::make_label(&t, &theme))
            .collect();
        this.widget.item_rects.clear();
        this.widget.hover_index = None;
        this.widget.highlighted = None;
        this.ctx.children_changed();
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    pub(crate) fn set_highlighted(this: &mut WidgetMut<'_, Self>, index: Option<usize>) {
        if this.widget.highlighted != index {
            this.widget.highlighted = index;
            this.ctx.request_paint_only();
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for SuggestionList {
    type Action = SuggestionSelected;

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &masonry::core::PointerEvent,
    ) {
        use masonry::core::{PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate};
        match event {
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let local = Self::to_local(ctx, current.logical_point());
                let new_hover = self.hit_item(local);
                if new_hover != self.hover_index {
                    self.hover_index = new_hover;
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                ctx.capture_pointer();
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) if ctx.is_active() && ctx.is_hovered() => {
                let local = Self::to_local(ctx, state.logical_point());
                if let Some(i) = self.hit_item(local) {
                    ctx.submit_action::<Self::Action>(SuggestionSelected(i));
                    ctx.set_handled();
                }
            }
            PointerEvent::Leave(_) if self.hover_index.is_some() => {
                self.hover_index = None;
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn update(&mut self, _ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, _event: &Update) {}

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for label in &mut self.labels {
            ctx.register_child(label);
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let item_h = self.item_height();
        let pad_h = self.pad_h();
        let n = self.labels.len();
        match axis {
            Axis::Vertical => {
                let n_f64 = f64::from(u32::try_from(n).unwrap_or(u32::MAX));
                Length::px(LIST_PAD_V * 2.0 + item_h * n_f64)
            }
            Axis::Horizontal => {
                let inner_cross =
                    cross_length.map(|c| Length::px((c.get() - 2.0 * pad_h).max(0.0)));
                let mut max_w = MIN_LIST_WIDTH;
                for label in &mut self.labels {
                    let w = ctx
                        .compute_length(
                            label,
                            len_req.into(),
                            LayoutSize::maybe(Axis::Vertical, inner_cross),
                            Axis::Horizontal,
                            inner_cross,
                        )
                        .get();
                    max_w = max_w.max(w);
                }
                Length::px(max_w + 2.0 * pad_h)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.item_rects.clear();
        let pad_h = self.pad_h();
        let item_h = self.item_height();
        let label_avail = Size::new((size.width - 2.0 * pad_h).max(0.0), item_h);

        let mut y = LIST_PAD_V;
        for label in &mut self.labels {
            let item_rect = Rect::from_origin_size(Point::new(0.0, y), Size::new(size.width, item_h));
            self.item_rects.push(item_rect);

            let label_size =
                ctx.compute_size(label, SizeDef::fit(label_avail), label_avail.into());
            ctx.run_layout(label, label_size);
            let label_y = y + (item_h - label_size.height) * 0.5;
            ctx.place_child(label, Point::new(pad_h, label_y));

            y += item_h;
        }
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let p = &self.theme.palette;
        let bg_rect = RoundedRect::from_origin_size(Point::ORIGIN, ctx.border_box_size(), LIST_CORNER);
        painter.fill(bg_rect, p.surface_hi).draw();
        painter.stroke(bg_rect, &Stroke::new(LIST_BORDER), p.border_strong).draw();

        if let Some(i) = self.hover_index
            && let Some(&rect) = self.item_rects.get(i)
        {
            painter.fill(rect, p.surface_2).draw();
        }

        if let Some(i) = self.highlighted
            && let Some(&rect) = self.item_rects.get(i)
        {
            let inset = HIGHLIGHT_RING_INSET;
            let ring = Rect::new(rect.x0 + inset, rect.y0 + inset, rect.x1 - inset, rect.y1 - inset);
            paint_focus_ring(painter, ring, &self.theme);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::ListBox
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, _node: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        let ids: Vec<_> = self.labels.iter().map(WidgetPod::id).collect();
        ChildrenIds::from_slice(&ids)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Case-insensitive prefix match, capped at [`MAX_SUGGESTIONS`].
pub(crate) fn compute_filtered(all: &[ArcStr], query: &str) -> Vec<ArcStr> {
    if query.is_empty() {
        return Vec::new();
    }
    let q = query.to_lowercase();
    all.iter()
        .filter(|s| s.to_lowercase().starts_with(&q))
        .take(MAX_SUGGESTIONS)
        .cloned()
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// AutocompleteWidget
// ─────────────────────────────────────────────────────────────────────────────

/// Composite widget backing the autocomplete component.
///
/// Hosts an [`AnchoredOverlay`] whose:
/// - **primary** is `SizedBox(InputFrame(TextInput(TextArea)))` — a themed
///   single-line text input with field chrome applied via masonry properties.
/// - **overlay** is a [`SuggestionList`] — shown when the typed text matches
///   at least one suggestion.
///
/// Intercepts actions from descendants and re-emits
/// [`AutocompleteAction::TextChanged`] for the view layer.
pub(crate) struct AutocompleteWidget {
    overlay_host: WidgetPod<AnchoredOverlay>,
    all_suggestions: Vec<ArcStr>,
    /// Mirrors the host-controlled text, kept in sync via [`Self::set_contents`]
    /// and updated eagerly in [`Self::on_action`] for keyboard nav and selection.
    contents: String,
    /// Pre-computed filtered slice of [`Self::all_suggestions`].
    filtered: Vec<ArcStr>,
    open: bool,
    /// Index into [`Self::filtered`] for roving keyboard highlight.
    highlighted: Option<usize>,
    theme: Theme,
}

impl AutocompleteWidget {
    #[must_use]
    pub(crate) fn new(
        contents: &str,
        placeholder: ArcStr,
        all_suggestions: Vec<ArcStr>,
        disabled: bool,
        theme: &Theme,
    ) -> Self {
        // ── TextArea ──────────────────────────────────────────────────────────
        let text_area = widgets::TextArea::new_editable(contents)
            .with_style(StyleProperty::FontSize(theme.typography.size_body));

        let area_props = {
            let mut p = PropertySet::new();
            p.insert(ContentColor::new(theme.palette.text));
            p.insert(CaretColor { color: theme.palette.teal });
            p.insert(SelectionColor { color: theme.palette.teal_soft });
            p
        };

        // ── TextInput — stripped chrome ───────────────────────────────────────
        let text_input = widgets::TextInput::from_text_area(
            NewWidget::new(text_area).with_props(area_props),
        )
        .with_placeholder(placeholder)
        .with_clip(true);

        let mut text_input_widget = NewWidget::new(text_input);
        text_input_widget.properties.insert(Background::Color(TRANSPARENT));
        text_input_widget.properties.insert(BorderWidth::all(Length::const_px(0.0)));
        text_input_widget.properties.insert(Padding::all(Length::const_px(0.0)));
        text_input_widget.properties.insert(PlaceholderColor::new(theme.palette.text_muted));
        text_input_widget.options.disabled = disabled;

        // ── InputFrame — adds Esc-to-clear behaviour ──────────────────────────
        let input_frame = InputFrame::new(text_input_widget);

        // ── SizedBox — field chrome via masonry property system ───────────────
        // SizedBox properly subtracts border+padding from the child's layout
        // space (unlike InputFrame, which is a transparent passthrough).
        let mut chrome_box = NewWidget::new(SizedBox::new(NewWidget::new(input_frame)));
        chrome_box.properties.insert(Background::Color(theme.palette.surface));
        chrome_box.properties.insert(BorderWidth::all(Length::const_px(1.0)));
        chrome_box.properties.insert(BorderColor::new(theme.palette.border));
        chrome_box.properties.insert(CornerRadius::all(Length::px(
            f64::from(theme.radius.small),
        )));
        chrome_box.properties.insert(Padding::from_vh(
            Length::px(f64::from(theme.density.button_pad_v)),
            Length::px(f64::from(theme.density.button_pad_h)),
        ));

        // ── SuggestionList ────────────────────────────────────────────────────
        let suggestion_list = SuggestionList::new([], theme);

        // ── AnchoredOverlay ───────────────────────────────────────────────────
        let overlay = AnchoredOverlay::new(
            chrome_box,
            NewWidget::new(suggestion_list),
            false,
            PopoverAnchor::BottomStart,
        )
        .with_gap(OVERLAY_GAP);

        let filtered = compute_filtered(&all_suggestions, contents);

        Self {
            overlay_host: NewWidget::new(overlay).to_pod(),
            all_suggestions,
            contents: contents.to_owned(),
            filtered,
            open: false,
            highlighted: None,
            theme: *theme,
        }
    }
}

// --- MARK: INTERNAL HELPERS
//
// The WidgetMut chain (AnchoredOverlay → SizedBox → InputFrame → TextInput →
// TextArea) cannot be returned from helper functions because each step borrows
// the previous local. Closure-based helpers keep all the intermediates alive
// for the duration of the callback, which the borrow checker accepts.

/// Navigate to the `SuggestionList` in the overlay slot and invoke `f`.
fn with_suggestion_list<R>(
    w: &mut WidgetMut<'_, AnchoredOverlay>,
    f: impl FnOnce(&mut WidgetMut<'_, SuggestionList>) -> R,
) -> R {
    let mut overlay = AnchoredOverlay::overlay_mut(w);
    let mut list = overlay.downcast::<SuggestionList>();
    f(&mut list)
}

/// Navigate to the `TextArea` in the primary slot and invoke `f`.
fn with_text_area<R>(
    w: &mut WidgetMut<'_, AnchoredOverlay>,
    f: impl FnOnce(&mut WidgetMut<'_, widgets::TextArea<true>>) -> R,
) -> R {
    let mut primary = AnchoredOverlay::primary_mut(w);
    let mut sb = primary.downcast::<SizedBox>();
    let mut child = SizedBox::child_mut(&mut sb).expect("SizedBox has child");
    let mut frame = child.downcast::<InputFrame>();
    let mut inner = InputFrame::child_mut(&mut frame);
    let mut ti = inner.downcast::<widgets::TextInput>();
    let mut ta = widgets::TextInput::text_mut(&mut ti);
    f(&mut ta)
}

/// Navigate to the `TextInput` in the primary slot and invoke `f`.
fn with_text_input<R>(
    w: &mut WidgetMut<'_, AnchoredOverlay>,
    f: impl FnOnce(&mut WidgetMut<'_, widgets::TextInput>) -> R,
) -> R {
    let mut primary = AnchoredOverlay::primary_mut(w);
    let mut sb = primary.downcast::<SizedBox>();
    let mut child = SizedBox::child_mut(&mut sb).expect("SizedBox has child");
    let mut frame = child.downcast::<InputFrame>();
    let mut inner = InputFrame::child_mut(&mut frame);
    let mut ti = inner.downcast::<widgets::TextInput>();
    f(&mut ti)
}

impl AutocompleteWidget {
    fn set_highlight(&mut self, ctx: &mut EventCtx<'_>, index: Option<usize>) {
        if self.highlighted == index {
            return;
        }
        self.highlighted = index;
        ctx.mutate_child_later(&mut self.overlay_host, move |mut w| {
            with_suggestion_list(&mut w, |list| {
                SuggestionList::set_highlighted(list, index);
            });
        });
    }

    fn move_highlight(&mut self, ctx: &mut EventCtx<'_>, delta: isize) {
        let n = self.filtered.len();
        if n == 0 {
            return;
        }
        let next = match self.highlighted {
            None => if delta >= 0 { 0 } else { n - 1 },
            Some(i) => (i.cast_signed() + delta).rem_euclid(n.cast_signed()).cast_unsigned(),
        };
        self.set_highlight(ctx, Some(next));
    }

    fn close_overlay_later(&mut self, ctx: &mut ActionCtx<'_>) {
        ctx.mutate_child_later(&mut self.overlay_host, |mut w| {
            with_suggestion_list(&mut w, |list| SuggestionList::set_items(list, []));
            AnchoredOverlay::set_overlay_visible(&mut w, false);
        });
    }

    fn select_suggestion(&mut self, ctx: &mut ActionCtx<'_>, selected: String) {
        let text = selected.clone();
        self.contents.clone_from(&selected);
        self.filtered.clear();
        self.highlighted = None;
        self.open = false;
        ctx.mutate_child_later(&mut self.overlay_host, move |mut w| {
            with_suggestion_list(&mut w, |list| SuggestionList::set_items(list, []));
            AnchoredOverlay::set_overlay_visible(&mut w, false);
            with_text_area(&mut w, |ta| widgets::TextArea::reset_text(ta, &text));
        });
        ctx.submit_action::<AutocompleteAction>(AutocompleteAction::TextChanged(selected));
        ctx.set_handled();
        ctx.request_paint_only();
    }
}

// --- MARK: WIDGETMUT SETTERS
impl AutocompleteWidget {
    /// Update the displayed and filtered text. Called from the view layer on
    /// rebuild when the host's `contents` value changes.
    pub(crate) fn set_contents(this: &mut WidgetMut<'_, Self>, contents: &str) {
        if this.widget.contents == contents {
            return;
        }
        this.widget.contents.clear();
        this.widget.contents.push_str(contents);

        let filtered = compute_filtered(&this.widget.all_suggestions, contents);
        let should_open = this.widget.open && !filtered.is_empty() && !contents.is_empty();
        let open_changed = this.widget.open != should_open;
        this.widget.filtered.clone_from(&filtered);
        this.widget.open = should_open;
        if this.widget.highlighted.is_some_and(|i| i >= filtered.len()) {
            this.widget.highlighted = None;
        }

        let mut h = this.ctx.get_mut(&mut this.widget.overlay_host);
        with_suggestion_list(&mut h, |list| SuggestionList::set_items(list, filtered));
        if open_changed {
            AnchoredOverlay::set_overlay_visible(&mut h, should_open);
        }
        // Sync text area if it diverged (e.g. programmatic update from host).
        with_text_area(&mut h, |ta| {
            let current = ta.widget.text().to_string();
            if current != contents {
                widgets::TextArea::reset_text(ta, contents);
            }
        });
    }

    /// Replace the full suggestion list. Filtered suggestions are recomputed
    /// based on the current text.
    pub(crate) fn set_all_suggestions(this: &mut WidgetMut<'_, Self>, suggestions: Vec<ArcStr>) {
        if this.widget.all_suggestions == suggestions {
            return;
        }
        this.widget.all_suggestions = suggestions;

        let filtered = compute_filtered(&this.widget.all_suggestions, &this.widget.contents);
        let should_open = this.widget.open && !filtered.is_empty();
        let open_changed = this.widget.open != should_open;
        if this.widget.highlighted.is_some_and(|i| i >= filtered.len()) {
            this.widget.highlighted = None;
        }
        this.widget.filtered.clone_from(&filtered);
        this.widget.open = should_open;

        let mut h = this.ctx.get_mut(&mut this.widget.overlay_host);
        with_suggestion_list(&mut h, |list| SuggestionList::set_items(list, filtered));
        if open_changed {
            AnchoredOverlay::set_overlay_visible(&mut h, should_open);
        }
    }

    /// Update the placeholder text shown while the field is empty.
    pub(crate) fn set_placeholder(this: &mut WidgetMut<'_, Self>, placeholder: ArcStr) {
        let mut h = this.ctx.get_mut(&mut this.widget.overlay_host);
        with_text_input(&mut h, |ti| widgets::TextInput::set_placeholder(ti, placeholder));
    }

    /// Enable or disable the field (stops input, mutes visuals).
    pub(crate) fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        this.ctx.set_disabled(disabled);
        if disabled && this.widget.open {
            this.widget.open = false;
            this.widget.highlighted = None;
            this.widget.filtered.clear();
            let mut h = this.ctx.get_mut(&mut this.widget.overlay_host);
            with_suggestion_list(&mut h, |list| SuggestionList::set_items(list, []));
            AnchoredOverlay::set_overlay_visible(&mut h, false);
        }
    }

    /// Re-apply theme colors to the input chrome and suggestion list.
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;

        {
            let mut h = this.ctx.get_mut(&mut this.widget.overlay_host);

            // Update SizedBox chrome properties.
            {
                let mut primary = AnchoredOverlay::primary_mut(&mut h);
                let mut sb = primary.downcast::<SizedBox>();
                sb.insert_prop(Background::Color(theme.palette.surface));
                sb.insert_prop(BorderColor::new(theme.palette.border));
                sb.insert_prop(CornerRadius::all(Length::px(f64::from(theme.radius.small))));
                // Padding/BorderWidth don't change with theme.

                // Update TextInput and TextArea properties (nested inside SizedBox).
                if let Some(mut child) = SizedBox::child_mut(&mut sb) {
                    let mut frame = child.downcast::<InputFrame>();
                    let mut inner = InputFrame::child_mut(&mut frame);
                    let mut ti = inner.downcast::<widgets::TextInput>();
                    ti.insert_prop(PlaceholderColor::new(theme.palette.text_muted));
                    let mut ta = widgets::TextInput::text_mut(&mut ti);
                    ta.insert_prop(ContentColor::new(theme.palette.text));
                    ta.insert_prop(CaretColor { color: theme.palette.teal });
                    ta.insert_prop(SelectionColor { color: theme.palette.teal_soft });
                    widgets::TextArea::insert_style(
                        &mut ta,
                        StyleProperty::FontSize(theme.typography.size_body),
                    );
                }
            }

            // Update suggestion list theme.
            with_suggestion_list(&mut h, |list| SuggestionList::set_theme(list, theme));
        } // h dropped here

        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }
}

// --- MARK: IMPL WIDGET
impl Widget for AutocompleteWidget {
    type Action = AutocompleteAction;

    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        _source: WidgetId,
    ) {
        // ── Text changes ──────────────────────────────────────────────────────
        if let Some(text_action) = action.downcast_ref::<TextAction>() {
            match text_action {
                TextAction::Changed(text) => {
                    self.contents.clone_from(text);
                    self.filtered = compute_filtered(&self.all_suggestions, text);
                    self.highlighted = None;

                    let should_open = !self.filtered.is_empty() && !text.is_empty();
                    let open_changed = self.open != should_open;
                    self.open = should_open;

                    let items = self.filtered.clone();
                    ctx.mutate_child_later(&mut self.overlay_host, move |mut w| {
                        with_suggestion_list(&mut w, |list| SuggestionList::set_items(list, items));
                        if open_changed {
                            AnchoredOverlay::set_overlay_visible(&mut w, should_open);
                        }
                    });

                    ctx.submit_action::<Self::Action>(AutocompleteAction::TextChanged(text.clone()));
                    ctx.set_handled();
                }
                // Enter selects the highlighted suggestion when the list is open;
                // otherwise it is left unhandled (bubbles for form submission).
                TextAction::Entered(_) => {
                    if self.open
                        && let Some(i) = self.highlighted
                        && let Some(selected) = self.filtered.get(i).cloned()
                    {
                        self.select_suggestion(ctx, selected.to_string());
                    }
                }
            }
            return;
        }

        // ── Escape / clear ────────────────────────────────────────────────────
        if action.downcast_ref::<InputCleared>().is_some() {
            self.contents.clear();
            self.filtered.clear();
            self.highlighted = None;
            self.open = false;
            self.close_overlay_later(ctx);
            ctx.submit_action::<Self::Action>(AutocompleteAction::TextChanged(String::new()));
            ctx.set_handled();
            ctx.request_paint_only();
            return;
        }

        // ── Suggestion selected by click ──────────────────────────────────────
        if let Some(&SuggestionSelected(i)) = action.downcast_ref::<SuggestionSelected>()
            && let Some(selected) = self.filtered.get(i).cloned()
        {
            self.select_suggestion(ctx, selected.to_string());
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if !self.open {
            return;
        }
        let TextEvent::Keyboard(key) = event else { return };
        if key.state != KeyState::Down {
            return;
        }
        match &key.key {
            Key::Named(NamedKey::ArrowDown) => {
                self.move_highlight(ctx, 1);
                ctx.set_handled();
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.move_highlight(ctx, -1);
                ctx.set_handled();
            }
            Key::Named(NamedKey::Home) if !self.filtered.is_empty() => {
                self.set_highlight(ctx, Some(0));
                ctx.set_handled();
            }
            Key::Named(NamedKey::End) if !self.filtered.is_empty() => {
                self.set_highlight(ctx, Some(self.filtered.len() - 1));
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            // Close when widget is stashed mid-open (e.g. hidden by a tab container).
            Update::StashedChanged(true) if self.open => {
                self.open = false;
                self.highlighted = None;
                self.filtered.clear();
                ctx.mutate_child_later(&mut self.overlay_host, |mut w| {
                    with_suggestion_list(&mut w, |list| SuggestionList::set_items(list, []));
                    AnchoredOverlay::set_overlay_visible(&mut w, false);
                });
                ctx.request_paint_only();
            }
            // Close when focus leaves the autocomplete's entire widget subtree.
            Update::ChildFocusChanged(false) if self.open => {
                self.open = false;
                self.highlighted = None;
                self.filtered.clear();
                ctx.mutate_child_later(&mut self.overlay_host, |mut w| {
                    with_suggestion_list(&mut w, |list| SuggestionList::set_items(list, []));
                    AnchoredOverlay::set_overlay_visible(&mut w, false);
                });
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.overlay_host);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.overlay_host, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.overlay_host, size);
        ctx.place_child(&mut self.overlay_host, Point::ORIGIN);
        ctx.derive_baselines(&self.overlay_host);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural — the SizedBox chrome and SuggestionList paint themselves.
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn accessibility_role(&self) -> Role {
        Role::ComboBox
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.add_action(masonry::accesskit::Action::SetValue);
        if self.open {
            node.set_expanded(true);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.overlay_host.id()])
    }
}
