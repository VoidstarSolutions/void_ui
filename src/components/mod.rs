//! Tessera-styled widget components built on the xilem/masonry primitives.
//!
//! Each component is a small builder that resolves to a `WidgetView` at the
//! supplied [`Theme`](crate::Theme). The pattern is intentionally explicit:
//!
//! ```ignore
//! use void_ui::components::button;
//! button("Reset view", |_: &mut State| {}).render(&theme)
//! ```
//!
//! Theme is passed at the render boundary rather than stored on each
//! component value so that swapping themes is a single state change in
//! the host, not a tree walk.

pub mod button;
pub mod button_group;
pub mod checkbox;
pub(crate) mod click;
pub mod clipboard;
pub mod code_view;
pub mod collapsible;
pub mod data_grid;
pub mod dropdown_button;
pub mod icon;
pub mod label;
pub mod popover;
pub mod radio;
pub mod resizable;
pub mod scroll_container;
pub mod separator;
pub mod sidebar;
pub mod slider;
pub mod spinner;
pub mod toggle;
pub mod tooltip;

pub use button::{Button, ButtonVariant, ButtonView, button};
pub use button_group::{ButtonGroup, button_group, toggle_button_group};
pub use checkbox::{Checkbox, CheckboxView, checkbox};
pub use clipboard::{Clipboard, ClipboardView, clipboard};
pub use code_view::{ReadOnlyText, ReadOnlyTextView, RustHighlighter, read_only_text};
pub use collapsible::{Collapsible, CollapsibleView, collapsible};
pub use data_grid::{
    CellAlign, ColumnDef, ColumnId, ColumnWidths, DataGrid, FilterState, MIN_COLUMN_WIDTH,
    SelectionState, SortDirection, SortState, colored_text_column, data_grid, filtered_indices,
    optional_text_column, text_column,
};
pub use dropdown_button::{DropdownButton, DropdownButtonView, dropdown_button};
pub use popover::{Popover, PopoverAnchor, PopoverHost, PopoverView, popover};
pub use icon::{Icon, IconName, icon};
pub use label::{Label, LabelAlignment, label};
pub use resizable::{
    MIN_PANEL_SIZE, Resizable, ResizablePanel, ResizablePanels, ResizablePanelsView, ResizableView,
    h_resizable, h_resizable_panels, v_resizable, v_resizable_panels,
};

pub use scroll_container::{
    ScrollBarVisibility, ScrollContainer, ScrollContainerView, scroll_container,
};
pub use separator::{Orientation, Separator, SeparatorStyle, separator};
pub use sidebar::{
    SidebarItem, SidebarItemView, SidebarPanel, SidebarPanelView, sidebar_item, sidebar_panel,
};
pub use slider::{RangeSlider, RangeSliderView, Slider, SliderView, range_slider, slider};
pub use spinner::{Spinner, SpinnerView, SpinnerWidget, spinner};
pub use toggle::{Toggle, ToggleView, toggle};
pub use tooltip::{Tooltip, TooltipView, tooltip};

/// One entry per component the gallery exposes.
///
/// The gallery uses this enum for two things:
/// - to iterate components when rendering its sidebar
/// - to dispatch to the focused component's `demo::panel` in the main pane
///
/// Adding a new component is one variant + one dispatch arm in the gallery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentKind {
    Button,
    ButtonGroup,
    DropdownButton,
    Checkbox,
    Clipboard,
    CodeView,
    Collapsible,
    DataGrid,
    Icon,
    Label,
	Popover,
    Radio,
    Resizable,
    ScrollContainer,
    Separator,
    Sidebar,
	
    Slider,
    Spinner,
    StockQuotes,
    Toggle,
    Tooltip,
}

impl ComponentKind {
    /// Human-readable name for the sidebar.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Button => "Button",
            Self::ButtonGroup => "Button Group",
            Self::DropdownButton => "Dropdown Button",
            Self::Checkbox => "Checkbox",
            Self::Clipboard => "Clipboard",
            Self::CodeView => "Code View",
            Self::Collapsible => "Collapsible",
            Self::DataGrid => "Data Grid",
            Self::Icon => "Icon",
            Self::Label => "Label",
            Self::Popover => "Popover",
            Self::Radio => "Radio",
            Self::Resizable => "Resizable",
            Self::ScrollContainer => "Scroll Container",
            Self::Separator => "Separator",
            Self::Sidebar => "Sidebar",
            Self::Slider => "Slider",
            Self::Spinner => "Spinner",
            Self::StockQuotes => "Stock Quotes",
            Self::Toggle => "Toggle",
            Self::Tooltip => "Tooltip",
        }
    }

    /// Every component in display order.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Button,
            Self::ButtonGroup,
            Self::DropdownButton,
            Self::Checkbox,
            Self::Clipboard,
            Self::CodeView,
            Self::Collapsible,
            Self::Label,
            Self::Popover,
            Self::Radio,
            Self::DataGrid,
            Self::Icon,
            Self::Resizable,
            Self::ScrollContainer,
            Self::Separator,
            Self::Sidebar,
            Self::Slider,
            Self::Spinner,
            Self::StockQuotes,
            Self::Toggle,
            Self::Tooltip,
        ]
    }
}
