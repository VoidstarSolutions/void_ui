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
use masonry::kurbo::{Axis, Size};
use masonry::layout::{LenReq, Length};

use crate::overlay::OverlayAnchor;
use crate::overlay::binding::{PortalBinding, PortalOpenCtx};
use crate::overlay_scope::OverlayScopeHandle;

/// Action submitted by [`DialogHost`] when the portal slot dismisses the
/// dialog's content in response to an outside press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogDismissed;

/// Dismiss hook registered with the portal slot (see
/// [`crate::overlay_portal::DismissHook`]): unlike the widget-downcast
/// hooks, dialogs are notified by submitting [`DialogDismissed`] from the
/// owner — the view layer's `with_action_widget` routing turns that into
/// the host app's `on_dismiss` callback.
pub(crate) fn dialog_dismiss_hook(mut w: WidgetMut<'_, dyn Widget>) {
    w.ctx.submit_action::<DialogDismissed>(DialogDismissed);
}

/// Zero-footprint widget pushing a dialog's open state to the enclosing
/// [`crate::overlay_scope`]'s portal slot.
///
/// The dialog's content lives in the portal slot, registered separately via
/// [`crate::overlay_portal::OverlayPortal`]; this widget exists purely to
/// hold the binding + `open` flag and to be the [`DialogDismissed`] action
/// source.
pub struct DialogHost {
    binding: PortalBinding,
    open: bool,
}

impl DialogHost {
    #[must_use]
    pub(crate) fn new(scope: OverlayScopeHandle, key: u64, open: bool) -> Self {
        Self {
            binding: PortalBinding::new(scope, key, dialog_dismiss_hook),
            open,
        }
    }

    /// Push the current open/closed state to the portal slot.
    /// [`OverlayAnchor::ViewportQuarter`] has no trigger rect and no gap;
    /// the binding passes [`Rect::ZERO`](masonry::kurbo::Rect::ZERO) and
    /// skips the re-anchor loop for that variant. A not-yet-mounted scope
    /// is a silent no-op (unified error handling — the scope's
    /// `WidgetAdded` runs before any descendant's, so in practice the
    /// handle is always filled here; the previous `expect` never fired).
    fn push_visibility(&mut self, ctx: &mut impl PortalOpenCtx) {
        if self.open {
            self.binding.open(ctx, OverlayAnchor::ViewportQuarter, 0.0);
        } else {
            self.binding.close(ctx);
        }
    }

    /// Push a new open/closed state to the portal slot, if it changed.
    pub(crate) fn set_open(this: &mut WidgetMut<'_, Self>, open: bool) {
        if this.widget.open == open {
            return;
        }
        this.widget.open = open;
        this.widget.push_visibility(&mut this.ctx);
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
            self.push_visibility(ctx);
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

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[])
    }
}
