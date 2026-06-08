//! `ThemedDropdownButton` — a [`crate::components::button::widget::ThemedButton`]
//! trigger (with a trailing chevron) that opens a floating menu on click.
//!
//! Composes a real `ThemedButton` as `AnchoredOverlay::primary` and delegates all
//! chrome/color/layout/focus/click handling to it — this widget owns only what's
//! genuinely dropdown-shaped: menu hosting (`overlay_host` / `OverlayScope`
//! integration), `items`, `open` state, and item-selection routing. Clicking
//! anywhere on the wrapped button (label, leading icon, or chevron) toggles the
//! menu — the inner `ThemedButton` submits a single `ButtonPress` for the whole
//! surface, same as it would for a plain action button.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ActionCtx, ArcStr, ChildrenIds, ComposeCtx, ErasedAction, LayoutCtx, MeasureCtx,
    NewWidget, PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty, Update,
    UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::{ButtonPress, Label};

use super::menu_layer::{MenuContent, MenuItemSelected};
use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::components::button::ButtonVariant;
use crate::components::button::widget::ThemedButton;
use crate::components::icon::IconName;
use crate::components::popover::PopoverAnchor;
use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

/// Action type emitted by [`ThemedDropdownButton`].
#[derive(Debug)]
pub enum DropdownButtonAction {
    /// Menu item at `index` was selected.
    ItemSelected(usize),
}

/// Button widget that opens a floating dropdown menu on click.
///
/// Wraps a [`ThemedButton`] (label + optional leading icon + trailing chevron)
/// as its trigger; toggles the menu in response to the trigger's `ButtonPress`.
pub struct ThemedDropdownButton {
    overlay_host: WidgetPod<AnchoredOverlay>,
    /// Nearest `OverlayScope` ancestor, discovered at `View::build` time via
    /// the Xilem `Environment` (see `crate::overlay_scope`). When present,
    /// the menu is pushed into the scope's overlay slot instead of
    /// `overlay_host` — see `push_into_scope`/`clear_from_scope`.
    scope: Option<OverlayScopeHandle>,
    /// Our anchor rect (window coords) as of the last scope-mode placement
    /// push — used by `compose` to detect "did I move" during scrolling
    /// without busy-looping (see `Widget::compose`).
    last_anchor_rect_window: Option<Rect>,
    icon: Option<IconName>,
    items: Vec<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    theme: Theme,
    pub(super) open: bool,
}

// --- MARK: BUILDERS
impl ThemedDropdownButton {
    #[must_use]
    pub fn new(
        label_text: ArcStr,
        icon: Option<IconName>,
        items: Vec<ArcStr>,
        variant: ButtonVariant,
        disabled: bool,
        theme: &Theme,
        scope: Option<OverlayScopeHandle>,
    ) -> Self {
        let text_color = Self::text_color_for(theme, variant, disabled);
        let icon_color = Self::icon_color_for(theme, disabled);

        let label = Label::new(label_text)
            .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
            .prepare();
        let mut label = label.erased();
        label.properties.insert(ContentColor::new(text_color));

        let mut trigger = ThemedButton::new(label, theme)
            .with_variant(variant)
            .with_disabled(disabled)
            .with_trailing_icon(
                crate::components::icon::icon(IconName::ChevronDown)
                    .color(icon_color)
                    .build_widget(theme),
            );
        if let Some(name) = icon {
            trigger = trigger.with_icon(
                crate::components::icon::icon(name)
                    .color(icon_color)
                    .build_widget(theme),
            );
        }

        let overlay_host = AnchoredOverlay::new(
            NewWidget::new(trigger).erased(),
            NewWidget::new(MenuContent::new(items.clone(), theme)),
            false,
            PopoverAnchor::BottomStart,
        );
        Self {
            overlay_host: NewWidget::new(overlay_host).to_pod(),
            scope,
            last_anchor_rect_window: None,
            icon,
            items,
            variant,
            disabled,
            theme: *theme,
            open: false,
        }
    }

    fn text_color_for(theme: &Theme, variant: ButtonVariant, disabled: bool) -> Color {
        if disabled {
            theme.palette.text_faint
        } else if variant == ButtonVariant::Link {
            theme.palette.teal
        } else {
            theme.palette.text
        }
    }

