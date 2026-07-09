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
mod anim;
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
/// Re-exports the union of every `pub use` in `components/mod.rs`, minus `item`
/// (re-exported as `menu_item`), plus `ComponentKind`.
pub use components::{
    Alert, AlertVariant, Autocomplete, Badge, Breadcrumb, BreadcrumbSegment, Button, ButtonGroup,
    ButtonVariant, ButtonView, Card, CellAlign, Checkbox, CheckboxView, ClickableRow, Clipboard,
    ClipboardView, Collapsible, CollapsibleView, ColumnDef, ColumnId, ColumnWidths, ComponentKind,
    ContentButton, ContentButtonView, ContextMenuAction, ContextMenuArea, ContextMenuAreaBuilder,
    ContextMenuAreaView, CurrencyFormat, CurrencyInput, DEFAULT_NOTIFICATION_WIDTH,
    DEFAULT_TIMEOUT, DataGrid, Dialog, DialogView, DropdownButton, DropdownButtonView,
    ExpansionState, FilterState, GroupBox, Highlighter, Icon, IconName, Input, Label,
    LabelAlignment, List, MIN_COLUMN_WIDTH, MIN_PANEL_SIZE, MaskedInput, Menu, MenuItem, MenuView,
    NoTitle, Notification, NotificationLayerView, NotificationPosition, NumberInput, OnClose,
    Orientation, Popover, PopoverAnchor, PopoverHost, PopoverOpenChanged, PopoverView, Radio,
    RadioView, RangeSlider, RangeSliderView, ReadOnlyText, ReadOnlyTextView, Resizable,
    ResizablePanel, ResizablePanels, ResizablePanelsView, ResizableView, RowClickAction,
    RowComparator, RowFilter, RustHighlighter, ScrollBarVisibility, ScrollContainer,
    ScrollContainerView, ScrollState, SelectionState, Separator, SeparatorStyle, SidebarItem,
    SidebarItemView, SidebarNav, SidebarNavItem, SidebarNavView, SidebarPanel, SidebarPanelView,
    Skeleton, SkeletonAnimation, SkeletonShape, SkeletonView, Slider, SliderView, SortDirection,
    SortState, Spinner, SpinnerView, SpinnerWidget, StatusDot, Submenu, TabItem, Tabs, TabsVariant,
    TabsView, TitleState, Toggle, ToggleView, TokenKind, TokenSpan, Tooltip, TooltipView,
    WithTitle, alert, autocomplete, badge, breadcrumb, button, button_group, card, checkbox,
    clickable_row, clipboard, collapsible, colored_text_column, content_button, context_menu_area,
    currency_input, data_grid, dialog, disclosure_chevron, disclosure_icon, dropdown_button,
    filtered_indices, format_currency, format_mask, group_box, h_resizable, h_resizable_panels,
    icon, input, label, list, masked_input, menu, notification, notification_layer,
    notification_overlay, notification_stack, number_input, optional_text_column, pill, popover,
    radio, range_slider, read_only_text, scroll_container, segment, separator, sidebar_item,
    sidebar_nav, sidebar_panel, skeleton, slider, sort_indices, spinner, status_dot, submenu, tabs,
    text_column, toggle, toggle_button_group, tooltip, v_resizable, v_resizable_panels,
    ActionRow, Alert, AlertVariant, Autocomplete, Badge, Breadcrumb, BreadcrumbSegment, Button,
    ButtonGroup, ButtonVariant, ButtonView, Card, CellAlign, Checkbox, CheckboxView, ClickableRow,
    Clipboard, ClipboardView, Collapsible, CollapsibleView, ColumnDef, ColumnId, ColumnWidths,
    ComponentKind, ContextMenuAction, ContextMenuArea, ContextMenuAreaBuilder, ContextMenuAreaView,
    CurrencyFormat, CurrencyInput, DEFAULT_NOTIFICATION_WIDTH, DEFAULT_TIMEOUT, DataGrid, Dialog,
    DialogView, DropdownButton, DropdownButtonView, ExpansionState, FilterState, GroupBox,
    Highlighter, Icon, IconName, Input, Label, LabelAlignment, List, MIN_COLUMN_WIDTH,
    MIN_PANEL_SIZE, MaskedInput, Menu, MenuItem, MenuView, NoTitle, Notification,
    NotificationLayerView, NotificationPosition, NumberInput, OnClose, Orientation, Popover,
    PopoverAnchor, PopoverHost, PopoverOpenChanged, PopoverView, Radio, RadioView, RangeSlider,
    RangeSliderView, ReadOnlyText, ReadOnlyTextView, Resizable, ResizablePanel, ResizablePanels,
    ResizablePanelsView, ResizableView, RowClickAction, RowComparator, RowFilter, RustHighlighter,
    ScrollBarVisibility, ScrollContainer, ScrollContainerView, ScrollState, SelectionState,
    Separator, SeparatorStyle, SidebarItem, SidebarItemView, SidebarNav, SidebarNavItem,
    SidebarNavView, SidebarPanel, SidebarPanelView, Skeleton, SkeletonAnimation, SkeletonShape,
    SkeletonView, Slider, SliderView, SortDirection, SortState, Spinner, SpinnerView,
    SpinnerWidget, StatusDot, Submenu, TabItem, Tabs, TabsVariant, TabsView, TitleState, Toggle,
    ToggleView, TokenKind, TokenSpan, Tooltip, TooltipView, WithTitle, action_row, alert,
    autocomplete, badge, breadcrumb, button, button_group, card, checkbox, clickable_row,
    clipboard, collapsible, colored_text_column, context_menu_area, currency_input, data_grid,
    dialog, disclosure_chevron, disclosure_icon, dropdown_button, filtered_indices,
    format_currency, format_mask, group_box, h_resizable, h_resizable_panels, icon, input, label,
    list, masked_input, menu, notification, notification_layer, notification_overlay,
    notification_stack, number_input, optional_text_column, pill, popover, radio, range_slider,
    read_only_text, scroll_container, segment, separator, sidebar_item, sidebar_nav, sidebar_panel,
    skeleton, slider, sort_indices, spinner, status_dot, submenu, tabs, text_column, toggle,
    toggle_button_group, tooltip, v_resizable, v_resizable_panels,
};

