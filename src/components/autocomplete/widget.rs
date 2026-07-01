//! Masonry widgets for the autocomplete component.
//!
//! Two widgets live here:
//!
//! - [`SuggestionList`] — item-list overlay, closely mirrors `MenuContent` from
//!   the dropdown button. Handles hover, keyboard highlight painting, and fires
//!   [`SuggestionSelected`] on click.
//! - [`AutocompleteWidget`] — composite host: when inside an [`crate::overlay_scope`]
//!   its input chrome is a standalone child and the suggestion list lives in the
//!   scope's always-on-top portal slot; otherwise falls back to an [`AnchoredOverlay`]
//!   exactly as before. Intercepts `TextAction`, `InputCleared`, and
//!   `SuggestionSelected` actions from its descendants and re-emits the single
//!   public [`AutocompleteAction::TextChanged`] that the view layer consumes.

use std::sync::{Arc, OnceLock};

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ActionCtx, ArcStr, ChildrenIds, ComposeCtx, ErasedAction, EventCtx, LayoutCtx,
    MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesMut, PropertiesRef, PropertySet,
    RegisterCtx, StyleProperty, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut,
    WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenDef, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::properties::{
    Background, BorderColor, BorderWidth, CaretColor, ContentColor, CornerRadius, Padding,
    PlaceholderColor, SelectionColor,
};
use masonry::widgets::{self, Label, Passthrough, SizedBox, TextAction};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::components::input::widget::{InputCleared, InputFrame};
use crate::components::popover::PopoverAnchor;
use crate::components::scroll_container::widget::{ContentClip, ScrollView};
use crate::focus_ring::{FOCUS_RING_INSET, paint_focus_ring};
use crate::overlay_portal::{OwnerKind, PortalSlot, PortalVisibility};
use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

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
/// Maximum visible height for the suggestion list before it scrolls, px.
const MAX_LIST_HEIGHT: f64 = 200.0;
/// Max results for a typed-prefix match. Browsing the unfiltered list (empty
/// query) isn't capped — the list scrolls — but typing should narrow to a
/// short, scannable set rather than every match in a huge dataset.
const MAX_SUGGESTIONS: usize = 20;
/// Gap between the input field and the suggestion list overlay, px.
const OVERLAY_GAP_PX: f64 = 2.0;
/// Gap as a [`Length`] (used by the in-tree `AnchoredOverlay`).
const OVERLAY_GAP: Length = Length::const_px(OVERLAY_GAP_PX);

// ─────────────────────────────────────────────────────────────────────────────
// SuggestionSelected action
// ─────────────────────────────────────────────────────────────────────────────

/// Fired when the user selects a suggestion. Carries the text directly so the
/// receiver never has to resolve a stale index against filtered state.
#[derive(Debug, Clone)]
pub(crate) struct SuggestionSelected(pub ArcStr);

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
// AutocompleteHandle
// ─────────────────────────────────────────────────────────────────────────────

/// Self-filling handle to an [`AutocompleteWidget`]'s widget id, filled at
/// `Update::WidgetAdded`. Given to portal-mounted [`SuggestionListView`] so
/// an item selection can `mutate_later` back into the widget to close the
/// suggestion list and emit the selected text.
#[derive(Clone, Default)]
pub(crate) struct AutocompleteHandle(Arc<OnceLock<WidgetId>>);

impl AutocompleteHandle {
    pub(crate) fn new() -> Self {
        Self(Arc::new(OnceLock::new()))
    }

    pub(crate) fn widget_id(&self) -> Option<WidgetId> {
        self.0.get().copied()
    }

