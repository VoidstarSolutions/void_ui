//! Masonry widgets for the autocomplete component.
//!
//! Two widgets live here:
//!
//! - [`SuggestionList`] — pure chrome (rounded-rect background/border,
//!   capped-height sizing) wrapping whatever `overlay_list(...)` (Task 5,
//!   `crate::collection`) builds — a virtualized, keyboard-navigable listbox.
//!   Hover/highlight painting, click selection, and keyboard highlight
//!   movement all live in that shared substrate now
//!   (`CollectionListWidget`/`OverlayListItem`); `SuggestionList` only still
//!   owns Enter/Escape/Tab (refocus the text input + close the dropdown —
//!   see its `on_text_event`).
//! - [`AutocompleteWidget`] — composite host: when inside an [`crate::overlay_scope`]
//!   its input chrome is a standalone child and the suggestion list lives in the
//!   scope's always-on-top portal slot; otherwise falls back to an [`AnchoredOverlay`]
//!   exactly as before. Intercepts `TextAction` and `InputCleared` actions from
//!   its descendants and re-emits the single public
//!   [`AutocompleteAction::TextChanged`] that the view layer consumes.
//!
//! Filtering (`compute_filtered`) and item content live entirely in
//! `super::view` now — `overlay_list`'s `virtual_scroll` child needs real,
//! rebuild-diffed item content, not an imperative push, so `AutocompleteView`
//! computes the filtered slice at view-build/rebuild time and this widget
//! only keeps the small summary it still needs synchronously (see
//! [`AutocompleteWidget::first_suggestion`]).

use masonry::accesskit::{HasPopup, Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ActionCtx, ArcStr, ChildrenIds, ComposeCtx, ErasedAction, EventCtx, LayoutCtx,
    MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::{ClipRef, Painter};
use masonry::kurbo::{Axis, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenDef, LenReq, Length};
use masonry::properties::{
    Background, BorderColor, BorderWidth, CaretColor, ContentColor, CornerRadius, Padding,
    PlaceholderColor, SelectionColor,
};
use masonry::widgets::{self, SizedBox, TextAction};

use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::components::input::widget::{InputCleared, InputFrame};
use crate::components::input::{body_line_height, stripped_text_input_props, text_area_props};
use crate::overlay::OverlayAnchor;
use crate::overlay::binding::{self, PortalBinding};
use crate::overlay_scope::OverlayScopeHandle;

/// Suggestion list border stroke width — hairline chrome, not density-scaled.
const LIST_BORDER: f64 = 1.0;
/// Maximum visible height for the suggestion list before it scrolls, px — a clamp, not a density-scaled dimension.
///
/// `pub(crate)` (not private): `dropdown_button::menu_layer::MenuContent`
/// reuses the same cap for its own vertical `measure()`, mirroring this
/// module's `SuggestionList` — re-exported from `super::MAX_LIST_HEIGHT`.
pub(crate) const MAX_LIST_HEIGHT: f64 = 200.0;
/// Gap between the input field and the suggestion list overlay, px — a fixed
/// anchor offset, not a density-scaled spacing token.
const OVERLAY_GAP_PX: f64 = 2.0;
/// Gap as a [`Length`] (used by the in-tree `AnchoredOverlay`).
const OVERLAY_GAP: Length = Length::const_px(OVERLAY_GAP_PX);

// ─────────────────────────────────────────────────────────────────────────────
// AutocompleteAction
// ─────────────────────────────────────────────────────────────────────────────

/// Action emitted by `AutocompleteWidget` to its view layer. Carries the
/// new text string (either typed or selected from the list).
#[derive(Debug)]
pub enum AutocompleteAction {
    TextChanged(String),
}

widget_id_handle!(
    /// Self-filling handle to an [`AutocompleteWidget`]'s widget id, filled at
    /// `Update::WidgetAdded`. Given to [`SuggestionList`] so Enter/Escape/Tab
    /// can `mutate_later` back into the widget to close the suggestion list.
    AutocompleteHandle
);

/// Self-filling, *repointable* handle to the listbox's widget id (the
/// `overlay_list(...)`-built `CollectionListWidget`, `Role::ListBox`), reset
/// by [`SuggestionList`] at every `Update::WidgetAdded` from its own
/// (already-known) child id — see `SuggestionList::update`. Lets
/// [`AutocompleteWidget`] expose `aria-controls` pointing at the listbox and
/// move real focus into it on forward-Tab, regardless of hosting mode (in
/// portal mode there's no tree path from `AutocompleteWidget` to the listbox
/// at all).
///
/// Deliberately *not* built via the `widget_id_handle!` macro
/// ([`AutocompleteHandle`]/[`TextAreaHandle`] are): that macro's `OnceLock`
/// backing is correct for those two, since their target widgets live for the
/// whole `AutocompleteWidget` lifetime and are never torn down — "latch on
/// the first `Update::WidgetAdded`" is equivalent to "always current" there.
/// The listbox itself *is* torn down and rebuilt with a fresh `WidgetId`:
/// `AutocompleteView::rebuild`'s portal branch deregisters the old
/// `SuggestionListView` portal entry and registers a new one on every
/// closed→open transition (`just_opened` — see
/// [`AutocompleteWidget::set_match_summary`]'s doc comment for why), reusing
/// this same handle across both mounts. A `OnceLock` would latch onto the
/// first mount forever, leaving a subsequent `ctx.set_focus(listbox_id)`
/// targeting an already-torn-down widget — reproduced by
/// `listbox_handle_repoints_after_the_portal_list_is_torn_down_and_rebuilt`.
#[derive(Clone, Default)]
pub(crate) struct ListboxHandle(std::sync::Arc<std::sync::Mutex<Option<WidgetId>>>);

impl ListboxHandle {
    pub(crate) fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(None)))
    }

    pub(crate) fn widget_id(&self) -> Option<WidgetId> {
        *self.0.lock().unwrap()
    }

    fn set(&self, id: WidgetId) {
        *self.0.lock().unwrap() = Some(id);
    }
}

widget_id_handle!(
    /// Handle to the autocomplete's editable `TextArea`'s widget id. Unlike
    /// [`AutocompleteHandle`]/[`ListboxHandle`], this is filled *eagerly* at
    /// [`AutocompleteWidget`] construction — `build_chrome` already reads the
    /// id synchronously off the `NewWidget` it builds, no need to wait for
    /// `Update::WidgetAdded`. Lets [`SuggestionList`] call `ctx.set_focus()`
    /// directly to return focus to the input after Enter/Escape/Tab, and lets
    /// a click-selection's `on_activated` hook (`super::view`) do the same
    /// synchronously from `OverlayListItem::on_pointer_event` — see the
    /// module docs for why arrow-key navigation moved from "focus stays in
    /// the textbox" (blocked: `TextArea` unconditionally claims arrow keys
    /// for cursor movement, even on a single line, so an ancestor never sees
    /// them) to "Tab moves focus into the open listbox".
    TextAreaHandle
);

// ─────────────────────────────────────────────────────────────────────────────
// SuggestionList
// ─────────────────────────────────────────────────────────────────────────────

/// Pure chrome (rounded-rect background/border, capped-height sizing)
/// wrapping whatever `overlay_list(...)` (`crate::collection`, Task 5)
/// builds — a virtualized, keyboard-navigable listbox widget
/// (`CollectionListWidget`).
///
/// Generic over the wrapped widget type `W` — mirroring
/// `CollapsibleWidget<W>` (`components/collapsible/widget.rs`), not an
/// erased `WidgetPod<dyn Widget>` — so `super::view::SuggestionListView`
/// (generic over the child *view*) can forward `rebuild`/`teardown`/
/// `message` straight through via `this.ctx.get_mut(&mut this.widget.list)`,
/// with no downcast needed at all. `W` gets erased exactly once, one level
/// up, wherever this widget is actually embedded: `AutocompleteWidget`'s
/// in-tree `AnchoredOverlay` overlay slot and the portal's `Passthrough`
/// wrapper both already take `NewWidget<dyn Widget>` (see
/// `NewWidget::erased`), so genericity here costs nothing at those
/// boundaries. `SuggestionListView`'s own `Element` needing to be a
/// concrete, nameable-outside-the-view type is what rules out erasing
/// *inside* this widget instead — see the design write-up for why: an
/// erased `WidgetPod<dyn Widget>` field can't hand back a concretely-typed
/// `Mut<'_, Pod<W>>` for the wrapped view's own `rebuild`/`message` to use.
///
/// Owns Enter/Escape/Tab: refocuses the text input and closes the dropdown
/// (`on_text_event`) — selection itself, hover, highlight painting, and
/// arrow-key navigation all live in the wrapped `CollectionListWidget`/
/// `OverlayListItem` substrate now.
pub(crate) struct SuggestionList<W: Widget> {
    list: WidgetPod<W>,
    theme: Theme,
    handle: AutocompleteHandle,
    text_area_handle: TextAreaHandle,
    listbox_handle: ListboxHandle,
}

impl<W: Widget> SuggestionList<W> {
    pub(crate) fn new(
        list: NewWidget<W>,
        theme: &Theme,
        handle: AutocompleteHandle,
        text_area_handle: TextAreaHandle,
        listbox_handle: ListboxHandle,
    ) -> Self {
        Self {
            list: list.to_pod(),
            theme: *theme,
            handle,
            text_area_handle,
            listbox_handle,
        }
    }

    /// Returns a mutable reference to the wrapped listbox — lets
    /// `super::view::SuggestionListView` forward `rebuild`/`teardown`/
    /// `message` straight through.
    pub(crate) fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, W> {
        this.ctx.get_mut(&mut this.widget.list)
    }

    /// Returns focus to the input field, if its id is known yet.
    fn refocus_input(&self, ctx: &mut EventCtx<'_>) {
        if let Some(id) = self.text_area_handle.widget_id() {
            ctx.set_focus(id);
        }
    }

    /// Closes the dropdown and suppresses the reopen a subsequent
    /// `refocus_input` would otherwise trigger — see
    /// [`AutocompleteWidget::mark_closed_suppressing_reopen`] for why this
    /// (not the plain, non-suppressing `mark_closed` the outside-press
    /// dismissal path uses) is required here.
    fn request_close(&self, ctx: &mut EventCtx<'_>) {
        if let Some(ac_id) = self.handle.widget_id() {
            ctx.mutate_later(ac_id, move |mut w| {
                let mut ac = w.downcast::<AutocompleteWidget>();
                AutocompleteWidget::mark_closed_suppressing_reopen(&mut ac);
            });
        }
    }
}

