//! Shared layout-axis enum.
//!
//! A plain horizontal/vertical axis used by more than one component (e.g.
//! [`separator`](crate::separator) and [`slider`](crate::slider)). It lives
//! here rather than inside any single component so that a component depending
//! on it does not have to depend on an unrelated component's module.

/// A horizontal or vertical layout axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}
