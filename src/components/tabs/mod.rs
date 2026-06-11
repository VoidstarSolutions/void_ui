//! Tabs component (issue #34).
//!
//! One masonry widget ([`widget::TabsWidget`]) backs six visual styles, selected via
//! [`TabsVariant`]. Selection is host-managed, mirroring
//! [`crate::components::radio`]: pass `selected: usize` and an `on_select`
//! callback that updates the host's own state.
//!
//! ```ignore
//! use void_ui::components::tabs::{tabs, TabItem, TabsVariant};
//!
//! tabs(
//!     vec![TabItem::label("Account"), TabItem::label("Profile")],
//!     state.selected_tab,
//!     |s: &mut State, i| s.selected_tab = i,
//! )
//! .variant(TabsVariant::Underline)
//! .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
pub mod view;
pub mod widget;

pub use view::{TabItem, Tabs, TabsView, tabs};

/// Visual style applied to a [`Tabs`] row — controls how the widget paints
/// its chrome (background, selection indicator, per-item borders).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TabsVariant {
    /// Plain row on a single rounded `surface_2` background; selection is
    /// shown only via text color (set by the view layer).
    #[default]
    Default,
    /// No background; a border line runs under the full row, with a thicker
    /// accent line under the selected item.
    Underline,
    /// Rounded `surface_2` background with a pill-shaped highlight behind the
    /// selected item.
    Pill,
    /// Each item has its own bordered, rounded box; the selected item is
    /// additionally filled.
    Outline,
    /// One bordered, rounded box around the whole row with a filled
    /// highlight behind the selected item. Items take their natural width.
    Segmented,
    /// Like [`Self::Segmented`], but items stretch to evenly fill the
    /// container's width.
    SegmentedFill,
}