    fn icon_color_for(theme: &Theme, disabled: bool) -> Color {
        if disabled {
            theme.palette.text_faint
        } else {
            theme.palette.text
        }
    }

    /// Refreshes the leading/trailing icon labels' color and font size —
    /// mirrors `ButtonView::rebuild`'s icon-prop refresh
    /// (`components/button/view.rs`), since this widget builds those icon
    /// children directly rather than through the view layer.
    fn refresh_icon_props(trigger: &mut WidgetMut<'_, ThemedButton>, color: Color, theme: &Theme) {
        if let Some(mut m) = ThemedButton::icon_mut(trigger) {
            m.insert_prop(ContentColor::new(color));
            Label::insert_style(&mut m, StyleProperty::FontSize(theme.density.ui_font_size));
        }
        if let Some(mut m) = ThemedButton::trailing_icon_mut(trigger) {
            m.insert_prop(ContentColor::new(color));
            Label::insert_style(&mut m, StyleProperty::FontSize(theme.density.ui_font_size));
        }
    }
}

// --- MARK: WIDGETMUT SETTERS
impl ThemedDropdownButton {
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();

            let text_color = Self::text_color_for(theme, this.widget.variant, this.widget.disabled);
            let icon_color = Self::icon_color_for(theme, this.widget.disabled);
            {
                let mut overlay_host = this.ctx.get_mut(&mut this.widget.overlay_host);
                let mut primary = AnchoredOverlay::primary_mut(&mut overlay_host);
                let mut trigger = primary.downcast::<ThemedButton>();
                ThemedButton::set_theme(&mut trigger, theme);
                {
                    let mut child = ThemedButton::child_mut(&mut trigger);
                    child.insert_prop(ContentColor::new(text_color));
                    let mut child = child.downcast::<Label>();
                    Label::insert_style(
                        &mut child,
                        StyleProperty::FontSize(theme.density.ui_font_size),
                    );
                }
                Self::refresh_icon_props(&mut trigger, icon_color, theme);
            }
            {
                let mut overlay_host = this.ctx.get_mut(&mut this.widget.overlay_host);
                let mut menu = AnchoredOverlay::overlay_mut(&mut overlay_host);
                let mut menu = menu.downcast::<MenuContent>();
                MenuContent::set_theme(&mut menu, theme);
            }
        }
    }

    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();

            // Disabling mid-open must close the menu — a disabled trigger
            // can no longer be clicked to dismiss it, and a stale open menu
            // would stay interactable (selections could still fire). Mirrors
            // `close_dropdown`/the `Update::ChildFocusChanged(false)` path.
            if disabled && this.widget.open {
                this.widget.open = false;
                match this
                    .widget
                    .scope
                    .as_ref()
                    .and_then(OverlayScopeHandle::widget_id)
                {
                    Some(scope_id) => this.ctx.mutate_later(scope_id, |mut w| {
                        let mut scope = w.downcast::<OverlayScope>();
                        OverlayScope::set_overlay(
                            &mut scope,
                            None,
                            Rect::ZERO,
                            PopoverAnchor::BottomStart,
                        );
                    }),
                    None => this
                        .ctx
                        .mutate_child_later(&mut this.widget.overlay_host, |mut w| {
                            AnchoredOverlay::set_overlay_visible(&mut w, false);
                        }),
                }
            }

            let theme = this.widget.theme;
            let text_color = Self::text_color_for(&theme, this.widget.variant, disabled);
            let icon_color = Self::icon_color_for(&theme, disabled);
            let mut overlay_host = this.ctx.get_mut(&mut this.widget.overlay_host);
            let mut primary = AnchoredOverlay::primary_mut(&mut overlay_host);
            let mut trigger = primary.downcast::<ThemedButton>();
            ThemedButton::set_disabled(&mut trigger, disabled);
            {
                let mut child = ThemedButton::child_mut(&mut trigger);
                child.insert_prop(ContentColor::new(text_color));
            }
            Self::refresh_icon_props(&mut trigger, icon_color, &theme);
        }
    }

    pub fn set_variant(this: &mut WidgetMut<'_, Self>, variant: ButtonVariant) {
        let prev_variant = this.widget.variant;
        if prev_variant != variant {
            this.widget.variant = variant;
            this.ctx.request_paint_only();

            let theme = this.widget.theme;
            let disabled = this.widget.disabled;
            let mut overlay_host = this.ctx.get_mut(&mut this.widget.overlay_host);
            let mut primary = AnchoredOverlay::primary_mut(&mut overlay_host);
            let mut trigger = primary.downcast::<ThemedButton>();
            ThemedButton::set_variant(&mut trigger, variant);
            // Link gains/loses teal text; other variants share the default color.
            if !disabled && (variant == ButtonVariant::Link || prev_variant == ButtonVariant::Link)
            {
                let text_color = Self::text_color_for(&theme, variant, disabled);
                let mut child = ThemedButton::child_mut(&mut trigger);
                child.insert_prop(ContentColor::new(text_color));
            }
        }
    }

    pub fn set_items(this: &mut WidgetMut<'_, Self>, items: Vec<ArcStr>) {
        this.widget.items.clone_from(&items);
        let mut overlay_host = this.ctx.get_mut(&mut this.widget.overlay_host);
        let mut menu = AnchoredOverlay::overlay_mut(&mut overlay_host);
        let mut menu = menu.downcast::<MenuContent>();
        MenuContent::set_items(&mut menu, items);
    }

    pub fn set_icon(this: &mut WidgetMut<'_, Self>, icon: Option<IconName>) {
        this.widget.icon = icon;
        let theme = this.widget.theme;
        let icon_color = Self::icon_color_for(&theme, this.widget.disabled);
        let mut overlay_host = this.ctx.get_mut(&mut this.widget.overlay_host);
        let mut primary = AnchoredOverlay::primary_mut(&mut overlay_host);
        let mut trigger = primary.downcast::<ThemedButton>();
        match icon {
            Some(name) => ThemedButton::attach_icon(
                &mut trigger,
                crate::components::icon::icon(name)
                    .color(icon_color)
                    .build_widget(&theme),
            ),
            None => ThemedButton::detach_icon(&mut trigger),
        }
    }
}