    fn set(&self, id: WidgetId) {
        let _ = self.0.set(id);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LabelListHandle
// ─────────────────────────────────────────────────────────────────────────────

/// Self-filling handle to a [`LabelList`]'s widget id, filled at
/// `Update::WidgetAdded`. Lets [`AutocompleteWidget`] expose `aria-controls`
/// pointing at the listbox. The active-descendant relationship is set
/// directly by [`LabelList`] itself (see its `accessibility` impl), since it
/// always has immediate, local access to which item is highlighted.
#[derive(Clone, Default)]
pub(crate) struct LabelListHandle(Arc<OnceLock<WidgetId>>);

impl LabelListHandle {
    pub(crate) fn new() -> Self {
        Self(Arc::new(OnceLock::new()))
    }

    pub(crate) fn widget_id(&self) -> Option<WidgetId> {
        self.0.get().copied()
    }

    fn set(&self, id: WidgetId) {
        let _ = self.0.set(id);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TextAreaHandle
// ─────────────────────────────────────────────────────────────────────────────

/// Handle to the autocomplete's editable `TextArea`'s widget id. Unlike the
/// other handles here, this is filled *eagerly* at [`AutocompleteWidget`]
/// construction — `build_chrome` already reads the id synchronously off the
/// `NewWidget` it builds, no need to wait for `Update::WidgetAdded`. Lets
/// [`LabelList`] call `ctx.set_focus()` directly to return focus to the input
/// after Enter-selection or Escape, in both hosting modes — see the module
/// docs for why arrow-key navigation moved from "focus stays in the textbox"
/// (blocked: `TextArea` unconditionally claims arrow keys for cursor
/// movement, even on a single line, so an ancestor never sees them) to
/// "Tab moves focus into the open listbox".
#[derive(Clone, Default)]
pub(crate) struct TextAreaHandle(Arc<OnceLock<WidgetId>>);

impl TextAreaHandle {
    pub(crate) fn new() -> Self {
        Self(Arc::new(OnceLock::new()))
    }

    pub(crate) fn widget_id(&self) -> Option<WidgetId> {
        self.0.get().copied()
    }

    fn set(&self, id: WidgetId) {
        let _ = self.0.set(id);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SuggestionList
// ─────────────────────────────────────────────────────────────────────────────

/// Filtered suggestion list overlay for the autocomplete.
///
/// Paints its own rounded-rect chrome (background + border) and hosts a
/// [`ScrollView`] wrapping [`LabelList`], which holds the actual item widgets
/// and handles hover/highlight/click. Split this way because `AnchoredOverlay`
/// and `PortalSlot` both size their overlay with `SizeDef::MIN`, and
/// `ScrollView` reports zero size for `MinContent` — `SuggestionList` is the
/// thing that reports a sensible (capped) size; `ScrollView` only sees
/// `MaxContent` requests forwarded from `SuggestionList::measure`.
pub(crate) struct SuggestionList {
    scroll: WidgetPod<ScrollView<LabelList>>,
    theme: Theme,
}

impl SuggestionList {
    pub(crate) fn new(
        items: impl IntoIterator<Item = ArcStr>,
        theme: &Theme,
        listbox_handle: LabelListHandle,
        autocomplete_handle: AutocompleteHandle,
        text_area_handle: TextAreaHandle,
    ) -> Self {
        let label_list = LabelList::new(
            items,
            theme,
            listbox_handle,
            autocomplete_handle,
            text_area_handle,
        );
        let scroll = ScrollView::new(NewWidget::new(label_list));
        Self {
            scroll: NewWidget::new(scroll).to_pod(),
            theme: *theme,
        }
    }
}

/// Navigate from a `ScrollView<LabelList>` `WidgetMut` down to the `LabelList`
/// and invoke `f`.
fn with_label_list<R>(
    scroll: &mut WidgetMut<'_, ScrollView<LabelList>>,
    f: impl FnOnce(&mut WidgetMut<'_, LabelList>) -> R,
) -> R {
    let mut clip = ScrollView::child_mut(scroll);
    let mut list = ContentClip::child_mut(&mut clip);
    f(&mut list)
}

// --- MARK: WIDGETMUT SETTERS
impl SuggestionList {
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        {
            let mut scroll = this.ctx.get_mut(&mut this.widget.scroll);
            with_label_list(&mut scroll, |list| LabelList::set_theme(list, theme));
        }
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    pub(crate) fn set_items(
        this: &mut WidgetMut<'_, Self>,
        items: impl IntoIterator<Item = ArcStr>,
    ) {
        let mut scroll = this.ctx.get_mut(&mut this.widget.scroll);
        with_label_list(&mut scroll, |list| LabelList::set_items(list, items));
        ScrollView::scroll_to_origin(&mut scroll);
    }
}

// --- MARK: IMPL WIDGET
impl Widget for SuggestionList {
    type Action = SuggestionSelected;

    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        _source: WidgetId,
    ) {
        // Re-emit from our own widget id so the xilem driver can route the action.
        //
        // The driver maps widget ids to view paths (registered by `with_action_widget`
        // in `SuggestionListView::build`). In portal mode, `LabelList` is deep inside
        // a `PortalSlot` subtree that is separate from `AutocompleteWidget`'s subtree,
        // so the action from `LabelList.id` has no registered view path and is dropped
        // with an "unknown widget" error. Re-emitting here promotes the action to
        // `SuggestionList.id`, which IS registered, so it reaches `SuggestionListView::message`.
        // In in-tree mode the re-emitted action bubbles to `AutocompleteWidget::on_action`
        // which handles it — so it never reaches the driver at all.
        if let Some(sel) = action.downcast_ref::<SuggestionSelected>() {
            ctx.submit_action::<Self::Action>(sel.clone());
            ctx.set_handled();
        }
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.scroll);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        // Always request the scroll view's *natural* (unconstrained) size —
        // `ScrollView` reports zero for `MinContent`, so we can't forward
        // whatever `len_req` we were given. Vertical gets capped below;
        // horizontal (the list's width) is left as-is.
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        let natural = ctx
            .compute_length(
                &mut self.scroll,
                LenDef::MaxContent,
                context_size,
                axis,
                cross_length,
            )
            .get();
        match axis {
            Axis::Vertical => Length::px(natural.min(MAX_LIST_HEIGHT)),
            Axis::Horizontal => Length::px(natural),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.scroll, size);
        ctx.place_child(&mut self.scroll, Point::ORIGIN);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let p = &self.theme.palette;
        let bg_rect =
            RoundedRect::from_origin_size(Point::ORIGIN, ctx.border_box_size(), LIST_CORNER);
        painter.fill(bg_rect, p.surface_hi).draw();
        painter
            .stroke(bg_rect, &Stroke::new(LIST_BORDER), p.border_strong)
            .draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.scroll.id()])
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LabelList
// ─────────────────────────────────────────────────────────────────────────────

/// The actual suggestion item widgets, scrolled by [`SuggestionList`]'s
/// [`ScrollView`]. Tracks hover, draws a focus-ring for keyboard navigation,
/// and fires [`SuggestionSelected`] on click. Closely mirrors `MenuContent`
/// from the dropdown button.
///
/// `Role::ListBox`. Reports its own widget id via [`LabelListHandle`] (read by
/// [`AutocompleteWidget`] for `aria-controls`) and, since it always has direct
/// access to which item is highlighted, sets `aria-activedescendant` on its
/// own node — accesskit's `ActiveDescendant` is documented for exactly this
/// "composite widget whose active item changes while focus stays elsewhere"
/// case, so no cross-widget id plumbing is needed for that part.
pub(crate) struct LabelList {
    labels: Vec<WidgetPod<SuggestionItem>>,
    /// Parallel to `labels` — the text of each item. Used to resolve a
    /// selected index to text at action-submit time, where `WidgetPod`
    /// internals are not accessible.
    label_texts: Vec<ArcStr>,
    /// Item rects in this widget's local (natural, un-scrolled) coordinate
    /// space — `ScrollView` applies the scroll offset via a transform, so
    /// `EventCtx::to_window`/`to_local` already account for it.
    item_rects: Vec<Rect>,
    hover_index: Option<usize>,
    /// Keyboard-highlighted row index. Driven by this widget's own
    /// `on_text_event` once it has real focus (see [`Self::accepts_focus`]).
    highlighted: Option<usize>,
    theme: Theme,
    handle: LabelListHandle,
    /// Lets Enter/Escape return focus to the input — see [`TextAreaHandle`].
    autocomplete_handle: AutocompleteHandle,
    text_area_handle: TextAreaHandle,
}

impl LabelList {
    fn new(
        items: impl IntoIterator<Item = ArcStr>,
        theme: &Theme,
        handle: LabelListHandle,
        autocomplete_handle: AutocompleteHandle,
        text_area_handle: TextAreaHandle,
    ) -> Self {
        let items: Vec<ArcStr> = items.into_iter().collect();
        let label_texts = items.clone();
        let labels = items.into_iter().map(|t| Self::make_item(t, theme)).collect();
        Self {
            labels,
            label_texts,
            item_rects: Vec::new(),
            hover_index: None,
            highlighted: None,
            theme: *theme,
            handle,
            autocomplete_handle,
            text_area_handle,
        }
    }

    fn make_item(text: ArcStr, theme: &Theme) -> WidgetPod<SuggestionItem> {
        NewWidget::new(SuggestionItem::new(text, theme)).to_pod()
    }

    fn item_height(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size)
            + 2.0 * f64::from(self.theme.density.button_pad_v)
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

    /// Returns focus to the input field, if its id is known yet.
    fn refocus_input(&self, ctx: &mut EventCtx<'_>) {
        if let Some(id) = self.text_area_handle.widget_id() {
            ctx.set_focus(id);
        }
    }

    /// Closes the dropdown via the same back-channel `portal_select`/outside-
    /// press dismissal already use — works identically in both hosting modes.
    fn request_close(&self, ctx: &mut EventCtx<'_>) {
        if let Some(ac_id) = self.autocomplete_handle.widget_id() {
            ctx.mutate_later(ac_id, |mut w| {
                let mut ac = w.downcast::<AutocompleteWidget>();
                AutocompleteWidget::mark_closed(&mut ac);
            });
        }
    }

    /// Moves the keyboard highlight by `delta` positions (wrapping), pushing
    /// the `aria-selected` flag to the affected items and repainting.
    fn move_highlight(&mut self, ctx: &mut EventCtx<'_>, delta: isize) {
        let n = self.labels.len();
        if n == 0 {
            return;
        }
        let next = match self.highlighted {
            None => {
                if delta >= 0 {
                    0
                } else {
                    n - 1
                }
            }
            Some(i) => (i.cast_signed() + delta)
                .rem_euclid(n.cast_signed())
                .cast_unsigned(),
        };
        self.set_highlight(ctx, Some(next));
    }

    fn set_highlight(&mut self, ctx: &mut EventCtx<'_>, index: Option<usize>) {
        if self.highlighted == index {
            return;
        }
        let prev = self.highlighted;
        self.highlighted = index;
        if let Some(i) = prev
            && let Some(label) = self.labels.get_mut(i)
        {
            ctx.mutate_child_later(label, |mut item| {
                SuggestionItem::set_selected(&mut item, false);
            });
        }
        if let Some(i) = index
            && let Some(label) = self.labels.get_mut(i)
        {
            ctx.mutate_child_later(label, |mut item| {
                SuggestionItem::set_selected(&mut item, true);
            });
        }
        ctx.request_paint_only();
        ctx.request_accessibility_update();
    }
}

// --- MARK: WIDGETMUT SETTERS
impl LabelList {
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        for label in &mut this.widget.labels {
            let mut item = this.ctx.get_mut(label);
            SuggestionItem::set_theme(&mut item, theme);
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
        let items: Vec<ArcStr> = items.into_iter().collect();
        this.widget.label_texts = items.clone();
        this.widget.labels = items.into_iter().map(|t| Self::make_item(t, &theme)).collect();
        this.widget.item_rects.clear();
        this.widget.hover_index = None;
        this.widget.highlighted = None;
        this.ctx.children_changed();
        this.ctx.request_layout();
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }
}

// --- MARK: IMPL WIDGET
impl Widget for LabelList {
    type Action = SuggestionSelected;

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    /// Lets keyboard focus move here via Tab while the dropdown is open (see
    /// [`AutocompleteWidget::on_text_event`]'s Tab handling) — once focus is
    /// here, arrow keys navigate without any conflict, since this widget has
    /// no competing key bindings the way `TextArea` does.
    fn accepts_focus(&self) -> bool {
        true
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
                if let Some(i) = self.hit_item(local)
                    && let Some(text) = self.label_texts.get(i)
                {
                    ctx.submit_action::<Self::Action>(SuggestionSelected(text.clone()));
                    // Return focus to the input so the user can keep typing.
                    self.refocus_input(ctx);
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

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key) = event else {
            return;
        };
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
            Key::Named(NamedKey::Home) if !self.labels.is_empty() => {
                self.set_highlight(ctx, Some(0));
                ctx.set_handled();
            }
            Key::Named(NamedKey::End) if !self.labels.is_empty() => {
                self.set_highlight(ctx, Some(self.labels.len() - 1));
                ctx.set_handled();
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(i) = self.highlighted
                    && let Some(text) = self.label_texts.get(i)
                {
                    ctx.submit_action::<Self::Action>(SuggestionSelected(text.clone()));
                }
                self.refocus_input(ctx);
                self.request_close(ctx);
                ctx.set_handled();
            }
            Key::Named(NamedKey::Escape) => {
                self.refocus_input(ctx);
                self.request_close(ctx);
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::WidgetAdded = event {
            self.handle.set(ctx.widget_id());
        }
    }

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
            let item_rect =
                Rect::from_origin_size(Point::new(0.0, y), Size::new(size.width, item_h));
            self.item_rects.push(item_rect);

            let label_size = ctx.compute_size(label, SizeDef::fit(label_avail), label_avail.into());
            ctx.run_layout(label, label_size);
            let label_y = y + (item_h - label_size.height) * 0.5;
            ctx.place_child(label, Point::new(pad_h, label_y));

            y += item_h;
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let p = &self.theme.palette;

        if let Some(i) = self.hover_index
            && let Some(&rect) = self.item_rects.get(i)
        {
            painter.fill(rect, p.surface_2).draw();
        }

        if let Some(i) = self.highlighted
            && let Some(&rect) = self.item_rects.get(i)
        {
            let inset = HIGHLIGHT_RING_INSET;
            let ring = Rect::new(
                rect.x0 + inset,
                rect.y0 + inset,
                rect.x1 - inset,
                rect.y1 - inset,
            );
            paint_focus_ring(painter, ring, &self.theme);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::ListBox
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if let Some(i) = self.highlighted
            && let Some(label) = self.labels.get(i)
        {
            node.set_active_descendant(label.id().into());
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        let ids: Vec<_> = self.labels.iter().map(WidgetPod::id).collect();
        ChildrenIds::from_slice(&ids)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SuggestionItem
// ─────────────────────────────────────────────────────────────────────────────

/// Thin accessibility wrapper around a [`Label`]: exposes the item as
/// `Role::ListBoxOption` with an explicit accessible name and `aria-selected`
/// state, since `Label`'s own role can't be overridden from outside. Painting,
/// hover, and the focus ring stay on [`LabelList`] — this widget adds no
/// visuals of its own, just identity + state for assistive tech.
pub(crate) struct SuggestionItem {
    label: WidgetPod<Label>,
    text: ArcStr,
    selected: bool,
}

impl SuggestionItem {
    fn new(text: ArcStr, theme: &Theme) -> Self {
        let mut lbl = Label::new(text.clone())
            .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
            .prepare();
        lbl.properties.insert(ContentColor::new(theme.palette.text));
        Self {
            label: lbl.to_pod(),
            text,
            selected: false,
        }
    }
}

// --- MARK: WIDGETMUT SETTERS
impl SuggestionItem {
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        let mut lbl = this.ctx.get_mut(&mut this.widget.label);
        lbl.insert_prop(ContentColor::new(theme.palette.text));
        Label::insert_style(
            &mut lbl,
            StyleProperty::FontSize(theme.density.ui_font_size),
        );
    }

    pub(crate) fn set_selected(this: &mut WidgetMut<'_, Self>, selected: bool) {
        if this.widget.selected != selected {
            this.widget.selected = selected;
            this.ctx.request_accessibility_update();
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for SuggestionItem {
    type Action = NoAction;

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.label);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        ctx.compute_length(
            &mut self.label,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.label, size);
        ctx.place_child(&mut self.label, Point::ZERO);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural — the inner `Label` paints itself.
    }

    fn accessibility_role(&self) -> Role {
        Role::ListBoxOption
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.text.to_string());
        if self.selected {
            node.set_selected(true);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.label.id()])
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SuggestionListView — portal content view
// ─────────────────────────────────────────────────────────────────────────────

/// Xilem view registered with the overlay scope's portal when a scope ancestor
/// exists. Wraps [`SuggestionList`] and routes [`SuggestionSelected`] actions
/// back to the owning [`AutocompleteWidget`] via [`AutocompleteHandle`].
pub(crate) struct SuggestionListView {
    pub(crate) filtered: Arc<Vec<ArcStr>>,
    pub(crate) handle: AutocompleteHandle,
    pub(crate) listbox_handle: LabelListHandle,
    pub(crate) text_area_handle: TextAreaHandle,
    pub(crate) theme: Theme,
}

impl ViewMarker for SuggestionListView {}

impl<State, Action> View<State, Action, ViewCtx> for SuggestionListView
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<SuggestionList>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = SuggestionList::new(
            (*self.filtered).iter().cloned(),
            &self.theme,
            self.listbox_handle.clone(),
            self.handle.clone(),
            self.text_area_handle.clone(),
        );
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _vs: &mut (),
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) {
        if self.theme != prev.theme {
            SuggestionList::set_theme(&mut element, &self.theme);
        }
        if !Arc::ptr_eq(&self.filtered, &prev.filtered) {
            SuggestionList::set_items(&mut element, (*self.filtered).iter().cloned());
        }
    }

    fn teardown(&self, _vs: &mut (), ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _vs: &mut (),
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) -> MessageResult<Action> {
        if let Some(boxed) = message.take_message::<SuggestionSelected>() {
            let SuggestionSelected(text) = *boxed;
            if let Some(ac_id) = self.handle.widget_id() {
                element.ctx.mutate_later(ac_id, move |mut w| {
                    let mut ac = w.downcast::<AutocompleteWidget>();
                    AutocompleteWidget::portal_select(&mut ac, text.to_string());
                });
            }
        }
        MessageResult::Nop
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Case-insensitive prefix match, capped at [`MAX_SUGGESTIONS`].
/// When `query` is empty, the **full, uncapped** list is returned — the
/// dropdown shows everything when the field first receives focus, and the
/// list scrolls to reach entries beyond the visible window.
pub(crate) fn compute_filtered(all: &[ArcStr], query: &str) -> Vec<ArcStr> {
    if query.is_empty() {
        return all.to_vec();
    }
    let q = query.to_lowercase();
    all.iter()
        .filter(|s| s.to_lowercase().starts_with(&q))
        .take(MAX_SUGGESTIONS)
        .cloned()
        .collect()
}

/// Navigate from a `SizedBox` `WidgetMut` down to the `TextArea` child and
/// invoke `f`. Used by both hosting modes (in-tree navigates through
/// `AnchoredOverlay::primary_mut` first; portal mode navigates directly).
fn with_text_area_in_chrome<R>(
    sb: &mut WidgetMut<'_, SizedBox>,
    f: impl FnOnce(&mut WidgetMut<'_, widgets::TextArea<true>>) -> R,
) -> R {
    let mut child = SizedBox::child_mut(sb).expect("SizedBox has child");
    let mut frame = child.downcast::<InputFrame>();
    let mut inner = InputFrame::child_mut(&mut frame);
    let mut ti = inner.downcast::<widgets::TextInput>();
    let mut ta = widgets::TextInput::text_mut(&mut ti);
    f(&mut ta)
}

/// Navigate from a `SizedBox` `WidgetMut` down to the `TextInput` child and
/// invoke `f`.
fn with_text_input_in_chrome<R>(
    sb: &mut WidgetMut<'_, SizedBox>,
    f: impl FnOnce(&mut WidgetMut<'_, widgets::TextInput>) -> R,
) -> R {
    let mut child = SizedBox::child_mut(sb).expect("SizedBox has child");
    let mut frame = child.downcast::<InputFrame>();
    let mut inner = InputFrame::child_mut(&mut frame);
    let mut ti = inner.downcast::<widgets::TextInput>();
    f(&mut ti)
}

/// Navigate to the `SuggestionList` in the in-tree overlay slot and invoke `f`.
fn with_suggestion_list<R>(
    w: &mut WidgetMut<'_, AnchoredOverlay>,
    f: impl FnOnce(&mut WidgetMut<'_, SuggestionList>) -> R,
) -> R {
    let mut overlay = AnchoredOverlay::overlay_mut(w);
    let mut list = overlay.downcast::<SuggestionList>();
    f(&mut list)
}

/// Navigate to the `TextArea` via the in-tree `AnchoredOverlay` primary slot
/// and invoke `f`.
fn with_text_area<R>(
    w: &mut WidgetMut<'_, AnchoredOverlay>,
    f: impl FnOnce(&mut WidgetMut<'_, widgets::TextArea<true>>) -> R,
) -> R {
    let mut primary = AnchoredOverlay::primary_mut(w);
    let mut sb = primary.downcast::<SizedBox>();
    with_text_area_in_chrome(&mut sb, f)
}

/// Navigate to the `TextInput` via the in-tree `AnchoredOverlay` primary slot
/// and invoke `f`.
fn with_text_input<R>(
    w: &mut WidgetMut<'_, AnchoredOverlay>,
    f: impl FnOnce(&mut WidgetMut<'_, widgets::TextInput>) -> R,
) -> R {
    let mut primary = AnchoredOverlay::primary_mut(w);
    let mut sb = primary.downcast::<SizedBox>();
    with_text_input_in_chrome(&mut sb, f)
}

// ─────────────────────────────────────────────────────────────────────────────
// Hosting enum
// ─────────────────────────────────────────────────────────────────────────────

/// Where the suggestion list lives: permanently in-tree (fallback, no scope
/// ancestor), or portal-mounted in the nearest scope's `PortalSlot` (paints
/// above all scope content, same as `dropdown_button`'s portal mode).
enum Hosting {
    InTree {
        overlay_host: WidgetPod<AnchoredOverlay>,
    },
    Portal {
        /// The input field chrome (`SizedBox` wrapping `InputFrame` → `TextInput` → `TextArea`).
        chrome: WidgetPod<SizedBox>,
        scope: OverlayScopeHandle,
        key: u64,
        /// Last window-space anchor rect pushed to the slot; `compose` uses
        /// it to re-anchor only when we actually moved.
        last_anchor_rect_window: Option<Rect>,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// AutocompleteWidget
// ─────────────────────────────────────────────────────────────────────────────

/// Construction inputs for [`AutocompleteWidget::new_portal`]; bundled to
/// keep its argument count under clippy's `too_many_arguments` threshold.
pub(crate) struct AutocompleteConfig {
    pub(crate) contents: String,
    pub(crate) placeholder: ArcStr,
    pub(crate) all_suggestions: Vec<ArcStr>,
    pub(crate) disabled: bool,
    pub(crate) theme: Theme,
}

/// Composite widget backing the autocomplete component.
///
/// Two hosting modes (see [`Hosting`]):
///
/// - **In-tree** (no scope ancestor, fallback): hosts an [`AnchoredOverlay`]
///   whose primary is the chromed input and whose overlay is the
///   [`SuggestionList`] — the original layout.
/// - **Portal** (scope ancestor present): hosts only the input chrome as a
///   direct child; the [`SuggestionList`] is registered with the scope's
///   [`crate::overlay_portal::OverlayPortal`] via [`SuggestionListView`] and
///   mounted in the always-on-top portal slot, so it paints above siblings.
///
/// Intercepts actions from descendants and re-emits
/// [`AutocompleteAction::TextChanged`] for the view layer.
pub(crate) struct AutocompleteWidget {
    hosting: Hosting,
    all_suggestions: Vec<ArcStr>,
    /// Mirrors the host-controlled text, kept in sync via [`Self::set_contents`]
    /// and updated eagerly in action handlers for keyboard nav and selection.
    contents: String,
    /// Pre-computed filtered slice of [`Self::all_suggestions`].
    filtered: Vec<ArcStr>,
    open: bool,
    /// Index into [`Self::filtered`] for roving keyboard highlight.
    highlighted: Option<usize>,
    /// Suppresses the next [`Self::open_on_focus`] call so that click-based
    /// selection does not reopen the dropdown when focus returns to the input.
    ///
    /// Click creates a focus gap (`TextArea` → `None` → `TextArea`) that fires
    /// `ChildFocusChanged(true)` on this widget, which would normally reopen
    /// the list. Set by [`Self::select_suggestion`] / [`Self::portal_select`]
    /// and cleared by the first [`Self::open_on_focus`] that sees it.
    suppress_focus_open: bool,
    theme: Theme,
    /// Resolves to the listbox's widget id once mounted, for `aria-controls`
    /// and for Tab-to-navigate (see [`Self::on_text_event`]).
    listbox_handle: LabelListHandle,
    /// Resolves to this widget's own id once mounted. Given to `LabelList` so
    /// Escape can close the dropdown via [`Self::mark_closed`] regardless of
    /// hosting mode (in portal mode there's no ancestor path back here).
    handle: AutocompleteHandle,
}

impl AutocompleteWidget {
    /// Build the input chrome (`SizedBox(InputFrame(TextInput(TextArea)))`),
    /// returning the `TextArea`'s id alongside it (known synchronously, no
    /// need to wait for `Update::WidgetAdded` — see [`TextAreaHandle`]).
    fn build_chrome(
        contents: &str,
        placeholder: ArcStr,
        disabled: bool,
        theme: &Theme,
    ) -> (NewWidget<SizedBox>, WidgetId) {
        // ── TextArea ──────────────────────────────────────────────────────────
        let text_area = widgets::TextArea::new_editable(contents)
            .with_style(StyleProperty::FontSize(theme.typography.size_body));

        let area_props = {
            let mut p = PropertySet::new();
            p.insert(ContentColor::new(theme.palette.text));
            p.insert(CaretColor {
                color: theme.palette.teal,
            });
            p.insert(SelectionColor {
                color: theme.palette.teal_soft,
            });
            p
        };

        let text_area_widget = NewWidget::new(text_area).with_props(area_props);
        let text_area_id = text_area_widget.id();

        // ── TextInput — stripped chrome ───────────────────────────────────────
        let text_input = widgets::TextInput::from_text_area(text_area_widget)
            .with_placeholder(placeholder)
            .with_clip(true);

        let mut text_input_widget = NewWidget::new(text_input);
        text_input_widget
            .properties
            .insert(Background::Color(TRANSPARENT));
        text_input_widget
            .properties
            .insert(BorderWidth::all(Length::const_px(0.0)));
        text_input_widget
            .properties
            .insert(Padding::all(Length::const_px(0.0)));
        text_input_widget
            .properties
            .insert(PlaceholderColor::new(theme.palette.text_muted));
        text_input_widget.options.disabled = disabled;

        // ── InputFrame — adds Esc-to-clear behaviour ──────────────────────────
        let input_frame = InputFrame::new(text_input_widget);

        // ── SizedBox — field chrome via masonry property system ───────────────
        let mut chrome_box = NewWidget::new(SizedBox::new(NewWidget::new(input_frame)));
        chrome_box
            .properties
            .insert(Background::Color(theme.palette.surface));
        chrome_box
            .properties
            .insert(BorderWidth::all(Length::const_px(1.0)));
        chrome_box
            .properties
            .insert(BorderColor::new(theme.palette.border));
        chrome_box
            .properties
            .insert(CornerRadius::all(Length::px(f64::from(theme.radius.small))));
        chrome_box.properties.insert(Padding::from_vh(
            Length::px(f64::from(theme.density.button_pad_v)),
            Length::px(f64::from(theme.density.button_pad_h)),
        ));

        (chrome_box, text_area_id)
    }

    /// In-tree constructor (fallback, no scope ancestor).
    #[must_use]
    pub(crate) fn new(
        contents: &str,
        placeholder: ArcStr,
        all_suggestions: Vec<ArcStr>,
        disabled: bool,
        theme: &Theme,
    ) -> Self {
        let (chrome, text_area_id) = Self::build_chrome(contents, placeholder, disabled, theme);
        let text_area_handle = TextAreaHandle::new();
        text_area_handle.set(text_area_id);
        let listbox_handle = LabelListHandle::new();
        let handle = AutocompleteHandle::new();
        let suggestion_list = SuggestionList::new(
            [],
            theme,
            listbox_handle.clone(),
            handle.clone(),
            text_area_handle.clone(),
        );

        let overlay = AnchoredOverlay::new(
            chrome,
            NewWidget::new(suggestion_list),
            false,
            PopoverAnchor::BottomStart,
        )
        .with_gap(OVERLAY_GAP);

        let filtered = compute_filtered(&all_suggestions, contents);

        Self {
            hosting: Hosting::InTree {
                overlay_host: NewWidget::new(overlay).to_pod(),
            },
            all_suggestions,
            contents: contents.to_owned(),
            filtered,
            open: false,
            highlighted: None,
            suppress_focus_open: false,
            theme: *theme,
            listbox_handle,
            handle,
        }
    }

    /// Portal-mode constructor: the suggestion list lives in the scope's portal
    /// slot under `key`. Only the input chrome is hosted here as a direct child.
    #[must_use]
    pub(crate) fn new_portal(
        config: AutocompleteConfig,
        scope: OverlayScopeHandle,
        key: u64,
        handle: AutocompleteHandle,
        listbox_handle: LabelListHandle,
        text_area_handle: &TextAreaHandle,
    ) -> Self {
        let AutocompleteConfig {
            contents,
            placeholder,
            all_suggestions,
            disabled,
            theme,
        } = config;
        let (chrome, text_area_id) = Self::build_chrome(&contents, placeholder, disabled, &theme);
        text_area_handle.set(text_area_id);
        let filtered = compute_filtered(&all_suggestions, &contents);

        Self {
            hosting: Hosting::Portal {
                chrome: chrome.to_pod(),
                scope,
                key,
                last_anchor_rect_window: None,
            },
            all_suggestions,
            contents,
            filtered,
            open: false,
            highlighted: None,
            suppress_focus_open: false,
            theme,
            listbox_handle,
            handle,
        }
    }
}

// --- MARK: INTERNAL HELPERS
impl AutocompleteWidget {
    fn open_on_focus(&mut self, ctx: &mut UpdateCtx<'_>) {
        if self.suppress_focus_open {
            self.suppress_focus_open = false;
            return;
        }
        self.filtered = compute_filtered(&self.all_suggestions, &self.contents);
        if self.filtered.is_empty() {
            return;
        }
        self.open = true;
        self.highlighted = None;
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                let items = self.filtered.clone();
                ctx.mutate_child_later(overlay_host, move |mut w| {
                    with_suggestion_list(&mut w, |list| SuggestionList::set_items(list, items));
                    AnchoredOverlay::set_overlay_visible(&mut w, true);
                });
            }
            Hosting::Portal { scope, key, .. } => {
                let Some(scope_id) = scope.widget_id() else {
                    return;
                };
                let key = *key;
                let owner_id = ctx.widget_id();
                let rect =
                    Rect::from_origin_size(ctx.to_window(Point::ZERO), ctx.border_box_size());
                let items = self.filtered.clone();
                if let Hosting::Portal {
                    last_anchor_rect_window,
                    ..
                } = &mut self.hosting
                {
                    *last_anchor_rect_window = Some(rect);
                }
                ctx.mutate_later(scope_id, move |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    let mut slot = OverlayScope::portal_slot_mut(&mut scope);
                    if let Some(mut child) = PortalSlot::child_mut(&mut slot, key) {
                        let mut pass = child.downcast::<Passthrough>();
                        let mut inner = Passthrough::child_mut(&mut pass);
                        let mut list = inner.downcast::<SuggestionList>();
                        SuggestionList::set_items(&mut list, items);
                    }
                });
                ctx.mutate_later(scope_id, move |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    OverlayScope::set_portal_visible(
                        &mut scope,
                        key,
                        true,
                        PortalVisibility {
                            owner: Some(owner_id),
                            owner_kind: OwnerKind::Autocomplete,
                            rect,
                            anchor: PopoverAnchor::BottomStart,
                            gap: OVERLAY_GAP_PX,
                        },
                    );
                });
                ctx.request_compose();
                ctx.request_anim_frame();
            }
        }
        ctx.request_paint_only();
    }

    fn close_overlay_later(&mut self, ctx: &mut ActionCtx<'_>) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.mutate_child_later(overlay_host, |mut w| {
                    with_suggestion_list(&mut w, |list| SuggestionList::set_items(list, []));
                    AnchoredOverlay::set_overlay_visible(&mut w, false);
                });
            }
            Hosting::Portal { scope, key, .. } => {
                let Some(scope_id) = scope.widget_id() else {
                    return;
                };
                let key = *key;
                ctx.mutate_later(scope_id, move |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    OverlayScope::set_portal_visible(
                        &mut scope,
                        key,
                        false,
                        PortalVisibility {
                            owner: None,
                            owner_kind: OwnerKind::Autocomplete,
                            rect: Rect::ZERO,
                            anchor: PopoverAnchor::BottomStart,
                            gap: 0.0,
                        },
                    );
                });
            }
        }
    }

    fn handle_text_changed(&mut self, ctx: &mut ActionCtx<'_>, text: &str) {
        self.contents.clear();
        self.contents.push_str(text);
        self.filtered = compute_filtered(&self.all_suggestions, text);
        self.highlighted = None;

        let should_open = !self.filtered.is_empty();
        let open_changed = self.open != should_open;
        self.open = should_open;

        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                let items = self.filtered.clone();
                ctx.mutate_child_later(overlay_host, move |mut w| {
                    with_suggestion_list(&mut w, |list| {
                        SuggestionList::set_items(list, items);
                    });
                    if open_changed {
                        AnchoredOverlay::set_overlay_visible(&mut w, should_open);
                    }
                });
            }
            Hosting::Portal { scope, key, .. } => {
                let Some(scope_id) = scope.widget_id() else {
                    ctx.submit_action::<AutocompleteAction>(AutocompleteAction::TextChanged(
                        text.to_owned(),
                    ));
                    ctx.set_handled();
                    return;
                };
                let key = *key;
                let items = self.filtered.clone();
                ctx.mutate_later(scope_id, move |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    let mut slot = OverlayScope::portal_slot_mut(&mut scope);
                    if let Some(mut child) = PortalSlot::child_mut(&mut slot, key) {
                        let mut pass = child.downcast::<Passthrough>();
                        let mut inner = Passthrough::child_mut(&mut pass);
                        let mut list = inner.downcast::<SuggestionList>();
                        SuggestionList::set_items(&mut list, items);
                    }
                });
                if open_changed {
                    let owner_id = ctx.widget_id();
                    let rect =
                        Rect::from_origin_size(ctx.to_window(Point::ZERO), ctx.border_box_size());
                    if let Hosting::Portal { last_anchor_rect_window, .. } = &mut self.hosting {
                        *last_anchor_rect_window = if should_open { Some(rect) } else { None };
                    }
                    ctx.mutate_later(scope_id, move |mut w| {
                        let mut scope = w.downcast::<OverlayScope>();
                        OverlayScope::set_portal_visible(
                            &mut scope,
                            key,
                            should_open,
                            PortalVisibility {
                                owner: Some(owner_id),
                                owner_kind: OwnerKind::Autocomplete,
                                rect,
                                anchor: PopoverAnchor::BottomStart,
                                gap: OVERLAY_GAP_PX,
                            },
                        );
                    });
                    if should_open {
                        ctx.request_compose();
                        ctx.request_anim_frame();
                    }
                }
            }
        }

        ctx.submit_action::<AutocompleteAction>(AutocompleteAction::TextChanged(text.to_owned()));
        ctx.set_handled();
    }

    fn select_suggestion(&mut self, ctx: &mut ActionCtx<'_>, selected: String) {
        let text = selected.clone();
        self.contents.clone_from(&selected);
        self.filtered.clear();
        self.highlighted = None;
        self.open = false;
        self.suppress_focus_open = true;

        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.mutate_child_later(overlay_host, move |mut w| {
                    with_suggestion_list(&mut w, |list| SuggestionList::set_items(list, []));
                    AnchoredOverlay::set_overlay_visible(&mut w, false);
                    with_text_area(&mut w, |ta| widgets::TextArea::reset_text(ta, &text));
                });
            }
            Hosting::Portal {
                chrome, scope, key, ..
            } => {
                let text_for_area = text.clone();
                ctx.mutate_child_later(chrome, move |mut w| {
                    let mut sb = w.downcast::<SizedBox>();
                    with_text_area_in_chrome(&mut sb, |ta| {
                        widgets::TextArea::reset_text(ta, &text_for_area);
                    });
                });
                let Some(scope_id) = scope.widget_id() else {
                    ctx.submit_action::<AutocompleteAction>(AutocompleteAction::TextChanged(
                        selected,
                    ));
                    ctx.set_handled();
                    ctx.request_paint_only();
                    return;
                };
                let key = *key;
                ctx.mutate_later(scope_id, move |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    OverlayScope::set_portal_visible(
                        &mut scope,
                        key,
                        false,
                        PortalVisibility {
                            owner: None,
                            owner_kind: OwnerKind::Autocomplete,
                            rect: Rect::ZERO,
                            anchor: PopoverAnchor::BottomStart,
                            gap: 0.0,
                        },
                    );
                });
            }
        }

        ctx.submit_action::<AutocompleteAction>(AutocompleteAction::TextChanged(selected));
        ctx.set_handled();
        ctx.request_paint_only();
        ctx.request_accessibility_update();
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
        let should_open = this.widget.open && !filtered.is_empty();
        let open_changed = this.widget.open != should_open;
        this.widget.filtered.clone_from(&filtered);
        this.widget.open = should_open;
        if this.widget.highlighted.is_some_and(|i| i >= filtered.len()) {
            this.widget.highlighted = None;
        }

        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut h = this.ctx.get_mut(overlay_host);
                with_suggestion_list(&mut h, |list| SuggestionList::set_items(list, filtered));
                if open_changed {
                    AnchoredOverlay::set_overlay_visible(&mut h, should_open);
                }
                with_text_area(&mut h, |ta| {
                    let current = ta.widget.text().to_string();
                    if current != contents {
                        widgets::TextArea::reset_text(ta, contents);
                    }
                });
            }
            Hosting::Portal {
                chrome, scope, key, ..
            } => {
                {
                    let mut c = this.ctx.get_mut(chrome);
                    let mut sb = c.downcast::<SizedBox>();
                    with_text_area_in_chrome(&mut sb, |ta| {
                        let current = ta.widget.text().to_string();
                        if current != contents {
                            widgets::TextArea::reset_text(ta, contents);
                        }
                    });
                }
                let scope_id = scope.widget_id();
                let key = *key;
                if let Some(scope_id) = scope_id {
                    let items = filtered.clone();
                    this.ctx.mutate_later(scope_id, move |mut w| {
                        let mut scope = w.downcast::<OverlayScope>();
                        let mut slot = OverlayScope::portal_slot_mut(&mut scope);
                        if let Some(mut child) = PortalSlot::child_mut(&mut slot, key) {
                            let mut pass = child.downcast::<Passthrough>();
                            let mut inner = Passthrough::child_mut(&mut pass);
                            let mut list = inner.downcast::<SuggestionList>();
                            SuggestionList::set_items(&mut list, items);
                        }
                    });
                    if open_changed {
                        this.ctx.mutate_later(scope_id, move |mut w| {
                            let mut scope = w.downcast::<OverlayScope>();
                            OverlayScope::set_portal_visible(
                                &mut scope,
                                key,
                                should_open,
                                PortalVisibility {
                                    owner: None,
                                    owner_kind: OwnerKind::Autocomplete,
                                    rect: Rect::ZERO,
                                    anchor: PopoverAnchor::BottomStart,
                                    gap: OVERLAY_GAP_PX,
                                },
                            );
                        });
                    }
                }
            }
        }
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

        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut h = this.ctx.get_mut(overlay_host);
                with_suggestion_list(&mut h, |list| SuggestionList::set_items(list, filtered));
                if open_changed {
                    AnchoredOverlay::set_overlay_visible(&mut h, should_open);
                }
            }
            Hosting::Portal { scope, key, .. } => {
                let scope_id = scope.widget_id();
                let key = *key;
                if let Some(scope_id) = scope_id {
                    let items = filtered.clone();
                    this.ctx.mutate_later(scope_id, move |mut w| {
                        let mut scope = w.downcast::<OverlayScope>();
                        let mut slot = OverlayScope::portal_slot_mut(&mut scope);
                        if let Some(mut child) = PortalSlot::child_mut(&mut slot, key) {
                            let mut pass = child.downcast::<Passthrough>();
                            let mut inner = Passthrough::child_mut(&mut pass);
                            let mut list = inner.downcast::<SuggestionList>();
                            SuggestionList::set_items(&mut list, items);
                        }
                    });
                    if open_changed {
                        this.ctx.mutate_later(scope_id, move |mut w| {
                            let mut scope = w.downcast::<OverlayScope>();
                            OverlayScope::set_portal_visible(
                                &mut scope,
                                key,
                                should_open,
                                PortalVisibility {
                                    owner: None,
                                    owner_kind: OwnerKind::Autocomplete,
                                    rect: Rect::ZERO,
                                    anchor: PopoverAnchor::BottomStart,
                                    gap: OVERLAY_GAP_PX,
                                },
                            );
                        });
                    }
                }
            }
        }
    }

    /// Update the placeholder text shown while the field is empty.
    pub(crate) fn set_placeholder(this: &mut WidgetMut<'_, Self>, placeholder: ArcStr) {
        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut h = this.ctx.get_mut(overlay_host);
                with_text_input(&mut h, |ti| {
                    widgets::TextInput::set_placeholder(ti, placeholder);
                });
            }
            Hosting::Portal { chrome, .. } => {
                let mut c = this.ctx.get_mut(chrome);
                let mut sb = c.downcast::<SizedBox>();
                with_text_input_in_chrome(&mut sb, |ti| {
                    widgets::TextInput::set_placeholder(ti, placeholder);
                });
            }
        }
    }

    /// Enable or disable the field (stops input, mutes visuals).
    pub(crate) fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        this.ctx.set_disabled(disabled);
        if disabled && this.widget.open {
            this.widget.open = false;
            this.widget.highlighted = None;
            this.widget.filtered.clear();
            match &mut this.widget.hosting {
                Hosting::InTree { overlay_host } => {
                    let mut h = this.ctx.get_mut(overlay_host);
                    with_suggestion_list(&mut h, |list| SuggestionList::set_items(list, []));
                    AnchoredOverlay::set_overlay_visible(&mut h, false);
                }
                Hosting::Portal { scope, key, .. } => {
                    let scope_id = scope.widget_id();
                    let key = *key;
                    if let Some(scope_id) = scope_id {
                        this.ctx.mutate_later(scope_id, move |mut w| {
                            let mut scope = w.downcast::<OverlayScope>();
                            OverlayScope::set_portal_visible(
                                &mut scope,
                                key,
                                false,
                                PortalVisibility {
                                    owner: None,
                                    owner_kind: OwnerKind::Autocomplete,
                                    rect: Rect::ZERO,
                                    anchor: PopoverAnchor::BottomStart,
                                    gap: 0.0,
                                },
                            );
                        });
                    }
                }
            }
        }
    }

    /// Re-apply theme colors to the input chrome and suggestion list.
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;

        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut h = this.ctx.get_mut(overlay_host);

                {
                    let mut primary = AnchoredOverlay::primary_mut(&mut h);
                    let mut sb = primary.downcast::<SizedBox>();
                    sb.insert_prop(Background::Color(theme.palette.surface));
                    sb.insert_prop(BorderColor::new(theme.palette.border));
                    sb.insert_prop(CornerRadius::all(Length::px(f64::from(theme.radius.small))));

                    if let Some(mut child) = SizedBox::child_mut(&mut sb) {
                        let mut frame = child.downcast::<InputFrame>();
                        let mut inner = InputFrame::child_mut(&mut frame);
                        let mut ti = inner.downcast::<widgets::TextInput>();
                        ti.insert_prop(PlaceholderColor::new(theme.palette.text_muted));
                        let mut ta = widgets::TextInput::text_mut(&mut ti);
                        ta.insert_prop(ContentColor::new(theme.palette.text));
                        ta.insert_prop(CaretColor {
                            color: theme.palette.teal,
                        });
                        ta.insert_prop(SelectionColor {
                            color: theme.palette.teal_soft,
                        });
                        widgets::TextArea::insert_style(
                            &mut ta,
                            StyleProperty::FontSize(theme.typography.size_body),
                        );
                    }
                }

                with_suggestion_list(&mut h, |list| SuggestionList::set_theme(list, theme));
            }
            Hosting::Portal {
                chrome, scope, key, ..
            } => {
                {
                    let mut c = this.ctx.get_mut(chrome);
                    let mut sb = c.downcast::<SizedBox>();
                    sb.insert_prop(Background::Color(theme.palette.surface));
                    sb.insert_prop(BorderColor::new(theme.palette.border));
                    sb.insert_prop(CornerRadius::all(Length::px(f64::from(theme.radius.small))));

                    if let Some(mut child) = SizedBox::child_mut(&mut sb) {
                        let mut frame = child.downcast::<InputFrame>();
                        let mut inner = InputFrame::child_mut(&mut frame);
                        let mut ti = inner.downcast::<widgets::TextInput>();
                        ti.insert_prop(PlaceholderColor::new(theme.palette.text_muted));
                        let mut ta = widgets::TextInput::text_mut(&mut ti);
                        ta.insert_prop(ContentColor::new(theme.palette.text));
                        ta.insert_prop(CaretColor {
                            color: theme.palette.teal,
                        });
                        ta.insert_prop(SelectionColor {
                            color: theme.palette.teal_soft,
                        });
                        widgets::TextArea::insert_style(
                            &mut ta,
                            StyleProperty::FontSize(theme.typography.size_body),
                        );
                    }
                }

                let scope_id = scope.widget_id();
                let key = *key;
                let theme_copy = *theme;
                if let Some(scope_id) = scope_id {
                    this.ctx.mutate_later(scope_id, move |mut w| {
                        let mut scope = w.downcast::<OverlayScope>();
                        let mut slot = OverlayScope::portal_slot_mut(&mut scope);
                        if let Some(mut child) = PortalSlot::child_mut(&mut slot, key) {
                            let mut pass = child.downcast::<Passthrough>();
                            let mut inner = Passthrough::child_mut(&mut pass);
                            let mut list = inner.downcast::<SuggestionList>();
                            SuggestionList::set_theme(&mut list, &theme_copy);
                        }
                    });
                }
            }
        }

        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    /// Notify that the portal slot was dismissed by an outside press. Syncs
    /// `open`/`highlighted`/`filtered` — the slot has already hidden itself.
    pub(crate) fn mark_closed(this: &mut WidgetMut<'_, Self>) {
        if !this.widget.open {
            return;
        }
        this.widget.open = false;
        this.widget.highlighted = None;
        this.widget.filtered.clear();

        // Also tell the scope/overlay to hide (idempotent if already hidden).
        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut w = this.ctx.get_mut(overlay_host);
                AnchoredOverlay::set_overlay_visible(&mut w, false);
            }
            Hosting::Portal { scope, key, .. } => {
                let scope_id = scope.widget_id();
                let key = *key;
                if let Some(scope_id) = scope_id {
                    this.ctx.mutate_later(scope_id, move |mut w| {
                        let mut scope = w.downcast::<OverlayScope>();
                        OverlayScope::set_portal_visible(
                            &mut scope,
                            key,
                            false,
                            PortalVisibility {
                                owner: None,
                                owner_kind: OwnerKind::Autocomplete,
                                rect: Rect::ZERO,
                                anchor: PopoverAnchor::BottomStart,
                                gap: 0.0,
                            },
                        );
                    });
                }
            }
        }
        this.ctx.request_paint_only();
    }

    /// Handle a suggestion selection that arrived from the portal
    /// [`SuggestionListView`] via `mutate_later`. Updates widget state, resets
    /// the text area, closes the portal slot, and submits
    /// [`AutocompleteAction::TextChanged`].
    pub(crate) fn portal_select(this: &mut WidgetMut<'_, Self>, text: String) {
        this.widget.contents.clone_from(&text);
        this.widget.filtered.clear();
        this.widget.highlighted = None;
        this.widget.open = false;
        // Do NOT set suppress_focus_open here. In portal mode, ChildFocusChanged(true)
        // fires while self.open is still true (portal_select is deferred via mutate_later),
        // so open_on_focus is never called and the flag would never be consumed — leaving it
        // stuck and silently preventing the dropdown from opening on the next focus event.
        // The self.open == true guard already prevents the reopen without the flag.

        let text_for_area = text.clone();

        // Reset the text area in the chrome.
        if let Hosting::Portal { chrome, .. } = &mut this.widget.hosting {
            let mut c = this.ctx.get_mut(chrome);
            let mut sb = c.downcast::<SizedBox>();
            with_text_area_in_chrome(&mut sb, |ta| {
                widgets::TextArea::reset_text(ta, &text_for_area);
            });
        }

        // Close the portal slot.
        let (scope_id, key) = match &this.widget.hosting {
            Hosting::Portal { scope, key, .. } => (scope.widget_id(), *key),
            Hosting::InTree { .. } => unreachable!("portal_select called in in-tree mode"),
        };
        if let Some(scope_id) = scope_id {
            this.ctx.mutate_later(scope_id, move |mut w| {
                let mut scope = w.downcast::<OverlayScope>();
                OverlayScope::set_portal_visible(
                    &mut scope,
                    key,
                    false,
                    PortalVisibility {
                        owner: None,
                        owner_kind: OwnerKind::Autocomplete,
                        rect: Rect::ZERO,
                        anchor: PopoverAnchor::BottomStart,
                        gap: 0.0,
                    },
                );
            });
        }

        this.ctx
            .submit_action::<AutocompleteAction>(AutocompleteAction::TextChanged(text));
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
                    self.handle_text_changed(ctx, text);
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

        // ── Suggestion selected by click (in-tree mode only) ──────────────────
        if let Some(SuggestionSelected(text)) = action.downcast_ref::<SuggestionSelected>() {
            self.select_suggestion(ctx, text.to_string());
        }
    }

    /// Tab moves real focus into the open listbox; once there,
    /// [`LabelList::on_text_event`] handles arrow keys/Enter/Escape directly.
    ///
    /// Arrow/Home/End list-navigation can't be intercepted *here* while focus
    /// is still in the text field: masonry's built-in `TextArea`
    /// unconditionally claims those keys for cursor movement — even on a
    /// single line, where the move is a no-op — and stops the event from
    /// reaching any ancestor's `on_text_event`. Tab isn't one of the keys
    /// `TextArea` claims, so it reaches us and we can redirect it.
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if !self.open {
            return;
        }
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        if key.key == Key::Named(NamedKey::Tab)
            && !key.modifiers.shift()
            && let Some(listbox_id) = self.listbox_handle.widget_id()
        {
            ctx.set_focus(listbox_id);
            ctx.set_handled();
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => {
                self.handle.set(ctx.widget_id());
            }
            // Close when stashed mid-open.
            Update::StashedChanged(true) if self.open => {
                self.open = false;
                self.highlighted = None;
                self.filtered.clear();
                match &mut self.hosting {
                    Hosting::InTree { overlay_host } => {
                        ctx.mutate_child_later(overlay_host, |mut w| {
                            with_suggestion_list(&mut w, |list| {
                                SuggestionList::set_items(list, []);
                            });
                            AnchoredOverlay::set_overlay_visible(&mut w, false);
                        });
                    }
                    Hosting::Portal { scope, key, .. } => {
                        let Some(scope_id) = scope.widget_id() else {
                            return;
                        };
                        let key = *key;
                        ctx.mutate_later(scope_id, move |mut w| {
                            let mut scope = w.downcast::<OverlayScope>();
                            OverlayScope::set_portal_visible(
                                &mut scope,
                                key,
                                false,
                                PortalVisibility {
                                    owner: None,
                                    owner_kind: OwnerKind::Autocomplete,
                                    rect: Rect::ZERO,
                                    anchor: PopoverAnchor::BottomStart,
                                    gap: 0.0,
                                },
                            );
                        });
                    }
                }
                ctx.request_paint_only();
            }
            // In-tree: close when focus truly leaves our subtree (Tab-out,
            // clicking elsewhere, etc.).  Portal: the slot's own
            // outside-press dismissal handles this instead.
            //
            // Skip if a pointer-capture is active — that means the user
            // pressed Down on a list item and the pointer-Up (click-select)
            // hasn't fired yet.  Closing here would clear `self.filtered`
            // and prevent the subsequent `SuggestionSelected` action from
            // finding the item, so the text field never auto-fills.
            Update::ChildFocusChanged(false)
                if self.open
                    && matches!(self.hosting, Hosting::InTree { .. })
                    && ctx.pointer_capture_target_id().is_none() =>
            {
                self.open = false;
                self.highlighted = None;
                self.filtered.clear();
                if let Hosting::InTree { overlay_host } = &mut self.hosting {
                    ctx.mutate_child_later(overlay_host, |mut w| {
                        with_suggestion_list(&mut w, |list| SuggestionList::set_items(list, []));
                        AnchoredOverlay::set_overlay_visible(&mut w, false);
                    });
                }
                ctx.request_paint_only();
            }
            // Open the dropdown when focus enters the input field.
            Update::ChildFocusChanged(true) if !self.open && !self.all_suggestions.is_empty() => {
                self.open_on_focus(ctx);
            }
            _ => {}
        }
    }

    /// Re-anchors a still-open portal-mode suggestion list as we move in
    /// window space (e.g. scrolling). No-op in-tree.
    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        if !self.open {
            return;
        }
        let Hosting::Portal {
            scope,
            key,
            last_anchor_rect_window,
            ..
        } = &mut self.hosting
        else {
            return;
        };
        let Some(scope_id) = scope.widget_id() else {
            return;
        };
        let rect = Rect::from_origin_size(ctx.to_window(Point::ZERO), ctx.border_box_size());
        if *last_anchor_rect_window == Some(rect) {
            return;
        }
        *last_anchor_rect_window = Some(rect);
        let key = *key;
        ctx.mutate_later(scope_id, move |mut w| {
            let mut scope = w.downcast::<OverlayScope>();
            OverlayScope::set_portal_placement(&mut scope, key, rect);
        });
    }

    /// Keeps compose running every frame while portal-mode is open, so the
    /// list re-anchors regardless of pointer position or which ancestor scrolled.
    fn on_anim_frame(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, _: u64) {
        if !self.open || !matches!(self.hosting, Hosting::Portal { .. }) {
            return;
        }
        ctx.request_compose();
        ctx.request_anim_frame();
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => ctx.register_child(overlay_host),
            Hosting::Portal { chrome, .. } => ctx.register_child(chrome),
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.redirect_measurement(overlay_host, axis, cross_length)
            }
            Hosting::Portal { chrome, .. } => ctx.redirect_measurement(chrome, axis, cross_length),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.run_layout(overlay_host, size);
                ctx.place_child(overlay_host, Point::ORIGIN);
                ctx.derive_baselines(overlay_host);
            }
            Hosting::Portal { chrome, .. } => {
                ctx.run_layout(chrome, size);
                ctx.place_child(chrome, Point::ORIGIN);
                ctx.derive_baselines(chrome);
            }
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural — children paint themselves.
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
        // Always reflect actual state (not just when open) — AT relies on an
        // explicit `false` to announce "collapsed", not just the property's
        // absence.
        node.set_expanded(self.open);
        if self.open
            && let Some(listbox_id) = self.listbox_handle.widget_id()
        {
            node.push_controlled(listbox_id.into());
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        match &self.hosting {
            Hosting::InTree { overlay_host } => ChildrenIds::from_slice(&[overlay_host.id()]),
            Hosting::Portal { chrome, .. } => ChildrenIds::from_slice(&[chrome.id()]),
        }
    }
}

