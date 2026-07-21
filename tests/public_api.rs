//! Consumer-facing smoke test.
//!
//! Every other test in this crate lives under `src/` as a `#[cfg(test)]`
//! module with access to crate internals via `crate::` paths. None of them
//! exercise the crate the way an external consumer actually does: through
//! `void_ui::` re-exports only. That's exactly the failure mode a
//! re-export-heavy public API (~150 names funneled through `src/lib.rs`) is
//! prone to — a name that fails to get re-exported, or two re-exports that
//! collide, compiles cleanly from inside the crate (internal code uses
//! fully-qualified `crate::components::foo::Foo` paths, never the
//! re-export) and only breaks for a downstream consumer.
//!
//! Two such bugs have already shipped: `data_grid`'s `SeparatorStyle`
//! collided with `separator`'s own `SeparatorStyle` at the crate root
//! (issue #156, fixed by renaming the former to `ColumnSeparatorStyle`), and
//! `ButtonIcon` exposed a `LucideIcon` type name the crate never actually
//! exported (issue #157). Both are caught by the explicit `use` list below:
//! an ambiguous glob re-export only errors where the ambiguous name is
//! actually *written*, so every public name has to be named here, not just
//! glob-imported.
//!
//! This file intentionally does not use `masonry::testing::TestHarness` or
//! any other `#[cfg(test)]`-gated internal — those aren't visible from an
//! external consumer's Cargo.toml either.

use std::fmt;
use std::sync::Arc;

#[allow(unused_imports)] // exercised by name below, not necessarily constructed
use void_ui::{
    Alert, AlertVariant, Autocomplete, AutocompleteAction, AutocompleteView, Badge, Breadcrumb,
    BreadcrumbSegment, Button, ButtonGroup, ButtonVariant, ButtonView, Card, CellAlign, Checkbox,
    CheckboxPress, CheckboxView, ClickableRow, Clipboard, ClipboardView, CloseCallback,
    Collapsible, CollapsibleView, ColumnDef, ColumnId, ColumnWidths, ComponentKind, ContentButton,
    ContentButtonView, ContextMenuAction, ContextMenuArea, ContextMenuAreaBuilder,
    ContextMenuAreaView, CurrencyFormat, CurrencyInput, DEFAULT_DELAY_MS,
    DEFAULT_NOTIFICATION_WIDTH, DEFAULT_TIMEOUT, DataGrid, DatePicker, DatePickerAction,
    DatePickerView, Dialog, DialogView, DropdownButton, DropdownButtonView, ExpansionState,
    FilterState, GroupBox, Highlighter, Icon, IconName, Input, Label, LabelAlignment, List,
    MIN_COLUMN_WIDTH, MIN_PANEL_SIZE, MaskedInput, Menu, MenuItem, MenuView, Meter, MeterFill,
    NoTitle, Notification, NotificationLayerView, NotificationPosition, NotificationView,
    NumberInput, OnClose, Orientation, Popover, PopoverAnchor, PopoverOpenChanged, PopoverView,
    Radio, RadioView, RangeSlider, RangeSliderView, ReadOnlyText, ReadOnlyTextView, Resizable,
    ResizablePanel, ResizablePanels, ResizablePanelsView, ResizableView, RowClickAction,
    RowComparator, RowFilter, RustHighlighter, ScrollBarVisibility, ScrollContainer,
    ScrollContainerView, ScrollState, SelectionState, Separator, SeparatorStyle, SidebarItem,
    SidebarItemView, SidebarNav, SidebarNavItem, SidebarNavView, SidebarPanel, SidebarPanelView,
    Skeleton, SkeletonAnimation, SkeletonShape, SkeletonView, Slider, SliderView, SortDirection,
    SortState, Spinner, SpinnerView, StatusDot, Submenu, TabItem, Tabs, TabsVariant, TabsView,
    TitleState, Toggle, TogglePress, ToggleView, TokenKind, TokenSpan, Tooltip, TooltipView,
    WithTitle, alert, autocomplete, badge, breadcrumb, button, button_group, card, checkbox,
    clickable_row, clipboard, collapsible, colored_text_column, content_button, context_menu_area,
    currency_input, data_grid, date_picker, dialog, disclosure_chevron, disclosure_icon,
    dropdown_button, filtered_indices, format_currency, format_mask, group_box, h_resizable,
    h_resizable_panels, icon, input, label, list, masked_input, menu, meter, notification,
    notification_layer, notification_overlay, notification_stack, number_input,
    optional_text_column, pill, popover, radio, range_slider, read_only_text, scroll_container,
    segment, separator, sidebar_item, sidebar_nav, sidebar_panel, skeleton, slider, sort_indices,
    spinner, status_dot, submenu, tabs, text_column, toggle, toggle_button_group, tooltip,
    v_resizable, v_resizable_panels,
};
#[allow(unused_imports)]
use void_ui::{
    AnimatedClip, FloatingOverlay, FloatingOverlayView, LUCIDE_FONT_BYTES, OverlayAnchor,
    OverlayScope, OverlayScopeHandle, PointerInert, PointerInertView, floating,
    interactive_floating, menu_item, overlay_scope, pointer_inert,
};
#[allow(unused_imports)]
use void_ui::{CodePalette, Density, FontStack, Palette, Radii, Theme, ThemeVariant, Typography};
use xilem::ViewCtx;
use xilem::core::{ProxyError, RawProxy, SendMessage, View, ViewId};

#[derive(Default)]
struct AppState;

struct NoopProxy;

