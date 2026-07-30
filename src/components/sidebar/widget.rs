//! Masonry widget for the sidebar nav item.
//!
//! A full-width, left-aligned nav row. When `selected`, a 3 px accent bar
//! is painted on the left edge and the label renders in the full text color.
//! Pointer state (hover, press) is read from the widget context, matching the
//! same paint-driven pattern as `crate::components::button::widget::ThemedButton`.
//!
//! Emits [`ButtonPress`] on primary-pointer release inside the widget and on
//! Space / Enter while focused.
//!
//! An optional trailing action (attached via [`ThemedSidebarItem::new_with_actions`])
//! is revealed on row hover or keyboard focus-within. The label and action
//! are arranged by a real masonry [`Flex`] row this widget owns as a typed
//! child — see the module docs in `super::reveal` and the design doc at
//! `docs/superpowers/specs/2026-07-30-sidebar-item-hover-reveal-actions-design.md`
//! for why: two prior attempts hand-computed the action's x-origin instead
//! and both landed it on top of the label.

use masonry::accesskit;
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, CollectionWidget, EventCtx, LayoutCtx, MeasureCtx,
    NewWidget, PaintCtx, PointerButton, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::widgets::{ButtonPress, Flex};

use super::reveal::RevealBox;
use crate::Theme;
use crate::components::click::{self, ClickPhase};
use crate::components::interaction;
use crate::focus_ring::{FOCUS_RING_OUTSET, paint_focus_ring};

/// Width of the active-state left accent bar — accent-bar chrome (stroke-like),
/// not density-scaled.
const ACCENT_WIDTH: f64 = 3.0;
/// Corner radius of the accent bar — accent-bar chrome, not density-scaled.
const ACCENT_RADIUS: f64 = 1.5;

/// Index of the label within the owned `Flex` row, when an action is
/// attached (see [`Content::Row`]).
pub(super) const LABEL_INDEX: usize = 0;
/// Index of the [`RevealBox`]-wrapped action within the owned `Flex` row.
pub(super) const ACTIONS_INDEX: usize = 1;

/// The row's single owned child.
///
/// `Label` is the original, unchanged shape (no action attached — the exact
/// `measure`/`layout` path this widget has always used). `Row` only exists
/// once an action is attached: a real `Flex` row `[label.flex(1.0),
/// RevealBox(action)]` the framework arranges, so no code computes child
/// origins by hand.
enum Content {
    Label(WidgetPod<dyn Widget>),
    Row(WidgetPod<Flex>),
}

/// Themed, interactive sidebar navigation item.
///
/// Owns its content (a label, optionally with a trailing action) and a
/// [`Theme`] value used to resolve background and accent colors at paint
/// time. The `selected` flag is host-controlled; pointer state (hovered,
/// pressed) is read from the widget context.
pub struct ThemedSidebarItem {
    content: Content,
    theme: Theme,
    /// Host-controlled selected-row state.
    selected: bool,
    /// True for the span between a Space/Enter key-down and its matching
    /// key-up (or an intervening focus loss) — the keyboard equivalent of
    /// the pointer-driven `pressed` flag read from the widget context, so
    /// keyboard activation shows the same pressed fill a pointer click does.
    keyboard_pressed: bool,
    /// Host-controlled disabled state.
    disabled: bool,
}

// --- MARK: BUILDERS
impl ThemedSidebarItem {
    /// Creates a new sidebar item with the supplied label and theme, no
    /// trailing action.
    #[must_use]
    pub fn new(label: NewWidget<impl Widget + ?Sized>, theme: &Theme) -> Self {
        Self {
            content: Content::Label(label.erased().to_pod()),
            theme: *theme,
            selected: false,
            keyboard_pressed: false,
            disabled: false,
        }
    }

