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

use masonry::accesskit::{HasPopup, Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ActionCtx, ArcStr, ChildrenIds, ComposeCtx, ErasedAction, EventCtx, LayoutCtx,
    MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenDef, LenReq, Length, SizeDef};
use masonry::properties::{
    Background, BorderColor, BorderWidth, CaretColor, ContentColor, CornerRadius, Padding,
    PlaceholderColor, SelectionColor,
};
use masonry::widgets::{self, Label, Passthrough, SizedBox, TextAction};

use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::components::input::widget::{InputCleared, InputFrame};
use crate::components::input::{stripped_text_input_props, text_area_props};
use crate::components::item_list;
use crate::components::popover::PopoverAnchor;
use crate::components::scroll_container::widget::{ContentClip, ScrollView};
use crate::focus_ring::{FOCUS_RING_INSET, paint_focus_ring};
use crate::overlay_portal::{OwnerKind, PortalSlot, PortalVisibility};
use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

/// Suggestion list chrome corner radius — owned by the `Radii` domain, not density.
const LIST_CORNER: f64 = 5.0;
/// Suggestion list border stroke width — hairline chrome, not density-scaled.
const LIST_BORDER: f64 = 1.0;
/// Inset of the keyboard-highlight ring from the item bounds — focus chrome, not density-scaled.
const HIGHLIGHT_RING_INSET: f64 = FOCUS_RING_INSET;
/// Minimum suggestion list width in logical pixels — a clamp, not a density-scaled dimension.
const MIN_LIST_WIDTH: f64 = 80.0;
/// Maximum visible height for the suggestion list before it scrolls, px — a clamp, not a density-scaled dimension.
const MAX_LIST_HEIGHT: f64 = 200.0;
/// Max results for a typed-prefix match. Browsing the unfiltered list (empty
/// query) isn't capped — the list scrolls — but typing should narrow to a
/// short, scannable set rather than every match in a huge dataset.
const MAX_SUGGESTIONS: usize = 20;
/// Gap between the input field and the suggestion list overlay, px — a fixed
/// anchor offset, not a density-scaled spacing token.
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

widget_id_handle!(
    /// Self-filling handle to an [`AutocompleteWidget`]'s widget id, filled at
    /// `Update::WidgetAdded`. Given to portal-mounted [`super::view::SuggestionListView`]
    /// so an item selection can `mutate_later` back into the widget to close the
    /// suggestion list and emit the selected text.
    AutocompleteHandle
);

widget_id_handle!(
    /// Self-filling handle to a [`LabelList`]'s widget id, filled at
    /// `Update::WidgetAdded`. Lets [`AutocompleteWidget`] expose `aria-controls`
    /// pointing at the listbox. The active-descendant relationship is set
    /// directly by [`LabelList`] itself (see its `accessibility` impl), since it
    /// always has immediate, local access to which item is highlighted.
    LabelListHandle
);