// --- MARK: INTERNAL HELPERS
impl ThemedDropdownButton {
    fn set_overlay_visible(&mut self, ctx: &mut ActionCtx<'_>, visible: bool) {
        ctx.mutate_child_later(&mut self.overlay_host, move |mut w| {
            AnchoredOverlay::set_overlay_visible(&mut w, visible);
        });
    }

    /// Our anchor rect — the full button's chrome+padding+icon+chevron box —
    /// in window coordinates. `ctx.to_window(Point::ZERO)` is *our* origin
    /// (not the inner button's), which is what fixes the old `AnchoredOverlay`
    /// misalignment: the menu now anchors flush to the whole button.
    fn anchor_rect_window(ctx: &ActionCtx<'_>) -> Rect {
        Rect::from_origin_size(ctx.to_window(Point::ZERO), ctx.border_box_size())
    }

    /// Push a freshly built `MenuContent` into the scope's overlay slot,
    /// anchored to our own box. The menu is ephemeral in scope-mode —
    /// created on open, cleared on close (see `clear_from_scope`) — since
    /// the slot is shared, transient infrastructure that may host different
    /// triggers' menus over its lifetime (never "owned" by one button).
    fn push_into_scope(&mut self, ctx: &mut ActionCtx<'_>, scope_id: WidgetId) {
        let anchor_rect_window = Self::anchor_rect_window(ctx);
        self.last_anchor_rect_window = Some(anchor_rect_window);
        let items = self.items.clone();
        let theme = self.theme;
        ctx.request_compose();
        ctx.mutate_later(scope_id, move |mut w| {
            let mut scope = w.downcast::<OverlayScope>();
            // `to_local` is the literal inverse of `window_transform`,
            // correctly handling the full chain of transforms/scroll/origin
            // between the scope and this trigger — robust to scrolling.
            let local_origin = scope.ctx.to_local(anchor_rect_window.origin());
            let placement = Rect::from_origin_size(local_origin, anchor_rect_window.size());
            let menu = NewWidget::new(MenuContent::new(items, &theme)).erased();
            OverlayScope::set_overlay(
                &mut scope,
                Some(menu),
                placement,
                PopoverAnchor::BottomStart,
            );
        });
    }