    /// Creates a new sidebar item with a label and a trailing action,
    /// revealed on row hover or keyboard focus-within.
    ///
    /// `label` and `actions` are arranged by a real masonry [`Flex`] row
    /// this widget owns — `label.flex(1.0)` plus a [`RevealBox`]-wrapped
    /// `actions` — so the framework content-sizes the action and flexes the
    /// label; no code computes child origins by hand.
    ///
    /// Whether a row has an action is fixed for the row's lifetime, the
    /// same contract as the label text (see [`Self::child_mut`]): use
    /// [`Self::set_content`] (a wholesale replace) if that ever needs to
    /// change, rather than mutating a `Label`-shaped row into a `Row`-shaped
    /// one in place.
    #[must_use]
    pub fn new_with_actions(
        label: NewWidget<impl Widget + ?Sized>,
        actions: NewWidget<impl Widget + ?Sized>,
        theme: &Theme,
    ) -> Self {
        Self {
            content: Content::Row(Self::build_row(label, actions)),
            theme: *theme,
            selected: false,
            keyboard_pressed: false,
            disabled: false,
        }
    }

    fn build_row(
        label: NewWidget<impl Widget + ?Sized>,
        actions: NewWidget<impl Widget + ?Sized>,
    ) -> WidgetPod<Flex> {
        let row = Flex::row()
            .with(label, 1.0)
            .with_fixed(NewWidget::new(RevealBox::new(actions)));
        NewWidget::new(row).to_pod()
    }

    /// Marks this item as the currently-selected nav entry.
    #[must_use]
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Suppresses all interaction and mutes the visual appearance.
    #[must_use]
    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

// --- MARK: WIDGETMUT
impl ThemedSidebarItem {
    /// Replaces the theme. Requests layout + repaint if the value changed.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    /// Toggles the host-driven `selected` flag. Requests a repaint on change.
    pub fn set_selected(this: &mut WidgetMut<'_, Self>, selected: bool) {
        if this.widget.selected != selected {
            this.widget.selected = selected;
            this.ctx.request_paint_only();
        }
    }

    /// Sets the disabled state. Syncs with masonry's event-routing flag.
    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();
        }
    }

    /// Mutable handle to the label — valid only for a row built with
    /// [`Self::new`] (no action attached). Panics otherwise; use
    /// [`Self::row_mut`] for a row with an action.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        let Content::Label(label) = &mut this.widget.content else {
            panic!("child_mut called on a sidebar item with an action; use row_mut");
        };
        this.ctx.get_mut(label)
    }

    /// Mutable handle to the owned `Flex` row — valid only for a row built
    /// with [`Self::new_with_actions`]. Panics otherwise; use
    /// [`Self::child_mut`] for a row without an action. The caller reaches
    /// the label or the action from here via [`LABEL_INDEX`]/[`ACTIONS_INDEX`]
    /// and [`CollectionWidget::get_mut`](masonry::core::CollectionWidget::get_mut).
    pub fn row_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, Flex> {
        let Content::Row(row) = &mut this.widget.content else {
            panic!("row_mut called on a sidebar item without an action; use child_mut");
        };
        this.ctx.get_mut(row)
    }

    /// Replaces the row's content wholesale: rebuilds it as either a bare
    /// label (`actions: None`) or a label-plus-action `Flex` row
    /// (`actions: Some(_)`), tearing down whatever was there before.
    ///
    /// This exists only for the rare case where a row's action presence
    /// changes across a rebuild of the same view (see the "action presence
    /// is fixed at construction" note in `view.rs`) — the steady-state path
    /// uses [`Self::child_mut`]/[`Self::row_mut`] for cheap in-place
    /// updates instead.
    pub fn set_content(
        this: &mut WidgetMut<'_, Self>,
        label: NewWidget<impl Widget + ?Sized>,
        actions: Option<NewWidget<impl Widget + ?Sized>>,
    ) {
        let new_content = match actions {
            None => Content::Label(label.erased().to_pod()),
            Some(actions) => Content::Row(Self::build_row(label, actions)),
        };
        match std::mem::replace(&mut this.widget.content, new_content) {
            Content::Label(old) => this.ctx.remove_child(old),
            Content::Row(old) => this.ctx.remove_child(old),
        }
        this.ctx.children_changed();
        this.ctx.request_layout();
    }
}

