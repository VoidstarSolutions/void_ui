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
pub mod data_grid;
pub mod label;
pub mod radio;
pub mod scroll_container;
pub mod sidebar;
pub mod tooltip;

pub use button::{Button, ButtonVariant, ButtonView, button};
pub use button_group::{ButtonGroup, button_group, toggle_button_group};
pub use checkbox::{Checkbox, CheckboxView, checkbox};
pub use clipboard::{Clipboard, ClipboardView, clipboard};
pub use code_view::{ReadOnlyText, ReadOnlyTextView, RustHighlighter, read_only_text};
pub use data_grid::{
    CellAlign, ColumnDef, ColumnId, ColumnWidths, DataGrid, FilterState, MIN_COLUMN_WIDTH,
    SelectionState, SortDirection, SortState, colored_text_column, data_grid, filtered_indices,
    optional_text_column, text_column,
};
pub use label::{Label, LabelAlignment, label};
pub use scroll_container::{
    ScrollBarVisibility, ScrollContainer, ScrollContainerView, scroll_container,
};
pub use sidebar::{
    SidebarItem, SidebarItemView, SidebarPanel, SidebarPanelView, sidebar_item, sidebar_panel,
};
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
    Checkbox,
    Clipboard,
    CodeView,
    DataGrid,
    Label,
    StockQuotes,
    Radio,
    ScrollContainer,
    Sidebar,
    Tooltip,
}

impl ComponentKind {
    /// Human-readable name for the sidebar.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Button => "Button",
            Self::ButtonGroup => "Button Group",
            Self::Checkbox => "Checkbox",
            Self::Clipboard => "Clipboard",
            Self::CodeView => "Code View",
            Self::DataGrid => "Data Grid",
            Self::Label => "Label",
            Self::StockQuotes => "Stock Quotes",
            Self::Radio => "Radio",
            Self::ScrollContainer => "Scroll Container",
            Self::Sidebar => "Sidebar",
            Self::Tooltip => "Tooltip",
        }
    }

    /// Every component in display order.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Button,
            Self::ButtonGroup,
            Self::Checkbox,
            Self::Clipboard,
            Self::CodeView,
            Self::Label,
            Self::Radio,
            Self::DataGrid,
            Self::StockQuotes,
            Self::ScrollContainer,
            Self::Sidebar,
            Self::Tooltip,
        ]
    }
}
