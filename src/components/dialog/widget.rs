//! Masonry widget for the dialog component.
//!
//! [`DialogHost`] is a zero-footprint marker widget — it never contributes to
//! its parent's layout and paints nothing. Its only job is to push the
//! dialog's open/closed state to the enclosing [`crate::overlay_scope`]'s
//! portal slot (where the dialog's actual content lives, registered via
//! [`crate::overlay_portal::OverlayPortal`]) and to act as the
//! [`DialogDismissed`] action source for outside-click dismissal.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, PaintCtx, PropertiesRef, RegisterCtx, Update,
    UpdateCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Rect, Size};
use masonry::layout::{LenReq, Length};

use crate::components::popover::PopoverAnchor;
use crate::overlay_portal::OwnerKind;
use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

/// Action submitted by [`DialogHost`] when the portal slot dismisses the
/// dialog's content in response to an outside press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogDismissed;

/// Zero-footprint widget pushing a dialog's open state to the enclosing
/// [`crate::overlay_scope`]'s portal slot.
///
/// The dialog's content lives in the portal slot, registered separately via
/// [`crate::overlay_portal::OverlayPortal`]; this widget exists purely to
/// hold the `(scope, key, open)` triple and to be the [`DialogDismissed`]
/// action source.
pub struct DialogHost {
    scope: OverlayScopeHandle,
    key: u64,
    open: bool,
}

/// Push `$self`'s current open/closed state to the portal slot via
/// `$ctx.mutate_later`. Shared between [`DialogHost::set_open`] (a
/// [`WidgetMut`]'s `MutateCtx`) and [`Widget::update`]'s `UpdateCtx` — both
/// expose `widget_id`/`mutate_later` as separate inherent impls rather than a
/// shared trait, hence the macro (mirrors `push_open_state_body!` in
/// `popover/widget.rs`).
///
/// `anchor_rect_window` is ignored for [`PopoverAnchor::ViewportQuarter`]
/// (see [`OverlayScope::set_portal_visible`]), so `Rect::ZERO` is passed
/// unconditionally; likewise there is no gap concept for a centered dialog.
macro_rules! push_visibility_body {
    ($self:expr, $ctx:expr) => {{
        // The scope's `WidgetAdded` runs before any descendant's (it's an
        // ancestor), so by the time this widget exists the handle is filled.
        let scope_id = $self
            .scope
            .widget_id()
            .expect("overlay_scope ancestor must be mounted before DialogHost");
        let key = $self.key;
        let open = $self.open;
        let owner = $ctx.widget_id();
        $ctx.mutate_later(scope_id, move |mut w| {
            let mut scope = w.downcast::<OverlayScope>();
            OverlayScope::set_portal_visible(
                &mut scope,
                key,
                open,
                Some(owner),
                OwnerKind::Dialog,
                Rect::ZERO,
                PopoverAnchor::ViewportQuarter,
                0.0,
            );
        });
    }};
}

impl DialogHost {
    #[must_use]
    pub(crate) fn new(scope: OverlayScopeHandle, key: u64, open: bool) -> Self {
        Self { scope, key, open }
    }

    /// Push a new open/closed state to the portal slot, if it changed.
    pub(crate) fn set_open(this: &mut WidgetMut<'_, Self>, open: bool) {
        if this.widget.open == open {
            return;
        }
        this.widget.open = open;
        push_visibility_body!(this.widget, this.ctx);
    }
}

impl Widget for DialogHost {
    type Action = DialogDismissed;

    fn update(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut masonry::core::PropertiesMut<'_>,
        event: &Update,
    ) {
        if let Update::WidgetAdded = event
            && self.open
        {
            push_visibility_body!(self, ctx);
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::ZERO
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _painter: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, _node: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[])
    }
}