/// `components::context_menu::item`, renamed at the root so a glob import of
/// the crate root doesn't claim the generic name `item`.
pub use components::context_menu::item as menu_item;
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

#[cfg(test)]
mod root_export_tests {
    /// Every public view builder resolves at the crate root. Mostly a pure
    /// name-resolution test — a missing re-export fails compilation — plus
    /// real builder calls so a name that resolves to the wrong item (e.g. a
    /// module shadowing a builder fn of the same name) also fails.
    #[test]
    fn root_reexports_are_exhaustive() {
        #[expect(unused_imports, reason = "name-resolution smoke test")]
        use crate::{
            ActionRow, Alert, AlertVariant, Autocomplete, Badge, Breadcrumb, BreadcrumbSegment,
            Button, ButtonGroup, ButtonVariant, ButtonView, Card, CellAlign, Checkbox,
            CheckboxView, ClickableRow, Clipboard, ClipboardView, Collapsible, CollapsibleView,
            ColumnDef, ColumnId, ColumnWidths, ComponentKind, ContextMenuAction, ContextMenuArea,
            ContextMenuAreaBuilder, ContextMenuAreaView, CurrencyFormat, CurrencyInput,
            DEFAULT_NOTIFICATION_WIDTH, DEFAULT_TIMEOUT, DataGrid, Dialog, DialogView,
            DropdownButton, DropdownButtonView, ExpansionState, FilterState, GroupBox, Highlighter,
            Icon, IconName, Input, Label, LabelAlignment, List, MIN_COLUMN_WIDTH, MIN_PANEL_SIZE,
            MaskedInput, Menu, MenuItem, MenuView, NoTitle, Notification, NotificationLayerView,
            NotificationPosition, NumberInput, OnClose, Orientation, Popover, PopoverAnchor,
            PopoverHost, PopoverOpenChanged, PopoverView, Radio, RadioView, RangeSlider,
            RangeSliderView, ReadOnlyText, ReadOnlyTextView, Resizable, ResizablePanel,
            ResizablePanels, ResizablePanelsView, ResizableView, RowClickAction, RowComparator,
            RowFilter, RustHighlighter, ScrollBarVisibility, ScrollContainer, ScrollContainerView,
            ScrollState, SelectionState, Separator, SeparatorStyle, SidebarItem, SidebarItemView,
            SidebarNav, SidebarNavItem, SidebarNavView, SidebarPanel, SidebarPanelView, Slider,
            SliderView, SortDirection, SortState, Spinner, SpinnerView, SpinnerWidget, StatusDot,
            Submenu, TabItem, Tabs, TabsVariant, TabsView, TitleState, Toggle, ToggleView,
            TokenKind, TokenSpan, Tooltip, TooltipView, WithTitle, action_row, alert, autocomplete,
            badge, breadcrumb, button, button_group, card, checkbox, clickable_row, clipboard,
            collapsible, colored_text_column, context_menu_area, currency_input, data_grid, dialog,
            Alert, AlertVariant, Autocomplete, Badge, Breadcrumb, BreadcrumbSegment, Button,
            ButtonGroup, ButtonVariant, ButtonView, Card, CellAlign, Checkbox, CheckboxView,
            ClickableRow, Clipboard, ClipboardView, Collapsible, CollapsibleView, ColumnDef,
            ColumnId, ColumnWidths, ComponentKind, ContentButton, ContentButtonView,
            ContextMenuAction, ContextMenuArea, ContextMenuAreaBuilder, ContextMenuAreaView,
            CurrencyFormat, CurrencyInput, DEFAULT_NOTIFICATION_WIDTH, DEFAULT_TIMEOUT, DataGrid,
            Dialog, DialogView, DropdownButton, DropdownButtonView, ExpansionState, FilterState,
            GroupBox, Highlighter, Icon, IconName, Input, Label, LabelAlignment, List,
            MIN_COLUMN_WIDTH, MIN_PANEL_SIZE, MaskedInput, Menu, MenuItem, MenuView, NoTitle,
            Notification, NotificationLayerView, NotificationPosition, NumberInput, OnClose,
            Orientation, Popover, PopoverAnchor, PopoverHost, PopoverOpenChanged, PopoverView,
            Radio, RadioView, RangeSlider, RangeSliderView, ReadOnlyText, ReadOnlyTextView,
            Resizable, ResizablePanel, ResizablePanels, ResizablePanelsView, ResizableView,
            RowClickAction, RowComparator, RowFilter, RustHighlighter, ScrollBarVisibility,
            ScrollContainer, ScrollContainerView, ScrollState, SelectionState, Separator,
            SeparatorStyle, SidebarItem, SidebarItemView, SidebarNav, SidebarNavItem,
            SidebarNavView, SidebarPanel, SidebarPanelView, Slider, SliderView, SortDirection,
            SortState, Spinner, SpinnerView, SpinnerWidget, StatusDot, Submenu, TabItem, Tabs,
            TabsVariant, TabsView, TitleState, Toggle, ToggleView, TokenKind, TokenSpan, Tooltip,
            TooltipView, WithTitle, alert, autocomplete, badge, breadcrumb, button, button_group,
            card, checkbox, clickable_row, clipboard, collapsible, colored_text_column,
            content_button, context_menu_area, currency_input, data_grid, dialog,
            disclosure_chevron, disclosure_icon, dropdown_button, filtered_indices,
            format_currency, format_mask, group_box, h_resizable, h_resizable_panels, icon, input,
            label, list, masked_input, menu, menu_item, notification, notification_layer,
            notification_overlay, notification_stack, number_input, optional_text_column, pill,
            popover, radio, range_slider, read_only_text, scroll_container, segment, separator,
            sidebar_item, sidebar_nav, sidebar_panel, slider, sort_indices, spinner, status_dot,
            submenu, tabs, text_column, toggle, toggle_button_group, tooltip, v_resizable,
            v_resizable_panels,
        };

        // Exercise a sample of builder fns so a root name that resolves to
        // the wrong item kind (e.g. the `radio` MODULE shadowing the `radio`
        // builder fn) fails to compile, not just to import.
        let _: crate::Radio<_> = crate::radio("Option A", |(): &mut ()| {});
        let _: crate::Checkbox<_> = crate::checkbox(true, |(): &mut (), _: bool| {});
        let _: crate::Toggle<_> = crate::toggle(false, |(): &mut (), _: bool| {});
        let _: crate::MenuItem<(), ()> = crate::menu_item("Copy");
        let _: crate::Menu<(), ()> = crate::menu();
        let _: crate::Submenu<(), ()> = crate::submenu("Open Recent");
        let _: crate::Input<_> = crate::input("text", |(): &mut (), _: String| {});
    }
}
