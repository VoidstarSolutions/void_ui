//! Icon-only child widget for the clipboard button.
//!
//! Shows a Copy icon at rest and a Check icon after being activated. The check
//! reverts automatically after [`COPIED_DURATION`] seconds, driven by
//! `request_anim_frame` — no timer thread required.
//!
//! This widget is passive: it emits no actions and handles no events.
//! Interaction (pointer, keyboard, focus, background, focus ring) is owned by
//! the [`ThemedButton`] parent that wraps it.
//!
//! [`ThemedButton`]: crate::components::button::widget::ThemedButton

use std::borrow::Cow;

use lucide_icons::Icon as LucideIcon;
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty, UpdateCtx, Widget, WidgetMut,
    WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length};
use masonry::parley::{FontFamily, FontFamilyName};
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use crate::Theme;

/// Seconds the check icon is shown before reverting to the copy icon.
const COPIED_DURATION: f64 = 1.5;

fn make_icon(copied: bool, theme: &Theme) -> NewWidget<Label> {
    let (lucide, color) = if copied {
        (LucideIcon::Check, theme.palette.accent)
    } else {
        (LucideIcon::Copy, theme.palette.text_muted)
    };
    let ch = char::from(lucide);
    let mut lbl = Label::new(ArcStr::from(String::from(ch)))
        .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
        .with_style(StyleProperty::FontFamily(FontFamily::Single(
            FontFamilyName::Named(Cow::Borrowed("lucide")),
        )))
        .prepare();
    lbl.properties.insert(ContentColor::new(color));
    lbl
}

/// Passive icon widget for the clipboard button.
///
/// Shows the copy or check icon; the parent `ThemedButton` owns all
/// interaction, background painting, and focus handling.
pub struct ClipboardWidget {
    theme: Theme,
    copied: bool,
    copied_t: f64,
    icon: WidgetPod<Label>,
}

// --- MARK: BUILDERS
impl ClipboardWidget {
    #[must_use]
    pub fn new(theme: &Theme) -> Self {
        Self {
            theme: *theme,
            copied: false,
            copied_t: 0.0,
            icon: make_icon(false, theme).to_pod(),
        }
    }
}

// --- MARK: WIDGETMUT
impl ClipboardWidget {
    /// Replaces the theme. Requests a repaint on change.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            let color = if this.widget.copied {
                theme.palette.accent
            } else {
                theme.palette.text_muted
            };
            {
                let mut icon = this.ctx.get_mut(&mut this.widget.icon);
                icon.insert_prop(ContentColor::new(color));
                Label::insert_style(
                    &mut icon,
                    StyleProperty::FontSize(theme.density.ui_font_size),
                );
            }
            this.ctx.request_paint_only();
        }
    }

    /// Transitions into (or out of) the copied-feedback state.
    ///
    /// Setting `true` resets the elapsed timer to zero and arms the animation
    /// loop — including when already copied, so repeated activations restart
    /// the countdown. Setting `false` cancels an in-progress countdown
    /// immediately.
    pub fn set_copied(this: &mut WidgetMut<'_, Self>, copied: bool) {
        if this.widget.copied != copied || copied {
            this.widget.copied = copied;
            this.widget.copied_t = 0.0;
            let (lucide, color) = if copied {
                (LucideIcon::Check, this.widget.theme.palette.accent)
            } else {
                (LucideIcon::Copy, this.widget.theme.palette.text_muted)
            };
            let new_char = ArcStr::from(String::from(char::from(lucide)));
            {
                let mut icon = this.ctx.get_mut(&mut this.widget.icon);
                Label::set_text(&mut icon, new_char);
                icon.insert_prop(ContentColor::new(color));
            }
            if copied {
                this.ctx.request_anim_frame();
            }
            this.ctx.request_paint_only();
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ClipboardWidget {
    type Action = NoAction;

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        if self.copied {
            let interval_ns = u32::try_from(interval).unwrap_or(u32::MAX);
            self.copied_t += f64::from(interval_ns) * 1e-9;
            if self.copied_t >= COPIED_DURATION {
                self.copied = false;
                self.copied_t = 0.0;
                let new_char = ArcStr::from(String::from(char::from(LucideIcon::Copy)));
                let color = self.theme.palette.text_muted;
                ctx.mutate_child_later(&mut self.icon, move |mut icon| {
                    Label::set_text(&mut icon, new_char);
                    icon.insert_prop(ContentColor::new(color));
                });
            } else {
                ctx.request_anim_frame();
            }
            ctx.request_paint_only();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.icon);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let sz = Length::px(f64::from(self.theme.density.ui_font_size));
        let ctx_size = LayoutSize::maybe(axis.cross(), Some(sz));
        let _ = ctx.compute_length(&mut self.icon, len_req.into(), ctx_size, axis, Some(sz));
        sz
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let icon_sz = f64::from(self.theme.density.ui_font_size);
        ctx.run_layout(&mut self.icon, Size::new(icon_sz, icon_sz));
        let icon_x = (size.width - icon_sz) * 0.5;
        let icon_y = (size.height - icon_sz) * 0.5;
        ctx.place_child(&mut self.icon, Point::new(icon_x, icon_y));
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Icon is a self-painting Label child placed during layout.
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
        ChildrenIds::from_slice(&[self.icon.id()])
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn accepts_focus(&self) -> bool {
        false
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}