widget_id_handle!(
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
    TextAreaHandle
);

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
        // in `view::SuggestionListView::build`). In portal mode, `LabelList` is deep inside
        // a `PortalSlot` subtree that is separate from `AutocompleteWidget`'s subtree,
        // so the action from `LabelList.id` has no registered view path and is dropped
        // with an "unknown widget" error. Re-emitting here promotes the action to
        // `SuggestionList.id`, which IS registered, so it reaches `view::SuggestionListView::message`.
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
        let labels = items
            .into_iter()
            .map(|t| Self::make_item(t, theme))
            .collect();
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
        item_list::item_height(&self.theme.density)
    }

    fn pad_h(&self) -> f64 {
        item_list::pad_h(&self.theme.density)
    }

    fn list_pad_v(&self) -> f64 {
        item_list::menu_pad_v(&self.theme.density)
    }

    fn hit_item(&self, local: Point) -> Option<usize> {
        item_list::hit_item(&self.item_rects, local)
    }

    fn to_local(ctx: &EventCtx<'_>, window_pos: Point) -> Point {
        item_list::to_local(ctx, window_pos)
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
        if let Some(i) = index {
            // item_rects is cleared by set_items and repopulated in layout. If a
            // keypress arrives before layout runs the vec is empty; fall back to
            // computing the rect from theme metrics so the scroll request isn't lost.
            let rect = self.item_rects.get(i).copied().unwrap_or_else(|| {
                let item_h = self.item_height();
                let i_f64 = f64::from(u32::try_from(i).unwrap_or(u32::MAX));
                let y = self.list_pad_v() + i_f64 * item_h;
                Rect::new(0.0, y, 0.0, y + item_h)
            });
            ctx.request_scroll_to(rect);
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
        this.widget.label_texts.clone_from(&items);
        this.widget.labels = items
            .into_iter()
            .map(|t| Self::make_item(t, &theme))
            .collect();
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

    /// Must be `true`, not just for `ctx.set_focus(listbox_id)` (forward-Tab
    /// handling in [`AutocompleteWidget::on_text_event`]) — masonry's
    /// accessibility pass only adds `accesskit::Action::Focus` to a widget's
    /// a11y node when `accepts_focus()` is true, so screen readers/AT tools
    /// that focus elements via the accessibility action API (rather than
    /// raw key events) need this to reach the listbox at all.
    ///
    /// This does mean native backward-Tab search can treat this widget as a
    /// valid Shift+Tab target when the input (not the list) holds focus —
    /// in-tree, this widget lives in the same subtree as the input, so
    /// backward cycling from the input could land here instead of exiting
    /// to the previous page field. `on_text_event`'s explicit Shift+Tab
    /// interception (for both hosting modes) exists specifically to
    /// preempt that native search rather than relying on `accepts_focus`
    /// to keep this widget out of it.
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
            // `is_active()` alone, not `is_hovered()`: masonry clears hovered
            // status once a captured pointer drags outside this widget's
            // bounds (is_active stays true throughout the capture). Gating on
            // is_hovered() would skip this arm entirely for a press-drag-
            // release-outside gesture — no selection, no set_handled(), and
            // no close, since the scope's outside-press dismissal only fires
            // on Down events landing outside, and this gesture's Down was
            // inside (that's what started the capture). `hit_item` below
            // already resolves to `None` for an outside release, so dropping
            // the hover gate doesn't change behavior for a real miss.
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) if ctx.is_active() => {
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
            // Escape or Tab (either direction): return focus to the text
            // input explicitly and close, rather than letting masonry cycle
            // focus starting from the listbox's actual tree position. The
            // listbox lives in the portal slot, mounted elsewhere in the
            // tree, so an unhandled Tab would make masonry search for the
            // next focusable widget from there instead of from the
            // autocomplete's page position, landing on the wrong widget in
            // either direction. This lands back on the text field instead;
            // a second real Tab/Shift+Tab from there cycles correctly since
            // the input's tree position does match its page position.
            Key::Named(NamedKey::Escape | NamedKey::Tab) => {
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
        _cross_length: Option<Length>,
    ) -> Length {
        let item_h = self.item_height();
        let pad_h = self.pad_h();
        let n = self.labels.len();
        match axis {
            Axis::Vertical => {
                let n_f64 = f64::from(u32::try_from(n).unwrap_or(u32::MAX));
                Length::px(self.list_pad_v() * 2.0 + item_h * n_f64)
            }
            Axis::Horizontal => {
                // Each item's height is fixed at `item_h` regardless of any
                // incoming cross-axis constraint (see the `Axis::Vertical` arm
                // above, which also ignores inbound constraints and always
                // reports the intrinsic height) — this must match `layout()`,
                // which allocates exactly `item_h` per label. `pad_h` is
                // horizontal padding and has no bearing on this vertical hint.
                let row_height = Some(Length::px(item_h));
                let mut max_w = MIN_LIST_WIDTH;
                for label in &mut self.labels {
                    let w = ctx
                        .compute_length(
                            label,
                            len_req.into(),
                            LayoutSize::maybe(Axis::Vertical, row_height),
                            Axis::Horizontal,
                            row_height,
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

        let mut y = self.list_pad_v();
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
        node.set_selected(self.selected);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.label.id()])
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Case-insensitive prefix match, capped at [`MAX_SUGGESTIONS`].
/// When `query` is empty, the first [`MAX_SUGGESTIONS`] entries are returned —
/// the dropdown shows an initial window when the field first receives focus.
/// The cap applies to the empty query too: the list is not virtualized, so an
/// uncapped result would materialize one widget per entry (see the tracked
/// follow-up to move this onto the shared virtualization substrate).
///
/// `all_lower` must be the pre-lowercased mirror of `all` (same length,
/// same order). This avoids a `to_lowercase` allocation per item on the
/// hot keystroke path.
pub(crate) fn compute_filtered(all: &[ArcStr], all_lower: &[String], query: &str) -> Vec<ArcStr> {
    if query.is_empty() {
        return all.iter().take(MAX_SUGGESTIONS).cloned().collect();
    }
    let q = query.to_lowercase();
    all.iter()
        .zip(all_lower)
        .filter(|(_, lower)| lower.starts_with(&q))
        .map(|(s, _)| s.clone())
        .take(MAX_SUGGESTIONS)
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

/// Apply all theme properties to the input chrome (the `SizedBox` that wraps
/// `InputFrame → TextInput → TextArea`). Called from both hosting arms of
/// `set_theme` to avoid spelling out the traversal twice.
fn apply_chrome_theme(sb: &mut WidgetMut<'_, SizedBox>, theme: &Theme) {
    sb.insert_prop(Background::Color(theme.palette.surface));
    sb.insert_prop(BorderColor::new(theme.palette.border));
    sb.insert_prop(CornerRadius::all(Length::px(f64::from(theme.radius.small))));
    sb.insert_prop(Padding::from_vh(
        Length::px(f64::from(theme.density.button_pad_v)),
        Length::px(f64::from(theme.density.button_pad_h)),
    ));
    with_text_input_in_chrome(sb, |ti| {
        ti.insert_prop(PlaceholderColor::new(theme.palette.text_muted));
        let mut ta = widgets::TextInput::text_mut(ti);
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
    });
}

/// Canonical close-portal sentinel. Every "hide the portal slot" path uses
/// this so the `PortalVisibility` fields can't silently diverge across callers.
fn close_portal_slot(scope: &mut WidgetMut<'_, OverlayScope>, key: u64) {
    OverlayScope::set_portal_visible(
        scope,
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

/// Navigate to the portal-mounted `SuggestionList` for `key` — behind the
/// scope's `PortalSlot` → `Passthrough` wrapping — and invoke `f`, when the
/// slot child is currently mounted. The Portal-mode counterpart to
/// [`with_suggestion_list`]; called from inside `mutate_later` closures so
/// every setter drives the list through one downcast chain instead of five
/// hand-copied ones.
fn with_portal_list(
    scope: &mut WidgetMut<'_, OverlayScope>,
    key: u64,
    f: impl FnOnce(&mut WidgetMut<'_, SuggestionList>),
) {
    let mut slot = OverlayScope::portal_slot_mut(scope);
    if let Some(mut child) = PortalSlot::child_mut(&mut slot, key) {
        let mut pass = child.downcast::<Passthrough>();
        let mut inner = Passthrough::child_mut(&mut pass);
        let mut list = inner.downcast::<SuggestionList>();
        f(&mut list);
    }
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
///   [`crate::overlay_portal::OverlayPortal`] via [`super::view::SuggestionListView`]
///   and mounted in the always-on-top portal slot, so it paints above siblings.
///
/// Intercepts actions from descendants and re-emits
/// [`AutocompleteAction::TextChanged`] for the view layer.
pub(crate) struct AutocompleteWidget {
    hosting: Hosting,
    all_suggestions: Vec<ArcStr>,
    /// Lower-case mirror of [`Self::all_suggestions`], pre-computed at
    /// set time so `compute_filtered` never allocates on the hot path.
    all_suggestions_lower: Vec<String>,
    /// Mirrors the host-controlled text, kept in sync via [`Self::set_contents`]
    /// and updated eagerly in action handlers for keyboard nav and selection.
    contents: String,
    /// Pre-computed filtered slice of [`Self::all_suggestions`].
    filtered: Vec<ArcStr>,
    open: bool,
    /// Suppresses the next [`Self::open_on_focus`] call so that click-based
    /// selection does not reopen the dropdown when focus returns to the input.
    ///
    /// Click creates a focus gap (`TextArea` → `None` → `TextArea`) that fires
    /// `ChildFocusChanged(true)` on this widget, which would normally reopen
    /// the list. Set by [`Self::select_suggestion`] / [`Self::portal_select`]
    /// and cleared by the first [`Self::open_on_focus`] that sees it.
    suppress_focus_open: bool,
    /// Set the moment `on_text_event`'s forward-Tab handler calls
    /// `ctx.set_focus(listbox_id)`, consumed by the very next
    /// `ChildFocusChanged(false)`. Portal mode's listbox lives outside our
    /// subtree, so this transfer fires that event on us — we can't tell it
    /// apart from a real blur by comparing `ctx.focus_target_id()` against
    /// the listbox id, because masonry updates `global_state.focused_widget`
    /// *after* dispatching `ChildFocusChanged`, so that accessor still
    /// reflects the *previous* focus target at the point the handler runs.
    /// This flag sidesteps that entirely: we already know, synchronously,
    /// that we just initiated this exact transfer ourselves.
    focus_moving_to_listbox: bool,
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

        let text_area_widget = NewWidget::new(text_area).with_props(text_area_props(theme));
        let text_area_id = text_area_widget.id();

        // ── TextInput — stripped chrome ───────────────────────────────────────
        let text_input = widgets::TextInput::from_text_area(text_area_widget)
            .with_placeholder(placeholder)
            .with_clip(true);

        let mut text_input_widget = NewWidget::new(text_input);
        text_input_widget.properties = stripped_text_input_props(theme);
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

        let all_suggestions_lower: Vec<String> =
            all_suggestions.iter().map(|s| s.to_lowercase()).collect();
        let filtered = compute_filtered(&all_suggestions, &all_suggestions_lower, contents);

        Self {
            hosting: Hosting::InTree {
                overlay_host: NewWidget::new(overlay).to_pod(),
            },
            all_suggestions,
            all_suggestions_lower,
            contents: contents.to_owned(),
            filtered,
            open: false,
            suppress_focus_open: false,
            focus_moving_to_listbox: false,
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
        let all_suggestions_lower: Vec<String> =
            all_suggestions.iter().map(|s| s.to_lowercase()).collect();
        let filtered = compute_filtered(&all_suggestions, &all_suggestions_lower, &contents);

        Self {
            hosting: Hosting::Portal {
                chrome: chrome.to_pod(),
                scope,
                key,
                last_anchor_rect_window: None,
            },
            all_suggestions,
            all_suggestions_lower,
            contents,
            filtered,
            open: false,
            suppress_focus_open: false,
            focus_moving_to_listbox: false,
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
        self.filtered = compute_filtered(
            &self.all_suggestions,
            &self.all_suggestions_lower,
            &self.contents,
        );
        if self.filtered.is_empty() {
            return;
        }
        // Bail before flipping `open` if the portal scope isn't mounted yet:
        // otherwise we'd get stuck open with nothing visible and no AT update,
        // and the `!self.open` guard on ChildFocusChanged would permanently
        // block future opens.
        if let Hosting::Portal { scope, .. } = &self.hosting
            && scope.widget_id().is_none()
        {
            return;
        }
        self.open = true;
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                let items = self.filtered.clone();
                ctx.mutate_child_later(overlay_host, move |mut w| {
                    with_suggestion_list(&mut w, |list| SuggestionList::set_items(list, items));
                    AnchoredOverlay::set_overlay_visible(&mut w, true);
                });
            }
            Hosting::Portal { scope, key, .. } => {
                let scope_id = scope.widget_id().expect("checked above");
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
                    with_portal_list(&mut scope, key, |list| {
                        SuggestionList::set_items(list, items);
                    });
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
        ctx.request_accessibility_update();
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
                    close_portal_slot(&mut scope, key);
                });
            }
        }
    }

    fn handle_text_changed(&mut self, ctx: &mut ActionCtx<'_>, text: &str) {
        self.contents.clear();
        self.contents.push_str(text);
        self.filtered = compute_filtered(&self.all_suggestions, &self.all_suggestions_lower, text);

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
                if let Some(scope_id) = scope.widget_id() {
                    let key = *key;
                    let items = self.filtered.clone();
                    let visibility_update = if open_changed {
                        let owner_id = ctx.widget_id();
                        let rect = Rect::from_origin_size(
                            ctx.to_window(Point::ZERO),
                            ctx.border_box_size(),
                        );
                        if let Hosting::Portal {
                            last_anchor_rect_window,
                            ..
                        } = &mut self.hosting
                        {
                            *last_anchor_rect_window = if should_open { Some(rect) } else { None };
                        }
                        Some((
                            should_open,
                            PortalVisibility {
                                owner: Some(owner_id),
                                owner_kind: OwnerKind::Autocomplete,
                                rect,
                                anchor: PopoverAnchor::BottomStart,
                                gap: OVERLAY_GAP_PX,
                            },
                        ))
                    } else {
                        None
                    };
                    ctx.mutate_later(scope_id, move |mut w| {
                        let mut scope = w.downcast::<OverlayScope>();
                        with_portal_list(&mut scope, key, |list| {
                            SuggestionList::set_items(list, items);
                        });
                        if let Some((visible, visibility)) = visibility_update {
                            OverlayScope::set_portal_visible(&mut scope, key, visible, visibility);
                        }
                    });
                    if open_changed && should_open {
                        ctx.request_compose();
                        ctx.request_anim_frame();
                    }
                }
            }
        }

        ctx.submit_action::<AutocompleteAction>(AutocompleteAction::TextChanged(text.to_owned()));
        if open_changed {
            ctx.request_accessibility_update();
        }
        ctx.set_handled();
    }

    fn select_suggestion(&mut self, ctx: &mut ActionCtx<'_>, selected: String) {
        let text = selected.clone();
        self.contents.clone_from(&selected);
        self.filtered.clear();
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
                    ctx.request_accessibility_update();
                    return;
                };
                let key = *key;
                ctx.mutate_later(scope_id, move |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    close_portal_slot(&mut scope, key);
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
    /// Opens or closes the Portal-mode slot to match `should_open`, called
    /// after a setter has already determined `open_changed`. Shared by every
    /// `WidgetMut`-based setter that can flip the dropdown open or closed as
    /// a side effect of something other than direct user interaction (typing
    /// and focus events compute this inline instead, since they run from
    /// `ActionCtx`/`UpdateCtx` rather than a `WidgetMut`).
    fn sync_portal_visibility(this: &mut WidgetMut<'_, Self>, scope_id: WidgetId, key: u64) {
        let should_open = this.widget.open;
        if should_open {
            let owner_id = this.ctx.widget_id();
            let rect =
                Rect::from_origin_size(this.ctx.to_window(Point::ZERO), this.ctx.border_box_size());
            if let Hosting::Portal {
                last_anchor_rect_window,
                ..
            } = &mut this.widget.hosting
            {
                *last_anchor_rect_window = Some(rect);
            }
            this.ctx.mutate_later(scope_id, move |mut w| {
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
            this.ctx.request_compose();
            this.ctx.request_anim_frame();
        } else {
            if let Hosting::Portal {
                last_anchor_rect_window,
                ..
            } = &mut this.widget.hosting
            {
                *last_anchor_rect_window = None;
            }
            this.ctx.mutate_later(scope_id, move |mut w| {
                let mut scope = w.downcast::<OverlayScope>();
                close_portal_slot(&mut scope, key);
            });
        }
    }

    /// Update the displayed and filtered text. Called from the view layer on
    /// rebuild when the host's `contents` value changes.
    pub(crate) fn set_contents(this: &mut WidgetMut<'_, Self>, contents: &str) {
        if this.widget.contents == contents {
            return;
        }
        this.widget.contents.clear();
        this.widget.contents.push_str(contents);

        let filtered = compute_filtered(
            &this.widget.all_suggestions,
            &this.widget.all_suggestions_lower,
            contents,
        );
        // Also open (not just stay open) when the field already has focus:
        // an external content change (e.g. host-driven autofill or restoring
        // a draft into a controlled field) can happen while focused, and
        // should surface matching suggestions the same way handle_text_changed
        // does for a typed keystroke, rather than only ever narrowing/closing.
        let should_open = (this.widget.open || this.ctx.has_focus_target()) && !filtered.is_empty();
        let open_changed = this.widget.open != should_open;
        this.widget.filtered.clone_from(&filtered);
        this.widget.open = should_open;

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
                        with_portal_list(&mut scope, key, |list| {
                            SuggestionList::set_items(list, items);
                        });
                    });
                    if open_changed {
                        Self::sync_portal_visibility(this, scope_id, key);
                    }
                }
            }
        }

        if open_changed {
            this.ctx.request_accessibility_update();
        }
    }

    /// Replace the full suggestion list. Filtered suggestions are recomputed
    /// based on the current text.
    pub(crate) fn set_all_suggestions(this: &mut WidgetMut<'_, Self>, suggestions: Vec<ArcStr>) {
        if this.widget.all_suggestions == suggestions {
            return;
        }
        this.widget.all_suggestions_lower = suggestions.iter().map(|s| s.to_lowercase()).collect();
        this.widget.all_suggestions = suggestions;

        let filtered = compute_filtered(
            &this.widget.all_suggestions,
            &this.widget.all_suggestions_lower,
            &this.widget.contents,
        );
        // Also open (not just stay open) when the field already has focus:
        // suggestions can arrive asynchronously (e.g. a debounced/network
        // fetch) after the user has already focused an initially-empty
        // field, in which case open_on_focus already ran and bailed out
        // with nothing to show. Without this, the dropdown would never
        // appear until the user types or blurs and refocuses.
        let should_open = (this.widget.open || this.ctx.has_focus_target()) && !filtered.is_empty();
        let open_changed = this.widget.open != should_open;
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
                        with_portal_list(&mut scope, key, |list| {
                            SuggestionList::set_items(list, items);
                        });
                    });
                    if open_changed {
                        Self::sync_portal_visibility(this, scope_id, key);
                    }
                }
            }
        }

        if open_changed {
            this.ctx.request_accessibility_update();
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
                            close_portal_slot(&mut scope, key);
                        });
                    }
                }
            }
            this.ctx.request_accessibility_update();
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
                    apply_chrome_theme(&mut sb, theme);
                }
                with_suggestion_list(&mut h, |list| SuggestionList::set_theme(list, theme));
            }
            Hosting::Portal {
                chrome, scope, key, ..
            } => {
                {
                    let mut c = this.ctx.get_mut(chrome);
                    let mut sb = c.downcast::<SizedBox>();
                    apply_chrome_theme(&mut sb, theme);
                }

                let scope_id = scope.widget_id();
                let key = *key;
                let theme_copy = *theme;
                if let Some(scope_id) = scope_id {
                    this.ctx.mutate_later(scope_id, move |mut w| {
                        let mut scope = w.downcast::<OverlayScope>();
                        with_portal_list(&mut scope, key, |list| {
                            SuggestionList::set_theme(list, &theme_copy);
                        });
                    });
                }
            }
        }

        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    /// Notify that the portal slot was dismissed by an outside press. Syncs
    /// `open`/`filtered` — the slot has already hidden itself.
    pub(crate) fn mark_closed(this: &mut WidgetMut<'_, Self>) {
        if !this.widget.open {
            return;
        }
        this.widget.open = false;
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
                        close_portal_slot(&mut scope, key);
                    });
                }
            }
        }
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }

    /// Handle a suggestion selection that arrived from the portal
    /// [`super::view::SuggestionListView`] via `mutate_later`. Updates widget
    /// state, resets the text area, closes the portal slot, and submits
    /// [`AutocompleteAction::TextChanged`].
    pub(crate) fn portal_select(this: &mut WidgetMut<'_, Self>, text: String) {
        this.widget.contents.clone_from(&text);
        this.widget.filtered.clear();
        this.widget.open = false;
        // Do NOT set suppress_focus_open here. This function only runs once masonry
        // flushes the mutate_later queued by SuggestionListView::message, and that
        // flush happens strictly *after* the focus pass that fires ChildFocusChanged
        // on this widget for the refocus_input() call in LabelList's Enter handler:
        //
        //   RenderRoot::handle_text_event (masonry_core/src/app/render_root.rs):
        //     run_on_text_event_pass   — LabelList::on_text_event runs here,
        //                                 calling ctx.set_focus(text_area_id)
        //     run_update_focus_pass    — ChildFocusChanged(true) fires HERE,
        //                                 with self.open still true
        //     run_rewrite_passes()     — run_mutate_pass (first in the loop) is
        //                                 what actually calls portal_select, i.e. HERE
        //
        // So ChildFocusChanged(true) always sees self.open == true and never calls
        // open_on_focus — the flag would never be consumed, leaving it stuck and
        // silently preventing the dropdown from opening on a later, real focus event.
        // The self.open == true guard on ChildFocusChanged(true) already prevents the
        // reopen without the flag, given this ordering.
        //
        // This ordering is a real masonry guarantee (RenderRoot::handle_text_event's
        // fixed pass sequence), not incidental — but void_ui tracks masonry's `main`
        // branch (see workspace Cargo.toml), so it could shift on a future upstream
        // change without a compile break. If `enter_in_listbox_selects_closes_and_
        // returns_focus_to_input` starts failing with the dropdown reopening after
        // Enter-selection, re-check this ordering first.

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
                close_portal_slot(&mut scope, key);
            });
        }

        this.ctx
            .submit_action::<AutocompleteAction>(AutocompleteAction::TextChanged(text));
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
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
                // Enter over an open dropdown accepts the first suggestion —
                // the near-universal combobox affordance (the listbox, once
                // Tab-focused, has its own highlight-aware Enter handling in
                // LabelList::on_text_event). `select_suggestion` fills the
                // field, closes the popup, and consumes the key. When closed,
                // it's left unhandled so an enclosing form can submit.
                TextAction::Entered(_) => {
                    if self.open
                        && let Some(first) = self.filtered.first()
                    {
                        let first = first.to_string();
                        self.select_suggestion(ctx, first);
                    }
                }
            }
            return;
        }

        // ── Escape / clear ────────────────────────────────────────────────────
        if action.downcast_ref::<InputCleared>().is_some() {
            if self.open {
                // Escape while the suggestion popup is open dismisses the
                // popup only, preserving the typed text (standard combobox
                // affordance). Emitting no TextChanged keeps the host's bound
                // value intact — the field is not cleared.
                self.open = false;
                self.filtered.clear();
                self.close_overlay_later(ctx);
            } else {
                // No popup to dismiss: fall back to the bare-input clear
                // behavior (InputFrame emits InputCleared on Escape), emptying
                // the field and reporting the change.
                self.contents.clear();
                self.filtered.clear();
                ctx.submit_action::<Self::Action>(AutocompleteAction::TextChanged(String::new()));
            }
            ctx.set_handled();
            ctx.request_paint_only();
            ctx.request_accessibility_update();
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
            // Only Portal mode's ChildFocusChanged(false) needs (and gets)
            // this flag consumed: in-tree, the listbox is a descendant of
            // this widget just like the input is, so has_focus_target never
            // toggles for this transition and ChildFocusChanged never fires
            // at all — setting the flag unconditionally would leak it
            // forever after the first in-tree Tab-into-listbox, silently
            // disabling every future real close-on-blur.
            if matches!(self.hosting, Hosting::Portal { .. }) {
                self.focus_moving_to_listbox = true;
            }
            ctx.set_focus(listbox_id);
            ctx.set_handled();
        } else if key.key == Key::Named(NamedKey::Tab) && key.modifiers.shift() {
            // Explicitly consumed (set_handled), not left for masonry's
            // native backward search: `LabelList::accepts_focus()` must
            // return `true` (accesskit only exposes `Action::Focus` — and so
            // AT-driven Tab navigation — to widgets that do), so native
            // search can and does treat the listbox as a valid Shift+Tab
            // target when the input holds focus, since in-tree it lives in
            // the same subtree as the input. Rather than fight that search,
            // close the dropdown here and consume the keypress; a second
            // real Shift+Tab, with the listbox no longer relevant, then
            // cycles backward correctly on its own.
            self.open = false;
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
                    if let Some(scope_id) = scope.widget_id() {
                        let key = *key;
                        ctx.mutate_later(scope_id, move |mut w| {
                            let mut scope = w.downcast::<OverlayScope>();
                            close_portal_slot(&mut scope, key);
                        });
                    }
                }
            }
            ctx.request_paint_only();
            ctx.request_accessibility_update();
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
                        if let Some(scope_id) = scope.widget_id() {
                            let key = *key;
                            ctx.mutate_later(scope_id, move |mut w| {
                                let mut scope = w.downcast::<OverlayScope>();
                                close_portal_slot(&mut scope, key);
                            });
                        }
                    }
                }
                ctx.request_paint_only();
                ctx.request_accessibility_update();
            }
            // Close when focus truly leaves our subtree (Tab-out, clicking
            // elsewhere, a programmatic `set_focus` steal from a modal or
            // validation error, etc.). Portal mode also gets an
            // outside-press dismissal from the slot itself, but that only
            // covers presses — this covers everything else.
            //
            // Skip if a pointer-capture is active — that means the user
            // pressed Down on a list item and the pointer-Up (click-select)
            // hasn't fired yet.  Closing here would clear `self.filtered`
            // and prevent the subsequent `SuggestionSelected` action from
            // finding the item, so the text field never auto-fills.
            Update::ChildFocusChanged(false) => {
                // In portal mode the listbox lives outside our subtree, so
                // Tab-ing forward into it also fires this event. That's an
                // internal transition, not a real focus-leave — don't treat
                // it as a blur at all (not even for the suppress_focus_open
                // reset below). Consume the flag `on_text_event` set right
                // before calling `ctx.set_focus(listbox_id)` — we can't
                // instead compare `ctx.focus_target_id()` against the
                // listbox id here, because masonry updates
                // `global_state.focused_widget` only *after* dispatching
                // this event, so that accessor would still read the
                // *previous* target and never match.
                if self.focus_moving_to_listbox {
                    self.focus_moving_to_listbox = false;
                    return;
                }
                // select_suggestion sets this to suppress the reopen that
                // would otherwise follow its own refocus_input() call, but
                // masonry's pass ordering means the corresponding
                // ChildFocusChanged(true) doesn't reliably arrive while
                // self.open is still true to be ignored naturally — so the
                // flag can end up set with nothing left to consume it,
                // silently blocking every future open. Any real blur means
                // that grace period is over one way or another (either it
                // did its job, or the moment it was meant for already
                // passed), so clear it here unconditionally: a future
                // ChildFocusChanged(true) can only happen after a preceding
                // blur like this one.
                self.suppress_focus_open = false;
                if !(self.open && ctx.pointer_capture_target_id().is_none()) {
                    return;
                }
                self.open = false;
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
                        if let Some(scope_id) = scope.widget_id() {
                            let key = *key;
                            ctx.mutate_later(scope_id, move |mut w| {
                                let mut scope = w.downcast::<OverlayScope>();
                                close_portal_slot(&mut scope, key);
                            });
                        }
                    }
                }
                ctx.request_paint_only();
                ctx.request_accessibility_update();
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
    ///
    /// This is a deliberate busy-poll, not an oversight: masonry's compose
    /// pass only calls a widget's `compose()` if that widget already
    /// requested it, and there's no `Update` variant or timer API in the
    /// pinned masonry version that notifies an arbitrary descendant when an
    /// unrelated ancestor scrolls. Without that upstream hook, per-frame
    /// polling while open is the only way to catch "some ancestor scrolled"
    /// regardless of which one. Revisit this once masonry exposes a
    /// scroll-changed notification or a timer primitive that isn't tied to
    /// the display's refresh rate.
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
        // Static: this combobox can always potentially show a listbox popup,
        // independent of whether it's currently open — that's aria-expanded
        // (set_expanded below), not aria-haspopup. Required by the ARIA 1.2
        // combobox pattern for AT to announce the popup type.
        node.set_has_popup(HasPopup::Listbox);
        // Always reflect actual state (not just when open) — AT relies on an
        // explicit `false` to announce "collapsed", not just the property's
        // absence.
        node.set_expanded(self.open);
        // Gated on `self.open`, not just presence of the handle: in-tree,
        // `AnchoredOverlay::set_overlay_visible(false)` stashes the overlay
        // (`ctx.set_stashed`), and stashed widgets are excluded from the
        // accessibility tree entirely. Pointing aria-controls at a widget
        // with no corresponding a11y node is a dangling reference —
        // `accesskit_consumer::Node::controls()` unconditionally `.unwrap()`s
        // the resolved node per id, so this would crash any AT consumer that
        // reads it while collapsed, not just render an inaccurate attribute.
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
    use masonry::core::keyboard::{Code, Key, KeyboardEvent, Modifiers, NamedKey};
    use masonry::core::{NewWidget, PointerButton, TextEvent, WidgetRef};
    use masonry::testing::TestHarness;
    use masonry::widgets::TextArea;

    use super::*;
    use crate::overlay_portal::PortalPlacement;

    fn find_text_area(widget: WidgetRef<'_, dyn Widget>) -> Option<WidgetRef<'_, TextArea<true>>> {
        if let Some(area) = widget.downcast::<TextArea<true>>() {
            return Some(area);
        }
        widget.children().into_iter().find_map(find_text_area)
    }

    fn find_autocomplete(
        widget: WidgetRef<'_, dyn Widget>,
    ) -> Option<WidgetRef<'_, AutocompleteWidget>> {
        if let Some(ac) = widget.downcast::<AutocompleteWidget>() {
            return Some(ac);
        }
        widget.children().into_iter().find_map(find_autocomplete)
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

    /// Builds a portal-mode autocomplete with 3 fixed suggestions, wired
    /// into a real `OverlayScope` the way the view layer assembles it
    /// (mirrors `popover::widget::tests::portal_scope_with_host`), and
    /// locates its combobox/text-area ids.
    fn harness_with_fruit_portal() -> (TestHarness<OverlayScope>, WidgetId, WidgetId) {
        let theme = Theme::default();
        let suggestions: Vec<ArcStr> = ["Apple", "Banana", "Cherry"]
            .into_iter()
            .map(ArcStr::from)
            .collect();
        let scope_handle = OverlayScopeHandle::new();
        let handle = AutocompleteHandle::new();
        let listbox_handle = LabelListHandle::new();
        let text_area_handle = TextAreaHandle::new();
        let key = 1;

        let autocomplete = AutocompleteWidget::new_portal(
            AutocompleteConfig {
                contents: String::new(),
                placeholder: ArcStr::from("Pick a fruit"),
                all_suggestions: suggestions,
                disabled: false,
                theme,
            },
            scope_handle.clone(),
            key,
            handle.clone(),
            listbox_handle.clone(),
            &text_area_handle,
        );
        let suggestion_list =
            SuggestionList::new([], &theme, listbox_handle, handle, text_area_handle);
        // Matches what the view layer produces: `PortalContentView` is typed
        // to a `Pod<Passthrough>` element, and the widget-side mutate_later
        // closures (open_on_focus, handle_text_changed, ...) all navigate
        // `PortalSlot::child_mut` -> downcast::<Passthrough> ->
        // Passthrough::child_mut -> downcast::<SuggestionList>.
        let passthrough = Passthrough::new(NewWidget::new(suggestion_list).erased());

        let scope = OverlayScope::new(
            scope_handle,
            NewWidget::new(autocomplete).erased(),
            vec![(
                key,
                NewWidget::new(passthrough).erased(),
                PortalPlacement::BareTrigger,
            )],
        );
        let harness = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(scope),
            (300, 300),
        );

        let autocomplete_id = find_autocomplete(harness.root_widget().as_dyn())
            .expect("tree should contain the portal-mode autocomplete")
            .id();
        let text_area_id = find_text_area(harness.root_widget().as_dyn())
            .expect("autocomplete should host a TextArea")
            .id();
        (harness, autocomplete_id, text_area_id)
    }

    /// Real proof that clicking a plain, non-interactive sibling widget
    /// elsewhere on the page closes the in-tree dropdown. Neither
    /// `AnchoredOverlay` nor `AutocompleteWidget` implements
    /// `on_pointer_event` for outside-click detection — there's no bespoke
    /// geometric check. This instead relies on masonry's own pointer-down
    /// handling, which resolves the click to the nearest pointer-interactive
    /// ancestor of whatever's under the cursor (here, the `Flex` container,
    /// since `Label` opts out of pointer interaction) and clears focus
    /// whenever that resolved target isn't in the focused widget's own
    /// ancestor path — regardless of whether the literal clicked widget
    /// itself accepts focus. Same mechanism `ThemedPopover::update`
    /// documents and relies on for its own in-tree fallback.
    #[test]
    fn clicking_a_non_focusable_sibling_closes_the_in_tree_dropdown() {
        let theme = Theme::default();
        let suggestions: Vec<ArcStr> = ["Apple", "Banana", "Cherry"]
            .into_iter()
            .map(ArcStr::from)
            .collect();
        let autocomplete =
            AutocompleteWidget::new("", ArcStr::from("Pick a fruit"), suggestions, false, &theme);
        let label_widget = NewWidget::new(widgets::Label::new("Elsewhere on the page"));
        let label_id = label_widget.id();

        let root = widgets::Flex::column()
            .with_fixed(NewWidget::new(autocomplete))
            .with_fixed(label_widget);

        let mut harness = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(root),
            (300, 300),
        );

        let autocomplete_id = find_autocomplete(harness.root_widget().as_dyn())
            .expect("tree should contain the autocomplete")
            .id();
        let text_area_id = find_text_area(harness.root_widget().as_dyn())
            .expect("autocomplete should host a TextArea")
            .id();

        harness.focus_on(Some(text_area_id));
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(true),
            "focusing the field should open the dropdown"
        );

        // `mouse_click_on`/`mouse_move_to` refuse a widget that doesn't
        // accept pointer events (by design, `Label` doesn't) — compute the
        // window-space center manually instead, matching what a real click
        // on that visible label would resolve to.
        let label_ref = harness.get_widget_with_id(label_id);
        let label_size = label_ref.ctx().border_box_size();
        let label_center = label_ref
            .ctx()
            .to_window(Point::new(label_size.width / 2.0, label_size.height / 2.0));
        harness.mouse_move(label_center);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.render();

        assert_ne!(
            harness.focused_widget_id(),
            Some(text_area_id),
            "clicking a non-focusable sibling should clear focus from the field"
        );
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(false),
            "clicking outside should close the in-tree dropdown"
        );
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
                node.data().has_popup(),
                Some(masonry::accesskit::HasPopup::Listbox),
                "aria-haspopup should announce a listbox popup, closed or not"
            );
            assert_eq!(
                node.data().is_expanded(),
                Some(false),
                "closed before focus"
            );
            // aria-controls is deliberately absent here: the listbox is
            // stashed while collapsed (excluded from the a11y tree), so
            // pointing aria-controls at it would be a dangling reference —
            // see the comment on the `push_controlled` call in
            // `AutocompleteWidget::accessibility`.
            assert_eq!(
                node.controls().count(),
                0,
                "no aria-controls while the listbox is stashed/collapsed"
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
            // Regression coverage: LabelList::accepts_focus() must stay
            // `true` (a raw-keyboard-only test wouldn't catch a regression
            // here — masonry's accessibility pass only grants
            // accesskit::Action::Focus, which AT tools use to move focus,
            // to widgets that accept focus).
            assert!(
                controlled[0]
                    .data()
                    .supports_action(masonry::accesskit::Action::Focus),
                "the listbox must be focusable via the accessibility action API, \
                 not just raw keyboard Tab, for AT-driven navigation to reach it"
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

    /// Shift+Tab pressed from the text input (dropdown open, never having
    /// Tab'd into the listbox) must not land real focus on the listbox: with
    /// `LabelList::accepts_focus()` correctly `true` (required for AT
    /// focusability, see the previous test), masonry's native backward
    /// search would otherwise treat the in-tree listbox as a valid target
    /// since it shares the input's subtree. `on_text_event`'s explicit
    /// Shift+Tab interception closes the dropdown and consumes the keypress
    /// instead, so focus stays on the input rather than jumping into
    /// (now-closing) list content.
    #[test]
    fn shift_tab_from_input_closes_dropdown_instead_of_focusing_listbox() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit();

        harness.focus_on(Some(text_area_id));
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(true)
        );

        let shift_tab = TextEvent::Keyboard(KeyboardEvent {
            key: Key::Named(NamedKey::Tab),
            modifiers: Modifiers::SHIFT,
            ..KeyboardEvent::key_down(Key::Named(NamedKey::Tab), Code::Unidentified)
        });
        harness.process_text_event(shift_tab);
        harness.render();

        assert_eq!(
            harness.focused_widget_id(),
            Some(text_area_id),
            "Shift+Tab from the input should leave focus on the input, not jump \
             into the listbox"
        );
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(false),
            "Shift+Tab from the input should close the dropdown"
        );
    }

    /// Portal-mode-specific regression: forward Tab from the input must move
    /// real focus into the (portal-mounted) listbox *without* closing the
    /// dropdown. In portal mode the listbox lives outside the autocomplete's
    /// own subtree, so this transfer does fire `ChildFocusChanged(false)` on
    /// the widget — `on_text_event` sets `focus_moving_to_listbox` right
    /// before calling `ctx.set_focus`, and the handler must recognize that
    /// and skip closing. It previously tried to detect this via
    /// `ctx.focus_target_id() == listbox_id`, which never matched: masonry
    /// updates `global_state.focused_widget` *after* dispatching
    /// `ChildFocusChanged`, so that accessor still read the old value at the
    /// point the handler ran, and the dropdown closed out from under the
    /// user on every forward Tab.
    #[test]
    fn tab_into_portal_listbox_does_not_close_dropdown() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit_portal();

        harness.focus_on(Some(text_area_id));
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(true),
            "focusing the field should open the portal-mode dropdown"
        );

        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        harness.render();

        assert_ne!(
            harness.focused_widget_id(),
            Some(text_area_id),
            "Tab should move focus off the input and into the listbox"
        );
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(true),
            "Tab-ing into the portal-mode listbox must not close the dropdown"
        );
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

    /// After an in-tree Enter-selection, blurring and re-focusing the field
    /// must reopen the dropdown. `select_suggestion` sets
    /// `suppress_focus_open` to swallow the reopen that would otherwise
    /// follow its own `refocus_input()` call, but masonry's pass ordering
    /// for `handle_text_event` resolves the corresponding
    /// `ChildFocusChanged(true)` *before* `select_suggestion` itself runs
    /// (actions are processed later, in `run_rewrite_passes`) — so that
    /// event sees `self.open` still `true` and never consumes the flag,
    /// leaving it stuck and silently blocking every future open. See the
    /// unconditional reset in `update()`'s `ChildFocusChanged(false)` arm.
    #[test]
    fn reopens_after_selection_then_blur_and_refocus() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit();

        harness.focus_on(Some(text_area_id));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));

        harness.focus_on(None);
        harness.focus_on(Some(text_area_id));
        harness.render();

        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(true),
            "re-focusing after a selection should reopen the dropdown, not leave it \
             permanently stuck closed"
        );
    }

    /// Same leak, via a click-based selection instead of Enter — click and
    /// keyboard selection go through different masonry pass orderings
    /// (`handle_pointer_event` vs `handle_text_event`), so both paths need
    /// their own coverage.
    #[test]
    fn reopens_after_click_selection_then_blur_and_refocus() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit();

        harness.focus_on(Some(text_area_id));
        harness.render();

        let mut first_item_id = None;
        harness.inspect_widgets(|w| {
            if first_item_id.is_none() && w.accessibility_role() == Role::ListBoxOption {
                first_item_id = Some(w.id());
            }
        });
        let item_id = first_item_id.expect("dropdown should have rendered list items");

        harness.mouse_move_to_unchecked(item_id);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));

        harness.focus_on(None);
        harness.focus_on(Some(text_area_id));
        harness.render();

        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(true),
            "re-focusing after a click selection should reopen the dropdown"
        );
    }

    /// Suggestions delivered asynchronously (e.g. a debounced fetch) after
    /// the field already has focus must open the dropdown themselves —
    /// `open_on_focus` already ran and bailed out when the field was
    /// focused with no suggestions yet, so nothing else would open it.
    #[test]
    fn async_suggestions_open_dropdown_while_already_focused() {
        let theme = Theme::default();
        let widget =
            AutocompleteWidget::new("", ArcStr::from("Pick a fruit"), Vec::new(), false, &theme);
        let mut harness = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(widget),
            (300, 300),
        );
        let autocomplete_id = harness.root_id();
        let text_area_id = find_text_area(harness.root_widget().as_dyn())
            .expect("autocomplete should host a TextArea")
            .id();

        harness.focus_on(Some(text_area_id));
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(false),
            "no suggestions yet, so focusing shouldn't open anything"
        );

        let suggestions: Vec<ArcStr> = ["Apple", "Banana", "Cherry"]
            .into_iter()
            .map(ArcStr::from)
            .collect();
        harness.edit_root_widget(|mut root| {
            AutocompleteWidget::set_all_suggestions(&mut root, suggestions);
        });

        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(true),
            "suggestions arriving while the field is focused should open the dropdown"
        );
    }

    /// An external content change (e.g. host-driven autofill into a
    /// controlled field) while the field already has focus must open the
    /// dropdown itself if the new text matches suggestions — not just narrow
    /// or close an already-open one.
    #[test]
    fn set_contents_opens_dropdown_while_already_focused() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit();

        harness.focus_on(Some(text_area_id));
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(true),
            "focusing an empty field with suggestions available should open it"
        );

        // Type nothing further, but simulate an external reset to no match,
        // closing the dropdown while focus remains on the field.
        harness.edit_root_widget(|mut root| {
            AutocompleteWidget::set_contents(&mut root, "zzz");
        });
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(false),
            "content change to a non-matching value should close the dropdown"
        );

        // Now an external content change to a matching value, still focused,
        // must reopen it.
        harness.edit_root_widget(|mut root| {
            AutocompleteWidget::set_contents(&mut root, "Ap");
        });
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(true),
            "external content change to a matching value while focused should reopen the dropdown"
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

    /// The portal-mounted `SuggestionList` is built empty by the view layer
    /// (`SuggestionListView` no longer carries a `filtered` mirror); the widget
    /// is the single source of truth and must populate it via `set_items` when
    /// the dropdown opens. `harness_with_fruit_portal` mirrors the view by
    /// building the list with `[]`, so this proves the widget — not the view —
    /// fills the list on open.
    #[test]
    fn portal_dropdown_populates_items_from_widget() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit_portal();

        harness.focus_on(Some(text_area_id));
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(true),
            "focusing should open the portal dropdown",
        );

        let mut option_count = 0;
        harness.inspect_widgets(|w| {
            if w.accessibility_role() == Role::ListBoxOption {
                option_count += 1;
            }
        });
        assert_eq!(
            option_count, 3,
            "the widget must populate the empty portal list with all suggestions",
        );
    }

    /// Like [`harness_with_fruit`] but seeds the field with `initial` text, so
    /// text-preserving keyboard behavior (Escape, Enter) can be exercised.
    fn harness_with_fruit_contents(
        initial: &str,
    ) -> (TestHarness<AutocompleteWidget>, WidgetId, WidgetId) {
        let theme = Theme::default();
        let suggestions: Vec<ArcStr> = ["Apple", "Banana", "Cherry"]
            .into_iter()
            .map(ArcStr::from)
            .collect();
        let widget = AutocompleteWidget::new(
            initial,
            ArcStr::from("Pick a fruit"),
            suggestions,
            false,
            &theme,
        );
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

    /// Escape while the dropdown is open must dismiss the popup only — the
    /// standard combobox affordance — leaving the typed text intact and *not*
    /// emitting a text change that would reset the host's bound value.
    #[test]
    fn escape_in_field_closes_dropdown_without_clearing_text() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit_contents("App");

        // Focus opens the dropdown: compute_filtered("App") -> ["Apple"].
        harness.focus_on(Some(text_area_id));
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(true),
            "focusing a field with matches should open the dropdown",
        );
        while harness.pop_action::<AutocompleteAction>().is_some() {}

        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
        harness.render();

        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(false),
            "Escape should close the open dropdown",
        );
        assert!(
            harness.pop_action::<AutocompleteAction>().is_none(),
            "Escape-to-close must not emit a text change — it dismisses the \
             popup, it does not clear the field",
        );
        let ac = find_autocomplete(harness.root_widget().as_dyn()).expect("autocomplete");
        assert_eq!(ac.contents, "App", "typed text must survive Escape");
    }

    /// Escape with no dropdown open falls back to the bare-input clear
    /// behavior: there's no popup to dismiss, so it empties the field.
    #[test]
    fn escape_with_closed_dropdown_still_clears_the_field() {
        // "zzz" matches nothing, so focusing never opens the dropdown.
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit_contents("zzz");

        harness.focus_on(Some(text_area_id));
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(false),
            "no matches -> dropdown stays closed",
        );
        while harness.pop_action::<AutocompleteAction>().is_some() {}

        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
        harness.render();

        let (action, _) = harness
            .pop_action::<AutocompleteAction>()
            .expect("Escape with no popup should clear and report the change");
        let AutocompleteAction::TextChanged(text) = action;
        assert_eq!(text, "", "Escape with no open dropdown clears the field");
        let ac = find_autocomplete(harness.root_widget().as_dyn()).expect("autocomplete");
        assert!(ac.contents.is_empty(), "field should be cleared");
    }

    /// Pressing Enter in the text field while the dropdown is open accepts the
    /// first suggestion (the near-universal combobox affordance), fills the
    /// field, closes the popup, and reports the change.
    #[test]
    fn enter_in_field_selects_first_suggestion() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit();

        harness.focus_on(Some(text_area_id));
        harness.render();
        while harness.pop_action::<AutocompleteAction>().is_some() {}

        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
        harness.render();

        let area = find_text_area(harness.root_widget().as_dyn()).expect("TextArea");
        assert_eq!(
            area.text().to_string(),
            "Apple",
            "Enter should accept the first suggestion",
        );
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(false),
            "accepting a suggestion closes the dropdown",
        );
        let (action, _) = harness
            .pop_action::<AutocompleteAction>()
            .expect("accepting a suggestion should report the change");
        let AutocompleteAction::TextChanged(text) = action;
        assert_eq!(text, "Apple");
    }
}