#[cfg(test)]
mod scroll_tests {
    use masonry::kurbo::Vec2;
    use masonry::testing::TestHarness;

    use super::*;

    /// Real rendering proof that the `ScrollView`-backed `SuggestionList`
    /// actually scrolls: more items than fit in `MAX_LIST_HEIGHT`, drive a
    /// real wheel event through `TestHarness`, and compare painted pixels —
    /// not internal state — before and after.
    #[test]
    fn wheel_scroll_changes_painted_content() {
        let theme = Theme::default();
        let items: Vec<ArcStr> = (0..30).map(|i| ArcStr::from(format!("Item {i}"))).collect();
        let list = SuggestionList::new(
            items,
            &theme,
            LabelListHandle::default(),
            AutocompleteHandle::default(),
            TextAreaHandle::default(),
        );

        let mut harness = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(list),
            (200, 200),
        );

        // Hover a point inside the top padding (above any item rect) so the
        // wheel events below don't also toggle hover-highlight paint.
        harness.mouse_move((100.0, 1.0));
        let top = harness.render().clone();

        harness.mouse_wheel(Vec2::new(0.0, -1_000_000.0));
        let scrolled_one_way = harness.render().clone();

        harness.mouse_wheel(Vec2::new(0.0, 2_000_000.0));
        let scrolled_other_way = harness.render().clone();

