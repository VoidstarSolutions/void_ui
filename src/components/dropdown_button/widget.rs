//! `ThemedDropdownButton` — a `crate::components::button::widget::ThemedButton`
//! trigger (with a trailing chevron) that opens a floating menu on click.
//!
//! Composes a real `ThemedButton` as its trigger and delegates all
//! chrome/color/layout/focus/click handling to it — this widget owns only what's
//! genuinely dropdown-shaped: menu hosting, `items`, `open` state, and
//! item-selection routing. Clicking anywhere on the wrapped button (label,
//! leading icon, or chevron) toggles the menu — the inner `ThemedButton`
//! submits a single `ButtonPress` for the whole surface, same as it would for
//! a plain action button.
//!
//! Two hosting modes, mirroring `crate::components::popover::widget::PopoverHost`:
//!
//! - **Portal** (scope ancestor present): the menu is registered as a
//!   [`super::view::MenuContentView`] in the scope's [`crate::overlay_portal::OverlayPortal`]
//!   with [`crate::overlay_portal::PortalPlacement::BareTrigger`] — anchored
//!   like a popover but unwrapped, since `MenuContent` paints its own chrome.
//!   Open/close/highlight are pushed as plain data via `mutate_later`.
//! - **In-tree** (fallback, no scope): `MenuContent` is permanently mounted in
//!   an [`AnchoredOverlay`] below the trigger, toggled via
//!   `AnchoredOverlay::set_overlay_visible`.

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ActionCtx, ArcStr, ChildrenIds, ComposeCtx, ErasedAction, EventCtx, LayoutCtx,
    MeasureCtx, NewWidget, PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty,
    TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::{ButtonPress, Label, Passthrough};

use super::menu_layer::MenuContent;
use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::collection::CollectionListWidget;
use crate::components::button::ButtonVariant;
use crate::components::button::widget::ThemedButton;
use crate::components::icon::IconName;
use crate::overlay::OverlayAnchor;
use crate::overlay::binding::{self, PortalBinding, PortalCtx, PortalOpenCtx};
use crate::overlay_portal::PortalSlot;
use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

/// Action type emitted by [`ThemedDropdownButton`].
#[derive(Debug)]
pub enum DropdownButtonAction {
    /// Menu item at `index` was selected.
    ItemSelected(usize),
    /// The menu's open state changed (uncontrolled), or the user requested a
    /// change (controlled — the widget does not self-toggle then).
    OpenChanged(bool),
}

widget_id_handle!(
    /// Self-filling handle to a [`ThemedDropdownButton`]'s widget id, filled at
    /// `Update::WidgetAdded` — mirrors [`OverlayScopeHandle`]'s bootstrapping.
    ///
    /// Given to `super::view::build_menu_view`'s `on_activated` hook (both
    /// hosting modes now, not just portal — see `super::view::MenuBinding`)
    /// so a click selection can `mutate_later` back into the dropdown to
    /// close the menu: item selection resolves entirely through
    /// `overlay_list`'s View-layer `on_select`/`on_activated` now, which
    /// never bubbles a masonry action to [`ThemedDropdownButton::on_action`]
    /// at all, in either hosting mode.
    DropdownButtonHandle
);

/// Trigger-construction inputs shared by [`ThemedDropdownButton::new`] and
/// [`ThemedDropdownButton::new_portal`]; bundled to keep `new_portal`'s
/// argument count under clippy's `too_many_arguments` threshold.
pub(crate) struct DropdownButtonConfig {
    pub(crate) label_text: ArcStr,
    pub(crate) icon: Option<IconName>,
    pub(crate) items: Vec<ArcStr>,
    pub(crate) variant: ButtonVariant,
    pub(crate) disabled: bool,
    pub(crate) theme: Theme,
}

/// How this dropdown mounts its menu: permanently in-tree (fallback, no scope
/// ancestor), or portal-mounted in the nearest scope's `PortalSlot` (the menu
/// is a view child of the *scope*; we only hold the key).
enum Hosting {
    InTree {
        overlay_host: WidgetPod<AnchoredOverlay>,
    },
    Portal {
        trigger: WidgetPod<dyn Widget>,
        binding: PortalBinding,
    },
}

/// Get the wrapped `ThemedButton` trigger as a `WidgetMut`, regardless of
/// hosting mode — in-tree it's `AnchoredOverlay::primary`, in portal mode
/// it's the lone hosted child.
macro_rules! with_trigger {
    ($this:ident, |$trigger:ident| $body:block) => {
        match &mut $this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut overlay_host = $this.ctx.get_mut(overlay_host);
                let mut primary = AnchoredOverlay::primary_mut(&mut overlay_host);
                let mut $trigger = primary.downcast::<ThemedButton>();
                $body
            }
            Hosting::Portal { trigger, .. } => {
                let mut trigger = $this.ctx.get_mut(trigger);
                let mut $trigger = trigger.downcast::<ThemedButton>();
                $body
            }
        }
    };
}

/// Button widget that opens a floating dropdown menu on click.
///
/// Wraps a [`ThemedButton`] (label + optional leading icon + trailing chevron)
/// as its trigger; toggles the menu in response to the trigger's `ButtonPress`.
pub struct ThemedDropdownButton {
    hosting: Hosting,
    handle: DropdownButtonHandle,
    label: ArcStr,
    icon: Option<IconName>,
    items: Vec<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    theme: Theme,
    pub(super) open: bool,
    /// When true, `open` mirrors a host prop and internal toggles only
    /// submit [`DropdownButtonAction::OpenChanged`] instead of self-mutating.
    controlled: bool,
    /// Keyboard-highlighted item index for roving-tab-stop navigation.
    /// `None` means no keyboard highlight; updated on arrow keys and cleared
    /// when the menu closes or an item is selected.
    highlighted: Option<usize>,
}

// --- MARK: BUILDERS
impl ThemedDropdownButton {
    fn build_trigger(
        label_text: &ArcStr,
        icon: Option<IconName>,
        variant: ButtonVariant,
        disabled: bool,
        theme: &Theme,
    ) -> NewWidget<dyn Widget> {
        let text_color = Self::text_color_for(theme, variant, disabled);
        let icon_color = Self::icon_color_for(theme, disabled);

        let label = Label::new(label_text.clone())
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
        NewWidget::new(trigger).erased()
    }