// --- MARK: PAINT STATE
impl ThemedSidebarItem {
    /// Resolves the row background color for the current interaction state.
    ///
    /// | state          | bg           |
    /// |----------------|--------------|
    /// | default        | transparent  |
    /// | hover          | `surface_2`  |
    /// | pressed        | `surface_hi` |
    /// | selected       | `surface_hi` |
    ///
    /// `selected` and `hover` resolve to distinct fills (rather than sharing
    /// one) because they're independent per-row widget states: hovering one
    /// row while a different row is selected must not make the two
    /// indistinguishable.
    fn resolve_bg(&self, hovered: bool, pressed: bool) -> Color {
        if self.disabled {
            return Color::TRANSPARENT;
        }
        let p = &self.theme.palette;
        if pressed || self.selected {
            p.surface_hi
        } else if hovered {
            p.surface_2
        } else {
            Color::TRANSPARENT
        }
    }

    /// Pushes the current reveal predicate to the action's `RevealBox`, if
    /// any: `revealed = !disabled && (has_hovered || has_focus_target)`,
    /// recomputed from live context on every relevant `Update` rather than
    /// stored and drifted (the Pass-2 bug on the quarantined branch was
    /// exactly a stored `revealed` bool going stale). `RevealBox::set_revealed`
    /// is a no-op when unchanged, so driving it unconditionally is cheap. A
    /// no-op when the row has no action.
    ///
    /// The caller must pass the *subtree-wide* hovered/focus signal —
    /// `ctx.has_hovered()`/`ctx.has_focus_target()` ("am I or any
    /// descendant"), not the narrower `ctx.is_hovered()`/`is_focus_target()`
    /// ("am I, specifically, the pointer's literal target") that back
    /// `Update::HoveredChanged`/`FocusChanged`'s own payload. The row *is*
    /// sometimes the pointer's literal hover leaf — e.g. the cursor sitting
    /// over the row's own padding rather than the label or action inside it
    /// — confirmed live: `HoveredChanged` does fire directly on the row, not
    /// only on its children. Using that event's narrow payload as "am I
    /// hovered" (as an earlier version of this method did) meant that the
    /// literal hover leaf moving from the row onto a descendant — a routine
    /// occurrence, not an edge case — fired `HoveredChanged(false)` and
    /// incorrectly hid the action even though a descendant was still under
    /// the pointer. See each `update()` call site for which value is safe
    /// to read fresh for which event.
    fn drive_reveal(&mut self, ctx: &mut UpdateCtx<'_>, hovered: bool, focus_target: bool) {
        let Content::Row(row) = &mut self.content else {
            return;
        };
        let revealed = !self.disabled && (hovered || focus_target);
        ctx.mutate_child_later(row, move |mut row| {
            let mut action_widget = Flex::get_mut(&mut row, ACTIONS_INDEX);
            let mut action = action_widget.downcast::<RevealBox>();
            RevealBox::set_revealed(&mut action, revealed);
        });
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedSidebarItem {
    type Action = ButtonPress;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if self.disabled {
            return;
        }
        // A revealed action already captured this `Down` if
        // `pointer_capture_target_id()` is already set when we get here:
        // pointer events bubble target-to-root, the action sits below us in
        // the tree, and nothing else in this subtree calls
        // `capture_pointer`. Decline so we don't steal its capture and
        // select the row instead (see the module docs).
        if matches!(event, PointerEvent::Down(_)) && ctx.pointer_capture_target_id().is_some() {
            return;
        }
        match click::primary_click(ctx, event) {
            Some(ClickPhase::Down(_)) => {
                ctx.request_focus();
                ctx.request_paint_only();
            }
            Some(ClickPhase::Up { completed, .. }) => {
                if completed {
                    ctx.submit_action::<Self::Action>(ButtonPress {
                        button: Some(PointerButton::Primary),
                    });
                }
                ctx.request_paint_only();
            }
            None => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if self.disabled {
            return;
        }
        if interaction::keyboard_press_start(event, true) {
            ctx.set_handled();
            self.keyboard_pressed = true;
            ctx.request_paint_only();
        } else if interaction::keyboard_activate(event, true) {
            ctx.set_handled();
            self.keyboard_pressed = false;
            ctx.request_paint_only();
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if self.disabled {
            return;
        }
        if interaction::is_access_click(event) {
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::FocusChanged(false)) {
            // Losing focus mid-press (e.g. Tab away while Space is still
            // held) would otherwise leave `keyboard_pressed` stuck true
            // with no matching key-up ever arriving to clear it.
            self.keyboard_pressed = false;
        }
        match event {
            // Sync masonry's disabled flag on first attach (matches the
            // checkbox/button pattern; previously missing here). Sync the
            // reveal too, in case the row is added already hovered/focused
            // (both false at this point, since nothing has interacted yet).
            Update::WidgetAdded => {
                ctx.set_disabled(self.disabled);
                self.drive_reveal(ctx, ctx.has_hovered(), ctx.has_focus_target());
            }
            // `ChildHoveredChanged`'s bool payload IS the fresh
            // `has_hovered` value for this widget — masonry writes it into
            // `widget_state.has_hovered` only *after* this dispatch
            // returns, so the payload (not `ctx.has_hovered()`) is the only
            // fresh source here. See `drive_reveal`'s docs.
            Update::ChildHoveredChanged(hovered) => {
                ctx.request_paint_only();
                self.drive_reveal(ctx, *hovered, ctx.has_focus_target());
            }
            // Mirrors the `ChildHoveredChanged` arm above:
            // `ChildFocusChanged`'s payload is the fresh `has_focus_target`,
            // written after dispatch, so it's used directly.
            Update::ChildFocusChanged(focus_target) => {
                ctx.request_paint_only();
                self.drive_reveal(ctx, ctx.has_hovered(), *focus_target);
            }
            // `HoveredChanged`'s bool payload is `is_hovered` — whether
            // *this exact widget* is the pointer's literal target — not
            // `has_hovered`, and `FocusChanged`'s payload is the analogous
            // narrow `is_focus_target`. Neither stands in for the broad
            // "is anything in my subtree hovered/focused" signal
            // `drive_reveal` needs: the row does become the literal
            // pointer target directly at times (e.g. the pointer sitting
            // over the row's own padding rather than a child), so treating
            // that narrow payload as the broad signal — as an earlier
            // version of this method did — meant the literal target moving
            // from the row onto a still-hovered child fired
            // `HoveredChanged(false)` while `has_hovered` stayed `true`,
            // reveal-then-hiding the action under a pointer that never
            // left it. By the time either event dispatches, masonry has
            // already finished committing `has_hovered`/`has_focus_target`
            // for the whole hovered/focused path earlier in this same
            // pass, so both `ctx` accessors are safe to read fresh here —
            // unlike in the `Child*Changed` arms above, where the
            // corresponding field is still stale mid-dispatch (see
            // `drive_reveal`'s docs).
            Update::HoveredChanged(_) | Update::FocusChanged(_) | Update::DisabledChanged(_) => {
                ctx.request_paint_only();
                self.drive_reveal(ctx, ctx.has_hovered(), ctx.has_focus_target());
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        match &mut self.content {
            Content::Label(label) => ctx.register_child(label),
            Content::Row(row) => ctx.register_child(row),
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let pad_h = f64::from(self.theme.density.pad_h);
        let pad_v = f64::from(self.theme.density.pad_v);
        let (main_pad, cross_pad) = match axis {
            Axis::Horizontal => (ACCENT_WIDTH + 2.0 * pad_h, 2.0 * pad_v),
            Axis::Vertical => (2.0 * pad_v, ACCENT_WIDTH + 2.0 * pad_h),
        };
        let inner_cross = cross_length.map(|c| Length::px((c.get() - cross_pad).max(0.0)));
        let auto_length = len_req.into();
        let context_size = LayoutSize::maybe(axis.cross(), inner_cross);
        let content_length = match &mut self.content {
            Content::Label(label) => {
                ctx.compute_length(label, auto_length, context_size, axis, inner_cross)
            }
            Content::Row(row) => {
                ctx.compute_length(row, auto_length, context_size, axis, inner_cross)
            }
        };
        Length::px(content_length.get() + main_pad)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_h = f64::from(self.theme.density.pad_h);
        let pad_v = f64::from(self.theme.density.pad_v);
        let inner = Size::new(
            (size.width - ACCENT_WIDTH - 2.0 * pad_h).max(0.0),
            (size.height - 2.0 * pad_v).max(0.0),
        );
        let content_size = match &mut self.content {
            Content::Label(label) => {
                // Unchanged from before this widget had actions: the label
                // fits (shrink-wraps) within `inner`, left-aligned, and is
                // centered vertically below.
                let s = ctx.compute_size(label, SizeDef::fit(inner), inner.into());
                ctx.run_layout(label, s);
                s
            }
            Content::Row(row) => {
                // The row must occupy the *full* inner box — not shrink-wrap
                // — so `Flex` has free space to hand the flexed label when
                // the action is hidden (zero width) and take back when
                // revealed. Hand the row its final size directly (mirrors
                // `SizedBox::layout`, which does the same for its one
                // child) rather than negotiating via `compute_size`.
                ctx.run_layout(row, inner);
                inner
            }
        };
        let content_x = ACCENT_WIDTH + pad_h;
        let content_y = pad_v + ((inner.height - content_size.height) * 0.5).max(0.0);
        match &mut self.content {
            Content::Label(label) => ctx.place_child(label, Point::new(content_x, content_y)),
            Content::Row(row) => ctx.place_child(row, Point::new(content_x, content_y)),
        }
        match &self.content {
            Content::Label(label) => ctx.derive_baselines(label),
            Content::Row(row) => ctx.derive_baselines(row),
        }
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box().size();
        // `has_hovered`, not the leaf-level `is_hovered`: once a row has a
        // revealed, interactive action child, hovering that action makes it
        // (not the row) the innermost hover target, but the row must still
        // show its hover fill — the row highlights when hovered anywhere in
        // its subtree. `pressed`/`focused` stay row-local: a press or focus
        // on the action shows the action's own feedback, not the row's.
        // (`has_hovered() == is_hovered()` when there's no action, so this
        // is a no-op change for a plain label row.)
        let hovered = ctx.has_hovered();
        let pressed = (ctx.is_active() && ctx.is_hovered()) || self.keyboard_pressed;
        let focused = ctx.is_focus_target();
        let p = &self.theme.palette;

        let bg = self.resolve_bg(hovered, pressed);
        let bg_rect = RoundedRect::from_origin_size(Point::ORIGIN, size, 0.0);
        if bg.components[3] > 0.0 {
            painter.fill(bg_rect, bg).draw();
        }

        if self.selected && !self.disabled {
            let accent = RoundedRect::from_origin_size(
                Point::ORIGIN,
                Size::new(ACCENT_WIDTH, size.height),
                ACCENT_RADIUS,
            );
            painter.fill(accent, p.accent).draw();
        }

        if focused {
            let inset = FOCUS_RING_OUTSET;
            let focus_rect = RoundedRect::from_origin_size(
                Point::new(inset, inset),
                Size::new(
                    (size.width - 2.0 * inset).max(0.0),
                    (size.height - 2.0 * inset).max(0.0),
                ),
                0.0,
            );
            paint_focus_ring(painter, focus_rect, &self.theme);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if !self.disabled {
            node.add_action(accesskit::Action::Click);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        match &self.content {
            Content::Label(label) => ChildrenIds::from_slice(&[label.id()]),
            Content::Row(row) => ChildrenIds::from_slice(&[row.id()]),
        }
    }

    // `propagates_pointer_interaction` is left at its `true` default (no
    // override): a revealed action is an interactive child that must be
    // hit-tested and hoverable. Overriding it to `false` (as this widget
    // used to, back when its only child was a passive label) would make
    // `find_widget_under_pointer` skip the action's subtree entirely,
    // making it permanently unclickable.

    fn accepts_focus(&self) -> bool {
        !self.disabled
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{CollectionWidget, NewWidget, PointerButton, PointerEvent, TextEvent};
    use masonry::kurbo::{Axis, Point};
    use masonry::layout::Length;
    use masonry::testing::{ModularWidget, TestHarness};
    use masonry::theme::default_property_set;
    use masonry::widgets::{ButtonPress, Flex, Label};

    use super::super::reveal::RevealBox;
    use super::{ACTIONS_INDEX, LABEL_INDEX, ThemedSidebarItem};
    use crate::Theme;

    fn harness() -> TestHarness<ThemedSidebarItem> {
        let widget = ThemedSidebarItem::new(NewWidget::new(Label::new("Nav")), &Theme::dark());
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (160, 28))
    }

    /// A row with a trailing action that's a plain (non-interactive) label
    /// stand-in — enough to exercise reveal/layout behavior without needing
    /// a real interactive widget.
    fn harness_with_actions() -> TestHarness<ThemedSidebarItem> {
        let theme = Theme::dark();
        let label = NewWidget::new(Label::new("Nav"));
        let actions = NewWidget::new(Label::new("gear"));
        let widget = ThemedSidebarItem::new_with_actions(label, actions, &theme);
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (160, 28))
    }

    /// A minimal interactive stand-in for a real action button: captures
    /// the pointer on `Down`, like every press widget in this crate (see
    /// `click::primary_click`). Used to verify the row correctly declines
    /// to also capture — see
    /// `pressing_the_actions_region_does_not_select_the_row`.
    fn capturing_probe() -> ModularWidget<()> {
        ModularWidget::new(())
            .accepts_pointer_interaction(true)
            .measure_fn(|(), _, _, axis, _, _| match axis {
                Axis::Horizontal | Axis::Vertical => Length::px(20.0),
            })
            .pointer_event_fn(|(), ctx, _props, event| {
                if matches!(event, PointerEvent::Down(_)) {
                    ctx.capture_pointer();
                }
            })
    }

    fn harness_with_capturing_action() -> TestHarness<ThemedSidebarItem> {
        let theme = Theme::dark();
        let label = NewWidget::new(Label::new("Nav"));
        let actions = NewWidget::new(capturing_probe());
        let widget = ThemedSidebarItem::new_with_actions(label, actions, &theme);
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (160, 28))
    }

    fn action_is_stashed(h: &mut TestHarness<ThemedSidebarItem>) -> bool {
        h.edit_root_widget(|mut wm| {
            let mut row = ThemedSidebarItem::row_mut(&mut wm);
            let mut reveal_widget = Flex::get_mut(&mut row, ACTIONS_INDEX);
            // `is_stashed()` is purely top-down (`is_explicitly_stashed ||
            // parent_stashed`, see masonry's `update.rs`), and `RevealBox`
            // stashes its *wrapped child*, not itself — so the check has to
            // go one level deeper than the `RevealBox` node the `Flex` row
            // owns directly, or it would always read `false` regardless of
            // reveal state.
            let mut reveal_box = reveal_widget.downcast::<RevealBox>();
            RevealBox::child_mut(&mut reveal_box).ctx.is_stashed()
        })
    }

    #[test]
    fn selected_and_hovered_resolve_to_different_fills() {
        // Regression for #95: a selected row and a separately-hovered row
        // are different widget instances, so `resolve_bg` must not give
        // them the same fill or the two become visually indistinguishable.
        let theme = Theme::dark();
        let selected =
            ThemedSidebarItem::new(NewWidget::new(Label::new("A")), &theme).with_selected(true);
        let hovered_unselected = ThemedSidebarItem::new(NewWidget::new(Label::new("B")), &theme);

        let selected_bg = selected.resolve_bg(false, false);
        let hovered_bg = hovered_unselected.resolve_bg(true, false);

        assert_ne!(selected_bg, hovered_bg);
        assert_eq!(selected_bg, theme.palette.surface_hi);
        assert_eq!(hovered_bg, theme.palette.surface_2);
    }

    #[test]
    fn pressed_takes_priority_over_selected() {
        let theme = Theme::dark();
        let widget =
            ThemedSidebarItem::new(NewWidget::new(Label::new("A")), &theme).with_selected(true);
        assert_eq!(widget.resolve_bg(true, true), theme.palette.surface_hi);
    }

    #[test]
    fn pointer_click_submits_press() {
        let mut h = harness();
        h.mouse_move(Point::new(80.0, 14.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<ButtonPress>().is_some());
    }

    #[test]
    fn drag_out_cancels_the_press() {
        let mut h = harness();
        h.mouse_move(Point::new(80.0, 14.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_move(Point::new(400.0, 400.0));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<ButtonPress>().is_none());
    }

    #[test]
    fn space_and_enter_activate_when_focused() {
        let mut h = harness();
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(h.pop_action::<ButtonPress>().is_some());

        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(h.pop_action::<ButtonPress>().is_some());
    }

    #[test]
    fn space_key_down_shows_the_pressed_fill_until_key_up() {
        // Regression: on_text_event used to only ever fire on key-up, so
        // Space/Enter "clicking" showed no pressed-fill feedback the way a
        // pointer click does.
        let mut h = harness();
        h.focus_on(Some(h.root_id()));

        assert!(!h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
        h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
        assert!(h.pop_action::<ButtonPress>().is_none(), "not yet activated");

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(!h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
        assert!(h.pop_action::<ButtonPress>().is_some());
    }

    #[test]
    fn losing_focus_mid_press_clears_the_keyboard_pressed_flag() {
        let mut h = harness();
        h.focus_on(Some(h.root_id()));
        h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.keyboard_pressed));

        h.focus_on(None);
        assert!(!h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
    }

    #[test]
    fn disabled_suppresses_click() {
        let theme = Theme::default();
        let widget =
            ThemedSidebarItem::new(NewWidget::new(Label::new("Nav")), &theme).with_disabled(true);
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (200, 32),
        );
        h.mouse_move(Point::new(20.0, 16.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(
            h.pop_action::<ButtonPress>().is_none(),
            "disabled sidebar item must not emit ButtonPress"
        );
    }

    #[test]
    fn hovering_reveals_and_leaving_hides_the_action() {
        let mut h = harness_with_actions();
        assert!(action_is_stashed(&mut h), "hidden before hover");
        h.mouse_move(Point::new(20.0, 14.0));
        assert!(
            !action_is_stashed(&mut h),
            "hovering the row reveals the action"
        );
        h.mouse_move(Point::new(400.0, 400.0));
        assert!(action_is_stashed(&mut h), "leaving the row hides it again");
    }

    #[test]
    fn keyboard_focus_reveals_the_action() {
        // The accessibility path: no pointer, focus lands on the row and
        // the action must appear so a keyboard user can Tab into it.
        let mut h = harness_with_actions();
        assert!(action_is_stashed(&mut h));
        h.focus_on(Some(h.root_id()));
        assert!(
            !action_is_stashed(&mut h),
            "focusing the row reveals the action"
        );
    }

    #[test]
    fn disabled_rows_never_reveal_even_when_hovered() {
        let theme = Theme::dark();
        let label = NewWidget::new(Label::new("Nav"));
        let actions = NewWidget::new(Label::new("gear"));
        let widget =
            ThemedSidebarItem::new_with_actions(label, actions, &theme).with_disabled(true);
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (160, 28),
        );
        h.mouse_move(Point::new(20.0, 14.0));
        assert!(
            action_is_stashed(&mut h),
            "a disabled row must never reveal its action"
        );
    }

    #[test]
    fn hiding_the_action_reclaims_its_space_for_the_label() {
        // The design's reclaim decision: hidden actions collapse to zero
        // width and the label reflows to fill the row; revealing shrinks
        // the label back down. This is what the real `Flex` layout buys us
        // over the old hand-computed two-child math, which instead tried to
        // reserve a stable slot and got the geometry wrong.
        let mut h = harness_with_actions();
        let hidden_label_width = h.edit_root_widget(|mut wm| {
            let mut row = ThemedSidebarItem::row_mut(&mut wm);
            Flex::get_mut(&mut row, LABEL_INDEX)
                .ctx
                .border_box()
                .width()
        });
        h.mouse_move(Point::new(20.0, 14.0));
        let revealed_label_width = h.edit_root_widget(|mut wm| {
            let mut row = ThemedSidebarItem::row_mut(&mut wm);
            Flex::get_mut(&mut row, LABEL_INDEX)
                .ctx
                .border_box()
                .width()
        });
        assert!(
            hidden_label_width > revealed_label_width,
            "the label should reflow into the reclaimed space while hidden: \
             hidden={hidden_label_width}, revealed={revealed_label_width}"
        );
    }

    #[test]
    fn pressing_the_label_region_still_selects_the_row() {
        let mut h = harness_with_actions();
        h.mouse_move(Point::new(10.0, 14.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(
            h.pop_action::<ButtonPress>().is_some(),
            "clicking the label area should still select the row"
        );
    }

    #[test]
    fn pressing_the_actions_region_does_not_select_the_row() {
        let mut h = harness_with_capturing_action();
        h.mouse_move(Point::new(20.0, 14.0)); // hover the row to reveal the action
        let action_center = h.edit_root_widget(|mut wm| {
            let mut row = ThemedSidebarItem::row_mut(&mut wm);
            let action = Flex::get_mut(&mut row, ACTIONS_INDEX);
            // `border_box()` is in the widget's own content-box coordinate
            // space, not the window's — `mouse_move` needs a window-space
            // point, so it has to go through `window_transform()` too (the
            // same composition `TestHarness::mouse_move_to` uses
            // internally).
            action.ctx.window_transform() * action.ctx.border_box().center()
        });
        h.mouse_move(action_center); // move onto the now-revealed, hit-testable action
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(
            h.pop_action::<ButtonPress>().is_none(),
            "clicking a trailing action must not select the row"
        );
    }

    #[test]
    fn set_content_can_add_and_remove_an_action_without_panicking() {
        let theme = Theme::dark();
        let widget = ThemedSidebarItem::new(NewWidget::new(Label::new("Nav")), &theme);
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (160, 28),
        );
        h.edit_root_widget(|mut wm| {
            ThemedSidebarItem::set_content(
                &mut wm,
                NewWidget::new(Label::new("Nav")),
                Some(NewWidget::new(Label::new("gear"))),
            );
        });
        assert!(
            action_is_stashed(&mut h),
            "a freshly-attached action starts hidden"
        );
        h.edit_root_widget(|mut wm| {
            ThemedSidebarItem::set_content(
                &mut wm,
                NewWidget::new(Label::new("Nav")),
                None::<NewWidget<Label>>,
            );
        });
    }
}