    /// Clear our menu from the scope's overlay slot (no-op if some other
    /// trigger has since claimed it — last-writer-wins, see `OverlayScope::set_overlay`).
    fn clear_from_scope(ctx: &mut ActionCtx<'_>, scope_id: WidgetId) {
        ctx.mutate_later(scope_id, move |mut w| {
            let mut scope = w.downcast::<OverlayScope>();
            OverlayScope::set_overlay(&mut scope, None, Rect::ZERO, PopoverAnchor::BottomStart);
        });
    }

    fn open_dropdown(&mut self, ctx: &mut ActionCtx<'_>) {
        self.open = true;
        match self.scope.as_ref().and_then(OverlayScopeHandle::widget_id) {
            Some(scope_id) => self.push_into_scope(ctx, scope_id),
            None => self.set_overlay_visible(ctx, true),
        }
        ctx.request_paint_only();
    }

    fn close_dropdown(&mut self, ctx: &mut ActionCtx<'_>) {
        self.open = false;
        match self.scope.as_ref().and_then(OverlayScopeHandle::widget_id) {
            Some(scope_id) => Self::clear_from_scope(ctx, scope_id),
            None => self.set_overlay_visible(ctx, false),
        }
        ctx.request_paint_only();
    }

    fn toggle_dropdown(&mut self, ctx: &mut ActionCtx<'_>) {
        if self.open {
            self.close_dropdown(ctx);
        } else {
            self.open_dropdown(ctx);
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedDropdownButton {
    type Action = DropdownButtonAction;

    /// Routes actions bubbling up from our two children: a `ButtonPress` from
    /// the wrapped `ThemedButton` trigger (any click on the button surface, or
    /// Space/Enter while it's focused) toggles the menu; a `MenuItemSelected`
    /// from `MenuContent` (nested inside `overlay_host`) closes the menu and
    /// re-emits the selection as our own [`DropdownButtonAction::ItemSelected`].
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        _source: WidgetId,
    ) {
        if action.downcast_ref::<ButtonPress>().is_some() {
            if !self.disabled {
                self.toggle_dropdown(ctx);
            }
            ctx.set_handled();
            ctx.request_paint_only();
            return;
        }
        if let Some(&MenuItemSelected(index)) = action.downcast_ref::<MenuItemSelected>() {
            self.open = false;
            match self.scope.as_ref().and_then(OverlayScopeHandle::widget_id) {
                Some(scope_id) => ctx.mutate_later(scope_id, |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    OverlayScope::set_overlay(
                        &mut scope,
                        None,
                        Rect::ZERO,
                        PopoverAnchor::BottomStart,
                    );
                }),
                None => ctx.mutate_child_later(&mut self.overlay_host, |mut w| {
                    AnchoredOverlay::set_overlay_visible(&mut w, false);
                }),
            }
            ctx.submit_action::<Self::Action>(DropdownButtonAction::ItemSelected(index));
            ctx.set_handled();
            ctx.request_paint_only();
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => {
                ctx.set_disabled(self.disabled);
            }
            // The wrapped `ThemedButton` is the actual focus target now, so we
            // react to `ChildFocusChanged` — masonry's "focus entered/left my
            // subtree" signal for ancestors — rather than `FocusChanged`. A
            // click landing outside our subtree clears focus from the button,
            // which is the standard "click outside to dismiss" path; the menu
            // remains a *descendant* (clicks on the trigger or inside the menu
            // keep our subtree focused), so this only fires for genuine
            // outside clicks.
            Update::ChildFocusChanged(false) if self.open => {
                self.open = false;
                match self.scope.as_ref().and_then(OverlayScopeHandle::widget_id) {
                    Some(scope_id) => ctx.mutate_later(scope_id, |mut w| {
                        let mut scope = w.downcast::<OverlayScope>();
                        OverlayScope::set_overlay(
                            &mut scope,
                            None,
                            Rect::ZERO,
                            PopoverAnchor::BottomStart,
                        );
                    }),
                    None => ctx.mutate_child_later(&mut self.overlay_host, |mut w| {
                        AnchoredOverlay::set_overlay_visible(&mut w, false);
                    }),
                }
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    /// Re-anchors a still-open scope-mode menu as we move in window space —
    /// e.g. while the user scrolls a `ScrollContainer` containing us. The
    /// menu lives in a structurally separate subtree (the scope's overlay
    /// slot), so — unlike `AnchoredOverlay`, which tracks for free as a
    /// rigidly-attached descendant — nothing re-places it automatically.
    ///
    /// Self-renews only while we're actually moving: each call compares our
    /// current window-space anchor rect to the last one we pushed, and only
    /// re-arms (`mutate_self_later` → `request_compose`) when it changed.
    /// `ComposeCtx` exposes neither `transform_has_changed` nor
    /// `request_compose` (both are `MutateCtx`-only), hence the comparison —
    /// it gives the same "stop cleanly once stable, no busy-loop" behavior.
    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        if !self.open {
            return;
        }
        let Some(scope_id) = self.scope.as_ref().and_then(OverlayScopeHandle::widget_id) else {
            return;
        };
        let anchor_rect_window =
            Rect::from_origin_size(ctx.to_window(Point::ZERO), ctx.border_box_size());
        if self.last_anchor_rect_window == Some(anchor_rect_window) {
            return;
        }
        self.last_anchor_rect_window = Some(anchor_rect_window);
        ctx.mutate_later(scope_id, move |mut w| {
            let mut scope = w.downcast::<OverlayScope>();
            let local_origin = scope.ctx.to_local(anchor_rect_window.origin());
            let placement = Rect::from_origin_size(local_origin, anchor_rect_window.size());
            OverlayScope::set_placement(&mut scope, placement, PopoverAnchor::BottomStart);
        });
        ctx.mutate_self_later(|mut w| {
            w.ctx.request_compose();
        });
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.overlay_host);
    }

    /// Pure transparent forward to `overlay_host` — `AnchoredOverlay` already
    /// sizes itself to its `primary` (the wrapped `ThemedButton`), so there's
    /// nothing dropdown-shaped to add here. Mirrors `pointer_inert`'s
    /// single-child passthrough.
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
        // Purely structural — `overlay_host` (and transitively the wrapped
        // `ThemedButton`) paints itself.
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &masonry::core::PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if !self.disabled {
            node.add_action(masonry::accesskit::Action::Click);
        }
        node.add_action(masonry::accesskit::Action::Expand);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.overlay_host.id()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_always_wins_regardless_of_variant() {
        let theme = Theme::default();
        for variant in [
            ButtonVariant::Default,
            ButtonVariant::Link,
            ButtonVariant::Primary,
        ] {
            assert_eq!(
                ThemedDropdownButton::text_color_for(&theme, variant, true),
                theme.palette.text_faint,
                "{variant:?} should still read as faint while disabled"
            );
        }
        assert_eq!(
            ThemedDropdownButton::icon_color_for(&theme, true),
            theme.palette.text_faint
        );
    }

    #[test]
    fn link_variant_reads_in_the_accent_color_when_enabled() {
        let theme = Theme::default();
        assert_eq!(
            ThemedDropdownButton::text_color_for(&theme, ButtonVariant::Link, false),
            theme.palette.teal
        );
    }

    #[test]
    fn non_link_variants_read_as_plain_text_when_enabled() {
        let theme = Theme::default();
        for variant in [
            ButtonVariant::Default,
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
        ] {
            assert_eq!(
                ThemedDropdownButton::text_color_for(&theme, variant, false),
                theme.palette.text,
                "{variant:?} should use the plain text color"
            );
        }
    }

    #[test]
    fn icon_color_ignores_variant_and_only_responds_to_disabled() {
        let theme = Theme::default();
        assert_eq!(
            ThemedDropdownButton::icon_color_for(&theme, false),
            theme.palette.text
        );
        assert_eq!(
            ThemedDropdownButton::icon_color_for(&theme, true),
            theme.palette.text_faint
        );
    }
}