// --- MARK: WIDGETMUT SETTERS
impl<W: Widget> SuggestionList<W> {
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        this.ctx.request_paint_only();
    }
}

// --- MARK: IMPL WIDGET
impl<W: Widget> Widget for SuggestionList<W> {
    type Action = NoAction;

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
        // Enter, Escape, or Tab (either direction): return focus to the text
        // input explicitly and close, rather than letting masonry cycle
        // focus starting from the listbox's actual tree position. The
        // listbox may live in the portal slot, mounted elsewhere in the
        // tree, so an unhandled Tab would make masonry search for the next
        // focusable widget from there instead of from the autocomplete's
        // page position, landing on the wrong widget in either direction.
        // This lands back on the text field instead; a second real
        // Tab/Shift+Tab from there cycles correctly since the input's tree
        // position does match its page position.
        //
        // Selection itself (Enter on a highlighted row) already happened
        // before this handler runs: `CollectionListWidget::on_text_event`'s
        // own Enter arm (which fires first, since this widget's child is
        // the one that has real focus) submits the highlighted row's
        // selection action and deliberately does not `set_handled()`, so
        // the same keypress bubbles up here afterward.
        if let Key::Named(NamedKey::Enter | NamedKey::Escape | NamedKey::Tab) = &key.key {
            self.refocus_input(ctx);
            self.request_close(ctx);
            ctx.set_handled();
        }
    }

    fn update(&mut self, _ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // The child's `WidgetPod` id is known synchronously (no need to wait
        // for the child's own `Update::WidgetAdded`) — see `TextAreaHandle`'s
        // doc comment for the general pattern.
        if let Update::WidgetAdded = event {
            self.listbox_handle.set(self.list.id());
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.list);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        let natural = ctx
            .compute_length(
                &mut self.list,
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
        ctx.run_layout(&mut self.list, size);
        ctx.place_child(&mut self.list, Point::ORIGIN);
    }

    /// Paints the rounded background/border chrome, then pushes a
    /// same-shaped clip that stays active through `paint()` and every
    /// child's own paint (popped in [`Self::post_paint`]) — the reason this
    /// (and not `paint`) is where the clip is pushed: masonry's
    /// `LayoutCtx::set_clip_path` is the framework-level subtree-clip
    /// mechanism, but it's hardcoded to a plain `Rect`
    /// (`masonry_core::passes::paint`'s paint pass always records it as
    /// `Geometry::Rect`), so it can't express this rounded shape. `pre_paint`
    /// (documented as unconstrained by `set_clip_path`, for exactly this
    /// "paint the shape that defines the clip" case) plus a manual
    /// `Painter::push_clip`/`pop_clip` pair — which *does* accept an
    /// arbitrary `GeometryRef`, including `RoundedRect` directly
    /// (`imaging::GeometryRef::RoundedRect` — confirmed against the pinned
    /// `imaging` crate source) — achieves it instead. Verified against
    /// `masonry_core::passes::paint::paint_widget`'s actual compositing
    /// order that this works: each widget's `pre_paint`/`paint`/`post_paint`
    /// scenes, and every child's full paint (`pre_paint`+`paint`+
    /// `post_paint`, recursively), all append into one shared, per-window
    /// scene collector in that exact sequence — so an unbalanced
    /// `push_clip` left open at the end of `pre_paint` genuinely stays
    /// active across everything appended after it, until the matching
    /// `pop_clip` in `post_paint` (which runs *after* every child has been
    /// painted) closes it. This replaced a per-row "round my own corners if
    /// I'm first/last in the list" approach that was unsound for a scrolled,
    /// `MAX_LIST_HEIGHT`-clamped list: the row actually sitting at the
    /// visual bottom edge there is essentially never the last item in the
    /// full list, so index-based corner rounding rounded the wrong row (or
    /// none at all) once a list scrolled — this clip is a real geometric
    /// mask, so it's correct regardless of scroll position or which rows
    /// happen to be materialized.
    fn pre_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let p = &self.theme.palette;
        let corner = f64::from(self.theme.radius.small);
        let bg_rect = RoundedRect::from_origin_size(Point::ORIGIN, ctx.border_box().size(), corner);
        painter.fill(bg_rect, p.surface_hi).draw();
        painter
            .stroke(bg_rect, &Stroke::new(LIST_BORDER), p.border_strong)
            .draw();
        painter.push_clip(ClipRef::fill(bg_rect));
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    /// Closes the clip [`Self::pre_paint`] opened, now that every child has
    /// been painted (see that method's doc comment for the full mechanism).
    fn post_paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        painter.pop_clip();
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
        ChildrenIds::from_slice(&[self.list.id()])
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

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
    let line_px = body_line_height(theme);
    with_text_input_in_chrome(sb, |ti| {
        ti.insert_prop(PlaceholderColor::new(theme.palette.text_muted));
        // Re-apply the placeholder's font size + line height (the `WidgetAdded`
        // one-shot in `InputFrame` has long since run); mirrors the editor below
        // so the empty field keeps matching after a theme swap.
        {
            let mut placeholder = widgets::TextInput::placeholder_mut(ti);
            widgets::Label::insert_style(
                &mut placeholder,
                StyleProperty::FontSize(theme.typography.size_body),
            );
            widgets::Label::insert_style(
                &mut placeholder,
                StyleProperty::LineHeight(masonry::parley::LineHeight::Absolute(line_px)),
            );
        }
        let mut ta = widgets::TextInput::text_mut(ti);
        ta.insert_prop(ContentColor::new(theme.palette.text));
        ta.insert_prop(CaretColor {
            color: theme.palette.accent,
        });
        ta.insert_prop(SelectionColor {
            color: theme.palette.accent_soft,
        });
        widgets::TextArea::insert_style(
            &mut ta,
            StyleProperty::FontSize(theme.typography.size_body),
        );
        widgets::TextArea::insert_style(
            &mut ta,
            StyleProperty::LineHeight(masonry::parley::LineHeight::Absolute(line_px)),
        );
    });
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
        binding: PortalBinding,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// AutocompleteWidget
// ─────────────────────────────────────────────────────────────────────────────

/// Construction inputs shared by [`AutocompleteWidget::new`] and
/// [`AutocompleteWidget::new_portal`]; bundled to keep their argument counts
/// under clippy's `too_many_arguments` threshold.
pub(crate) struct AutocompleteConfig {
    pub(crate) contents: String,
    pub(crate) placeholder: ArcStr,
    /// The text of the highest-ranked (first) suggestion currently matching
    /// `contents`, or `None` if nothing matches — the one piece of the
    /// view-owned filtered set this widget still needs synchronously (for
    /// `TextAction::Entered`'s "accept the top suggestion" shortcut, and to
    /// know whether focusing/typing should open the dropdown at all). See
    /// [`AutocompleteWidget::first_suggestion`].
    pub(crate) first_suggestion: Option<ArcStr>,
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
/// Both hosting modes build their `SuggestionList` (and the `overlay_list`
/// listbox it wraps) via `super::view::SuggestionListView` now — this widget
/// never constructs one itself, only embeds an already-built element (see
/// [`Self::new`]). Item content and filtering (`compute_filtered`) live
/// entirely in `super::view::AutocompleteView` — a real, rebuild-diffed View
/// prop — not here.
///
/// Intercepts actions from descendants and re-emits
/// [`AutocompleteAction::TextChanged`] for the view layer.
pub struct AutocompleteWidget {
    hosting: Hosting,
    /// See [`AutocompleteConfig::first_suggestion`]. Kept in sync by
    /// [`Self::set_match_summary`], which `super::view::AutocompleteView`
    /// calls whenever `contents`/`suggestions` change (i.e. whenever the
    /// view's own `compute_filtered` result could have changed) —
    /// deliberately not folded into [`Self::set_contents`], since a typed
    /// keystroke round-trips `contents` back unchanged (host-controlled) and
    /// `set_contents` intentionally no-ops on that to avoid disturbing the
    /// text area's cursor, but the matching set still needs to be
    /// re-evaluated every time.
    first_suggestion: Option<ArcStr>,
    /// Mirrors the host-controlled text, kept in sync via [`Self::set_contents`]
    /// and updated eagerly in action handlers for keyboard nav and selection.
    contents: String,
    open: bool,
    /// Suppresses both of the two independent "reopen because focus is back
    /// on the input" paths — [`Self::open_on_focus`] (via
    /// `ChildFocusChanged(true)`) and [`Self::set_match_summary`]'s own
    /// `ctx.has_focus_target()` check — so that a click-based selection
    /// doesn't reopen the dropdown it just closed.
    ///
    /// Click creates a focus gap (`TextArea` → `None` → `TextArea`) that
    /// fires `ChildFocusChanged(true)`, and the *same* click's `on_select`
    /// (routed through `overlay_list`, calling the host's `on_changed`)
    /// typically also feeds `contents`/`suggestions` back as new View props,
    /// triggering `AutocompleteView::rebuild` → `set_match_summary` in the
    /// very same reactive cascade — both would reopen the list on their own.
    /// Set by [`Self::select_suggestion`], by
    /// [`super::view::build_list_view`]'s `on_activated` hook, and by
    /// [`SuggestionList::on_text_event`]'s Enter/Escape/Tab handling (all
    /// three via [`Self::mark_closed_suppressing_reopen`]). **Neither reopen check clears
    /// it** (a previous version had `open_on_focus` clear it on read, which
    /// broke the moment both checks fired in the same cascade: whichever
    /// ran first silently spent the flag, leaving the other unprotected).
    /// Cleared instead by [`Self::handle_text_changed`] (a genuine new
    /// keystroke ends the "just selected, ignore focus echoes" window) and
    /// by `Update::ChildFocusChanged(false)` (a real blur — see that arm's
    /// comment for why an unconditional clear there is safe).
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
    listbox_handle: ListboxHandle,
    /// Resolves to this widget's own id once mounted. Given to
    /// `SuggestionList` so Enter/Escape/Tab can close the dropdown via
    /// [`Self::mark_closed`] regardless of hosting mode (in portal mode
    /// there's no ancestor path back here).
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
        // Pin an absolute line height (matching the `input` component) so the
        // glyph seats on the box's optical midline and descenders clear the clip
        // rect — the font's natural, ascent-heavy metrics push the text up and
        // let `with_clip(true)` shear the descenders off.
        let text_area = widgets::TextArea::new_editable(contents)
            .with_style(StyleProperty::FontSize(theme.typography.size_body))
            .with_style(StyleProperty::LineHeight(
                masonry::parley::LineHeight::Absolute(body_line_height(theme)),
            ));

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
        // Stamp the placeholder's font size + line height before first paint, so
        // the empty field's placeholder matches the editor instead of rendering
        // oversized and low (masonry builds it at its own default size with
        // natural metrics — see `InputFrame::with_placeholder_style`).
        let input_frame = InputFrame::with_placeholder_style(
            text_input_widget,
            theme.typography.size_body,
            body_line_height(theme),
        );

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

    /// In-tree constructor (fallback, no scope ancestor). `suggestion_list`
    /// is the already-built, erased element `super::view::SuggestionListView`
    /// (built by `AutocompleteView::build`) produced — this widget only
    /// embeds it in the `AnchoredOverlay`'s overlay slot.
    #[must_use]
    pub(crate) fn new(
        config: AutocompleteConfig,
        suggestion_list: NewWidget<dyn Widget>,
        handle: AutocompleteHandle,
        listbox_handle: ListboxHandle,
        text_area_handle: &TextAreaHandle,
    ) -> Self {
        let AutocompleteConfig {
            contents,
            placeholder,
            first_suggestion,
            disabled,
            theme,
        } = config;
        let (chrome, text_area_id) = Self::build_chrome(&contents, placeholder, disabled, &theme);
        text_area_handle.set(text_area_id);

        let overlay =
            AnchoredOverlay::new(chrome, suggestion_list, false, OverlayAnchor::BottomStart)
                .with_gap(OVERLAY_GAP);

        Self {
            hosting: Hosting::InTree {
                overlay_host: NewWidget::new(overlay).to_pod(),
            },
            first_suggestion,
            contents,
            open: false,
            suppress_focus_open: false,
            focus_moving_to_listbox: false,
            theme,
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
        listbox_handle: ListboxHandle,
        text_area_handle: &TextAreaHandle,
    ) -> Self {
        let AutocompleteConfig {
            contents,
            placeholder,
            first_suggestion,
            disabled,
            theme,
        } = config;
        let (chrome, text_area_id) = Self::build_chrome(&contents, placeholder, disabled, &theme);
        text_area_handle.set(text_area_id);

        Self {
            hosting: Hosting::Portal {
                chrome: chrome.to_pod(),
                binding: PortalBinding::new(scope, key, autocomplete_dismiss_hook),
            },
            first_suggestion,
            contents,
            open: false,
            suppress_focus_open: false,
            focus_moving_to_listbox: false,
            theme,
            listbox_handle,
            handle,
        }
    }

    /// Navigate to the in-tree `AnchoredOverlay`'s overlay slot content and
    /// invoke `f`. The overlay slot holds erased `NewWidget<dyn Widget>`
    /// content (a `Passthrough` wrapping `SuggestionList`, once
    /// `super::view::AutocompleteView` builds it — see
    /// `super::view::build_list_view`); `f` is expected to `downcast::<
    /// Passthrough>()` before forwarding into the nested `SuggestionListView`
    /// it wraps.
    ///
    /// Used by `AutocompleteView::rebuild`/`teardown`/`message` to forward
    /// into the in-tree nested list view — portal mode never needs this
    /// (its `SuggestionListView` is registered with, and dispatched by, the
    /// scope's own portal registry instead). Panics if called in portal mode.
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
}

// --- MARK: INTERNAL HELPERS
impl AutocompleteWidget {
    /// Opens the dropdown when focus enters the input field. Uses the
    /// already-cached [`Self::first_suggestion`] rather than recomputing
    /// anything — the view keeps it fresh on every rebuild, and nothing
    /// async can have invalidated it since the last one (suggestions
    /// arriving asynchronously while already focused is handled separately,
    /// by [`Self::set_match_summary`]'s own `ctx.has_focus_target()` check,
    /// which reads — but, like this method, does not clear —
    /// [`Self::suppress_focus_open`]; see that field's doc comment for why
    /// clearing it here used to be wrong).
    fn open_on_focus(&mut self, ctx: &mut UpdateCtx<'_>) {
        if self.suppress_focus_open {
            return;
        }
        if self.first_suggestion.is_none() {
            return;
        }
        // Bail before flipping `open` if the portal scope isn't mounted yet:
        // otherwise we'd get stuck open with nothing visible and no AT update,
        // and the `!self.open` guard on ChildFocusChanged would permanently
        // block future opens.
        if let Hosting::Portal { binding, .. } = &self.hosting
            && !binding.is_ready()
        {
            return;
        }
        self.open = true;
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.mutate_child_later(overlay_host, |mut w| {
                    AnchoredOverlay::set_overlay_visible(&mut w, true);
                });
            }
            Hosting::Portal { binding, .. } => {
                binding.open(ctx, OverlayAnchor::BottomStart, OVERLAY_GAP_PX);
            }
        }
        ctx.request_paint_only();
        ctx.request_accessibility_update();
    }

    fn close_overlay_later(&mut self, ctx: &mut ActionCtx<'_>) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.mutate_child_later(overlay_host, |mut w| {
                    AnchoredOverlay::set_overlay_visible(&mut w, false);
                });
            }
            Hosting::Portal { binding, .. } => binding.close(ctx),
        }
    }

    /// Reacts to a typed keystroke. Does *not* decide open/closed itself —
    /// filtering moved to the view layer (`AutocompleteView::rebuild`,
    /// `compute_filtered`), so submitting `TextChanged` here and letting the
    /// resulting host round-trip (`on_changed` → new `contents` prop →
    /// `AutocompleteView::rebuild` → [`Self::set_match_summary`]) settle the
    /// open/closed state is enough: xilem processes an action and rebuilds
    /// the tree synchronously, before the next paint, so there's no visible
    /// staleness.
    fn handle_text_changed(&mut self, ctx: &mut ActionCtx<'_>, text: &str) {
        self.contents.clear();
        self.contents.push_str(text);
        // A genuine new keystroke ends any "just click-selected, ignore
        // focus echoes" window a prior selection may have opened — see
        // `suppress_focus_open`'s doc comment. This only ever fires for a
        // real edit bubbling up from the child `TextArea`'s own change
        // notification, never for a host-driven `TextArea::reset_text`
        // (`set_contents`/`select_suggestion` call that directly, bypassing
        // this action entirely) — so it can't be the click-selection's own
        // echo re-arming itself.
        self.suppress_focus_open = false;
        ctx.submit_action::<AutocompleteAction>(AutocompleteAction::TextChanged(text.to_owned()));
        ctx.set_handled();
    }

    /// Accepts a suggestion chosen while the *text field itself* has focus
    /// (`TextAction::Entered`'s "accept the top suggestion" shortcut) — the
    /// one remaining widget-level selection path, since focus never left the
    /// input there's nothing to refocus. Selections made from inside the
    /// open listbox (click, or Enter once Tab'd in) resolve through
    /// `overlay_list`'s `on_select` instead (`super::view`), which calls the
    /// host's `on_changed` directly — this method is not involved for that
    /// path at all.
    fn select_suggestion(&mut self, ctx: &mut ActionCtx<'_>, selected: String) {
        let text = selected.clone();
        self.contents.clone_from(&selected);
        self.open = false;
        self.suppress_focus_open = true;

        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.mutate_child_later(overlay_host, move |mut w| {
                    AnchoredOverlay::set_overlay_visible(&mut w, false);
                    with_text_area(&mut w, |ta| widgets::TextArea::reset_text(ta, &text));
                });
            }
            Hosting::Portal {
                chrome, binding, ..
            } => {
                let text_for_area = text.clone();
                ctx.mutate_child_later(chrome, move |mut w| {
                    let mut sb = w.downcast::<SizedBox>();
                    with_text_area_in_chrome(&mut sb, |ta| {
                        widgets::TextArea::reset_text(ta, &text_for_area);
                    });
                });
                binding.close(ctx);
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
    /// Push the current `open` state to the Portal-mode slot. No-op in-tree or
    /// while the scope is unmounted. Shared by every `WidgetMut`-based setter
    /// that can flip the dropdown open or closed as a side effect (typing and
    /// focus events go through `binding.open`/`close` inline instead, from
    /// their own `ActionCtx`/`UpdateCtx`).
    fn sync_portal_visibility(this: &mut WidgetMut<'_, Self>) {
        let should_open = this.widget.open;
        if let Hosting::Portal { binding, .. } = &mut this.widget.hosting {
            if should_open {
                binding.open(&mut this.ctx, OverlayAnchor::BottomStart, OVERLAY_GAP_PX);
            } else {
                binding.close(&mut this.ctx);
            }
        }
    }

    /// Repoints the portal binding at a freshly-registered entry — called by
    /// `AutocompleteView::rebuild` right after it deregisters the old
    /// suggestion-list entry and registers a new one on reopen (see that
    /// call site's doc comment). No-op in-tree.
    ///
    /// `set_match_summary` (called earlier in the same `rebuild`, before the
    /// view knows a key swap is needed) already pushed visibility for the
    /// *old* key via `sync_portal_visibility` — a harmless no-op once that
    /// entry is deregistered, since `PortalSlot::set_visible` silently
    /// ignores an unknown key. This re-pushes visibility for the *new* key
    /// (`PortalBinding::set_key` alone doesn't), so the reopened dropdown
    /// actually shows instead of silently pointing at a dead entry.
    pub(crate) fn set_portal_key(this: &mut WidgetMut<'_, Self>, key: u64) {
        let open = this.widget.open;
        if let Hosting::Portal { binding, .. } = &mut this.widget.hosting {
            binding.set_key(key);
            if open {
                binding.open(&mut this.ctx, OverlayAnchor::BottomStart, OVERLAY_GAP_PX);
            }
        }
    }

    /// Update the displayed text. Called from the view layer on rebuild when
    /// the host's `contents` value changes. Does *not* touch open/closed
    /// state or `first_suggestion` — see [`Self::set_match_summary`], called
    /// separately (and unconditionally, since this early-returns on an
    /// unchanged value to avoid disturbing the text area's cursor mid-type,
    /// which a typed keystroke's round-trip always is).
    pub(crate) fn set_contents(this: &mut WidgetMut<'_, Self>, contents: &str) {
        if this.widget.contents == contents {
            return;
        }
        this.widget.contents.clear();
        this.widget.contents.push_str(contents);

        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut h = this.ctx.get_mut(overlay_host);
                with_text_area(&mut h, |ta| {
                    let current = ta.widget.text().to_string();
                    if current != contents {
                        widgets::TextArea::reset_text(ta, contents);
                    }
                });
            }
            Hosting::Portal { chrome, .. } => {
                let mut c = this.ctx.get_mut(chrome);
                let mut sb = c.downcast::<SizedBox>();
                with_text_area_in_chrome(&mut sb, |ta| {
                    let current = ta.widget.text().to_string();
                    if current != contents {
                        widgets::TextArea::reset_text(ta, contents);
                    }
                });
            }
        }
    }

    /// Pushes the view-computed match summary — see
    /// [`AutocompleteConfig::first_suggestion`] — and reconciles open/closed
    /// state against it. Called by `AutocompleteView::rebuild` whenever
    /// `contents` or `suggestions` changes, unconditionally (not gated on
    /// `contents` equality the way [`Self::set_contents`] is): a typed
    /// keystroke round-trips `contents` back unchanged (host-controlled), but
    /// the matching set still needs to be re-evaluated on every such
    /// round-trip.
    ///
    /// Returns whether the dropdown just transitioned closed → open — see
    /// `AutocompleteView::rebuild`'s use of this to force a fresh suggestion
    /// list mount on reopen, rather than diffing against the one left over
    /// from before close: while closed, a `VirtualScroll`-backed list's own
    /// subtree is stashed and so never gets a layout pass, which is the only
    /// place it reconciles a shrunk item count (a `VirtualScrollAction::Fetch`
    /// round-trip removing now-excess rows). A row a *prior* shrink (e.g. a
    /// selection collapsing the suggestions down to the one exact match)
    /// left materialized-but-stashed can therefore still be sitting in the
    /// list's child map, unremoved, when the dropdown reopens — and masonry's
    /// `VirtualScroll::layout` unconditionally lays out every child it still
    /// holds, panicking ("trying to compute layout of stashed widget") on
    /// that leftover row before its removal ever gets a chance to run. Live,
    /// reproduced crash: select a suggestion (shrinks + closes), then press
    /// Backspace (a genuine keystroke, so `handle_text_changed` correctly
    /// clears `suppress_focus_open` and this legitimately reopens with a
    /// regrown list) — the reopen's own layout pass is the first to touch the
    /// still-stashed leftover row from the selection's shrink.
    pub(crate) fn set_match_summary(this: &mut WidgetMut<'_, Self>, first: Option<ArcStr>) -> bool {
        this.widget.first_suggestion = first;
        let has_matches = this.widget.first_suggestion.is_some();
        // Also open (not just stay open) when the field already has focus:
        // an external content/suggestions change (e.g. host-driven autofill,
        // restoring a draft into a controlled field, or suggestions arriving
        // asynchronously after an initially-empty field was already focused)
        // should surface matching suggestions the same way a typed keystroke
        // does, rather than only ever narrowing/closing. Gated on
        // `!suppress_focus_open` for the same reason `open_on_focus` gates
        // on it — see that field's doc comment: a click-selection's own
        // `on_changed` round-trip lands here (via `AutocompleteView::rebuild`
        // → `set_match_summary`) in the very same cascade as the click, and
        // without this guard it would reopen the dropdown
        // `mark_closed_suppressing_reopen` just closed, focus having already
        // been restored to the field by the click's `on_activated` hook.
        let focus_wants_open = this.ctx.has_focus_target() && !this.widget.suppress_focus_open;
        let should_open = (this.widget.open || focus_wants_open) && has_matches;
        let open_changed = this.widget.open != should_open;
        this.widget.open = should_open;

        if open_changed {
            let is_portal = matches!(this.widget.hosting, Hosting::Portal { .. });
            if is_portal {
                Self::sync_portal_visibility(this);
            } else if let Hosting::InTree { overlay_host } = &mut this.widget.hosting {
                let mut h = this.ctx.get_mut(overlay_host);
                AnchoredOverlay::set_overlay_visible(&mut h, should_open);
            }
            this.ctx.request_accessibility_update();
        }

        open_changed && should_open
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
            match &mut this.widget.hosting {
                Hosting::InTree { overlay_host } => {
                    let mut h = this.ctx.get_mut(overlay_host);
                    AnchoredOverlay::set_overlay_visible(&mut h, false);
                }
                Hosting::Portal { binding, .. } => binding.close(&mut this.ctx),
            }
            this.ctx.request_accessibility_update();
        }
    }

    /// Re-apply theme colors to the input chrome. The suggestion list's own
    /// theme flows through `super::view::SuggestionListView`'s `rebuild`
    /// instead (both hosting modes now build/rebuild it as a real, nested
    /// View — see `AutocompleteView`), so this widget doesn't need to reach
    /// into it directly at all.
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;

        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut h = this.ctx.get_mut(overlay_host);
                let mut primary = AnchoredOverlay::primary_mut(&mut h);
                let mut sb = primary.downcast::<SizedBox>();
                apply_chrome_theme(&mut sb, theme);
            }
            Hosting::Portal { chrome, .. } => {
                let mut c = this.ctx.get_mut(chrome);
                let mut sb = c.downcast::<SizedBox>();
                apply_chrome_theme(&mut sb, theme);
            }
        }

        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    /// Notify that the dropdown should close — the slot/overlay may already
    /// be hidden (outside-press dismissal, via [`autocomplete_dismiss_hook`])
    /// or this may be the trigger doing the hiding (called through
    /// [`Self::mark_closed_suppressing_reopen`] by `SuggestionList`'s
    /// Enter/Escape/Tab handling or a click selection's `on_activated` hook
    /// in `super::view`); idempotent either way.
    pub(crate) fn mark_closed(this: &mut WidgetMut<'_, Self>) {
        if !this.widget.open {
            return;
        }
        this.widget.open = false;

        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut w = this.ctx.get_mut(overlay_host);
                AnchoredOverlay::set_overlay_visible(&mut w, false);
            }
            Hosting::Portal { binding, .. } => binding.close(&mut this.ctx),
        }
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }

    /// Closes the dropdown and suppresses the focus-driven reopen that would
    /// otherwise immediately follow a refocus back onto the text input.
    ///
    /// Two call sites need this, for two different pass-ordering reasons:
    ///
    /// - A click-selection's synchronous refocus (the `on_activated` hook
    ///   `super::view::build_list_view` wires into `overlay_list`, run from
    ///   `OverlayListItem::on_pointer_event`): masonry's *pointer*-event pass
    ///   ordering runs `mutate_later` callbacks (this one included) before
    ///   the focus update pass resolves the `ctx.set_focus(text_area_id)`
    ///   call `on_activated` already made — confirmed by driving a real
    ///   click through `TestHarness` and observing `mark_closed` run, then
    ///   `Update::ChildFocusChanged(true)` fire immediately after, both
    ///   within the same `mouse_button_release`. Without suppressing it,
    ///   that focus-in sees `open == false` (this method having just set
    ///   it) and immediately reopens the dropdown via
    ///   [`Self::open_on_focus`]'s own `!self.open` guard.
    /// - [`SuggestionList::on_text_event`]'s Enter/Escape/Tab handling: in
    ///   portal mode the listbox lives outside `AutocompleteWidget`'s own
    ///   subtree, so `refocus_input`'s `ctx.set_focus(text_area_id)` is a
    ///   real focus transition and does fire `Update::ChildFocusChanged(true)`
    ///   on this widget — after `mark_closed` already ran (text-event pass
    ///   ordering resolves focus *after* event dispatch, so this is not the
    ///   same race as the click case, but the outcome is identical: without
    ///   suppression, `set_match_summary`/`open_on_focus` sees a live focus
    ///   target and matching suggestions and reopens immediately). Live
    ///   crash, confirmed via tracing: selecting a suggestion with the
    ///   keyboard narrows `compute_filtered` down to the exact match in the
    ///   same cascade as this reopen, so the reopen's layout request lands
    ///   on `VirtualScroll` while a just-shrunk-out row is still tracked but
    ///   `is_explicitly_stashed` — masonry's layout pass panics ("trying to
    ///   compute layout of stashed widget") because nothing had suppressed
    ///   the reopen to give the pending `Fetch`/`remove_child` a chance to
    ///   run first.
    pub(crate) fn mark_closed_suppressing_reopen(this: &mut WidgetMut<'_, Self>) {
        this.widget.suppress_focus_open = true;
        Self::mark_closed(this);
    }
}