impl fmt::Debug for NoopProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NoopProxy")
    }
}

impl RawProxy for NoopProxy {
    fn send_message(&self, _path: Arc<[ViewId]>, _message: SendMessage) -> Result<(), ProxyError> {
        Ok(())
    }
    fn dyn_debug(&self) -> &dyn fmt::Debug {
        self
    }
}

fn view_ctx() -> ViewCtx {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap(),
    );
    ViewCtx::new(Arc::new(NoopProxy), runtime)
}

/// `SeparatorStyle` is `separator`'s own type. Before issue #156's fix,
/// `data_grid` re-exported an unrelated same-named `SeparatorStyle` at the
/// crate root, so `void_ui::SeparatorStyle` was ambiguous for any consumer
/// who wrote it out (as opposed to internal code, which only ever used
/// fully-qualified module paths and never noticed). Referencing the bare
/// name here is what would have caught that.
#[allow(dead_code)]
fn separator_style_resolves_unambiguously(style: SeparatorStyle) -> SeparatorStyle {
    style
}

/// `IconName` is the crate's public icon-name type (see issue #157: a prior
/// version of `button`'s `ButtonIcon` API leaked an internal `LucideIcon`
/// name the crate never exported). Referencing it directly locks in that
/// `IconName` is the one and only public name for this concept.
#[allow(dead_code)]
fn icon_name_is_the_public_icon_type(name: IconName) -> IconName {
    name
}

#[test]
fn simple_leaf_and_composition_components_build_from_outside_the_crate() {
    let theme = Theme::default();
    let mut ctx = view_ctx();
    let mut state = AppState;

    let _ = alert("message")
        .render::<AppState, ()>(&theme)
        .build(&mut ctx, &mut state);
    let _ = badge("Draft")
        .render::<AppState, ()>(&theme)
        .build(&mut ctx, &mut state);
    let _ = pill("Active")
        .render::<AppState, ()>(&theme)
        .build(&mut ctx, &mut state);
    let _ = breadcrumb()
        .segment(segment("Home").on_select(|_: &mut AppState| ()))
        .segment(segment::<AppState, ()>("Current"))
        .render(&theme)
        .build(&mut ctx, &mut state);
    let _ = button(|_: &mut AppState| ())
        .label("Click me")
        .render(&theme)
        .build(&mut ctx, &mut state);
    let _ = button_group(["Cut", "Copy", "Paste"], |_: &mut AppState, _i: usize| ())
        .render(&theme)
        .build(&mut ctx, &mut state);
    let _ = toggle_button_group(["Day", "Week"], 0, |_: &mut AppState, _i: usize| ())
        .render(&theme)
        .build(&mut ctx, &mut state);
    let _ = card(label("content").render::<AppState, ()>(&theme))
        .render::<AppState, ()>(&theme)
        .build(&mut ctx, &mut state);
    let _ = checkbox(false, |_: &mut AppState, _checked: bool| ())
        .render(&theme)
        .build(&mut ctx, &mut state);
    let _ = clipboard("https://example.com", |_: &mut AppState, _text: &str| ())
        .render(&theme)
        .build(&mut ctx, &mut state);
    let _ = group_box(label("content").render::<AppState, ()>(&theme))
        .render::<AppState, ()>(&theme)
        .build(&mut ctx, &mut state);
    let _ = icon(IconName::Bell)
        .render::<AppState, ()>(&theme)
        .build(&mut ctx, &mut state);
    let _ = disclosure_chevron::<AppState, ()>(true, &theme).build(&mut ctx, &mut state);
    let _ = label("hello")
        .render::<AppState, ()>(&theme)
        .build(&mut ctx, &mut state);
    let _ = meter(0.5)
        .render::<AppState, ()>(&theme)
        .build(&mut ctx, &mut state);
    let _ = radio("Option A", |_: &mut AppState| ())
        .render(&theme)
        .build(&mut ctx, &mut state);
    let _ = separator()
        .render::<AppState, ()>(&theme)
        .build(&mut ctx, &mut state);
    let _ = slider(50.0, |_: &mut AppState, _v: f64| ())
        .render(&theme)
        .build(&mut ctx, &mut state);
    let _ = range_slider(20.0, 80.0, |_: &mut AppState, _lo: f64, _hi: f64| ())
        .render(&theme)
        .build(&mut ctx, &mut state);
    let _ = spinner()
        .render::<AppState, ()>(&theme)
        .build(&mut ctx, &mut state);
    let _ = status_dot(theme.palette.green)
        .render::<AppState, ()>(&theme)
        .build(&mut ctx, &mut state);
    let _ = toggle(false, |_: &mut AppState, _on: bool| ())
        .render(&theme)
        .build(&mut ctx, &mut state);
}

#[test]
fn dialog_builds_under_a_root_overlay_scope_from_outside_the_crate() {
    let theme = Theme::default();
    let mut ctx = view_ctx();
    let mut state = AppState;

    let content = label("content").render::<AppState, ()>(&theme);
    let scope = overlay_scope(dialog(true, content).render(&theme));
    let _ = scope.build(&mut ctx, &mut state);
}

#[test]
fn tooltip_builds_under_a_root_overlay_scope_from_outside_the_crate() {
    let theme = Theme::default();
    let mut ctx = view_ctx();
    let mut state = AppState;

    let content = label("hint").render::<AppState, ()>(&theme);
    let child = label("target").render::<AppState, ()>(&theme);
    let scope = overlay_scope(tooltip(content, child).render(&theme));
    let _ = scope.build(&mut ctx, &mut state);
}