    /// In-tree constructor (fallback, no scope ancestor): `menu_content` is
    /// the already-built, erased element `super::view::MenuContentView`
    /// (built by `DropdownButtonView::build`) produced — this widget only
    /// embeds it in the `AnchoredOverlay`'s overlay slot, permanently
    /// mounted below the trigger. Mirrors
    /// `crate::components::autocomplete::widget::AutocompleteWidget::new`.
    #[must_use]
    pub(crate) fn new(
        config: DropdownButtonConfig,
        menu_content: NewWidget<dyn Widget>,
        handle: DropdownButtonHandle,
    ) -> Self {
        let DropdownButtonConfig {
            label_text,
            icon,
            items,
            variant,
            disabled,
            theme,
        } = config;
        let trigger = Self::build_trigger(&label_text, icon, variant, disabled, &theme);
        let overlay_host =
            AnchoredOverlay::new(trigger, menu_content, false, OverlayAnchor::BottomStart);
        Self {
            hosting: Hosting::InTree {
                overlay_host: NewWidget::new(overlay_host).to_pod(),
            },
            handle,
            label: label_text,
            icon,
            items,
            variant,
            disabled,
            theme,
            open: false,
            controlled: false,
            highlighted: None,
        }
    }

    /// Portal-mode constructor: the menu lives in the scope's slot under
    /// `key`, registered by the view layer as a `MenuContentView`; we host
    /// only the trigger. `handle` is filled at `Update::WidgetAdded` and
    /// given to the registered `MenuContentView` so it can notify us back.
    #[must_use]
    pub(crate) fn new_portal(
        config: DropdownButtonConfig,
        handle: DropdownButtonHandle,
        scope: OverlayScopeHandle,
        key: u64,
    ) -> Self {
        let DropdownButtonConfig {
            label_text,
            icon,
            items,
            variant,
            disabled,
            theme,
        } = config;
        let trigger = Self::build_trigger(&label_text, icon, variant, disabled, &theme);
        Self {
            hosting: Hosting::Portal {
                trigger: trigger.to_pod(),
                binding: PortalBinding::new(scope, key, dropdown_dismiss_hook),
            },
            handle,
            label: label_text,
            icon,
            items,
            variant,
            disabled,
            theme,
            open: false,
            controlled: false,
            highlighted: None,
        }
    }

    /// Set the initial open state and whether it is host-controlled.
    #[must_use]
    pub fn with_open_state(mut self, open: bool, controlled: bool) -> Self {
        self.open = open;
        self.controlled = controlled;
        self
    }

