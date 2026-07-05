//! Void UI components.
//!
//! A general-purpose Xilem/Masonry component library: buttons, data grids,
//! sidebars, overlays, and shared layout primitives. The crate absorbs Xilem
//! view-layer churn so application code stays insulated from upstream
//! renames, and stays product-agnostic so it can be reused across
//! independent UIs.
//!
//! ## Design tokens
//!
//! Components read their colors, sizes, and type stack from a [`Theme`]
//! value owned by the host application; see [`theme`] for the primitives.

#![forbid(unsafe_code)]

// Make `::void_ui::...` paths resolvable from within this crate itself, so
// that proc-macros (specifically `void_ui_macros::with_source!`) can emit
// absolute paths into this crate that compile both from external callers
// and from our own modules.
extern crate self as void_ui;

/// Generates a named newtype around `Arc<OnceLock<WidgetId>>` used to pass a
/// widget's id across view/widget boundaries.  Each invocation produces a
/// **distinct type** so the compiler prevents mixing handles at call sites.
macro_rules! widget_id_handle {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Default)]
        pub(crate) struct $name(
            ::std::sync::Arc<::std::sync::OnceLock<::masonry::core::WidgetId>>,
        );

        impl $name {
            pub(crate) fn new() -> Self {
                Self(::std::sync::Arc::new(::std::sync::OnceLock::new()))
            }

            pub(crate) fn widget_id(&self) -> Option<::masonry::core::WidgetId> {
                self.0.get().copied()
            }

            fn set(&self, id: ::masonry::core::WidgetId) {
                let _ = self.0.set(id);
            }
        }
    };
}

pub mod anchored_overlay;
pub mod animated_clip;
mod collection;
pub mod components;
pub mod floating;
pub mod focus_ring;
#[cfg(feature = "gallery")]
pub mod gallery;
pub mod layout;
pub mod overlay;
pub mod overlay_portal;
pub mod overlay_scope;
pub mod pointer_inert;
#[cfg(test)]
pub(crate) mod test_support;
pub mod theme;

pub use animated_clip::AnimatedClip;
pub use components::{
    Alert, AlertVariant, Autocomplete, Badge, Breadcrumb, BreadcrumbSegment, Button, ButtonGroup,
    ButtonVariant, ButtonView, Card, CellAlign, ClickableRow, ColumnDef, ColumnWidths, DataGrid,
    DropdownButton, DropdownButtonView, FilterState, GroupBox, Icon, IconName, Label,
    LabelAlignment, List, MIN_COLUMN_WIDTH, MIN_PANEL_SIZE, NoTitle, Notification,
    NotificationPosition, Orientation, Popover, PopoverAnchor, PopoverHost, PopoverView,
    RangeSlider, RangeSliderView, ReadOnlyText, ReadOnlyTextView, Resizable, ResizablePanel,
    ResizablePanels, ResizablePanelsView, ResizableView, RowClickAction, RustHighlighter,
    ScrollContainer, ScrollContainerView, ScrollState, SelectionState, Separator, SeparatorStyle,
    SidebarItem, SidebarItemView, SidebarNav, SidebarNavItem, SidebarNavView,
    SidebarPanel, SidebarPanelView, Slider, SliderView, SortDirection, SortState, Spinner,
    SpinnerView, SpinnerWidget, StatusDot, TitleState, Tooltip, TooltipView, WithTitle, alert,
    autocomplete, badge, breadcrumb, button, button_group, card, clickable_row,
    colored_text_column, data_grid, dropdown_button, filtered_indices, group_box, h_resizable,
    h_resizable_panels, icon, label, list, notification, notification_layer, notification_overlay,
    notification_stack, optional_text_column, pill, popover, range_slider, read_only_text,
    scroll_container, segment, separator, sidebar_item, sidebar_nav, sidebar_panel, slider,
    spinner, status_dot, text_column, toggle_button_group, tooltip, v_resizable,
    v_resizable_panels,
    Alert, AlertVariant, Autocomplete, Badge, Button, ButtonGroup, ButtonVariant, ButtonView, Card,
    CellAlign, ClickableRow, ColumnDef, ColumnWidths, DataGrid, DropdownButton, DropdownButtonView,
    FilterState, GroupBox, Icon, IconName, Label, LabelAlignment, List, MIN_COLUMN_WIDTH,
    MIN_PANEL_SIZE, NoTitle, Notification, NotificationPosition, Orientation, Popover,
    PopoverAnchor, PopoverHost, PopoverView, RangeSlider, RangeSliderView, ReadOnlyText,
    ReadOnlyTextView, Resizable, ResizablePanel, ResizablePanels, ResizablePanelsView,
    ResizableView, RowClickAction, RustHighlighter, ScrollContainer, ScrollContainerView,
    ScrollState, SelectionState, Separator, SeparatorStyle, SidebarItem, SidebarItemView,
    SidebarNav, SidebarNavItem, SidebarNavView, SidebarPanel, SidebarPanelView, Slider, SliderView,
    SortDirection, SortState, Spinner, SpinnerView, SpinnerWidget, StatusDot, TitleState, Tooltip,
    TooltipView, WithTitle, alert, autocomplete, badge, button, button_group, card, clickable_row,
    colored_text_column, data_grid, dropdown_button, filtered_indices, group_box, h_resizable,
    h_resizable_panels, icon, label, list, notification, notification_layer, notification_overlay,
    notification_stack, optional_text_column, pill, popover, range_slider, read_only_text,
    scroll_container, separator, sidebar_item, sidebar_nav, sidebar_panel, slider, spinner,
    status_dot, text_column, toggle_button_group, tooltip, v_resizable, v_resizable_panels,
};
pub use floating::{FloatingOverlay, FloatingOverlayView, floating, interactive_floating};
#[cfg(feature = "gallery")]
pub use gallery::code_block;
pub use lucide_icons::LUCIDE_FONT_BYTES;
pub use overlay::OverlayAnchor;
pub use overlay_scope::{OverlayScope, OverlayScopeHandle, overlay_scope};
pub use pointer_inert::{PointerInert, PointerInertView, pointer_inert};
pub use theme::{CodePalette, Density, FontStack, Palette, Radii, Theme, ThemeVariant, Typography};
#[cfg(feature = "gallery")]
pub use void_ui_macros::with_source;
