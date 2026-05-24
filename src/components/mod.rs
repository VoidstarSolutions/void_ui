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
pub mod checkbox;
pub mod data_grid;
pub mod radio;
pub mod sidebar;
pub mod tooltip;

pub use button::{Button, ButtonVariant, ButtonView, button};
pub use checkbox::{Checkbox, CheckboxView, checkbox};
pub use data_grid::{
    CellAlign, ColumnDef, SelectionState, data_grid, optional_text_column, text_column,
};

pub use sidebar::{SidebarItem, SidebarItemView, sidebar_item};
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
    Checkbox,
    DataGrid,
    Radio,
    Sidebar,
    Tooltip,
}

impl ComponentKind {
    /// Human-readable name for the sidebar.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Button => "Button",
            Self::Checkbox => "Checkbox",
            Self::DataGrid => "Data Grid",
            Self::Radio => "Radio",
            Self::Sidebar => "Sidebar",
            Self::Tooltip => "Tooltip",
        }
    }

    /// Every component in display order.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Button,
            Self::Radio,
            Self::DataGrid,
            Self::Checkbox,
            Self::Sidebar,
            Self::Tooltip,
        ]
    }
}