    /// Navigate to the in-tree `AnchoredOverlay`'s overlay slot content and
    /// invoke `f`. The overlay slot holds erased `NewWidget<dyn Widget>`
    /// content (a `Passthrough` wrapping `MenuContent`, once
    /// `super::view::DropdownButtonView` builds it — see
    /// `super::view::build_menu_view`); `f` is expected to
    /// `downcast::<Passthrough>()` before forwarding into the nested
    /// `MenuContentView` it wraps.
    ///
    /// Used by `DropdownButtonView::rebuild`/`teardown`/`message` to forward
    /// into the in-tree nested list view — portal mode never needs this (its
    /// `MenuContentView` is registered with, and dispatched by, the scope's
    /// own portal registry instead). Panics if called in portal mode.
    /// Mirrors `AutocompleteWidget::with_overlay_content`.
    pub(crate) fn with_overlay_content<R>(
        this: &mut WidgetMut<'_, Self>,
        f: impl FnOnce(WidgetMut<'_, dyn Widget>) -> R,
    ) -> R {
        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut h = this.ctx.get_mut(overlay_host);
                let content = AnchoredOverlay::overlay_mut(&mut h);
                f(content)
            }
            Hosting::Portal { .. } => {
                unreachable!("with_overlay_content called in portal mode")
            }
        }
    }

    fn text_color_for(theme: &Theme, variant: ButtonVariant, disabled: bool) -> Color {
        if disabled {
            theme.palette.text_faint
        } else if variant == ButtonVariant::Link {
            theme.palette.accent
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
            with_trigger!(this, |trigger| {
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
            });

            // The menu's own theme flows through `super::view::
            // MenuContentView`'s `rebuild` instead (both hosting modes now
            // build/rebuild it as a real, nested View — see
            // `DropdownButtonView`), so this widget doesn't need to reach
            // into it directly at all — mirrors `AutocompleteWidget::
            // set_theme`.
        }
    }

    pub fn set_label(this: &mut WidgetMut<'_, Self>, label: ArcStr) {
        if this.widget.label == label {
            return;
        }
        this.widget.label = label.clone();
        let theme = this.widget.theme;
        let text_color = Self::text_color_for(&theme, this.widget.variant, this.widget.disabled);
        with_trigger!(this, |trigger| {
            let mut child = ThemedButton::child_mut(&mut trigger);
            child.insert_prop(ContentColor::new(text_color));
            let mut child = child.downcast::<Label>();
            Label::insert_style(
                &mut child,
                StyleProperty::FontSize(theme.density.ui_font_size),
            );
            Label::set_text(&mut child, label);
        });
    }

    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();

            // Disabling mid-open must close the menu — a disabled trigger
            // can no longer be clicked to dismiss it, and a stale open menu
            // would stay interactable (selections could still fire). Mirrors
            // `PopoverHost`'s `Update::StashedChanged(true)` path.
            if disabled && this.widget.open {
                this.widget.open = false;
                this.widget.highlighted = None;
                this.widget.close_menu(&mut this.ctx);
                this.ctx
                    .submit_action::<DropdownButtonAction>(DropdownButtonAction::OpenChanged(
                        false,
                    ));
            }

            let theme = this.widget.theme;
            let text_color = Self::text_color_for(&theme, this.widget.variant, disabled);
            let icon_color = Self::icon_color_for(&theme, disabled);
            with_trigger!(this, |trigger| {
                ThemedButton::set_disabled(&mut trigger, disabled);
                {
                    let mut child = ThemedButton::child_mut(&mut trigger);
                    child.insert_prop(ContentColor::new(text_color));
                }
                Self::refresh_icon_props(&mut trigger, icon_color, &theme);
            });
        }
    }

    pub fn set_variant(this: &mut WidgetMut<'_, Self>, variant: ButtonVariant) {
        let prev_variant = this.widget.variant;
        if prev_variant != variant {
            this.widget.variant = variant;
            this.ctx.request_paint_only();

            let theme = this.widget.theme;
            let disabled = this.widget.disabled;
            with_trigger!(this, |trigger| {
                ThemedButton::set_variant(&mut trigger, variant);
                // Link gains/loses accent text; other variants share the default color.
                if !disabled
                    && (variant == ButtonVariant::Link || prev_variant == ButtonVariant::Link)
                {
                    let text_color = Self::text_color_for(&theme, variant, disabled);
                    let mut child = ThemedButton::child_mut(&mut trigger);
                    child.insert_prop(ContentColor::new(text_color));
                }
            });
        }
    }

    /// Replace the item list. Item *content* now flows entirely through
    /// `super::view::DropdownButtonView::rebuild`'s forward into the nested
    /// `MenuContentView` (both hosting modes — see its doc comment), so this
    /// only updates our own copy of `items`, used for Home/End bounds checks
    /// and `move_highlight`'s wrap length.
    pub fn set_items(this: &mut WidgetMut<'_, Self>, items: Vec<ArcStr>) {
        this.widget.items = items;
    }

    pub fn set_icon(this: &mut WidgetMut<'_, Self>, icon: Option<IconName>) {
        this.widget.icon = icon;
        let theme = this.widget.theme;
        let icon_color = Self::icon_color_for(&theme, this.widget.disabled);
        with_trigger!(this, |trigger| {
            match icon {
                Some(name) => ThemedButton::attach_icon(
                    &mut trigger,
                    crate::components::icon::icon(name)
                        .color(icon_color)
                        .build_widget(&theme),
                ),
                None => ThemedButton::detach_icon(&mut trigger),
            }
        });
    }

    /// Sync `open`/`highlighted` after the portal slot dismissed the menu
    /// without going through our own event handlers (outside press;
    /// `PortalSlot::dismiss_outside`), called via `mutate_later(handle)` from
    /// [`dropdown_dismiss_hook`]. The slot has *already hidden* the content,
    /// so the mirror must go `false` in both modes — a genuine safety close,
    /// like `Update::StashedChanged(true)`. Controlled hosts observe
    /// [`DropdownButtonAction::OpenChanged`] and re-apply `true` via
    /// [`Self::set_open`] if they disagree. Also pushes the closed state to
    /// the slot — idempotent since it's already hidden.
    pub(crate) fn mark_closed(this: &mut WidgetMut<'_, Self>) {
        if this.widget.open {
            this.widget.open = false;
            this.widget.highlighted = None;
            this.widget.close_menu(&mut this.ctx);
            this.ctx
                .submit_action::<DropdownButtonAction>(DropdownButtonAction::OpenChanged(false));
            this.ctx.request_paint_only();
        }
    }

    /// Close after a *click* completes an ordinary item selection, in either
    /// hosting mode (see `super::view::build_menu_view`'s `on_activated`
    /// hook, which calls this via `mutate_later(handle)` from
    /// `OverlayListItem::on_pointer_event`). Unlike [`Self::mark_closed`],
    /// nothing has pre-hidden the menu content here — this *is* the close —
    /// so it must honor controlled mode exactly like `on_action`'s
    /// `ButtonPress` handling of a keyboard Enter-select-highlighted-item:
    /// gate the mutation behind `!controlled`, always report the
    /// `OpenChanged(false)` desire.
    pub(crate) fn close_for_selection(this: &mut WidgetMut<'_, Self>) {
        if !this.widget.open {
            return;
        }
        if !this.widget.controlled {
            this.widget.open = false;
            this.widget.highlighted = None;
            this.widget.close_menu(&mut this.ctx);
        }
        this.ctx
            .submit_action::<DropdownButtonAction>(DropdownButtonAction::OpenChanged(false));
        this.ctx.request_paint_only();
    }

    /// Apply a host-driven open state (the rebuild path for `.open(bool)`).
    /// No-op when the value is already current, so it is safe to call on
    /// every rebuild.
    pub fn set_open(this: &mut WidgetMut<'_, Self>, open: bool) {
        if this.widget.open == open {
            return;
        }
        // A disabled dropdown must never open, even via the controlled prop —
        // otherwise a host that keeps `open=true` set across rebuilds could
        // re-open a dropdown that was just disabled. The disabled force-close
        // path lives in `set_disabled` and still runs regardless.
        if open && this.widget.disabled {
            return;
        }
        this.widget.open = open;
        if open {
            this.widget.open_menu(&mut this.ctx);
        } else {
            this.widget.highlighted = None;
            this.widget.close_menu(&mut this.ctx);
        }
        this.ctx.request_paint_only();
    }

    /// Switch between controlled and uncontrolled mode (the rebuild path for
    /// a view that gains/loses its `.open(bool)` prop across rebuilds).
    pub fn set_controlled(this: &mut WidgetMut<'_, Self>, controlled: bool) {
        this.widget.controlled = controlled;
    }
}

/// Dismiss hook registered with the portal slot (see
/// [`crate::overlay_portal::DismissHook`]): syncs `open`/`highlighted`
/// after an outside-press dismissal via [`ThemedDropdownButton::mark_closed`].
pub(crate) fn dropdown_dismiss_hook(mut w: WidgetMut<'_, dyn Widget>) {
    let mut dropdown = w.downcast::<ThemedDropdownButton>();
    ThemedDropdownButton::mark_closed(&mut dropdown);
}

