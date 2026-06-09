//! Two-pane resizable split panel.
//!
//! Use [`h_resizable`] for a left/right split and [`v_resizable`] for a
//! top/bottom split. Drive the ratio from app state; the `on_resize` callback
//! is called with the updated fraction on every drag step.
//!
//! ```ignore
//! use void_ui::components::{h_resizable, v_resizable};
//!
//! h_resizable(
//!     left_panel,
//!     right_panel,
//!     |s: &mut State, ratio: f32| s.split_ratio = ratio,
//! )
//! .ratio(state.split_ratio)
//! .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub mod widget;

pub use view::{
    Resizable, ResizablePanel, ResizablePanels, ResizablePanelsView, ResizableView, h_resizable,
    h_resizable_panels, v_resizable, v_resizable_panels,
};
pub use widget::MIN_PANEL_SIZE;
