//! Generic popover component — trigger widget + floating content panel.
//!
//! ```ignore
//! use void_ui::components::popover;
//! popover(
//!     button("Show info").render(&theme),
//!     label("Here is some info.").render(&theme),
//! )
//! .anchor(PopoverAnchor::BottomStart)
//! .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub mod widget;

pub use view::{Popover, PopoverView, popover};
pub use widget::PopoverHost;

/// Where the popover content appears relative to the trigger widget.
///
/// ```text
/// TopStart    TopCenter    TopEnd
/// [  trigger widget  ]
/// BottomStart BottomCenter BottomEnd
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopoverAnchor {
    /// Below the trigger, left-aligned.
    #[default]
    BottomStart,
    /// Below the trigger, centered.
    BottomCenter,
    /// Below the trigger, right-aligned.
    BottomEnd,
    /// Above the trigger, left-aligned.
    TopStart,
    /// Above the trigger, centered.
    TopCenter,
    /// Above the trigger, right-aligned.
    TopEnd,
}

impl PopoverAnchor {
    /// Compute the content's local-coordinate origin given the trigger's and
    /// content's measured sizes and this anchor.
    #[must_use]
    pub(crate) fn child_offset(
        self,
        trigger: masonry::kurbo::Size,
        content: masonry::kurbo::Size,
    ) -> masonry::kurbo::Point {
        use masonry::kurbo::Point;
        match self {
            Self::BottomStart => Point::new(0.0, trigger.height),
            Self::BottomCenter => Point::new((trigger.width - content.width) / 2.0, trigger.height),
            Self::BottomEnd => Point::new(trigger.width - content.width, trigger.height),
            Self::TopStart => Point::new(0.0, -content.height),
            Self::TopCenter => Point::new((trigger.width - content.width) / 2.0, -content.height),
            Self::TopEnd => Point::new(trigger.width - content.width, -content.height),
        }
    }
}

#[cfg(test)]
mod tests {
    use masonry::kurbo::{Point, Size};

    use super::PopoverAnchor;

    /// Float comparison with a tolerance — offsets are derived via
    /// subtraction/division so exact bit-equality isn't guaranteed, and
    /// clippy's `float_cmp` (pedantic) forbids `==` on floats anyway.
    fn approx_point(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    #[test]
    fn bottom_variants_sit_below_the_trigger() {
        let trigger = Size::new(100.0, 30.0);
        let content = Size::new(100.0, 50.0);
        // Equal widths: Start/End/Center all align flush at x = 0.
        assert!(approx_point(
            PopoverAnchor::BottomStart.child_offset(trigger, content),
            Point::new(0.0, 30.0)
        ));
        assert!(approx_point(
            PopoverAnchor::BottomCenter.child_offset(trigger, content),
            Point::new(0.0, 30.0)
        ));
        assert!(approx_point(
            PopoverAnchor::BottomEnd.child_offset(trigger, content),
            Point::new(0.0, 30.0)
        ));
    }

    #[test]
    fn top_variants_sit_above_the_trigger() {
        let trigger = Size::new(100.0, 30.0);
        let content = Size::new(100.0, 50.0);
        // The content's origin is offset upward by its own height so it ends
        // flush against the trigger's top edge, regardless of anchor.
        assert!(approx_point(
            PopoverAnchor::TopStart.child_offset(trigger, content),
            Point::new(0.0, -50.0)
        ));
        assert!(approx_point(
            PopoverAnchor::TopCenter.child_offset(trigger, content),
            Point::new(0.0, -50.0)
        ));
        assert!(approx_point(
            PopoverAnchor::TopEnd.child_offset(trigger, content),
            Point::new(0.0, -50.0)
        ));
    }

    #[test]
    fn center_and_end_variants_account_for_width_difference() {
        // Content (160px) is wider than the trigger (100px) — Center and End
        // both push the origin negative so the content overflows symmetrically
        // / to the left, which is the easy-to-get-backwards case.
        let trigger = Size::new(100.0, 30.0);
        let content = Size::new(160.0, 50.0);

        assert!(approx_point(
            PopoverAnchor::BottomCenter.child_offset(trigger, content),
            Point::new(-30.0, 30.0)
        ));
        assert!(approx_point(
            PopoverAnchor::BottomEnd.child_offset(trigger, content),
            Point::new(-60.0, 30.0)
        ));
        assert!(approx_point(
            PopoverAnchor::TopCenter.child_offset(trigger, content),
            Point::new(-30.0, -50.0)
        ));
        assert!(approx_point(
            PopoverAnchor::TopEnd.child_offset(trigger, content),
            Point::new(-60.0, -50.0)
        ));
    }

    #[test]
    fn start_variants_ignore_width_entirely() {
        // Start always anchors to x = 0, no matter how trigger/content widths relate.
        let content = Size::new(160.0, 50.0);
        assert!(approx_point(
            PopoverAnchor::BottomStart.child_offset(Size::new(40.0, 30.0), content),
            Point::new(0.0, 30.0)
        ));
        assert!(approx_point(
            PopoverAnchor::TopStart.child_offset(Size::new(40.0, 30.0), content),
            Point::new(0.0, -50.0)
        ));
    }

    #[test]
    fn zero_sized_content_collapses_to_the_anchor_point() {
        let trigger = Size::new(100.0, 30.0);
        let content = Size::ZERO;
        assert!(approx_point(
            PopoverAnchor::BottomCenter.child_offset(trigger, content),
            Point::new(50.0, 30.0)
        ));
        assert!(approx_point(
            PopoverAnchor::TopEnd.child_offset(trigger, content),
            Point::new(100.0, 0.0)
        ));
    }
}