// --- MARK: INTERNAL HELPERS
impl ThemedDropdownButton {
    /// Close the menu in whichever host mounts it. Shared by every close path
    /// (item selection, Escape, outside focus loss, disabled mid-open, stash);
    /// generic over the context — see [`PortalCtx`].
    fn close_menu(&mut self, ctx: &mut impl PortalCtx) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.queue_mutate(overlay_host.id(), |mut w| {
                    let mut overlay = w.downcast::<AnchoredOverlay>();
                    AnchoredOverlay::set_overlay_visible(&mut overlay, false);
                });
            }
            Hosting::Portal { binding, .. } => binding.close(ctx),
        }
    }

    /// Open the menu in whichever host mounts it (trigger toggle only).
    fn open_menu(&mut self, ctx: &mut impl PortalOpenCtx) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.queue_mutate(overlay_host.id(), |mut w| {
                    let mut overlay = w.downcast::<AnchoredOverlay>();
                    AnchoredOverlay::set_overlay_visible(&mut overlay, true);
                });
            }
            Hosting::Portal { binding, .. } => {
                binding.open(ctx, OverlayAnchor::BottomStart, 0.0);
            }
        }
    }

    /// Push `index` into the wrapped `CollectionListWidget` for
    /// painting/scroll-into-view, then store it.
    ///
    /// Reaches all the way into `CollectionListWidget::set_highlight`
    /// (`crate::collection`), not just `MenuContent` itself — unlike the
    /// pre-virtualization `MenuContent::set_highlighted`, which owned its
    /// own highlight-painting state directly. `ThemedDropdownButton` keeps
    /// real keyboard focus on its trigger button throughout the whole menu
    /// interaction (roving-highlight model), so `CollectionListWidget`'s own
    /// `on_text_event` arrow-key handling never fires here (it would only
    /// run if the listbox itself had real focus, as in autocomplete's
    /// Tab-into-listbox model) — this external push is the only way the
    /// highlight ever reaches it. See `crate::collection`'s
    /// `CollectionListWidget` re-export doc comment for why this needs the
    /// type nameable outside `crate::collection` at all.
    fn set_highlight(&mut self, ctx: &mut EventCtx<'_>, index: Option<usize>) {
        self.highlighted = index;
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.mutate_child_later(overlay_host, move |mut w| {
                    let mut menu = AnchoredOverlay::overlay_mut(&mut w);
                    let mut menu = menu.downcast::<MenuContent<CollectionListWidget>>();
                    let mut list = MenuContent::child_mut(&mut menu);
                    CollectionListWidget::set_highlight(&mut list, index);
                });
            }
            Hosting::Portal { binding, .. } => {
                let Some(scope_id) = binding.scope_widget_id() else {
                    return;
                };
                let key = binding.key();
                ctx.mutate_later(scope_id, move |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    let mut slot = OverlayScope::portal_slot_mut(&mut scope);
                    if let Some(mut child) = PortalSlot::child_mut(&mut slot, key) {
                        let mut pass = child.downcast::<Passthrough>();
                        let mut menu = Passthrough::child_mut(&mut pass);
                        let mut menu = menu.downcast::<MenuContent<CollectionListWidget>>();
                        let mut list = MenuContent::child_mut(&mut menu);
                        CollectionListWidget::set_highlight(&mut list, index);
                    }
                });
            }
        }
    }

    /// Move the keyboard highlight by `delta` positions (wrapping).
    ///
    /// Called from `on_text_event` in response to arrow keys.
    fn move_highlight(&mut self, ctx: &mut EventCtx<'_>, delta: isize) {
        let n = self.items.len();
        if n == 0 {
            return;
        }
        let next = match self.highlighted {
            None => {
                if delta >= 0 {
                    0usize
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
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedDropdownButton {
    type Action = DropdownButtonAction;

    /// Routes actions bubbling up from our children: a `ButtonPress` from the
    /// wrapped `ThemedButton` trigger (any click on the button surface, or
    /// Space/Enter while it's focused) toggles the menu, or — if the menu is
    /// open with a keyboard highlight — selects that item directly (Enter
    /// goes to the focused trigger, not the menu, regardless of hosting
    /// mode). Pointer-driven item selection no longer bubbles a masonry
    /// action here at all (in either hosting mode) — it resolves entirely
    /// through `super::view::MenuContentView`'s `on_select`/`on_activated`,
    /// which call the item's callback directly and close the menu via
    /// `Self::close_for_selection` (see `super::view::build_menu_view`'s
    /// doc comment).
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        _source: WidgetId,
    ) {
        if let Some(press) = action.downcast_ref::<ButtonPress>() {
            if !self.disabled {
                if self.open
                    && press.button.is_none()
                    && let Some(index) = self.highlighted
                {
                    if !self.controlled {
                        self.open = false;
                        self.highlighted = None;
                        self.close_menu(ctx);
                    }
                    ctx.submit_action::<Self::Action>(DropdownButtonAction::ItemSelected(index));
                    ctx.submit_action::<Self::Action>(DropdownButtonAction::OpenChanged(false));
                    ctx.set_handled();
                    ctx.request_paint_only();
                    return;
                }
                let desired = !self.open;
                if !self.controlled {
                    if desired {
                        self.open = true;
                        self.open_menu(ctx);
                    } else {
                        self.open = false;
                        self.highlighted = None;
                        self.close_menu(ctx);
                    }
                }
                ctx.submit_action::<Self::Action>(DropdownButtonAction::OpenChanged(desired));
            }
            ctx.set_handled();
            ctx.request_paint_only();
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
            Key::Named(NamedKey::Home) if !self.items.is_empty() => {
                self.set_highlight(ctx, Some(0));
                ctx.set_handled();
            }
            Key::Named(NamedKey::End) if !self.items.is_empty() => {
                self.set_highlight(ctx, Some(self.items.len() - 1));
                ctx.set_handled();
            }
            Key::Named(NamedKey::Escape) => {
                // Close without selecting; returns focus to the trigger button
                // naturally (it was always focused).
                if !self.controlled {
                    self.open = false;
                    self.highlighted = None;
                    self.close_menu(ctx);
                }
                ctx.submit_action::<Self::Action>(DropdownButtonAction::OpenChanged(false));
                ctx.set_handled();
                ctx.request_paint_only();
            }
            // Keeps real keyboard focus on the trigger for the whole time
            // the menu is open (the roving-highlight model documented on the
            // struct and on `CollectionListWidget::accepts_focus`) by
            // consuming Tab outright rather than leaving it unhandled.
            //
            // This is required in addition to `CollectionListWidget::
            // accepts_focus` being `false`: that flag stops masonry's
            // default unhandled-Tab focus search from landing on
            // `CollectionListWidget` itself, but the search still descends
            // into *its* children when it doesn't accept focus, and the
            // `VirtualScroll` widget wrapped inside it (`masonry::widgets::
            // VirtualScroll`) hardcodes `accepts_focus() -> true` for its
            // own unrelated reasons (see that widget's doc comment — it
            // wants PageDown to work with no focusable child). Left
            // unhandled, Tab would still walk past `CollectionListWidget`
            // and land inside the virtualized list on `VirtualScroll`
            // itself — in portal mode that reproduces the exact keyboard
            // trap this all guards against, since `VirtualScroll` there
            // isn't a `ThemedDropdownButton` descendant and has no Escape
            // handling of its own. Consuming Tab here, at the point where
            // focus is still on the trigger (a `ThemedDropdownButton`
            // ancestor bubbles into before any of that runs), stops
            // masonry's default search from running at all, so it never
            // gets the chance to descend into the menu subtree.
            Key::Named(NamedKey::Tab) => {
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => {
                ctx.set_disabled(self.disabled);
                self.handle.set(ctx.widget_id());
                if self.open {
                    self.open_menu(ctx);
                }
            }
            // The wrapped `ThemedButton` is the actual focus target now, so we
            // react to `ChildFocusChanged` — masonry's "focus entered/left my
            // subtree" signal for ancestors — rather than `FocusChanged`. Only
            // meaningful in-tree: there the menu is a *descendant* (clicks on
            // the trigger or inside the menu keep our subtree focused), so
            // this only fires for genuine outside clicks. In portal mode the
            // menu lives in the scope's slot, not under us — the slot's own
            // outside-press dismissal handles it instead (see
            // `PortalSlot::dismiss_outside`).
            Update::ChildFocusChanged(false)
                if self.open && matches!(self.hosting, Hosting::InTree { .. }) =>
            {
                if !self.controlled {
                    self.open = false;
                    self.highlighted = None;
                    self.close_menu(ctx);
                }
                ctx.submit_action::<Self::Action>(DropdownButtonAction::OpenChanged(false));
                ctx.request_paint_only();
            }
            // A trigger stashed mid-open (e.g. a tab/panel container hiding us
            // without tearing us down) can no longer be clicked to dismiss its
            // menu, and the menu would stay visible/painted once the host is
            // unstashed even though `self.open` is now false. Close eagerly —
            // mirrors `PopoverHost`'s `Update::StashedChanged(true)` path —
            // for both hosting modes.
            Update::StashedChanged(true) if self.open => {
                self.open = false;
                self.highlighted = None;
                self.close_menu(ctx);
                ctx.submit_action::<Self::Action>(DropdownButtonAction::OpenChanged(false));
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    /// Re-anchors a still-open portal-mode menu as we move in window space —
    /// e.g. while the user scrolls a `ScrollContainer` containing us. The menu
    /// lives in a structurally separate subtree (the scope's portal slot), so
    /// — unlike `AnchoredOverlay`, which tracks for free as a rigidly-attached
    /// descendant — nothing re-places it automatically. Mirrors
    /// `PopoverHost::compose`; no-op in-tree.
    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        let binding = match &mut self.hosting {
            Hosting::Portal { binding, .. } => Some(binding),
            Hosting::InTree { .. } => None,
        };
        binding::compose_reanchor(ctx, self.open, binding);
    }

    /// Keeps a still-open portal-mode menu's [`Self::compose`] running every
    /// frame — see [`binding::arm_reanchor_on_anim_frame`].
    fn on_anim_frame(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, _: u64) {
        binding::arm_reanchor_on_anim_frame(
            ctx,
            self.open,
            matches!(self.hosting, Hosting::Portal { .. }),
        );
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => ctx.register_child(overlay_host),
            Hosting::Portal { trigger, .. } => ctx.register_child(trigger),
        }
    }

    /// Pure transparent forward to whichever child hosts the trigger —
    /// `AnchoredOverlay` (in-tree) already sizes itself to its `primary`, and
    /// in portal mode the trigger is our only child. Mirrors `pointer_inert`'s
    /// single-child passthrough.
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
            Hosting::Portal { trigger, .. } => {
                ctx.redirect_measurement(trigger, axis, cross_length)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.run_layout(overlay_host, size);
                ctx.place_child(overlay_host, Point::ORIGIN);
                ctx.derive_baselines(overlay_host);
            }
            Hosting::Portal { trigger, .. } => {
                ctx.run_layout(trigger, size);
                ctx.place_child(trigger, Point::ORIGIN);
                ctx.derive_baselines(trigger);
            }
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural — whichever child hosts the trigger paints itself.
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
        match &self.hosting {
            Hosting::InTree { overlay_host } => ChildrenIds::from_slice(&[overlay_host.id()]),
            Hosting::Portal { trigger, .. } => ChildrenIds::from_slice(&[trigger.id()]),
        }
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
            theme.palette.accent
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

    /// Builds an erased `MenuContent<CollectionListWidget>` with `item_count`
    /// items — the same shape `super::view::DropdownButtonView` builds via
    /// `overlay_list(...)`, for widget-level tests that don't go through the
    /// view layer.
    fn dummy_menu_content(
        theme: &Theme,
        item_count: usize,
    ) -> masonry::core::NewWidget<dyn Widget> {
        use masonry::core::NewWidget;
        use xilem::masonry::widgets::VirtualScroll as VirtualScrollWidget;

        let vs = NewWidget::new(VirtualScrollWidget::new(0, item_count));
        // `false`: matches the real `build_menu_view` call site
        // (`view.rs`) — dropdown_button's roving-highlight-on-trigger model
        // means this must never be a Tab stop.
        let list = CollectionListWidget::new(vs, item_count, Role::Menu, false);
        NewWidget::new(MenuContent::new(NewWidget::new(list), theme)).erased()
    }

    #[test]
    fn controlled_mode_reports_open_changed_without_self_toggling() {
        use masonry::core::NewWidget;
        use masonry::kurbo::Point;
        use masonry::testing::TestHarness;
        use masonry::theme::default_property_set;
        use xilem::view::PointerButton;

        let theme = Theme::default();
        let menu = dummy_menu_content(&theme, 2);
        let widget = ThemedDropdownButton::new(
            DropdownButtonConfig {
                label_text: "Menu".into(),
                icon: None,
                items: vec!["A".into(), "B".into()],
                variant: ButtonVariant::Default,
                disabled: false,
                theme,
            },
            menu,
            DropdownButtonHandle::new(),
        )
        .with_open_state(false, true);
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.mouse_move(Point::new(10.0, 10.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));

        assert!(
            !h.edit_root_widget(|wm| wm.widget.open),
            "controlled dropdown must not self-open"
        );
        let open_changed = std::iter::from_fn(|| h.pop_action::<DropdownButtonAction>()).find_map(
            |(a, _)| match a {
                DropdownButtonAction::OpenChanged(v) => Some(v),
                DropdownButtonAction::ItemSelected(_) => None,
            },
        );
        assert_eq!(open_changed, Some(true));

        h.edit_root_widget(|mut wm| ThemedDropdownButton::set_open(&mut wm, true));
        assert!(h.edit_root_widget(|wm| wm.widget.open));
    }

    /// Portal-mode item selection routes through
    /// `MenuContentView::message` → `mutate_later` →
    /// [`ThemedDropdownButton::close_for_selection`] — nothing pre-hides the
    /// slot content on that path, so in controlled mode it must NOT
    /// self-close (unlike [`ThemedDropdownButton::mark_closed`]'s genuine
    /// externally-dismissed case); it only reports the `OpenChanged(false)`
    /// desire and leaves the menu visible until the host applies the prop.
    #[test]
    fn portal_selection_close_respects_controlled_mode() {
        use masonry::core::NewWidget;
        use masonry::layout::AsUnit;
        use masonry::testing::TestHarness;
        use masonry::theme::default_property_set;

        // Navigate scope content → Align → SizedBox → dropdown and run `f`.
        fn with_dropdown<R>(
            h: &mut TestHarness<OverlayScope>,
            f: impl FnOnce(&mut WidgetMut<'_, ThemedDropdownButton>) -> R,
        ) -> R {
            h.edit_root_widget(|mut wm| {
                let mut content = OverlayScope::content_mut(&mut wm);
                let mut align = content.downcast::<masonry::widgets::Align>();
                let mut sized = masonry::widgets::Align::child_mut(&mut align);
                let mut sized = sized.downcast::<masonry::widgets::SizedBox>();
                let mut dropdown = masonry::widgets::SizedBox::child_mut(&mut sized)
                    .expect("sized box has the dropdown child");
                let mut dropdown = dropdown.downcast::<ThemedDropdownButton>();
                f(&mut dropdown)
            })
        }

        let theme = Theme::default();
        let scope_handle = OverlayScopeHandle::new();
        let key = 7;

        let dropdown = ThemedDropdownButton::new_portal(
            DropdownButtonConfig {
                label_text: "Menu".into(),
                icon: None,
                items: vec!["A".into(), "B".into()],
                variant: ButtonVariant::Default,
                disabled: false,
                theme,
            },
            DropdownButtonHandle::new(),
            scope_handle.clone(),
            key,
        )
        .with_open_state(false, true);
        let sized = masonry::widgets::SizedBox::new(NewWidget::new(dropdown).erased())
            .width(100.0.px())
            .height(40.0.px())
            .prepare();
        let content =
            masonry::widgets::Align::new(masonry::layout::UnitPoint::TOP_LEFT, sized.erased())
                .prepare()
                .erased();
        let menu = dummy_menu_content(&theme, 2);
        let scope = OverlayScope::new(
            scope_handle,
            content,
            vec![(
                key,
                menu,
                crate::overlay_portal::PortalPlacement::BareTrigger,
            )],
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(scope));

        // The host applies `.open(true)` (controlled): menu becomes visible.
        with_dropdown(&mut h, |d| ThemedDropdownButton::set_open(d, true));
        h.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(
                slot.widget.placed_rect(key).is_some(),
                "slot child must be visible after the host applies open"
            );
        });

        // An ordinary portal item selection lands here via `mutate_later`.
        with_dropdown(&mut h, |d| {
            ThemedDropdownButton::close_for_selection(d);
        });

        assert!(
            with_dropdown(&mut h, |d| d.widget.open),
            "controlled dropdown must not self-close on portal item selection"
        );
        h.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(
                slot.widget.placed_rect(key).is_some(),
                "slot child must stay visible while the host keeps open=true"
            );
        });
        // The action queue also now carries `VirtualScrollAction::Fetch`
        // requests from the real, virtualized menu content `dummy_menu_content`
        // builds (unlike the pre-rewrite plain-label `MenuContent`) — filter
        // via `pop_action_erased`'s `downcast` (which returns the value back
        // on a type mismatch instead of panicking, unlike `pop_action::<T>`)
        // rather than assuming every queued action is a `DropdownButtonAction`.
        let open_changed = std::iter::from_fn(|| h.pop_action_erased())
            .filter_map(|(action, _)| action.downcast::<DropdownButtonAction>().ok())
            .filter_map(|action| match *action {
                DropdownButtonAction::OpenChanged(v) => Some(v),
                DropdownButtonAction::ItemSelected(_) => None,
            })
            .last();
        assert_eq!(
            open_changed,
            Some(false),
            "selection must still report the OpenChanged(false) desire"
        );
    }

    /// Drains `VirtualScrollAction::Fetch` requests from `harness`'s action
    /// queue, materializing real `OverlayListItem` rows for `labels`, the
    /// same way `super::view::build_menu_view`'s `overlay_list(...)` call
    /// would. Mirrors `crate::collection::imperative_list`'s own
    /// `drive_to_fixpoint`, reached one level deeper through
    /// `AnchoredOverlay` -> `MenuContent<CollectionListWidget>`.
    fn drive_in_tree_menu_to_fixpoint(
        h: &mut masonry::testing::TestHarness<ThemedDropdownButton>,
        labels: &[&str],
        on_activated: Option<&crate::collection::OnActivated>,
    ) {
        use xilem::masonry::widgets::{VirtualScroll as VirtualScrollWidget, VirtualScrollAction};

        let mut iteration = 0;
        loop {
            iteration += 1;
            assert!(iteration <= 1000, "Took too long to reach fixpoint");
            // Unlike `crate::collection::imperative_list`'s own
            // `drive_to_fixpoint` (which drives an isolated
            // `CollectionListWidget` harness whose action queue only ever
            // holds `VirtualScrollAction`s), this drives a full
            // `ThemedDropdownButton`, whose queue may also carry
            // `DropdownButtonAction` (e.g. the `OpenChanged(true)` from the
            // trigger click that opened the menu before this runs) — filter
            // via `pop_action_erased`'s `downcast` instead of assuming every
            // queued action is a `VirtualScrollAction`.
            let Some((action, _id)) =
                std::iter::from_fn(|| h.pop_action_erased()).find_map(|(action, id)| {
                    action
                        .downcast::<VirtualScrollAction>()
                        .ok()
                        .map(|action| (*action, id))
                })
            else {
                break;
            };
            let VirtualScrollAction::Fetch(action) = action else {
                continue;
            };
            h.edit_root_widget(|mut root| {
                ThemedDropdownButton::with_overlay_content(&mut root, |mut content| {
                    let mut menu = content.downcast::<MenuContent<CollectionListWidget>>();
                    let mut list = MenuContent::child_mut(&mut menu);
                    {
                        let mut vs = CollectionListWidget::virtual_scroll_mut(&mut list);
                        VirtualScrollWidget::will_handle_action(&mut vs, &action);
                        for idx in action.old_active().clone() {
                            if !action.target().contains(&idx) {
                                VirtualScrollWidget::remove_child(&mut vs, idx);
                            }
                        }
                        for idx in action.target().clone() {
                            if !action.old_active().contains(&idx) {
                                let row = crate::collection::render_overlay_list_item(
                                    &ArcStr::from(labels[idx]),
                                    false,
                                    &Theme::default(),
                                    Role::MenuItem,
                                    on_activated.cloned(),
                                );
                                VirtualScrollWidget::add_child(&mut vs, idx, row);
                            }
                        }
                    }
                    CollectionListWidget::set_active_start(&mut list, action.target().start);
                });
            });
        }
    }

    /// Clicking a materialized row runs its `on_activated` hook — the same
    /// mechanism `super::view::build_menu_view` wires in production — which
    /// closes the menu via `Self::close_for_selection`.
    ///
    /// Checked directly against a real click (rather than assumed): masonry's
    /// own default pointer-down handling clears focus from the trigger the
    /// moment a row is clicked, since `OverlayListItem` (like the old,
    /// pre-rewrite plain-label menu rows before it) never calls
    /// `ctx.request_focus()` and isn't in the trigger's ancestor chain — same
    /// mechanism as autocomplete's own
    /// `clicking_a_non_focusable_sibling_closes_the_in_tree_dropdown` test.
    /// Unlike autocomplete, though, `ThemedDropdownButton` has no
    /// `Update::ChildFocusChanged(true)` handler at all (no
    /// focus-enters-subtree auto-reopen logic, unlike autocomplete's
    /// `open_on_focus`), so that focus-clearing has nothing to trigger a
    /// reopen from — there is no `suppress_focus_open`-style pass-ordering
    /// hazard for a `dropdown_button` counterpart to forget. This test proves
    /// that directly: the menu stays closed, and only a single `Closed`
    /// transition is reported, not a close immediately followed by a
    /// spurious reopen.
    #[test]
    fn click_selection_via_on_activated_closes_the_menu_without_touching_focus() {
        use masonry::core::{NewWidget, PointerButton};
        use masonry::kurbo::Point;
        use masonry::testing::TestHarness;
        use masonry::theme::default_property_set;

        let theme = Theme::default();
        let handle = DropdownButtonHandle::new();
        let on_activated: crate::collection::OnActivated = {
            let handle = handle.clone();
            std::sync::Arc::new(move |ctx: &mut EventCtx<'_>| {
                if let Some(id) = handle.widget_id() {
                    ctx.mutate_later(id, |mut w| {
                        let mut dropdown = w.downcast::<ThemedDropdownButton>();
                        ThemedDropdownButton::close_for_selection(&mut dropdown);
                    });
                }
            })
        };

        let menu = dummy_menu_content(&theme, 2);
        let widget = ThemedDropdownButton::new(
            DropdownButtonConfig {
                label_text: "Menu".into(),
                icon: None,
                items: vec!["A".into(), "B".into()],
                variant: ButtonVariant::Default,
                disabled: false,
                theme,
            },
            menu,
            handle,
        )
        .with_open_state(false, false);

        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (300, 300),
        );

        // Open via a real click on the trigger — mirrors
        // `controlled_mode_reports_open_changed_without_self_toggling`.
        h.mouse_move(Point::new(10.0, 10.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(
            h.edit_root_widget(|wm| wm.widget.open),
            "an uncontrolled dropdown should self-open on trigger click"
        );

        drive_in_tree_menu_to_fixpoint(&mut h, &["A", "B"], Some(&on_activated));

        let mut first_item_id = None;
        h.inspect_widgets(|w| {
            if first_item_id.is_none() && w.accessibility_role() == Role::MenuItem {
                first_item_id = Some(w.id());
            }
        });
        let item_id = first_item_id.expect("menu should have rendered items");

        h.mouse_move_to_unchecked(item_id);
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));

        assert!(
            !h.edit_root_widget(|wm| wm.widget.open),
            "click selection should close the menu via on_activated -> close_for_selection"
        );

        // No spurious reopen: the only `OpenChanged` transitions reported
        // after the row click are close ones (ideally exactly one `false`),
        // never a `true` sneaking back in behind it — which is what a
        // pass-ordering hazard analogous to autocomplete's would produce.
        let open_changes: Vec<bool> = std::iter::from_fn(|| h.pop_action_erased())
            .filter_map(|(action, _)| action.downcast::<DropdownButtonAction>().ok())
            .filter_map(|action| match *action {
                DropdownButtonAction::OpenChanged(v) => Some(v),
                DropdownButtonAction::ItemSelected(_) => None,
            })
            .collect();
        assert_eq!(
            open_changes,
            vec![false],
            "click selection should report exactly one close, no spurious reopen"
        );
    }

    /// Regression test for the keyboard-trap review finding: with
    /// `CollectionListWidget::accepts_focus` unconditionally `true`,
    /// `ThemedDropdownButton` (which has no `Tab` arm in `on_text_event`,
    /// unlike autocomplete's `SuggestionList`) let masonry's default
    /// unhandled-Tab focus search land on the virtualized menu itself —
    /// silently switching from the documented roving-highlight-on-trigger
    /// model to the list's own independent keyboard-nav state. In-tree
    /// variant: `AnchoredOverlay` orders its children `[trigger, overlay]`,
    /// so an unhandled Tab's preorder walk from the focused trigger reaches
    /// the menu next.
    #[test]
    fn tab_does_not_move_focus_into_the_menu_in_tree() {
        use masonry::core::{NewWidget, PointerButton};
        use masonry::kurbo::Point;
        use masonry::testing::TestHarness;
        use masonry::theme::default_property_set;

        let theme = Theme::default();
        let menu = dummy_menu_content(&theme, 3);
        let widget = ThemedDropdownButton::new(
            DropdownButtonConfig {
                label_text: "Menu".into(),
                icon: None,
                items: vec!["A".into(), "B".into(), "C".into()],
                variant: ButtonVariant::Default,
                disabled: false,
                theme,
            },
            menu,
            DropdownButtonHandle::new(),
        )
        .with_open_state(false, false);

        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (300, 300),
        );

        // A real click opens the menu and focuses the trigger (masonry's
        // default click-to-focus), mirroring
        // `click_selection_via_on_activated_closes_the_menu_without_touching_focus`.
        h.mouse_move(Point::new(10.0, 10.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(
            h.edit_root_widget(|wm| wm.widget.open),
            "menu should be open after the trigger click"
        );

        drive_in_tree_menu_to_fixpoint(&mut h, &["A", "B", "C"], None);

        let trigger_focus = h.focused_widget_id();
        assert!(
            trigger_focus.is_some(),
            "clicking the trigger should have focused it"
        );

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));

        let focus_after_tab = h.focused_widget_id();
        let role_after_tab = focus_after_tab
            .and_then(|id| h.access_node(id))
            .map(|n| n.role());
        assert_ne!(
            role_after_tab,
            Some(Role::Menu),
            "Tab must not move focus into the virtualized menu — \
             CollectionListWidget::accepts_focus must be false for dropdown_button"
        );
        assert_eq!(
            focus_after_tab, trigger_focus,
            "with no other focusable widget in the tree, an unhandled Tab \
             (CollectionListWidget no longer being a stop) should leave \
             focus exactly where it was, on the trigger"
        );

        // Escape must still close the menu — proves this doesn't regress
        // into depending on the (now-removed) Tab-into-list focus path.
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
        assert!(
            !h.edit_root_widget(|wm| wm.widget.open),
            "Escape should still close the menu"
        );
    }

    /// Portal-mode counterpart of `tab_does_not_move_focus_into_the_menu_in_tree`
    /// — the review specifically flagged portal mode as the worse case:
    /// `CollectionListWidget` is mounted under `OverlayScope`'s `PortalSlot`,
    /// not under `ThemedDropdownButton` at all, so once focus lands inside it
    /// there, Escape never bubbles back to `ThemedDropdownButton::on_text_event`
    /// (which is where menu-close-on-Escape lives) and portal mode has no
    /// focus-driven auto-close either — a genuine keyboard trap. Masonry's
    /// unhandled-Tab search walks the *entire* window tree in preorder from
    /// the focus anchor (confirmed against `find_next_focusable` in
    /// `masonry_core::passes::update`), which is how it can reach the portal
    /// slot's content even though it isn't a `ThemedDropdownButton` descendant.
    #[test]
    fn tab_does_not_move_focus_into_the_menu_in_portal_mode() {
        use masonry::core::NewWidget;
        use masonry::layout::AsUnit;
        use masonry::testing::TestHarness;
        use masonry::theme::default_property_set;

        fn with_dropdown<R>(
            h: &mut TestHarness<OverlayScope>,
            f: impl FnOnce(&mut WidgetMut<'_, ThemedDropdownButton>) -> R,
        ) -> R {
            h.edit_root_widget(|mut wm| {
                let mut content = OverlayScope::content_mut(&mut wm);
                let mut align = content.downcast::<masonry::widgets::Align>();
                let mut sized = masonry::widgets::Align::child_mut(&mut align);
                let mut sized = sized.downcast::<masonry::widgets::SizedBox>();
                let mut dropdown = masonry::widgets::SizedBox::child_mut(&mut sized)
                    .expect("sized box has the dropdown child");
                let mut dropdown = dropdown.downcast::<ThemedDropdownButton>();
                f(&mut dropdown)
            })
        }

        let theme = Theme::default();
        let scope_handle = OverlayScopeHandle::new();
        let key = 11;

        let dropdown = ThemedDropdownButton::new_portal(
            DropdownButtonConfig {
                label_text: "Menu".into(),
                icon: None,
                items: vec!["A".into(), "B".into(), "C".into()],
                variant: ButtonVariant::Default,
                disabled: false,
                theme,
            },
            DropdownButtonHandle::new(),
            scope_handle.clone(),
            key,
        )
        .with_open_state(false, false);
        let sized = masonry::widgets::SizedBox::new(NewWidget::new(dropdown).erased())
            .width(100.0.px())
            .height(40.0.px())
            .prepare();
        let content =
            masonry::widgets::Align::new(masonry::layout::UnitPoint::TOP_LEFT, sized.erased())
                .prepare()
                .erased();
        let menu = dummy_menu_content(&theme, 3);
        let scope = OverlayScope::new(
            scope_handle,
            content,
            vec![(
                key,
                menu,
                crate::overlay_portal::PortalPlacement::BareTrigger,
            )],
        );
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(scope),
            (300, 300),
        );

        // `trigger` is a `WidgetPod<dyn Widget>` in portal mode (see
        // `Hosting::Portal`), so its id is available without needing a
        // `WidgetMut`/downcast at all — simpler than `with_trigger!` here
        // since this test only needs the id, never mutates the trigger.
        let trigger_id = with_dropdown(&mut h, |d| match &d.widget.hosting {
            Hosting::Portal { trigger, .. } => trigger.id(),
            Hosting::InTree { .. } => unreachable!("this fixture always uses Hosting::Portal"),
        });
        h.focus_on(Some(trigger_id));

        with_dropdown(&mut h, |d| ThemedDropdownButton::set_open(d, true));
        h.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(
                slot.widget.placed_rect(key).is_some(),
                "menu should be visible once open"
            );
        });

        assert_eq!(
            h.focused_widget_id(),
            Some(trigger_id),
            "sanity: focus starts on the trigger"
        );

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));

        let focus_after_tab = h.focused_widget_id();
        let role_after_tab = focus_after_tab
            .and_then(|id| h.access_node(id))
            .map(|n| n.role());
        assert_ne!(
            role_after_tab,
            Some(Role::Menu),
            "Tab must not move focus into the portal-mounted virtualized menu — \
             CollectionListWidget::accepts_focus must be false for dropdown_button"
        );

        // The actual keyboard trap the review flagged: Escape must still
        // close the menu even though we never Tabbed into the list, and even
        // though the list lives outside ThemedDropdownButton's own subtree
        // in portal mode.
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
        assert!(
            !with_dropdown(&mut h, |d| d.widget.open),
            "Escape should still close the menu in portal mode"
        );
    }
}