        assert_ne!(
            top, scrolled_one_way,
            "wheel scroll did not change painted content at all"
        );
        assert_ne!(
            scrolled_one_way, scrolled_other_way,
            "scrolling to the opposite extreme produced identical pixels"
        );
    }
}

#[cfg(test)]
mod accessibility_tests {
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{NewWidget, TextEvent, WidgetRef};
    use masonry::testing::TestHarness;
    use masonry::widgets::TextArea;

    use super::*;

    fn find_text_area(widget: WidgetRef<'_, dyn Widget>) -> Option<WidgetRef<'_, TextArea<true>>> {
        if let Some(area) = widget.downcast::<TextArea<true>>() {
            return Some(area);
        }
        widget.children().into_iter().find_map(find_text_area)
    }

    /// Builds an in-tree autocomplete with 3 fixed suggestions and locates
    /// its combobox/text-area ids, ready for focus/keyboard driving.
    fn harness_with_fruit() -> (TestHarness<AutocompleteWidget>, WidgetId, WidgetId) {
        let theme = Theme::default();
        let suggestions: Vec<ArcStr> = ["Apple", "Banana", "Cherry"]
            .into_iter()
            .map(ArcStr::from)
            .collect();
        let widget =
            AutocompleteWidget::new("", ArcStr::from("Pick a fruit"), suggestions, false, &theme);

        let harness = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(widget),
            (300, 300),
        );

        let autocomplete_id = harness.root_id();
        let text_area_id = find_text_area(harness.root_widget().as_dyn())
            .expect("autocomplete should host a TextArea")
            .id();
        (harness, autocomplete_id, text_area_id)
    }

