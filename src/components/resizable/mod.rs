//! Two-pane resizable split panel.
//!
//! Use [`h_resizable`] for a left/right split and [`v_resizable`] for a
//! top/bottom split. Drive the ratio from app state; the `on_resize` callback
//! is called with the updated fraction on every drag step.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State { split_ratio: f32 }
//! # let state = State { split_ratio: 0.5 };
//! # let left_panel = void_ui::label("left").render(&theme);
//! # let right_panel = void_ui::label("right").render(&theme);
//! use void_ui::components::{h_resizable, v_resizable};
//!
//! h_resizable(
//!     left_panel,
//!     right_panel,
//!     |s: &mut State, ratio: f32| s.split_ratio = ratio,
//! )
//! .ratio(state.split_ratio)
//! .render(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{
    Resizable, ResizablePanel, ResizablePanels, ResizablePanelsView, ResizableView, h_resizable,
    h_resizable_panels, v_resizable, v_resizable_panels,
};
pub use widget::MIN_PANEL_SIZE;
