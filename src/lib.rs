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

pub mod anchored_overlay;
pub mod animated_clip;
pub mod components;
pub mod floating;
pub mod focus_ring;
#[cfg(feature = "gallery")]
pub mod gallery;
pub mod layout;
pub mod overlay_scope;
pub mod pointer_inert;
pub mod theme;

pub use animated_clip::AnimatedClip;
pub use components::{
    Button, ButtonGroup, ButtonVariant, ButtonView, CellAlign, ColumnDef, ColumnWidths, DataGrid,
    DropdownButton, DropdownButtonView, FilterState, GroupBox, GroupBoxVariant, Icon, IconName,
    Label, LabelAlignment, MIN_COLUMN_WIDTH, MIN_PANEL_SIZE, Orientation, Popover, PopoverAnchor,
    PopoverHost, PopoverView, RangeSlider, RangeSliderView, ReadOnlyText, ReadOnlyTextView,
    Resizable, ResizablePanel, ResizablePanels, ResizablePanelsView, ResizableView,
    RustHighlighter, ScrollContainer, ScrollContainerView, SelectionState, Separator,
    SeparatorStyle, SidebarItem, SidebarItemView, SidebarPanel, SidebarPanelView, Slider,
    SliderView, SortDirection, SortState, Spinner, SpinnerView, SpinnerWidget, Tooltip,
    TooltipView, button, button_group, colored_text_column, data_grid, dropdown_button,
    filtered_indices, group_box, h_resizable, h_resizable_panels, icon, label,
    optional_text_column, popover, range_slider, read_only_text, scroll_container, separator,
    sidebar_item, sidebar_panel, slider, spinner, text_column, toggle_button_group, tooltip,
    v_resizable, v_resizable_panels,
};
pub use floating::{FloatingOverlay, FloatingOverlayView, floating, interactive_floating};
#[cfg(feature = "gallery")]
pub use gallery::code_block;
pub use lucide_icons::LUCIDE_FONT_BYTES;
pub use overlay_scope::{OverlayScope, OverlayScopeHandle, overlay_scope};
pub use pointer_inert::{PointerInert, PointerInertView, pointer_inert};
pub use theme::{CodePalette, Density, FontStack, Palette, Radii, Theme, ThemeVariant, Typography};
#[cfg(feature = "gallery")]
pub use void_ui_macros::with_source;