    /// Real accessibility-tree proof, driven through the actual masonry
    /// focus/keyboard pipeline (not internal state): focusing the field opens
    /// the popup and wires the ARIA combobox relationship (`expanded` +
    /// `controls` -> a real `Role::ListBox` node). Arrow-key list navigation
    /// can't be driven from the textbox (masonry's `TextArea` unconditionally
    /// claims those keys for cursor movement before any ancestor sees them —
    /// see [`AutocompleteWidget::on_text_event`]), so Tab must move real
    /// focus into the listbox first; from there, arrow keys move
    /// `active_descendant` to real `Role::ListBoxOption` nodes with the right
    /// accessible name and `aria-selected`.
    #[test]
    fn tab_into_listbox_and_arrow_keys_set_active_descendant() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit();

        harness.render();
        {
            let node = harness.access_node(autocomplete_id).expect("combobox node");
            assert_eq!(node.role(), Role::ComboBox);
            assert_eq!(
                node.data().is_expanded(),
                Some(false),
                "closed before focus"
            );
        }

        // Focusing the text field opens the dropdown and wires aria-controls.
        harness.focus_on(Some(text_area_id));
        harness.render();
        {
            let node = harness.access_node(autocomplete_id).expect("combobox node");
            assert_eq!(node.data().is_expanded(), Some(true));
            let controlled: Vec<_> = node.controls().collect();
            assert_eq!(
                controlled.len(),
                1,
                "combobox should control exactly the listbox"
            );
            assert_eq!(controlled[0].role(), Role::ListBox);
            assert!(
                controlled[0].active_descendant().is_none(),
                "no keyboard highlight yet"
            );
        }