/// Dismiss hook registered with the portal slot (see
/// [`crate::overlay_portal::DismissHook`]): syncs `open`
/// after an outside-press dismissal via [`AutocompleteWidget::mark_closed`].
pub(crate) fn autocomplete_dismiss_hook(mut w: WidgetMut<'_, dyn Widget>) {
    let mut ac = w.downcast::<AutocompleteWidget>();
    AutocompleteWidget::mark_closed(&mut ac);
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
                // Tab-focused, has its own highlight-aware Enter handling —
                // see `CollectionListWidget::on_text_event` and
                // `SuggestionList::on_text_event`). `select_suggestion` fills
                // the field, closes the popup, and consumes the key. When
                // closed, it's left unhandled so an enclosing form can submit.
                TextAction::Entered(_) => {
                    if self.open
                        && let Some(first) = self.first_suggestion.clone()
                    {
                        self.select_suggestion(ctx, first.to_string());
                    }
                }
                // Escape/cancel from the text area — the popup-dismiss and
                // field semantics are handled via `InputCleared` below, so
                // there's nothing to do here.
                TextAction::Cancelled => {}
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
                self.close_overlay_later(ctx);
            } else {
                // No popup to dismiss: fall back to the bare-input clear
                // behavior (InputFrame emits InputCleared on Escape), emptying
                // the field and reporting the change.
                self.contents.clear();
                ctx.submit_action::<Self::Action>(AutocompleteAction::TextChanged(String::new()));
            }
            ctx.set_handled();
            ctx.request_paint_only();
            ctx.request_accessibility_update();
        }
    }

    /// Tab moves real focus into the open listbox; once there,
    /// [`CollectionListWidget`](crate::collection)'s own `on_text_event`
    /// handles arrow keys/Enter, and [`SuggestionList::on_text_event`]
    /// handles Enter/Escape/Tab (refocus + close).
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
            // native backward search: the listbox must return `true` from
            // `accepts_focus()` (accesskit only exposes `Action::Focus` —
            // and so AT-driven Tab navigation — to widgets that do), so
            // native search can and does treat it as a valid Shift+Tab
            // target when the input holds focus, since in-tree it lives in
            // the same subtree as the input. Rather than fight that search,
            // close the dropdown here and consume the keypress; a second
            // real Shift+Tab, with the listbox no longer relevant, then
            // cycles backward correctly on its own.
            self.open = false;
            match &mut self.hosting {
                Hosting::InTree { overlay_host } => {
                    ctx.mutate_child_later(overlay_host, |mut w| {
                        AnchoredOverlay::set_overlay_visible(&mut w, false);
                    });
                }
                Hosting::Portal { binding, .. } => binding.close(ctx),
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
                match &mut self.hosting {
                    Hosting::InTree { overlay_host } => {
                        ctx.mutate_child_later(overlay_host, |mut w| {
                            AnchoredOverlay::set_overlay_visible(&mut w, false);
                        });
                    }
                    Hosting::Portal { binding, .. } => binding.close(ctx),
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
            // hasn't fired yet. Closing here would race the click-selection's
            // own `on_activated` hook (`super::view`), which itself closes
            // the dropdown.
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
                match &mut self.hosting {
                    Hosting::InTree { overlay_host } => {
                        ctx.mutate_child_later(overlay_host, |mut w| {
                            AnchoredOverlay::set_overlay_visible(&mut w, false);
                        });
                    }
                    Hosting::Portal { binding, .. } => binding.close(ctx),
                }
                ctx.request_paint_only();
                ctx.request_accessibility_update();
            }
            // Open the dropdown when focus enters the input field.
            // `open_on_focus` itself bails out when `first_suggestion` is
            // `None`, so there's no need to duplicate that check here.
            Update::ChildFocusChanged(true) if !self.open => {
                self.open_on_focus(ctx);
            }
            _ => {}
        }
    }

    /// Re-anchors a still-open portal-mode suggestion list as we move in
    /// window space (e.g. scrolling). No-op in-tree.
    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        let binding = match &mut self.hosting {
            Hosting::Portal { binding, .. } => Some(binding),
            Hosting::InTree { .. } => None,
        };
        binding::compose_reanchor(ctx, self.open, binding);
    }

    /// Keeps compose running every frame while portal-mode is open — see
    /// [`binding::arm_reanchor_on_anim_frame`].
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
mod accessibility_tests {
    use masonry::accesskit::Role;
    use masonry::core::keyboard::{Code, Key, KeyboardEvent, Modifiers, NamedKey};
    use masonry::core::{NewWidget, PointerButton, TextEvent, WidgetRef};
    use masonry::testing::TestHarness;
    use masonry::widgets::{Passthrough, TextArea};
    use xilem::masonry::widgets::{VirtualScroll as VirtualScrollWidget, VirtualScrollAction};

