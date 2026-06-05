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

pub mod animated_clip;
pub mod components;
pub mod floating;
pub mod focus_ring;
#[cfg(feature = "gallery")]
pub mod gallery;
pub mod layout;
pub mod pointer_inert;
pub mod popover_layer;
pub mod theme;

pub use animated_clip::AnimatedClip;
pub use components::{
    Button, ButtonGroup, ButtonVariant, ButtonView, CellAlign, ColumnDef, ColumnWidths, DataGrid,
    FilterState, Icon, IconName, Label, LabelAlignment, MIN_COLUMN_WIDTH, MIN_PANEL_SIZE,
    Orientation, RangeSlider, RangeSliderView, ReadOnlyText, ReadOnlyTextView, Resizable,
    ResizablePanel, ResizablePanels, ResizablePanelsView, ResizableView, RustHighlighter,
    ScrollContainer, ScrollContainerView, SelectionState, Separator, SeparatorStyle, SidebarItem,
    SidebarItemView, SidebarPanel, SidebarPanelView, Slider, SliderView, SortDirection, SortState,
    Spinner, SpinnerView, SpinnerWidget, Tooltip, TooltipView, button, button_group,
    colored_text_column, data_grid, filtered_indices, h_resizable, h_resizable_panels, icon, label,
    optional_text_column, range_slider, read_only_text, scroll_container, separator, sidebar_item,
    sidebar_panel, slider, spinner, text_column, toggle_button_group, tooltip, v_resizable,
    v_resizable_panels,
    FilterState, Icon, IconName, Label, LabelAlignment, MIN_COLUMN_WIDTH, Orientation, RangeSlider,
    RangeSliderView, ReadOnlyText, ReadOnlyTextView, RustHighlighter, ScrollContainer,
    ScrollContainerView, SelectionState, Separator, SeparatorStyle, SidebarItem, SidebarItemView,
    SidebarPanel, SidebarPanelView, Slider, SliderView, SortDirection, SortState, Spinner,
    SpinnerView, SpinnerWidget, Tooltip, TooltipView, button, button_group, colored_text_column,
    data_grid, filtered_indices, icon, label, optional_text_column, range_slider, read_only_text,
    scroll_container, separator, sidebar_item, sidebar_panel, slider, spinner, text_column,
    toggle_button_group, tooltip,
    SidebarPanel, SidebarPanelView, Slider, SliderView, SortDirection, SortState, Tooltip,
    TooltipView, button, button_group, colored_text_column, data_grid, filtered_indices, icon,
    label, optional_text_column, range_slider, read_only_text, scroll_container, separator,
    sidebar_item, sidebar_panel, slider, text_column, toggle_button_group, tooltip,
    FilterState, Icon, IconName, Label, LabelAlignment, MIN_COLUMN_WIDTH, Orientation,
    ReadOnlyText, ReadOnlyTextView, RustHighlighter, ScrollContainer, ScrollContainerView,
    SelectionState, Separator, SeparatorStyle, SidebarItem, SidebarItemView, SidebarPanel,
    SidebarPanelView, SortDirection, SortState, Tooltip, TooltipView, button, button_group,
    colored_text_column, data_grid, filtered_indices, icon, label, optional_text_column,
    read_only_text, scroll_container, separator, sidebar_item, sidebar_panel, text_column,
    toggle_button_group, tooltip,
    FilterState, Icon, IconName, Label, LabelAlignment, MIN_COLUMN_WIDTH, ReadOnlyText,
    DropdownButton, DropdownButtonView, FilterState, Icon, IconName, Label, LabelAlignment,
    MIN_COLUMN_WIDTH, Popover, PopoverAnchor, PopoverHost, PopoverView, ReadOnlyText,
    ReadOnlyTextView, RustHighlighter, ScrollContainer, ScrollContainerView, SelectionState,
    SidebarItem, SidebarItemView, SidebarPanel, SidebarPanelView, SortDirection, SortState,
    Tooltip, TooltipView, button, button_group, colored_text_column, data_grid, dropdown_button,
    filtered_indices, icon, label, optional_text_column, popover, read_only_text, scroll_container,
    sidebar_item, sidebar_panel, text_column, toggle_button_group, tooltip,
};
pub use floating::{FloatingOverlay, FloatingOverlayView, floating, interactive_floating};
#[cfg(feature = "gallery")]
pub use gallery::code_block;
pub use lucide_icons::LUCIDE_FONT_BYTES;
pub use pointer_inert::{PointerInert, PointerInertView, pointer_inert};
pub use popover_layer::{OnOutsideClick, PopoverLayer};
pub use theme::{CodePalette, Density, FontStack, Palette, Radii, Theme, ThemeVariant, Typography};
#[cfg(feature = "gallery")]
pub use void_ui_macros::with_source;