        // Tab moves real focus into the listbox (not the next form field).
        // Verify by checking the newly-focused widget's accessibility role.
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        let focused_role = harness
            .focused_widget_id()
            .and_then(|id| harness.access_node(id))
            .map(|n| n.role());
        assert_eq!(
            focused_role,
            Some(Role::ListBox),
            "Tab should move focus into the open listbox"
        );

        // ArrowDown highlights "Apple": active-descendant resolves to a real
        // ListBoxOption node with the right name and aria-selected.
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        harness.render();
        {
            let node = harness.access_node(autocomplete_id).expect("combobox node");
            let listbox = node.controls().next().expect("controls the listbox");
            let active = listbox.active_descendant().expect("active descendant set");
            assert_eq!(active.role(), Role::ListBoxOption);
            assert_eq!(active.label().as_deref(), Some("Apple"));
            assert_eq!(active.is_selected(), Some(true));
        }

        // ArrowDown again moves the relationship to "Banana".
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        harness.render();
        {
            let node = harness.access_node(autocomplete_id).expect("combobox node");
            let listbox = node.controls().next().expect("controls the listbox");
            let active = listbox.active_descendant().expect("active descendant set");
            assert_eq!(active.label().as_deref(), Some("Banana"));
        }
    }

    /// Pressing Enter while focus is in the listbox selects the highlighted
    /// item, closes the dropdown, and returns real focus to the text field —
    /// driven through the real focus/keyboard pipeline, not internal state.
    #[test]
    fn enter_in_listbox_selects_closes_and_returns_focus_to_input() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit();

        harness.focus_on(Some(text_area_id));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));

        assert_eq!(
            harness.focused_widget_id(),
            Some(text_area_id),
            "Enter-selection should return focus to the input"
        );
        let area = find_text_area(harness.root_widget().as_dyn()).expect("TextArea");
        assert_eq!(area.text().to_string(), "Apple");

        harness.render();
        let node = harness.access_node(autocomplete_id).expect("combobox node");
        assert_eq!(
            node.data().is_expanded(),
            Some(false),
            "closed after selection"
        );
    }

    /// Escape while focus is in the listbox closes the dropdown without
    /// selecting and returns real focus to the text field.
    #[test]
    fn escape_in_listbox_closes_without_selecting_and_returns_focus_to_input() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit();

        harness.focus_on(Some(text_area_id));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));

        assert_eq!(
            harness.focused_widget_id(),
            Some(text_area_id),
            "Escape should return focus to the input"
        );
        let area = find_text_area(harness.root_widget().as_dyn()).expect("TextArea");
        assert_eq!(area.text().to_string(), "", "Escape must not select");

        harness.render();
        let node = harness.access_node(autocomplete_id).expect("combobox node");
        assert_eq!(
            node.data().is_expanded(),
            Some(false),
            "closed after Escape"
        );
    }

    /// Clicking an item in the open list auto-fills the text field even
    /// though the pointer-down clears keyboard focus before pointer-up fires.
    /// Previously, `ChildFocusChanged(false)` would close the dropdown and
    /// clear `filtered` between the two half-events, so `SuggestionSelected`
    /// arrived with no matching item and the field was never updated.
    #[test]
    fn click_selection_fills_the_text_field() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit();

        // Focus the input — opens the dropdown.
        harness.focus_on(Some(text_area_id));
        harness.render();
        {
            let node = harness.access_node(autocomplete_id).expect("combobox");
            assert_eq!(node.data().is_expanded(), Some(true));
        }

        // Find the first SuggestionItem (ListBoxOption) in the widget tree.
        let mut first_item_id = None;
        harness.inspect_widgets(|w| {
            if first_item_id.is_none() && w.accessibility_role() == Role::ListBoxOption {
                first_item_id = Some(w.id());
            }
        });

        let item_id = first_item_id.expect("dropdown should have rendered list items");

        // Use unchecked move + manual press/release to bypass the harness's
        // "widget must be visible in the render layer" constraint (the item
        // lives inside a clipped ScrollView, which confuses the bounding-box
        // check even though it is genuinely hittable).
        harness.mouse_move_to_unchecked(item_id);
        harness.mouse_button_press(Some(masonry::core::PointerButton::Primary));
        harness.mouse_button_release(Some(masonry::core::PointerButton::Primary));

        let area = find_text_area(harness.root_widget().as_dyn()).expect("TextArea");
        assert_eq!(
            area.text().to_string(),
            "Apple",
            "click should auto-fill the text field"
        );
        assert_eq!(
            harness.focused_widget_id(),
            Some(text_area_id),
            "focus returns to input after click"
        );
        harness.render();
        let node = harness.access_node(autocomplete_id).expect("combobox");
        assert_eq!(
            node.data().is_expanded(),
            Some(false),
            "closed after click-select"
        );
    }
}