    use super::*;
    use crate::collection::{CollectionListWidget, OnActivated, render_overlay_list_item};
    use crate::overlay_portal::{PortalPlacement, PortalSlot};
    use crate::overlay_scope::OverlayScope;

    const FRUITS: [&str; 3] = ["Apple", "Banana", "Cherry"];

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

    /// A synchronous, `EventCtx`-level hook matching what
    /// `super::view::build_list_view` wires in production: refocus the text
    /// input, then close the dropdown via `mark_closed`. Built directly here
    /// (rather than through the view layer, which these widget-level tests
    /// deliberately bypass — see [`harness_with_fruit`]'s doc comment) so
    /// click-selection tests can exercise the real refocus/close mechanism.
    fn make_on_activated(
        handle: &AutocompleteHandle,
        text_area_handle: &TextAreaHandle,
    ) -> OnActivated {
        let handle = handle.clone();
        let text_area_handle = text_area_handle.clone();
        std::sync::Arc::new(move |ctx: &mut EventCtx<'_>| {
            if let Some(id) = text_area_handle.widget_id() {
                ctx.set_focus(id);
            }
            if let Some(ac_id) = handle.widget_id() {
                ctx.mutate_later(ac_id, |mut w| {
                    let mut ac = w.downcast::<AutocompleteWidget>();
                    AutocompleteWidget::mark_closed_suppressing_reopen(&mut ac);
                });
            }
        })
    }

    /// Handles threaded through an in-tree fixture, so tests can build an
    /// `on_activated` hook or drive materialization after construction.
    #[allow(
        dead_code,
        reason = "handles kept for symmetry/future use by callers that need them"
    )]
    struct Fixture {
        harness: TestHarness<AutocompleteWidget>,
        autocomplete_id: WidgetId,
        text_area_id: WidgetId,
        handle: AutocompleteHandle,
        text_area_handle: TextAreaHandle,
    }

    /// Builds an in-tree autocomplete with 3 fixed suggestions ("Apple",
    /// "Banana", "Cherry"). The suggestion list starts as an *empty*, real
    /// `CollectionListWidget` (mirroring what `AutocompleteView` actually
    /// builds via `overlay_list`) — call [`Fixture::drive_to_fixpoint`] to
    /// materialize real, clickable `OverlayListItem` rows.
    ///
    /// Item content/selection wiring (`on_activated`) is supplied directly
    /// at the widget layer here, not through `AutocompleteView` — these
    /// tests exercise `AutocompleteWidget`/`SuggestionList`'s own
    /// Tab/Enter/Escape/click handling in isolation from the view layer's
    /// `on_select`/`on_changed` plumbing, which has its own coverage in
    /// `crate::collection::overlay_list_body`'s
    /// `overlay_list_body_virtualizes_and_routes_selection_through_real_view_messages`
    /// test (`on_select` firing through a real View message chain — the exact
    /// mechanism `AutocompleteView::build_list_view`'s own `on_select`
    /// closure, a one-line forward to `on_changed`, relies on).
    fn harness_with_fruit() -> Fixture {
        let theme = Theme::default();
        let handle = AutocompleteHandle::new();
        let listbox_handle = ListboxHandle::new();
        let text_area_handle = TextAreaHandle::new();
        let vs = VirtualScrollWidget::new(0, FRUITS.len());
        let list = CollectionListWidget::new(
            NewWidget::new(vs),
            FRUITS.len(),
            Role::ListBox,
            true,
            f64::from(theme.density.row_height),
        );
        let suggestion_list = SuggestionList::new(
            NewWidget::new(list),
            &theme,
            handle.clone(),
            text_area_handle.clone(),
            listbox_handle.clone(),
        );
        let widget = AutocompleteWidget::new(
            AutocompleteConfig {
                contents: String::new(),
                placeholder: ArcStr::from("Pick a fruit"),
                first_suggestion: Some(ArcStr::from("Apple")),
                disabled: false,
                theme,
            },
            NewWidget::new(suggestion_list).erased(),
            handle.clone(),
            listbox_handle,
            &text_area_handle,
        );

        let mut harness = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(widget),
            (300, 300),
        );

        let autocomplete_id = harness.root_id();
        let text_area_id = find_text_area(harness.root_widget().as_dyn())
            .expect("autocomplete should host a TextArea")
            .id();

        drive_in_tree(&mut harness, None);

        Fixture {
            harness,
            autocomplete_id,
            text_area_id,
            handle,
            text_area_handle,
        }
    }

    /// Like [`harness_with_fruit`] but seeds the field with `initial` text
    /// and an already-resolved `first_suggestion` (a simple prefix match
    /// against [`FRUITS`], mirroring what `AutocompleteView`'s own
    /// `compute_filtered` would produce), so text-preserving keyboard
    /// behavior (Escape, Enter) can be exercised without needing a full
    /// materialized listbox.
    fn harness_with_fruit_contents(initial: &str) -> Fixture {
        let theme = Theme::default();
        let handle = AutocompleteHandle::new();
        let listbox_handle = ListboxHandle::new();
        let text_area_handle = TextAreaHandle::new();
        let first_suggestion = FRUITS
            .iter()
            .find(|f| f.to_lowercase().starts_with(&initial.to_lowercase()))
            .map(|f| ArcStr::from(*f));
        let vs = VirtualScrollWidget::new(0, FRUITS.len());
        let list = CollectionListWidget::new(
            NewWidget::new(vs),
            FRUITS.len(),
            Role::ListBox,
            true,
            f64::from(theme.density.row_height),
        );
        let suggestion_list = SuggestionList::new(
            NewWidget::new(list),
            &theme,
            handle.clone(),
            text_area_handle.clone(),
            listbox_handle.clone(),
        );
        let widget = AutocompleteWidget::new(
            AutocompleteConfig {
                contents: initial.to_owned(),
                placeholder: ArcStr::from("Pick a fruit"),
                first_suggestion,
                disabled: false,
                theme,
            },
            NewWidget::new(suggestion_list).erased(),
            handle.clone(),
            listbox_handle,
            &text_area_handle,
        );
        let mut harness = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(widget),
            (300, 300),
        );
        let autocomplete_id = harness.root_id();
        let text_area_id = find_text_area(harness.root_widget().as_dyn())
            .expect("autocomplete should host a TextArea")
            .id();
        drive_in_tree(&mut harness, None);
        Fixture {
            harness,
            autocomplete_id,
            text_area_id,
            handle,
            text_area_handle,
        }
    }

    /// Builds a portal-mode autocomplete with 3 fixed suggestions, wired
    /// into a real `OverlayScope` the way the view layer assembles it
    /// (mirrors `popover::widget::tests::portal_scope_with_host`).
    fn harness_with_fruit_portal() -> (TestHarness<OverlayScope>, WidgetId, WidgetId) {
        let theme = Theme::default();
        let scope_handle = OverlayScopeHandle::new();
        let handle = AutocompleteHandle::new();
        let listbox_handle = ListboxHandle::new();
        let text_area_handle = TextAreaHandle::new();
        let key = 1;

        let autocomplete = AutocompleteWidget::new_portal(
            AutocompleteConfig {
                contents: String::new(),
                placeholder: ArcStr::from("Pick a fruit"),
                first_suggestion: Some(ArcStr::from("Apple")),
                disabled: false,
                theme,
            },
            scope_handle.clone(),
            key,
            handle.clone(),
            listbox_handle.clone(),
            &text_area_handle,
        );
        let vs = VirtualScrollWidget::new(0, FRUITS.len());
        let list = CollectionListWidget::new(
            NewWidget::new(vs),
            FRUITS.len(),
            Role::ListBox,
            true,
            f64::from(theme.density.row_height),
        );
        let suggestion_list = SuggestionList::new(
            NewWidget::new(list),
            &theme,
            handle,
            text_area_handle,
            listbox_handle,
        );
        // Matches what the view layer produces: portal content is a
        // `Pod<Passthrough>` element (the same `AnyElement<Pod<W>, ViewCtx>
        // for Pod<Passthrough>` blanket `overlay_list`/`SuggestionList` rely
        // on generally — see `super::view::SuggestionListView`'s doc
        // comment), so widget-side navigation downcasts through
        // `Passthrough`.
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
        let mut harness = TestHarness::create_with_size(
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

        drive_portal(&mut harness, key, None);

        (harness, autocomplete_id, text_area_id)
    }

    /// Drains `VirtualScrollAction::Fetch` requests from `harness`'s action
    /// queue, materializing real `OverlayListItem` rows (via
    /// `render_overlay_list_item`, carrying `on_activated`) for [`FRUITS`] —
    /// mirrors `crate::collection::imperative_list`'s own
    /// `drive_to_fixpoint`/`harness_with_materialized_rows`, one or two
    /// levels deeper (through `AnchoredOverlay`/`PortalSlot` →
    /// `SuggestionList` → `CollectionListWidget` → `VirtualScroll`).
    /// `navigate` reaches the `SuggestionList<CollectionListWidget>` from
    /// the harness root — the two hosting modes navigate differently, so
    /// it's supplied by the caller.
    fn drive_to_fixpoint<W: Widget>(
        harness: &mut TestHarness<W>,
        on_activated: Option<&OnActivated>,
        navigate: impl Fn(
            &mut WidgetMut<'_, W>,
            &mut dyn FnMut(&mut WidgetMut<'_, SuggestionList<CollectionListWidget>>),
        ),
    ) {
        let mut iteration = 0;
        loop {
            iteration += 1;
            assert!(iteration <= 1000, "Took too long to reach fixpoint");
            let Some((action, _id)) = harness.pop_action::<VirtualScrollAction>() else {
                break;
            };
            let VirtualScrollAction::Fetch(action) = action else {
                continue;
            };
            harness.edit_root_widget(|mut root| {
                navigate(&mut root, &mut |list| {
                    let mut collection = SuggestionList::child_mut(list);
                    {
                        let mut vs = CollectionListWidget::virtual_scroll_mut(&mut collection);
                        VirtualScrollWidget::will_handle_action(&mut vs, &action);
                        for idx in action.old_active().clone() {
                            if !action.target().contains(&idx) {
                                VirtualScrollWidget::remove_child(&mut vs, idx);
                            }
                        }
                        for idx in action.target().clone() {
                            if !action.old_active().contains(&idx) {
                                let row = render_overlay_list_item(
                                    &ArcStr::from(FRUITS[idx]),
                                    false,
                                    &Theme::default(),
                                    Role::ListBoxOption,
                                    on_activated.cloned(),
                                );
                                VirtualScrollWidget::add_child(&mut vs, idx, row);
                            }
                        }
                    }
                    // `overlay_list`'s wrapping View normally keeps this in
                    // sync every rebuild (see `imperative_list`'s module
                    // doc) — these tests bypass the view layer entirely, so
                    // they have to do it themselves, exactly like
                    // `crate::collection::imperative_list`'s own
                    // `harness_with_materialized_rows` does.
                    CollectionListWidget::set_active_start(&mut collection, action.target().start);
                });
            });
        }
    }

    /// Drives an in-tree fixture's listbox to a materialization fixpoint.
    /// Must be called *after* the dropdown is actually visible (focused) —
    /// `AnchoredOverlay`'s overlay slot is stashed (zero effective layout)
    /// until then, so `VirtualScroll`'s fetch requests while stashed settle
    /// on an empty/near-empty window; a real request for the full visible
    /// range only fires once the slot is un-stashed and laid out for real.
    /// Idempotent (a no-op once nothing is pending), so it's safe to call
    /// again after each interaction that might have re-anchored the list.
    fn drive_in_tree(
        harness: &mut TestHarness<AutocompleteWidget>,
        on_activated: Option<&OnActivated>,
    ) {
        drive_to_fixpoint(harness, on_activated, |root, f| {
            AutocompleteWidget::with_overlay_content(root, |mut content| {
                let mut list = content.downcast::<SuggestionList<CollectionListWidget>>();
                f(&mut list);
            });
        });
    }

    /// Portal-mode counterpart to [`drive_in_tree`] — same stashed-until-
    /// visible caveat applies.
    fn drive_portal(
        harness: &mut TestHarness<OverlayScope>,
        key: u64,
        on_activated: Option<&OnActivated>,
    ) {
        drive_to_fixpoint(harness, on_activated, |root, f| {
            let mut slot = OverlayScope::portal_slot_mut(root);
            if let Some(mut child) = PortalSlot::child_mut(&mut slot, key) {
                let mut pass = child.downcast::<Passthrough>();
                let mut inner = Passthrough::child_mut(&mut pass);
                let mut list = inner.downcast::<SuggestionList<CollectionListWidget>>();
                f(&mut list);
            }
        });
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
    /// itself accepts focus.
    #[test]
    fn clicking_a_non_focusable_sibling_closes_the_in_tree_dropdown() {
        let theme = Theme::default();
        let handle = AutocompleteHandle::new();
        let listbox_handle = ListboxHandle::new();
        let text_area_handle = TextAreaHandle::new();
        let vs = VirtualScrollWidget::new(0, FRUITS.len());
        let list = CollectionListWidget::new(
            NewWidget::new(vs),
            FRUITS.len(),
            Role::ListBox,
            true,
            f64::from(theme.density.row_height),
        );
        let suggestion_list = SuggestionList::new(
            NewWidget::new(list),
            &theme,
            handle.clone(),
            text_area_handle.clone(),
            listbox_handle.clone(),
        );
        let autocomplete = AutocompleteWidget::new(
            AutocompleteConfig {
                contents: String::new(),
                placeholder: ArcStr::from("Pick a fruit"),
                first_suggestion: Some(ArcStr::from("Apple")),
                disabled: false,
                theme,
            },
            NewWidget::new(suggestion_list).erased(),
            handle,
            listbox_handle,
            &text_area_handle,
        );
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

        let label_ref = harness.get_widget_with_id(label_id);
        let label_size = label_ref.ctx().border_box().size();
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

    /// Reproduces the real bug behind "tabbing into the list of autocomplete
    /// entries is broken": `AutocompleteView::rebuild`'s portal branch
    /// deregisters the old `SuggestionListView` portal entry and registers a
    /// fresh one on every closed→open transition (`just_opened` — see that
    /// method's doc comment for why: a stale, still-stashed leftover row
    /// from before close must not be diffed against). That mints a brand
    /// new `SuggestionList`/`CollectionListWidget` with a *different*
    /// `WidgetId`, but reuses the *same*, long-lived `ListboxHandle` across
    /// both mounts (`build_list_view` is always given `listbox_handle.clone()`
    /// of the one `AutocompleteView::build` created).
    ///
    /// `ListboxHandle` is built via the shared `widget_id_handle!` macro,
    /// which backs `AutocompleteHandle`/`TextAreaHandle` correctly (their
    /// target widgets live for the whole `AutocompleteWidget` lifetime and
    /// are never torn down, so "latch on first `Update::WidgetAdded`" is
    /// equivalent to "always up to date") — but the listbox itself *is* torn
    /// down and rebuilt here, so the first mount's `OnceLock` write wins
    /// forever, and the second mount's own `Update::WidgetAdded` silently
    /// no-ops. `AutocompleteWidget::on_text_event`'s forward-Tab handler
    /// then calls `ctx.set_focus` with that stale id, targeting a widget
    /// that no longer exists.
    #[test]
    fn listbox_handle_repoints_after_the_portal_list_is_torn_down_and_rebuilt() {
        let theme = Theme::default();
        let listbox_handle = ListboxHandle::new();

        let build_suggestion_list = |listbox_handle: ListboxHandle| {
            let handle = AutocompleteHandle::new();
            let text_area_handle = TextAreaHandle::new();
            let vs = VirtualScrollWidget::new(0, FRUITS.len());
            let list = CollectionListWidget::new(
                NewWidget::new(vs),
                FRUITS.len(),
                Role::ListBox,
                true,
                f64::from(theme.density.row_height),
            );
            let list_widget = NewWidget::new(list);
            let list_id = list_widget.id();
            let suggestion_list = SuggestionList::new(
                list_widget,
                &theme,
                handle,
                text_area_handle,
                listbox_handle,
            );
            let mut harness = TestHarness::create_with_size(
                masonry::theme::default_property_set(),
                NewWidget::new(suggestion_list),
                (300, 300),
            );
            harness.render();
            list_id
        };

        // First open: mounts the listbox the dropdown starts with.
        let first_id = build_suggestion_list(listbox_handle.clone());
        assert_eq!(listbox_handle.widget_id(), Some(first_id));

        // Reopen: the portal tears down the first entry and mounts a fresh
        // one — a different WidgetId — reusing the same handle.
        let second_id = build_suggestion_list(listbox_handle.clone());
        assert_ne!(
            first_id, second_id,
            "each fresh mount must mint a different WidgetId"
        );

        assert_eq!(
            listbox_handle.widget_id(),
            Some(second_id),
            "the handle must repoint at the freshly mounted listbox once the \
             old one is torn down, or Tab's ctx.set_focus(listbox_id) targets \
             a widget that no longer exists"
        );
    }

    /// Real accessibility-tree proof, driven through the actual masonry
    /// focus/keyboard pipeline: focusing the field opens the popup and wires
    /// the ARIA combobox relationship (`expanded` + `controls` -> a real
    /// `Role::ListBox` node). Arrow-key list navigation can't be driven from
    /// the textbox (masonry's `TextArea` unconditionally claims those keys
    /// for cursor movement before any ancestor sees them — see
    /// [`AutocompleteWidget::on_text_event`]), so Tab must move real focus
    /// into the listbox first; from there, arrow keys move
    /// `active_descendant` to real `Role::ListBoxOption` nodes — the
    /// index/highlight bookkeeping itself is `CollectionListWidget`'s own
    /// (exhaustively covered by `crate::collection::imperative_list`'s own
    /// tests), so this only confirms the wiring reaches it.
    #[test]
    fn tab_into_listbox_and_arrow_keys_set_active_descendant() {
        let mut fx = harness_with_fruit();

        fx.harness.render();
        {
            let node = fx
                .harness
                .access_node(fx.autocomplete_id)
                .expect("combobox node");
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
            assert_eq!(
                node.controls().count(),
                0,
                "no aria-controls while the listbox is stashed/collapsed"
            );
        }

        fx.harness.focus_on(Some(fx.text_area_id));
        fx.harness.render();
        drive_in_tree(&mut fx.harness, None);
        {
            let node = fx
                .harness
                .access_node(fx.autocomplete_id)
                .expect("combobox node");
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
            assert!(
                controlled[0]
                    .data()
                    .supports_action(masonry::accesskit::Action::Focus),
                "the listbox must be focusable via the accessibility action API"
            );
        }

        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        let focused_role = fx
            .harness
            .focused_widget_id()
            .and_then(|id| fx.harness.access_node(id))
            .map(|n| n.role());
        assert_eq!(
            focused_role,
            Some(Role::ListBox),
            "Tab should move focus into the open listbox"
        );

        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        fx.harness.render();
        {
            let node = fx
                .harness
                .access_node(fx.autocomplete_id)
                .expect("combobox node");
            let listbox = node.controls().next().expect("controls the listbox");
            let active = listbox.active_descendant().expect("active descendant set");
            assert_eq!(active.role(), Role::ListBoxOption);
            assert_eq!(active.label().as_deref(), Some("Apple"));
            assert_eq!(active.is_selected(), Some(true));
        }

        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        fx.harness.render();
        {
            let node = fx
                .harness
                .access_node(fx.autocomplete_id)
                .expect("combobox node");
            let listbox = node.controls().next().expect("controls the listbox");
            let active = listbox.active_descendant().expect("active descendant set");
            assert_eq!(active.label().as_deref(), Some("Banana"));
        }
    }

    /// Shift+Tab pressed from the text input (dropdown open, never having
    /// Tab'd into the listbox) must not land real focus on the listbox:
    /// `on_text_event`'s explicit Shift+Tab interception closes the dropdown
    /// and consumes the keypress instead.
    #[test]
    fn shift_tab_from_input_closes_dropdown_instead_of_focusing_listbox() {
        let mut fx = harness_with_fruit();

        fx.harness.focus_on(Some(fx.text_area_id));
        fx.harness.render();
        assert_eq!(
            fx.harness
                .access_node(fx.autocomplete_id)
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
        fx.harness.process_text_event(shift_tab);
        fx.harness.render();

        assert_eq!(
            fx.harness.focused_widget_id(),
            Some(fx.text_area_id),
            "Shift+Tab from the input should leave focus on the input, not jump \
             into the listbox"
        );
        assert_eq!(
            fx.harness
                .access_node(fx.autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(false),
            "Shift+Tab from the input should close the dropdown"
        );
    }

    /// Portal-mode-specific regression: forward Tab from the input must move
    /// real focus into the (portal-mounted) listbox *without* closing the
    /// dropdown.
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

    /// Pressing Enter while focus is in the listbox closes the dropdown and
    /// returns real focus to the text field — driven through the real
    /// focus/keyboard pipeline. `SuggestionList::on_text_event` does this
    /// unconditionally on Enter/Escape/Tab (see its doc comment), independent
    /// of whether a row was highlighted/activated — that part is
    /// `CollectionListWidget`'s own job (covered by
    /// `crate::collection::imperative_list`'s tests) and, for the
    /// `on_select` -> `on_changed` leg specifically, by
    /// `overlay_list_body`'s real-View-message test.
    #[test]
    fn enter_in_listbox_closes_and_returns_focus_to_input() {
        let mut fx = harness_with_fruit();

        fx.harness.focus_on(Some(fx.text_area_id));
        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));

        assert_eq!(
            fx.harness.focused_widget_id(),
            Some(fx.text_area_id),
            "Enter should return focus to the input"
        );

        fx.harness.render();
        let node = fx
            .harness
            .access_node(fx.autocomplete_id)
            .expect("combobox node");
        assert_eq!(node.data().is_expanded(), Some(false), "closed after Enter");
    }

    /// Portal-mode regression test for a real crash: selecting a suggestion
    /// via the keyboard (Tab into the listbox, `ArrowDown`, Enter) refocuses
    /// the text input, and in portal mode that's a real cross-subtree focus
    /// transition (the listbox lives in the scope's portal slot, not as this
    /// widget's descendant). `enter_in_listbox_closes_and_returns_focus_to_input`
    /// above only covers in-tree mode, where the listbox is a descendant and
    /// this reopen path can't fire at all — see `AutocompleteWidget::
    /// suppress_focus_open`'s doc comment — which is exactly how this bug
    /// went uncovered.
    ///
    /// This fixture bypasses the view layer entirely (see
    /// `harness_with_fruit_portal`'s doc comment), so nothing calls
    /// `set_match_summary` automatically the way `AutocompleteView::rebuild`
    /// does in production immediately after a selection's `on_changed`
    /// round-trip — simulated here by calling it directly, still reporting
    /// the same (unchanged, still-matching) suggestion. Before the fix,
    /// `SuggestionList::on_text_event`'s Enter/Escape/Tab handling closed the
    /// dropdown via the plain, non-suppressing `mark_closed`, so this
    /// `set_match_summary` call — seeing a live focus target and a matching
    /// `first_suggestion`, with nothing suppressing it — reopened the
    /// dropdown. Live, that reopen raced `VirtualScroll`'s own item-count
    /// shrink (selecting a suggestion narrows `compute_filtered`) and
    /// panicked laying out a row it had just stashed but not yet removed
    /// ("trying to compute layout of stashed widget"). Fixed by routing
    /// through `mark_closed_suppressing_reopen`, which sets
    /// `suppress_focus_open` before closing.
    #[test]
    fn enter_selection_in_portal_listbox_does_not_reopen_the_dropdown() {
        let (mut harness, autocomplete_id, text_area_id) = harness_with_fruit_portal();

        harness.focus_on(Some(text_area_id));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));

        harness.edit_widget_with_id(autocomplete_id, |mut w| {
            let mut ac = w.downcast::<AutocompleteWidget>();
            AutocompleteWidget::set_match_summary(&mut ac, Some(ArcStr::from("Apple")));
        });
        harness.render();

        let node = harness.access_node(autocomplete_id).expect("combobox node");
        assert_eq!(
            node.data().is_expanded(),
            Some(false),
            "selecting via keyboard must not leave the dropdown reopened once the \
             view's post-selection rebuild re-evaluates the match summary"
        );
    }

    /// Escape while focus is in the listbox closes the dropdown and returns
    /// real focus to the text field, without selecting.
    #[test]
    fn escape_in_listbox_closes_and_returns_focus_to_input() {
        let mut fx = harness_with_fruit();

        fx.harness.focus_on(Some(fx.text_area_id));
        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));

        assert_eq!(
            fx.harness.focused_widget_id(),
            Some(fx.text_area_id),
            "Escape should return focus to the input"
        );
        let area = find_text_area(fx.harness.root_widget().as_dyn()).expect("TextArea");
        assert_eq!(area.text().to_string(), "", "Escape must not select");

        fx.harness.render();
        let node = fx
            .harness
            .access_node(fx.autocomplete_id)
            .expect("combobox node");
        assert_eq!(
            node.data().is_expanded(),
            Some(false),
            "closed after Escape"
        );
    }

    /// Clicking a materialized row runs its `on_activated` hook — the same
    /// synchronous, `EventCtx`-level mechanism `AutocompleteView::
    /// build_list_view` wires in production — returning focus to the input
    /// and closing the dropdown, exactly like Enter/Escape/Tab. Proves the
    /// refocus/close mechanism works for *click* selection specifically:
    /// `on_select` (which actually updates the bound text) fires later from
    /// `View::message`, which has no `EventCtx` at all — see
    /// `crate::collection::item_row::OnActivated`'s doc comment for why this
    /// had to be a separate, synchronous hook.
    #[test]
    fn click_selection_refocuses_and_closes() {
        let handle = AutocompleteHandle::new();
        let text_area_handle = TextAreaHandle::new();
        let on_activated = make_on_activated(&handle, &text_area_handle);

        let theme = Theme::default();
        let listbox_handle = ListboxHandle::new();
        let vs = VirtualScrollWidget::new(0, FRUITS.len());
        let list = CollectionListWidget::new(
            NewWidget::new(vs),
            FRUITS.len(),
            Role::ListBox,
            true,
            f64::from(theme.density.row_height),
        );
        let suggestion_list = SuggestionList::new(
            NewWidget::new(list),
            &theme,
            handle.clone(),
            text_area_handle.clone(),
            listbox_handle.clone(),
        );
        let widget = AutocompleteWidget::new(
            AutocompleteConfig {
                contents: String::new(),
                placeholder: ArcStr::from("Pick a fruit"),
                first_suggestion: Some(ArcStr::from("Apple")),
                disabled: false,
                theme,
            },
            NewWidget::new(suggestion_list).erased(),
            handle,
            listbox_handle,
            &text_area_handle,
        );
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
        drive_in_tree(&mut harness, Some(&on_activated));
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(true)
        );

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

        assert_eq!(
            harness.focused_widget_id(),
            Some(text_area_id),
            "click selection should return focus to the input"
        );
        harness.render();
        let node = harness.access_node(autocomplete_id).expect("combobox");
        assert_eq!(
            node.data().is_expanded(),
            Some(false),
            "closed after click-select"
        );
    }

    /// Regression test for a real, live-reproduced bug: `click_selection_
    /// refocuses_and_closes` above proves the click itself closes the
    /// dropdown, but in production the *same* click also drives the host's
    /// `on_changed` (`super::view::build_list_view`'s `on_select`), which
    /// round-trips back as a new `contents`/`suggestions` prop through
    /// `AutocompleteView::rebuild` → `set_contents`/`set_match_summary` — in
    /// the very same reactive cascade as the click, with focus already
    /// restored to the text field. `set_match_summary` has its own,
    /// independent "reopen if focused and matches exist" check, separate
    /// from `open_on_focus`'s `ChildFocusChanged`-driven one that the click
    /// selection already correctly suppresses — so a selection that leaves
    /// an exact self-match (the overwhelmingly common case: the selected
    /// text always matches itself) reopened the dropdown it had just closed.
    /// This simulates that exact round-trip directly (widget-level, since
    /// `AutocompleteView::rebuild` isn't reachable from a bare
    /// `TestHarness<AutocompleteWidget>`) by calling `set_contents`/
    /// `set_match_summary` right after the click, exactly as
    /// `AutocompleteView::rebuild` would.
    #[test]
    fn click_selection_stays_closed_after_the_resulting_on_changed_round_trip() {
        let handle = AutocompleteHandle::new();
        let text_area_handle = TextAreaHandle::new();
        let on_activated = make_on_activated(&handle, &text_area_handle);

        let theme = Theme::default();
        let listbox_handle = ListboxHandle::new();
        let vs = VirtualScrollWidget::new(0, FRUITS.len());
        let list = CollectionListWidget::new(
            NewWidget::new(vs),
            FRUITS.len(),
            Role::ListBox,
            true,
            f64::from(theme.density.row_height),
        );
        let suggestion_list = SuggestionList::new(
            NewWidget::new(list),
            &theme,
            handle.clone(),
            text_area_handle.clone(),
            listbox_handle.clone(),
        );
        let widget = AutocompleteWidget::new(
            AutocompleteConfig {
                contents: String::new(),
                placeholder: ArcStr::from("Pick a fruit"),
                first_suggestion: Some(ArcStr::from("Apple")),
                disabled: false,
                theme,
            },
            NewWidget::new(suggestion_list).erased(),
            handle,
            listbox_handle,
            &text_area_handle,
        );
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
        drive_in_tree(&mut harness, Some(&on_activated));

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
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(false),
            "sanity: closed immediately after the click, same as \
             click_selection_refocuses_and_closes"
        );

        // The part that broke live: `AutocompleteView::rebuild` reacting to
        // the click's own `on_changed("Apple")` landing back as `contents ==
        // "Apple"` — an exact self-match, so `set_match_summary` sees
        // `has_matches == true` with focus already back on the text field.
        harness.edit_root_widget(|mut w| {
            AutocompleteWidget::set_contents(&mut w, "Apple");
            AutocompleteWidget::set_match_summary(&mut w, Some(ArcStr::from("Apple")));
        });
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(false),
            "the on_changed round-trip that follows a click-selection must not \
             reopen the dropdown it just closed"
        );
    }

    /// The suppression a click-selection sets up must not persist forever —
    /// a genuine new keystroke (not the click's own `on_changed` echo) has
    /// to be able to reopen the dropdown normally afterward. Regression
    /// guard for the fix to the bug above: naively never clearing
    /// `suppress_focus_open` (instead of re-arming it on real typing via
    /// `handle_text_changed`) would have silently broken this.
    #[test]
    fn typing_after_a_click_selection_can_reopen_the_dropdown() {
        let handle = AutocompleteHandle::new();
        let text_area_handle = TextAreaHandle::new();
        let on_activated = make_on_activated(&handle, &text_area_handle);

        let theme = Theme::default();
        let listbox_handle = ListboxHandle::new();
        let vs = VirtualScrollWidget::new(0, FRUITS.len());
        let list = CollectionListWidget::new(
            NewWidget::new(vs),
            FRUITS.len(),
            Role::ListBox,
            true,
            f64::from(theme.density.row_height),
        );
        let suggestion_list = SuggestionList::new(
            NewWidget::new(list),
            &theme,
            handle.clone(),
            text_area_handle.clone(),
            listbox_handle.clone(),
        );
        let widget = AutocompleteWidget::new(
            AutocompleteConfig {
                contents: String::new(),
                placeholder: ArcStr::from("Pick a fruit"),
                first_suggestion: Some(ArcStr::from("Apple")),
                disabled: false,
                theme,
            },
            NewWidget::new(suggestion_list).erased(),
            handle,
            listbox_handle,
            &text_area_handle,
        );
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
        drive_in_tree(&mut harness, Some(&on_activated));

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
        harness.render();
        harness.edit_root_widget(|mut w| {
            AutocompleteWidget::set_contents(&mut w, "Apple");
            AutocompleteWidget::set_match_summary(&mut w, Some(ArcStr::from("Apple")));
        });
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(false),
            "sanity: still closed right after selection, as above"
        );

        // A real character keystroke into the (still-focused) text field —
        // the actual `TextArea` → `TextAction::Changed` → `AutocompleteWidget
        // ::on_action` → `handle_text_changed` bubbling path a live keypress
        // takes, re-arming `suppress_focus_open` (see `handle_text_changed`'s
        // doc comment) — proven via `masonry`'s own `TextArea` tests to
        // produce a real `TextAction::Changed`, not a synthesized one.
        harness.process_text_event(TextEvent::key_down(Key::Character("x".into())));
        // The resulting round-trip (simulating what `AutocompleteView::
        // rebuild` would do once the host's `on_changed` feeds the new
        // contents back) should now be free to reopen — reusing "Apple" as
        // the match is enough to prove re-arming worked; the exact resulting
        // text content isn't what's under test here.
        harness.edit_root_widget(|mut w| {
            AutocompleteWidget::set_match_summary(&mut w, Some(ArcStr::from("Apple")));
        });
        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(true),
            "a genuine new keystroke after a click-selection should be able \
             to reopen the dropdown for its own matches"
        );
    }

    /// Escape while the dropdown is open (focus still in the text field, not
    /// the listbox) must dismiss the popup only — leaving the typed text
    /// intact and *not* emitting a text change that would reset the host's
    /// bound value.
    #[test]
    fn escape_in_field_closes_dropdown_without_clearing_text() {
        let mut fx = harness_with_fruit_contents("App");

        fx.harness.focus_on(Some(fx.text_area_id));
        fx.harness.render();
        assert_eq!(
            fx.harness
                .access_node(fx.autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(true),
            "focusing a field with matches should open the dropdown",
        );
        drive_in_tree(&mut fx.harness, None);
        while fx.harness.pop_action_erased().is_some() {}

        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
        fx.harness.render();

        assert_eq!(
            fx.harness
                .access_node(fx.autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(false),
            "Escape should close the open dropdown",
        );
        assert!(
            fx.harness.pop_action::<AutocompleteAction>().is_none(),
            "Escape-to-close must not emit a text change — it dismisses the \
             popup, it does not clear the field",
        );
        let ac = find_autocomplete(fx.harness.root_widget().as_dyn()).expect("autocomplete");
        assert_eq!(ac.contents, "App", "typed text must survive Escape");
    }

    /// Escape with no dropdown open falls back to the bare-input clear
    /// behavior: there's no popup to dismiss, so it empties the field.
    #[test]
    fn escape_with_closed_dropdown_still_clears_the_field() {
        // "zzz" matches nothing, so focusing never opens the dropdown.
        let mut fx = harness_with_fruit_contents("zzz");

        fx.harness.focus_on(Some(fx.text_area_id));
        fx.harness.render();
        assert_eq!(
            fx.harness
                .access_node(fx.autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(false),
            "no matches -> dropdown stays closed",
        );
        while fx.harness.pop_action::<AutocompleteAction>().is_some() {}

        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
        fx.harness.render();

        let (action, _) = fx
            .harness
            .pop_action::<AutocompleteAction>()
            .expect("Escape with no popup should clear and report the change");
        let AutocompleteAction::TextChanged(text) = action;
        assert_eq!(text, "", "Escape with no open dropdown clears the field");
        let ac = find_autocomplete(fx.harness.root_widget().as_dyn()).expect("autocomplete");
        assert!(ac.contents.is_empty(), "field should be cleared");
    }

    /// Pressing Enter in the text field while the dropdown is open accepts
    /// the top suggestion (`AutocompleteWidget::first_suggestion`, pushed by
    /// the view layer — here supplied directly at construction), fills the
    /// field, closes the popup, and reports the change.
    #[test]
    fn enter_in_field_selects_first_suggestion() {
        let mut fx = harness_with_fruit();

        fx.harness.focus_on(Some(fx.text_area_id));
        fx.harness.render();
        drive_in_tree(&mut fx.harness, None);
        while fx.harness.pop_action_erased().is_some() {}

        fx.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
        fx.harness.render();

        let area = find_text_area(fx.harness.root_widget().as_dyn()).expect("TextArea");
        assert_eq!(
            area.text().to_string(),
            "Apple",
            "Enter should accept the top suggestion",
        );
        assert_eq!(
            fx.harness
                .access_node(fx.autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(false),
            "accepting a suggestion closes the dropdown",
        );
        let (action, _) = fx
            .harness
            .pop_action::<AutocompleteAction>()
            .expect("accepting a suggestion should report the change");
        let AutocompleteAction::TextChanged(text) = action;
        assert_eq!(text, "Apple");
    }

    /// Suggestions delivered asynchronously (e.g. a debounced fetch) after
    /// the field already has focus must open the dropdown themselves —
    /// pushed via `set_match_summary`, the same setter `AutocompleteView`
    /// calls whenever the view-computed matching set changes.
    #[test]
    fn async_match_summary_opens_dropdown_while_already_focused() {
        let theme = Theme::default();
        let handle = AutocompleteHandle::new();
        let listbox_handle = ListboxHandle::new();
        let text_area_handle = TextAreaHandle::new();
        let vs = VirtualScrollWidget::new(0, 0);
        let list = CollectionListWidget::new(
            NewWidget::new(vs),
            0,
            Role::ListBox,
            true,
            f64::from(theme.density.row_height),
        );
        let suggestion_list = SuggestionList::new(
            NewWidget::new(list),
            &theme,
            handle.clone(),
            text_area_handle.clone(),
            listbox_handle.clone(),
        );
        let widget = AutocompleteWidget::new(
            AutocompleteConfig {
                contents: String::new(),
                placeholder: ArcStr::from("Pick a fruit"),
                first_suggestion: None,
                disabled: false,
                theme,
            },
            NewWidget::new(suggestion_list).erased(),
            handle,
            listbox_handle,
            &text_area_handle,
        );
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

        harness.edit_root_widget(|mut root| {
            AutocompleteWidget::set_match_summary(&mut root, Some(ArcStr::from("Apple")));
        });

        harness.render();
        assert_eq!(
            harness
                .access_node(autocomplete_id)
                .expect("combobox node")
                .data()
                .is_expanded(),
            Some(true),
            "a matching set arriving while the field is focused should open the dropdown"
        );
    }

    /// An external content change (e.g. host-driven autofill into a
    /// controlled field) while the field already has focus must open the
    /// dropdown itself if the new text matches suggestions — not just narrow
    /// or close an already-open one. `set_contents` only syncs the displayed
    /// text; `set_match_summary` (as the view layer would call it on the
    /// same rebuild) is what actually opens/closes.
    #[test]
    fn set_contents_and_match_summary_open_dropdown_while_already_focused() {
        let mut fx = harness_with_fruit();

        fx.harness.focus_on(Some(fx.text_area_id));
        fx.harness.render();
        assert_eq!(
            fx.harness
                .access_node(fx.autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(true),
            "focusing an empty field with suggestions available should open it"
        );

        fx.harness.edit_root_widget(|mut root| {
            AutocompleteWidget::set_contents(&mut root, "zzz");
            AutocompleteWidget::set_match_summary(&mut root, None);
        });
        fx.harness.render();
        assert_eq!(
            fx.harness
                .access_node(fx.autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(false),
            "a match summary reporting no matches should close the dropdown"
        );

        fx.harness.edit_root_widget(|mut root| {
            AutocompleteWidget::set_contents(&mut root, "Ap");
            AutocompleteWidget::set_match_summary(&mut root, Some(ArcStr::from("Apple")));
        });
        fx.harness.render();
        assert_eq!(
            fx.harness
                .access_node(fx.autocomplete_id)
                .expect("combobox")
                .data()
                .is_expanded(),
            Some(true),
            "external content change to a matching value while focused should reopen the dropdown"
        );
    }
}
